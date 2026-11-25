//! Grimlite Ri `.gbot` command runner, variable/math resolver, and execution state.

use crate::Bot;
use crate::flash::Flash;
use crate::gbot::{BotCommand, BotConfig, IntOp, StatementKind};
use crate::host::FlashArg;
use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::{Duration, Instant};

#[derive(Default)]
pub struct BotVars {
    pub strings: HashMap<String, String>,
    pub ints: HashMap<String, i32>,
}

impl BotVars {
    /// Resolves `{PLAYERNAME}`, `[var]` (up to 5 levels deep), and simple arithmetic expressions.
    pub fn resolve(&self, input: &str, player_name: &str) -> String {
        let mut result = input.replace("{PLAYERNAME}", player_name);
        for _ in 0..5 {
            let prev = result.clone();
            let mut out = String::with_capacity(result.len());
            let mut rest = result.as_str();
            while let Some(start) = rest.find('[') {
                out.push_str(&rest[..start]);
                let after = &rest[start + 1..];
                if let Some(end) = after.find(']') {
                    let key = &after[..end];
                    if let Some(v) = self.strings.get(key) {
                        out.push_str(v);
                    } else if let Some(n) = self.ints.get(key) {
                        out.push_str(&n.to_string());
                    } else {
                        out.push('[');
                        out.push_str(key);
                        out.push(']');
                    }
                    rest = &after[end + 1..];
                } else {
                    out.push('[');
                    rest = after;
                }
            }
            out.push_str(rest);
            result = out;
            if result == prev {
                break;
            }
        }
        eval_simple_math(&result).unwrap_or(result)
    }
}

/// Evaluates basic integer arithmetic (`+`, `-`, `*`, `/`) when the entire string is an expression.
fn eval_simple_math(expr: &str) -> Option<String> {
    let s = expr.trim();
    if s.is_empty()
        || !s
            .bytes()
            .all(|b| b.is_ascii_digit() || matches!(b, b'+' | b'-' | b'*' | b'/' | b' '))
    {
        return None;
    }
    // Only evaluate if there's an operator after the first character
    if !s[1..]
        .bytes()
        .any(|b| matches!(b, b'+' | b'-' | b'*' | b'/'))
    {
        return None;
    }
    let tokens = tokenize_math(s)?;
    let val = eval_tokens(&tokens)?;
    Some(val.to_string())
}

#[derive(Clone, Copy)]
enum Tok {
    Num(i64),
    Op(u8),
}

fn tokenize_math(s: &str) -> Option<Vec<Tok>> {
    let mut out = Vec::new();
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b' ' {
            i += 1;
            continue;
        }
        if matches!(bytes[i], b'+' | b'-' | b'*' | b'/') && !out.is_empty() {
            out.push(Tok::Op(bytes[i]));
            i += 1;
        } else {
            let start = i;
            if bytes[i] == b'-' {
                i += 1;
            }
            while i < bytes.len() && bytes[i].is_ascii_digit() {
                i += 1;
            }
            let num: i64 = std::str::from_utf8(&bytes[start..i]).ok()?.parse().ok()?;
            out.push(Tok::Num(num));
        }
    }
    Some(out)
}

fn eval_tokens(tokens: &[Tok]) -> Option<i64> {
    // Pass 1: * and /
    let mut reduced = Vec::new();
    let mut i = 0;
    while i < tokens.len() {
        match tokens[i] {
            Tok::Op(op @ (b'*' | b'/')) => {
                let Tok::Num(lhs) = reduced.pop()? else {
                    return None;
                };
                let Tok::Num(rhs) = *tokens.get(i + 1)? else {
                    return None;
                };
                let res = if op == b'*' {
                    lhs.checked_mul(rhs)?
                } else {
                    if rhs == 0 {
                        return None;
                    }
                    lhs / rhs
                };
                reduced.push(Tok::Num(res));
                i += 2;
            }
            t => {
                reduced.push(t);
                i += 1;
            }
        }
    }
    // Pass 2: + and -
    let mut iter = reduced.into_iter();
    let Tok::Num(mut acc) = iter.next()? else {
        return None;
    };
    while let Some(Tok::Op(op)) = iter.next() {
        let Tok::Num(rhs) = iter.next()? else {
            return None;
        };
        acc = match op {
            b'+' => acc.checked_add(rhs)?,
            b'-' => acc.checked_sub(rhs)?,
            _ => return None,
        };
    }
    Some(acc)
}

pub struct BotRunner {
    config: RwLock<BotConfig>,
    loaded_name: RwLock<String>,
    running: Arc<AtomicBool>,
    paused: Arc<AtomicBool>,
    index: Arc<AtomicUsize>,
}

impl Default for BotRunner {
    fn default() -> Self {
        Self {
            config: RwLock::new(BotConfig::default()),
            loaded_name: RwLock::new(String::new()),
            running: Arc::new(AtomicBool::new(false)),
            paused: Arc::new(AtomicBool::new(false)),
            index: Arc::new(AtomicUsize::new(0)),
        }
    }
}

impl BotRunner {
    pub fn load_config(&self, name: String, config: BotConfig) {
        self.stop();
        *self.loaded_name.write() = name;
        *self.config.write() = config;
        self.index.store(0, Ordering::Release);
    }

    pub fn config(&self) -> BotConfig {
        self.config.read().clone()
    }

    pub fn loaded_name(&self) -> String {
        self.loaded_name.read().clone()
    }

    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::Acquire)
    }

    pub fn is_paused(&self) -> bool {
        self.paused.load(Ordering::Acquire)
    }

    pub fn current_index(&self) -> usize {
        self.index.load(Ordering::Acquire)
    }

    pub fn set_paused(&self, paused: bool) {
        self.paused.store(paused, Ordering::Release);
    }

    pub fn stop(&self) {
        self.running.store(false, Ordering::Release);
        self.paused.store(false, Ordering::Release);
    }

    pub fn start(&self, bot: Arc<Bot>) {
        if self.running.swap(true, Ordering::AcqRel) {
            return;
        }
        self.paused.store(false, Ordering::Release);
        self.index.store(0, Ordering::Release);

        let running = self.running.clone();
        let paused = self.paused.clone();
        let index = self.index.clone();
        let config = self.config.read().clone();

        let _ = std::thread::Builder::new()
            .name("grimlite-runner".into())
            .spawn(move || {
                bot.logs.push_debug(format!(
                    "[Bot] Started ({} commands)",
                    config.commands.len()
                ));
                let mut vars = BotVars::default();
                let mut skill_cursor = 0usize;

                // Apply bot options from .gbot
                let mut opts = bot.options.get();
                opts.infinite_range |= config.infinite_attack_range;
                opts.provoke_monsters |= config.provoke_monsters;
                opts.enemy_magnet |= config.enemy_magnet;
                opts.lag_killer |= config.lag_killer;
                opts.skip_cutscenes |= config.skip_cutscenes;
                opts.disable_animations |= config.disable_animations;
                bot.apply_options(opts);

                while running.load(Ordering::Acquire) {
                    if config.commands.is_empty() {
                        break;
                    }
                    while paused.load(Ordering::Acquire) && running.load(Ordering::Acquire) {
                        std::thread::sleep(Duration::from_millis(100));
                    }
                    if !running.load(Ordering::Acquire) {
                        break;
                    }

                    let idx = index.load(Ordering::Acquire);
                    if idx >= config.commands.len() {
                        index.store(0, Ordering::Release);
                        continue;
                    }

                    let Some(flash) = bot.flash() else {
                        std::thread::sleep(Duration::from_millis(200));
                        continue;
                    };

                    // Auto pickup/reject drops
                    handle_drops(&bot, &config);

                    let cmd = &config.commands[idx];
                    let next_idx = execute_command(
                        &bot,
                        &flash,
                        &config,
                        cmd,
                        idx,
                        &mut vars,
                        &mut skill_cursor,
                        &running,
                    );
                    index.store(next_idx, Ordering::Release);

                    let delay = config.bot_delay.max(50);
                    sleep_interruptible(delay, &running);
                }
                bot.logs.push_debug("[Bot] Stopped");
            });
    }
}

fn sleep_interruptible(ms: u64, running: &AtomicBool) {
    let mut slept = 0u64;
    while slept < ms && running.load(Ordering::Acquire) {
        let step = (ms - slept).min(50);
        std::thread::sleep(Duration::from_millis(step));
        slept += step;
    }
}

fn handle_drops(bot: &Bot, config: &BotConfig) {
    if !config.enable_pickup && !config.enable_pickup_all && !config.enable_pickup_ac_tagged {
        return;
    }
    let room_id = bot.state.room_id();
    for drop in bot.state.drop_list() {
        let want = config.enable_pickup_all
            || (config.enable_pickup_ac_tagged && drop.is_ac)
            || config
                .drops
                .iter()
                .any(|d| d.eq_ignore_ascii_case(&drop.name));
        if want {
            let pkt = format!("%xt%zm%getDrop%{room_id}%{}%", drop.id);
            let _ = bot.capture.send_to_server(&pkt);
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn execute_command(
    bot: &Bot,
    flash: &Flash,
    config: &BotConfig,
    cmd: &BotCommand,
    idx: usize,
    vars: &mut BotVars,
    skill_cursor: &mut usize,
    running: &AtomicBool,
) -> usize {
    let username = bot.player_snapshot().username;
    match cmd {
        BotCommand::Delay { ms } => {
            sleep_interruptible(*ms, running);
        }
        BotCommand::Label { .. } | BotCommand::Blank { .. } | BotCommand::SkillSet { .. } => {}
        BotCommand::GotoLabel { name } => {
            let target = vars.resolve(name, &username);
            if let Some(pos) = config.commands.iter().position(|c| {
                matches!(c, BotCommand::Label { name: n } if n.eq_ignore_ascii_case(&target))
            }) {
                return pos;
            }
        }
        BotCommand::Index { index: delta, is_up } => {
            let d = (*delta).max(0) as usize;
            return if *is_up {
                idx.saturating_sub(d)
            } else {
                (idx + d).min(config.commands.len())
            };
        }
        BotCommand::Int { name, value, op } => {
            let entry = vars.ints.entry(name.clone()).or_insert(0);
            match op {
                IntOp::Set => *entry = *value,
                IntOp::Add => *entry += *value,
                IntOp::Sub => *entry -= *value,
            }
        }
        BotCommand::SetVar { name, value } => {
            let resolved = vars.resolve(value, &username);
            vars.strings.insert(name.clone(), resolved);
        }
        BotCommand::Log { text } => {
            bot.logs
                .push_debug(format!("[Bot] {}", vars.resolve(text, &username)));
        }
        BotCommand::Stop => {
            running.store(false, Ordering::Release);
            return idx;
        }
        BotCommand::Restart => {
            return 0;
        }
        BotCommand::Provoke { enabled } => {
            let mut opts = bot.options.get();
            opts.provoke_monsters = *enabled;
            bot.apply_options(opts);
        }
        BotCommand::SetFPS { fps } => {
            let mut opts = bot.options.get();
            opts.fps = *fps;
            bot.apply_options(opts);
        }
        BotCommand::Packet {
            packet,
            to_client,
            spam_times,
            delay_ms,
        } => {
            let resolved = vars.resolve(packet, &username);
            let count = (*spam_times).max(1);
            for i in 0..count {
                if !running.load(Ordering::Acquire) {
                    break;
                }
                let res = if *to_client {
                    bot.capture.send_to_client(&resolved)
                } else {
                    bot.capture.send_to_server(&resolved)
                };
                if let Err(e) = res {
                    bot.logs.push_debug(format!("[Packet Cmd] {e}"));
                    break;
                }
                if i + 1 < count && *delay_ms > 0 {
                    sleep_interruptible(*delay_ms, running);
                }
            }
        }
        BotCommand::Join { map, cell, pad } => {
            let m = normalize_join_map(&vars.resolve(map, &username));
            let c = vars.resolve(cell, &username);
            let p = vars.resolve(pad, &username);
            let _ = flash.call_bool(
                "joinMap",
                &[FlashArg::from(m), FlashArg::from(c), FlashArg::from(p)],
            );
            sleep_interruptible(1500, running);
        }
        BotCommand::Yulgar => {
            let _ = flash.call_bool(
                "joinMap",
                &[
                    FlashArg::from("yulgar"),
                    FlashArg::from("Enter"),
                    FlashArg::from("Spawn"),
                ],
            );
            sleep_interruptible(1500, running);
        }
        BotCommand::MoveToCell { cell, pad } => {
            let c = vars.resolve(cell, &username);
            let p = vars.resolve(pad, &username);
            let _ = flash.call_bool("moveToCell", &[FlashArg::from(c), FlashArg::from(p)]);
            sleep_interruptible(400, running);
        }
        BotCommand::Walk { x, y } => {
            let _ = flash.call_bool("walkTo", &[FlashArg::from(*x), FlashArg::from(*y)]);
            sleep_interruptible(400, running);
        }
        BotCommand::SetSpawn => {
            let _ = flash.call_bool("setSpawnPoint", &[]);
        }
        BotCommand::Attack { monster } => {
            let m = vars.resolve(monster, &username);
            let _ = flash.call_bool("attackMonster", &[FlashArg::from(m)]);
        }
        BotCommand::UseSkill { index: skill_idx } => {
            let _ = flash.call_bool("useSkill", &[FlashArg::from(skill_idx.as_str())]);
        }
        BotCommand::CancelTarget => {
            let _ = flash.call_bool("cancelTarget", &[]);
        }
        BotCommand::Rest { full } => {
            let _ = flash.call_bool("rest", &[FlashArg::Bool(*full)]);
            sleep_interruptible(800, running);
        }
        BotCommand::Kill { monster } => {
            let m = vars.resolve(monster, &username);
            kill_once(flash, config, &m, skill_cursor, running);
        }
        BotCommand::KillFor {
            monster,
            item_name,
            quantity,
            is_temp,
        } => {
            let m = vars.resolve(monster, &username);
            let item = vars.resolve(item_name, &username);
            let qty: i32 = vars.resolve(quantity, &username).parse().unwrap_or(1);
            let check_fn = if *is_temp { "hasTempItem" } else { "hasItem" };
            while running.load(Ordering::Acquire) {
                let has = flash
                    .call_bool(check_fn, &[FlashArg::from(item.as_str()), FlashArg::from(qty)])
                    .unwrap_or(false);
                if has {
                    break;
                }
                kill_once(flash, config, &m, skill_cursor, running);
                handle_drops(bot, config);
            }
        }
        BotCommand::ShortHunt {
            map,
            cell,
            pad,
            monster,
            item_name,
            quantity,
            is_temp,
            delay_after_kill,
        } => {
            let item = vars.resolve(item_name, &username);
            let qty: i32 = vars.resolve(quantity, &username).parse().unwrap_or(1);
            let check_fn = if *is_temp { "hasTempItem" } else { "hasItem" };
            if flash
                .call_bool(check_fn, &[FlashArg::from(item.as_str()), FlashArg::from(qty)])
                .unwrap_or(false)
            {
                return idx + 1;
            }
            let m = normalize_join_map(&vars.resolve(map, &username));
            let c = vars.resolve(cell, &username);
            let p = vars.resolve(pad, &username);
            let target_map = m.split('-').next().unwrap_or(&m);
            if !bot.player_snapshot().map.eq_ignore_ascii_case(target_map) {
                let _ = flash.call_bool(
                    "joinMap",
                    &[FlashArg::from(m.as_str()), FlashArg::from(c.as_str()), FlashArg::from(p.as_str())],
                );
                sleep_interruptible(1500, running);
            } else if !bot.player_snapshot().cell.eq_ignore_ascii_case(&c) {
                let _ = flash.call_bool("moveToCell", &[FlashArg::from(c.as_str()), FlashArg::from(p.as_str())]);
                sleep_interruptible(400, running);
            }
            let mon = vars.resolve(monster, &username);
            while running.load(Ordering::Acquire) {
                if flash
                    .call_bool(check_fn, &[FlashArg::from(item.as_str()), FlashArg::from(qty)])
                    .unwrap_or(false)
                {
                    break;
                }
                kill_once(flash, config, &mon, skill_cursor, running);
                handle_drops(bot, config);
                if *delay_after_kill > 0 {
                    sleep_interruptible(*delay_after_kill, running);
                }
            }
        }
        BotCommand::QuestHunt {
            map,
            cell,
            pad,
            monster,
            item_name,
            quantity,
            quest_id,
        } => {
            if *quest_id > 0 {
                let _ = flash.call_bool("loadQuest", &[FlashArg::from(*quest_id)]);
                sleep_interruptible(300, running);
                let _ = flash.call_bool("acceptQuest", &[FlashArg::from(*quest_id)]);
                sleep_interruptible(400, running);
            }
            let m = normalize_join_map(&vars.resolve(map, &username));
            let c = vars.resolve(cell, &username);
            let p = vars.resolve(pad, &username);
            let target_map = m.split('-').next().unwrap_or(&m);
            if !bot.player_snapshot().map.eq_ignore_ascii_case(target_map) {
                let _ = flash.call_bool(
                    "joinMap",
                    &[FlashArg::from(m.as_str()), FlashArg::from(c.as_str()), FlashArg::from(p.as_str())],
                );
                sleep_interruptible(1500, running);
            }
            let item = vars.resolve(item_name, &username);
            let qty: i32 = vars.resolve(quantity, &username).parse().unwrap_or(1);
            let mon = vars.resolve(monster, &username);
            while running.load(Ordering::Acquire) {
                let done = if *quest_id > 0 {
                    flash
                        .call_bool("canCompleteQuest", &[FlashArg::from(*quest_id)])
                        .unwrap_or(false)
                } else {
                    flash
                        .call_bool("hasItem", &[FlashArg::from(item.as_str()), FlashArg::from(qty)])
                        .unwrap_or(false)
                };
                if done {
                    break;
                }
                kill_once(flash, config, &mon, skill_cursor, running);
                handle_drops(bot, config);
            }
        }
        BotCommand::GetDrop { item_name } => {
            let name = vars.resolve(item_name, &username);
            let room_id = bot.state.room_id();
            for drop in bot.state.drop_list() {
                if drop.name.eq_ignore_ascii_case(&name) {
                    let _ = bot
                        .capture
                        .send_to_server(&format!("%xt%zm%getDrop%{room_id}%{}%", drop.id));
                }
            }
            sleep_interruptible(400, running);
        }
        BotCommand::LoadShop { shop_id } => {
            let _ = flash.call_bool("loadShop", &[FlashArg::from(*shop_id)]);
            sleep_interruptible(600, running);
        }
        BotCommand::Buy {
            shop_id,
            item_name,
            qty,
        } => {
            let item = vars.resolve(item_name, &username);
            if *shop_id > 0 {
                let _ = flash.call_bool("loadShop", &[FlashArg::from(*shop_id)]);
                sleep_interruptible(500, running);
            }
            for _ in 0..(*qty).max(1) {
                if !running.load(Ordering::Acquire) {
                    break;
                }
                let _ = flash.call_bool(
                    "buyItem",
                    &[FlashArg::from(*shop_id), FlashArg::from(item.as_str())],
                );
                sleep_interruptible(600, running);
            }
        }
        BotCommand::Sell { item_name } => {
            let item = vars.resolve(item_name, &username);
            let _ = flash.call_bool("sellItem", &[FlashArg::from(item)]);
            sleep_interruptible(600, running);
        }
        BotCommand::Equip { item_name } => {
            let item = vars.resolve(item_name, &username);
            let _ = flash.call_bool("equipItem", &[FlashArg::from(item)]);
            sleep_interruptible(500, running);
        }
        BotCommand::BankTransfer { item_name, to_bank } => {
            let item = vars.resolve(item_name, &username);
            let func = if *to_bank {
                "transferToBank"
            } else {
                "transferToInv"
            };
            let _ = flash.call_bool(func, &[FlashArg::from(item)]);
            sleep_interruptible(500, running);
        }
        BotCommand::MapItem { item_id } => {
            let _ = flash.call_bool("getMapItem", &[FlashArg::from(*item_id)]);
            sleep_interruptible(500, running);
        }
        BotCommand::Whitelist { .. } => {}
        BotCommand::AcceptQuest { quest_id } => {
            let _ = flash.call_bool("loadQuest", &[FlashArg::from(*quest_id)]);
            sleep_interruptible(300, running);
            let _ = flash.call_bool("acceptQuest", &[FlashArg::from(*quest_id)]);
            sleep_interruptible(500, running);
        }
        BotCommand::CompleteQuest { quest_id, item_id } => {
            let _ = flash.call_bool(
                "completeQuest",
                &[FlashArg::from(*quest_id), FlashArg::from(*item_id)],
            );
            sleep_interruptible(600, running);
        }
        BotCommand::Statement { kind, value1, value2 } => {
            let v1 = vars.resolve(value1, &username);
            let v2 = vars.resolve(value2, &username);
            let passed = eval_statement(bot, flash, vars, kind, &v1, &v2);
            if !passed {
                return idx + 2; // Skip next command when statement evaluates to false
            }
        }
        BotCommand::Unknown { type_name, .. } => {
            bot.logs
                .push_debug(format!("[Bot] Skipping unsupported command: {type_name}"));
        }
    }
    idx + 1
}

fn normalize_join_map(map: &str) -> String {
    if let Some((name, room)) = map.split_once('-') {
        if !room.is_empty() && room.parse::<u32>().is_err() {
            // Non-numeric room suffix (e.g. "spellcraft-a"): pick a random private room number.
            let seed = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.subsec_nanos())
                .unwrap_or(42424);
            let room_num = 10000 + (seed % 89999);
            return format!("{name}-{room_num}");
        }
    }
    map.to_string()
}

fn kill_once(
    flash: &Flash,
    config: &BotConfig,
    monster: &str,
    skill_cursor: &mut usize,
    running: &AtomicBool,
) {
    let _ = flash.call_bool("attackMonster", &[FlashArg::from(monster)]);
    let start = Instant::now();
    let skill_delay = config.skill_delay.max(150);
    while running.load(Ordering::Acquire) && start.elapsed() < Duration::from_secs(30) {
        if !flash.call_bool("hasTarget", &[]).unwrap_or(false) {
            break;
        }
        if !config.skills.is_empty() {
            let skill = &config.skills[*skill_cursor % config.skills.len()];
            *skill_cursor = skill_cursor.wrapping_add(1);
            if !skill.index.is_empty() {
                let _ = flash.call_bool("useSkill", &[FlashArg::from(skill.index.as_str())]);
            }
        } else {
            // Default basic rotation 1..4 if no skills configured
            let key = ((*skill_cursor % 4) + 1).to_string();
            *skill_cursor = skill_cursor.wrapping_add(1);
            let _ = flash.call_bool("useSkill", &[FlashArg::from(key)]);
        }
        sleep_interruptible(skill_delay, running);
    }
}

fn eval_statement(
    bot: &Bot,
    flash: &Flash,
    vars: &BotVars,
    kind: &StatementKind,
    v1: &str,
    v2: &str,
) -> bool {
    let snap = bot.player_snapshot();
    let qty = v2.parse::<i32>().unwrap_or(1);
    let n1 = v1.parse::<i32>().unwrap_or(0);
    let n2 = v2.parse::<i32>().unwrap_or(0);
    match kind {
        StatementKind::InInventory => flash
            .call_bool("hasItem", &[FlashArg::from(v1), FlashArg::from(qty)])
            .unwrap_or(false),
        StatementKind::NotInInventory => !flash
            .call_bool("hasItem", &[FlashArg::from(v1), FlashArg::from(qty)])
            .unwrap_or(false),
        StatementKind::InTemp => flash
            .call_bool("hasTempItem", &[FlashArg::from(v1), FlashArg::from(qty)])
            .unwrap_or(false),
        StatementKind::NotInTemp => !flash
            .call_bool("hasTempItem", &[FlashArg::from(v1), FlashArg::from(qty)])
            .unwrap_or(false),
        StatementKind::InBank => flash
            .call_bool("hasBankItem", &[FlashArg::from(v1), FlashArg::from(qty)])
            .unwrap_or(false),
        StatementKind::NotInBank => !flash
            .call_bool("hasBankItem", &[FlashArg::from(v1), FlashArg::from(qty)])
            .unwrap_or(false),
        StatementKind::InBankOrInvent => {
            flash
                .call_bool("hasItem", &[FlashArg::from(v1), FlashArg::from(qty)])
                .unwrap_or(false)
                || flash
                    .call_bool("hasBankItem", &[FlashArg::from(v1), FlashArg::from(qty)])
                    .unwrap_or(false)
        }
        StatementKind::NotInBankAndInvent => {
            !flash
                .call_bool("hasItem", &[FlashArg::from(v1), FlashArg::from(qty)])
                .unwrap_or(false)
                && !flash
                    .call_bool("hasBankItem", &[FlashArg::from(v1), FlashArg::from(qty)])
                    .unwrap_or(false)
        }
        StatementKind::MapIs => snap.map.eq_ignore_ascii_case(v1),
        StatementKind::MapIsNot => !snap.map.eq_ignore_ascii_case(v1),
        StatementKind::CellIs => snap.cell.eq_ignore_ascii_case(v1),
        StatementKind::CellIsNot => !snap.cell.eq_ignore_ascii_case(v1),
        StatementKind::QuestInProgress => flash
            .call_bool("isQuestInProgress", &[FlashArg::from(n1)])
            .unwrap_or(false),
        StatementKind::QuestNotInProgress => !flash
            .call_bool("isQuestInProgress", &[FlashArg::from(n1)])
            .unwrap_or(false),
        StatementKind::QuestCompleted => flash
            .call_bool("canCompleteQuest", &[FlashArg::from(n1)])
            .unwrap_or(false),
        StatementKind::QuestNotCompleted => !flash
            .call_bool("canCompleteQuest", &[FlashArg::from(n1)])
            .unwrap_or(false),
        StatementKind::MonsterInRoom => flash
            .call_bool("isMonsterAvailable", &[FlashArg::from(v1)])
            .unwrap_or(false),
        StatementKind::MonsterNotInRoom => !flash
            .call_bool("isMonsterAvailable", &[FlashArg::from(v1)])
            .unwrap_or(false),
        StatementKind::HasTarget => flash.call_bool("hasTarget", &[]).unwrap_or(false),
        StatementKind::NotHasTarget => !flash.call_bool("hasTarget", &[]).unwrap_or(false),
        StatementKind::HealthLessThan => snap.hp < n1,
        StatementKind::HealthGreaterThan => snap.hp > n1,
        StatementKind::ManaLessThan => snap.mp < n1,
        StatementKind::ManaGreaterThan => snap.mp > n1,
        StatementKind::GoldGreaterThan => snap.gold > i64::from(n1),
        StatementKind::GoldLessThan => snap.gold < i64::from(n1),
        StatementKind::LevelGreaterThan => snap.level > n1,
        StatementKind::LevelLessThan => snap.level < n1,
        StatementKind::IntIs => vars.ints.get(v1).copied().unwrap_or(0) == n2,
        StatementKind::IntIsNot => vars.ints.get(v1).copied().unwrap_or(0) != n2,
        StatementKind::IntGreaterThan => vars.ints.get(v1).copied().unwrap_or(0) > n2,
        StatementKind::IntLessThan => vars.ints.get(v1).copied().unwrap_or(0) < n2,
        StatementKind::IntEqualInt => {
            vars.ints.get(v1).copied().unwrap_or(0) == vars.ints.get(v2).copied().unwrap_or(0)
        }
        StatementKind::IntNotEqualInt => {
            vars.ints.get(v1).copied().unwrap_or(0) != vars.ints.get(v2).copied().unwrap_or(0)
        }
        StatementKind::VarEqualVar => v1.eq_ignore_ascii_case(v2),
        StatementKind::VarNotEqualVar => !v1.eq_ignore_ascii_case(v2),
        StatementKind::NameEquals => snap.username.eq_ignore_ascii_case(v1),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_vars_and_math_expressions() {
        let mut vars = BotVars::default();
        vars.strings.insert("map".into(), "yulgar".into());
        vars.ints.insert("a".into(), 6);
        vars.ints.insert("b".into(), 4);

        assert_eq!(vars.resolve("Hello {PLAYERNAME}", "Questwalker"), "Hello Questwalker");
        assert_eq!(vars.resolve("[map]-1", "Questwalker"), "yulgar-1");
        assert_eq!(vars.resolve("[a] + [b] * 2", "Questwalker"), "14");
    }
}
