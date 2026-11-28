//! Grimlite Ri `.gbot` execution loop (`BotRunner`) and variable/math resolver (`BotVars`).
//! Individual command implementations live in `crate::commands::*`.

use crate::Bot;
use crate::commands::{CommandContext, execute_command, handle_drops, sleep_interruptible};
use crate::gbot::BotConfig;
use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::Duration;

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

fn eval_simple_math(expr: &str) -> Option<String> {
    let s = expr.trim();
    if s.is_empty()
        || !s
            .bytes()
            .all(|b| b.is_ascii_digit() || matches!(b, b'+' | b'-' | b'*' | b'/' | b' '))
    {
        return None;
    }
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

    pub fn update_config(&self, config: BotConfig) {
        *self.config.write() = config;
    }

    pub fn set_quests(&self, quests: Vec<crate::gbot::QuestEntry>) {
        self.config.write().quests = quests;
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
        let mut config = self.config.read().clone();

        // Spawn dedicated background Auto Quest List thread (mirrors Grimlite-Li `StartQuestList`).
        // Keeps accepting/completing quests in `config.quests` concurrently even while the main
        // runner thread is blocked inside long commands such as `Kill for items`.
        {
            let q_bot = Arc::clone(&bot);
            let q_running = running.clone();
            let q_paused = paused.clone();
            let _ = std::thread::Builder::new()
                .name("grimlite-quest-list".into())
                .spawn(move || {
                    while q_running.load(Ordering::Acquire) {
                        if q_paused.load(Ordering::Acquire) || !q_bot.is_logged_in() {
                            sleep_interruptible(300, &q_running);
                            continue;
                        }
                        let (quests, exit_combat, bot_delay) = {
                            let cfg = q_bot.runner.config.read();
                            (
                                cfg.quests.clone(),
                                cfg.exit_combat_before_quest,
                                cfg.bot_delay,
                            )
                        };
                        if quests.is_empty() {
                            sleep_interruptible(800, &q_running);
                            continue;
                        }
                        let Some(flash) = q_bot.flash() else {
                            sleep_interruptible(400, &q_running);
                            continue;
                        };

                        for q in &quests {
                            if !q_running.load(Ordering::Acquire)
                                || q_paused.load(Ordering::Acquire)
                                || !q_bot.is_logged_in()
                            {
                                break;
                            }
                            if !flash
                                .call_bool("isQuestInProgress", &[crate::host::FlashArg::from(q.id)])
                                .unwrap_or(false)
                            {
                                crate::commands::quest::accept_quest_direct(
                                    &q_bot, &flash, &q_running, q.id,
                                );
                            } else if flash
                                .call_bool("canCompleteQuest", &[crate::host::FlashArg::from(q.id)])
                                .unwrap_or(false)
                            {
                                let item_id = q
                                    .item_id
                                    .as_deref()
                                    .and_then(|s| s.trim().parse::<i32>().ok())
                                    .unwrap_or(-1);
                                crate::commands::quest::complete_quest_direct(
                                    &q_bot,
                                    &flash,
                                    &q_running,
                                    q.id,
                                    item_id,
                                    1,
                                    exit_combat,
                                    bot_delay,
                                );
                                crate::commands::quest::accept_quest_direct(
                                    &q_bot, &flash, &q_running, q.id,
                                );
                            }
                        }

                        sleep_interruptible(1000, &q_running);
                    }
                });
        }

        // Spawn dedicated background Auto Pickup / Reject Drops thread so drops are collected
        // immediately even while the main runner is mid-fight inside `Kill for items` or `Delay`.
        {
            let d_bot = Arc::clone(&bot);
            let d_running = running.clone();
            let d_paused = paused.clone();
            let _ = std::thread::Builder::new()
                .name("grimlite-drops".into())
                .spawn(move || {
                    while d_running.load(Ordering::Acquire) {
                        if !d_paused.load(Ordering::Acquire) && d_bot.is_logged_in() {
                            let cfg = d_bot.runner.config.read().clone();
                            handle_drops(&d_bot, &cfg);
                        }
                        sleep_interruptible(400, &d_running);
                    }
                });
        }

        let _ = std::thread::Builder::new()
            .name("grimlite-runner".into())
            .spawn(move || {
                bot.logs.push_debug(format!(
                    "[Bot] Started ({} commands)",
                    config.commands.len()
                ));
                let mut vars = BotVars::default();
                let mut skill_cursor = 0usize;
                let mut active_skill_set = String::new();

                let mut opts = bot.options.get();
                opts.infinite_range |= config.infinite_attack_range;
                opts.provoke_monsters |= config.provoke_monsters;
                opts.enemy_magnet |= config.enemy_magnet;
                opts.lag_killer |= config.lag_killer;
                opts.hide_players |= config.hide_players;
                opts.skip_cutscenes |= config.skip_cutscenes;
                opts.disable_animations |= config.disable_animations;
                opts.anti_counter |= config.anti_counter;
                if config.walk_speed_enabled {
                    opts.walk_speed_enabled = true;
                    opts.walk_speed = config.walk_speed;
                }
                if config.follow_check && !config.follow_name.trim().is_empty() {
                    opts.follow_enabled = true;
                    opts.follow_player = config.follow_name.clone();
                }
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

                    if !bot.is_logged_in() {
                        if !config.auto_relogin {
                            bot.logs
                                .push_debug("[Bot] Stopped (logged out, auto-relogin disabled)");
                            running.store(false, Ordering::Release);
                            break;
                        }
                        if perform_auto_relogin(&bot, &config, &running) {
                            index.store(0, Ordering::Release);
                        } else {
                            sleep_interruptible(1000, &running);
                        }
                        continue;
                    }

                    let snap = bot.player_snapshot();
                    if config.restart_upon_death
                        && snap.max_hp > 0
                        && (!snap.alive || snap.hp <= 0)
                    {
                        bot.logs.push_debug("[Bot] Player died — waiting to respawn & restarting");
                        while running.load(Ordering::Acquire) {
                            sleep_interruptible(500, &running);
                            let s = bot.player_snapshot();
                            if s.alive && s.hp > 0 {
                                break;
                            }
                        }
                        index.store(0, Ordering::Release);
                        continue;
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

                    // Relogin or restart on AFK (mirrors Grimlite-Li chkAFK / chkRestartAFK)
                    if (config.auto_relogin && config.auto_relogin_any) || config.restart_on_afk {
                        let is_afk = flash
                            .call_str("IsAfk", &[])
                            .map(|s| s.eq_ignore_ascii_case("true"))
                            .unwrap_or(false);
                        if is_afk {
                            if config.auto_relogin && config.auto_relogin_any {
                                bot.logs.push_debug("[Bot] Player AFK — logging out for auto-relogin");
                                let _ = flash.call_str("Logout", &[]);
                                sleep_interruptible(1500, &running);
                                continue;
                            } else if config.restart_on_afk {
                                bot.logs.push_debug("[Bot] Player AFK — restarting bot");
                                index.store(0, Ordering::Release);
                                continue;
                            }
                        }
                    }

                    handle_drops(&bot, &config);

                    let cmd = config.commands[idx].clone();
                    let next_idx = {
                        let mut ctx = CommandContext {
                            bot: &bot,
                            flash: &flash,
                            config: &mut config,
                            idx,
                            vars: &mut vars,
                            skill_cursor: &mut skill_cursor,
                            active_skill_set: &mut active_skill_set,
                            running: &running,
                            username: bot.player_snapshot().username,
                        };
                        execute_command(&mut ctx, &cmd)
                    };
                    index.store(next_idx, Ordering::Release);

                    let skip_delay = matches!(
                        cmd,
                        crate::gbot::BotCommand::Statement { .. }
                            | crate::gbot::BotCommand::Index { .. }
                            | crate::gbot::BotCommand::Label { .. }
                            | crate::gbot::BotCommand::GotoLabel { .. }
                            | crate::gbot::BotCommand::Blank { .. }
                    );
                    if !skip_delay {
                        let delay = config.bot_delay.max(50);
                        sleep_interruptible(delay, &running);
                    }
                }
                bot.logs.push_debug("[Bot] Stopped");
            });
    }
}

/// Auto-relogin sequence matching Grimlite-Li `Grimoire/Tools/AutoRelogin.cs`.
fn perform_auto_relogin(bot: &Arc<Bot>, config: &BotConfig, running: &Arc<AtomicBool>) -> bool {
    let Some(flash) = bot.flash() else {
        return false;
    };
    let server = if config.server.trim().is_empty() {
        "Artix"
    } else {
        config.server.trim()
    };
    bot.logs
        .push_debug(format!("[AutoRelogin] Reconnecting to `{server}`..."));

    // Temporarily disable lag killer / disable animations during relogin
    let prev_opts = bot.options.get();
    if prev_opts.lag_killer || prev_opts.disable_animations || prev_opts.hide_players {
        let mut tmp = prev_opts.clone();
        tmp.lag_killer = false;
        tmp.disable_animations = false;
        tmp.hide_players = false;
        bot.apply_options(tmp);
    }

    // 1. Wait if temporarily kicked (up to 65s)
    for _ in 0..130 {
        if !running.load(Ordering::Acquire) {
            return false;
        }
        let kicked = flash
            .call_str("IsTemporarilyKicked", &[])
            .map(|s| s.eq_ignore_ascii_case("true"))
            .unwrap_or(false);
        if !kicked {
            break;
        }
        sleep_interruptible(500, running);
    }
    if !running.load(Ordering::Acquire) {
        return false;
    }

    // 2. Trigger Flash Login
    let _ = flash.call_str("Login", &[]);

    // 3. Wait for server list to load (up to 30s)
    let mut loaded = false;
    for _ in 0..60 {
        if !running.load(Ordering::Acquire) {
            return false;
        }
        if flash
            .call_str("AreServersLoaded", &[])
            .map(|s| s.eq_ignore_ascii_case("true"))
            .unwrap_or(false)
        {
            loaded = true;
            break;
        }
        sleep_interruptible(500, running);
    }
    if !loaded || !running.load(Ordering::Acquire) {
        bot.logs
            .push_debug("[AutoRelogin] Timed out waiting for server list");
        return false;
    }

    // 4. Connect to configured server
    let _ = flash.call_str("Connect", &[crate::host::FlashArg::from(server)]);

    // 5. Wait until logged in and map finished loading (up to 40s)
    let mut connected = false;
    for _ in 0..80 {
        if !running.load(Ordering::Acquire) {
            return false;
        }
        let logged_in = flash.call_bool("isLoggedIn", &[]).unwrap_or(false);
        let map_loading = flash.call_bool("isMapLoading", &[]).unwrap_or(true);
        if logged_in && !map_loading {
            connected = true;
            break;
        }
        sleep_interruptible(500, running);
    }
    if !connected {
        bot.logs
            .push_debug(format!("[AutoRelogin] Failed to connect to `{server}`"));
        return false;
    }

    // 6. Wait configured relog delay before restarting bot
    let delay = config.relog_delay.clamp(1000, 20_000);
    sleep_interruptible(delay, running);

    // 7. Restore visual options
    bot.apply_options(prev_opts);

    // 8. Optional retry if still stuck in battleon (mirrors EnsureLoginSuccess)
    if config.relog_retry_upon_failure
        && bot
            .player_snapshot()
            .map
            .eq_ignore_ascii_case("battleon")
    {
        bot.logs
            .push_debug("[AutoRelogin] Still in battleon — retrying relogin");
        let _ = flash.call_str("Logout", &[]);
        sleep_interruptible(1500, running);
        return false;
    }

    bot.logs
        .push_debug(format!("[AutoRelogin] Connected to `{server}`"));
    true
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

        assert_eq!(
            vars.resolve("Hello {PLAYERNAME}", "Questwalker"),
            "Hello Questwalker"
        );
        assert_eq!(vars.resolve("[map]-1", "Questwalker"), "yulgar-1");
        assert_eq!(vars.resolve("[a] + [b] * 2", "Questwalker"), "14");
    }
}
