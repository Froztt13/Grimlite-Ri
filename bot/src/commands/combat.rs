//! Combat commands: Kill, KillFor, ShortHunt, QuestHunt, Attack, UseSkill, SkillSet, Rest, CancelTarget
//! (aligned with Grimlite-Li `Grimoire/Botting/Commands/Combat/`).

use super::{CommandContext, map, quest};
use crate::flash::Flash;
use crate::gbot::{BotCommand, SkillEntry};
use crate::host::FlashArg;
use serde::Deserialize;
use std::collections::HashMap;
use std::time::{Duration, Instant};

pub fn execute(ctx: &mut CommandContext<'_>, cmd: &BotCommand) -> usize {
    match cmd {
        BotCommand::Attack { monster } => {
            let m = ctx.resolve(monster);
            find_target(ctx, &m);
        }
        BotCommand::UseSkill {
            monster,
            index,
            wait,
            force,
        } => {
            let target = ctx.resolve(monster);
            find_target(ctx, &target);
            if !*force && !ctx.flash.call_bool("hasTarget", &[]).unwrap_or(false) {
                return ctx.idx + 1;
            }
            if *wait {
                if let Ok(ms_str) = ctx
                    .flash
                    .call_str("skillAvailable", &[FlashArg::from(index.as_str())])
                    && let Ok(ms) = ms_str.trim().parse::<u64>()
                    && ms > 0
                {
                    ctx.sleep(ms.min(15_000));
                }
                find_target(ctx, &target);
            }
            let fn_name = if *force { "forceUseSkill" } else { "useSkill" };
            let _ = ctx
                .flash
                .call_bool(fn_name, &[FlashArg::from(index.as_str())]);
        }
        BotCommand::SkillSet { name } => {
            let resolved = ctx.resolve(name);
            let normalized = resolved
                .trim()
                .trim_start_matches('[')
                .trim_end_matches(']')
                .to_string();
            *ctx.active_skill_set = normalized;
            *ctx.skill_cursor = 0;
        }
        BotCommand::CancelTarget => {
            let _ = ctx.flash.call_bool("cancelTarget", &[]);
        }
        BotCommand::Rest { full } => {
            // 1. Always leave combat by cancelling target and jumping to Blank before resting
            {
                let snap = ctx.bot.player_snapshot();
                let prev_cell = if snap.cell.is_empty() {
                    "Enter".to_string()
                } else {
                    snap.cell
                };
                let prev_pad = if snap.pad.is_empty() {
                    "Spawn".to_string()
                } else {
                    snap.pad
                };

                ctx.exit_combat_to_blank(450);

                // 2. Jump back to previous cell before resting
                if !prev_cell.eq_ignore_ascii_case("Blank") {
                    let _ = ctx.flash.call_bool(
                        "moveToCell",
                        &[
                            FlashArg::from(prev_cell.as_str()),
                            FlashArg::from(prev_pad.as_str()),
                        ],
                    );
                    ctx.sleep(300);
                }
            }

            // 3. Rest (and wait until full HP/MP if `full` is requested)
            let _ = ctx.flash.call_bool("rest", &[FlashArg::Bool(*full)]);
            if *full {
                ctx.wait_until(15_000, 250, || {
                    let s = ctx.bot.player_snapshot();
                    s.max_hp > 0 && s.hp >= s.max_hp && (s.max_mp <= 0 || s.mp >= s.max_mp)
                });
            } else {
                ctx.sleep(800);
            }
        }
        BotCommand::Kill { monster } => {
            let m = ctx.resolve(monster);
            kill_once(ctx, &m);
        }
        BotCommand::KillFor {
            monster,
            item_name,
            quantity,
            is_temp,
        } => {
            let m = ctx.resolve(monster);
            let items = ctx.resolve(item_name);
            let qtys = ctx.resolve(quantity);
            while ctx.is_running() && !has_all_items(ctx, &items, &qtys, *is_temp) {
                kill_once(ctx, &m);
                ctx.handle_drops();
                ctx.sleep(250);
            }
            if ctx.flash.call_bool("hasTarget", &[]).unwrap_or(false) {
                let _ = ctx.flash.call_bool("cancelTarget", &[]);
            }
            ctx.sleep(500);
        }
        BotCommand::ShortHunt {
            map: target_map,
            cell,
            pad,
            monster,
            item_name,
            quantity,
            is_temp,
            delay_after_kill,
        } => {
            let items = ctx.resolve(item_name);
            let qtys = ctx.resolve(quantity);
            if has_all_items(ctx, &items, &qtys, *is_temp) {
                return ctx.idx + 1;
            }
            let cells: Vec<String> = ctx
                .resolve(cell)
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
            let pads: Vec<String> = ctx
                .resolve(pad)
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
            let first_cell = cells.first().map(String::as_str).unwrap_or("Enter");
            let first_pad = pads.first().map(String::as_str).unwrap_or("Spawn");
            map::ensure_in_map_and_cell(ctx, target_map, first_cell, first_pad);

            let mon = ctx.resolve(monster);
            let mut cell_idx = 0usize;
            while ctx.is_running() && !has_all_items(ctx, &items, &qtys, *is_temp) {
                // Multi-cell rotation (matches Grimlite-Li `CmdShortHunt.cs`)
                if cells.len() > 1
                    && !ctx
                        .flash
                        .call_bool("isMonsterAvailable", &[FlashArg::from(mon.as_str())])
                        .unwrap_or(false)
                {
                    cell_idx = (cell_idx + 1) % cells.len();
                    let next_cell = &cells[cell_idx];
                    let next_pad = pads.get(cell_idx).map(String::as_str).unwrap_or("Left");
                    map::move_to_cell_loop(ctx, next_cell, next_pad);
                }
                kill_once(ctx, &mon);
                ctx.handle_drops();
                if *delay_after_kill > 0 {
                    ctx.sleep(*delay_after_kill);
                }
            }
            if ctx.flash.call_bool("hasTarget", &[]).unwrap_or(false) {
                let _ = ctx.flash.call_bool("cancelTarget", &[]);
            }
        }
        BotCommand::QuestHunt {
            map: target_map,
            cell,
            pad,
            monster,
            item_name,
            quantity,
            quest_id,
        } => {
            if *quest_id > 0 {
                quest::accept_quest(ctx, *quest_id);
            }
            map::ensure_in_map_and_cell(ctx, target_map, cell, pad);
            let items = ctx.resolve(item_name);
            let qtys = ctx.resolve(quantity);
            let mon = ctx.resolve(monster);
            while ctx.is_running() {
                let done = if *quest_id > 0 {
                    ctx.flash
                        .call_bool("canCompleteQuest", &[FlashArg::from(*quest_id)])
                        .unwrap_or(false)
                } else {
                    has_all_items(ctx, &items, &qtys, false)
                        || has_all_items(ctx, &items, &qtys, true)
                };
                if done {
                    break;
                }
                kill_once(ctx, &mon);
                ctx.handle_drops();
            }
            if *quest_id > 0
                && ctx
                    .flash
                    .call_bool("canCompleteQuest", &[FlashArg::from(*quest_id)])
                    .unwrap_or(false)
            {
                ctx.sleep(500);
                quest::complete_quest(ctx, *quest_id, -1, 1);
            }
        }
        _ => {}
    }
    ctx.idx + 1
}

/// Targets a monster matching Grimlite-Li `FindTarget`:
/// if `"*"`, only attacks when player currently has no target.
fn find_target(ctx: &CommandContext<'_>, target: &str) {
    if target.trim() == "*" || target.trim().is_empty() {
        if !ctx.flash.call_bool("hasTarget", &[]).unwrap_or(false) {
            let _ = ctx.flash.call_bool("attackMonster", &[FlashArg::from("*")]);
        }
    } else {
        let _ = ctx
            .flash
            .call_bool("attackMonster", &[FlashArg::from(target)]);
    }
}

/// Checks comma-separated `items` and `quantities` (matches Grimlite-Li `CmdKillFor.cs`).
fn has_all_items(ctx: &CommandContext<'_>, items_csv: &str, qtys_csv: &str, is_temp: bool) -> bool {
    let items: Vec<&str> = items_csv
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .collect();
    if items.is_empty() {
        return true;
    }
    let qtys: Vec<&str> = qtys_csv.split(',').map(str::trim).collect();
    let check_fn = if is_temp { "hasTempItem" } else { "hasItem" };

    items.iter().enumerate().all(|(i, item)| {
        let qty: i32 = qtys
            .get(i)
            .or_else(|| qtys.last())
            .and_then(|q| q.parse::<i32>().ok())
            .unwrap_or(1);
        ctx.flash
            .call_bool(check_fn, &[FlashArg::from(*item), FlashArg::from(qty)])
            .unwrap_or(false)
    })
}

/// Evaluates whether a `Safe` skill (`skill_type == 1`) should fire right now (matches `Skill.cs`).
fn should_execute_skill(ctx: &CommandContext<'_>, s: &SkillEntry) -> bool {
    if s.skill_type != 1 {
        return true;
    }
    let snap = ctx.bot.player_snapshot();
    let use_mp = s.is_safe_mp || s.safe_mana;
    let pct = if use_mp {
        if snap.max_mp > 0 {
            (snap.mp as f64 / snap.max_mp as f64) * 100.0
        } else {
            100.0
        }
    } else if snap.max_hp > 0 {
        (snap.hp as f64 / snap.max_hp as f64) * 100.0
    } else {
        100.0
    };
    let threshold = s.safe_value as f64;
    match s.safe_type {
        1 => pct >= threshold,                     // GreaterThan (>=)
        2 => (pct - threshold).abs() < 0.5,        // Equals (==)
        _ => pct <= threshold,                     // LowerThan (<=)
    }
}

/// Resolves the active skill rotation slice based on `ctx.active_skill_set` (matches `CmdKill.cs`).
fn active_skill_range(skills: &[SkillEntry], active_set: &str) -> (usize, usize) {
    if skills.is_empty() {
        return (0, 0);
    }
    if !active_set.is_empty() {
        let target = active_set.trim().trim_start_matches('[').trim_end_matches(']');
        if let Some(label_pos) = skills.iter().position(|s| {
            s.skill_type == 2
                && s.text
                    .trim()
                    .trim_start_matches('[')
                    .trim_end_matches(']')
                    .eq_ignore_ascii_case(target)
        }) {
            let start = label_pos + 1;
            let end = skills[start..]
                .iter()
                .position(|s| s.skill_type == 2)
                .map(|rel| start + rel)
                .unwrap_or(skills.len());
            if start < end {
                return (start, end);
            }
        }
    }
    (0, skills.len())
}

pub fn kill_once(ctx: &mut CommandContext<'_>, monster: &str) {
    let _ = ctx.flash.call_bool("setSpawnPoint", &[]);

    // Wait up to 3s for monster to respawn (matches Grimlite-Li CmdKill.cs line 40)
    ctx.wait_until(3000, 150, || {
        ctx.flash
            .call_bool("isMonsterAvailable", &[FlashArg::from(monster)])
            .unwrap_or(false)
    });
    if !ctx.is_running() {
        return;
    }

    let _ = ctx
        .flash
        .call_bool("attackMonster", &[FlashArg::from(monster)]);
    let start = Instant::now();
    let skill_delay = ctx.config.skill_delay.max(150);
    let lower_mon = monster.trim().to_ascii_lowercase();

    while ctx.is_running() && start.elapsed() < Duration::from_secs(40) {
        // Respect Anti Counter Attack (mirrors Grimlite-Li CmdKill.AntiCounterHandler)
        if (ctx.config.anti_counter || ctx.bot.options.get().anti_counter)
            && ctx.bot.anti_counter.is_paused()
        {
            ctx.sleep(200);
            continue;
        }

        if !ctx.flash.call_bool("hasTarget", &[]).unwrap_or(false) {
            break;
        }

        // Boss priority targets (matches Grimlite-Li CmdKill.cs lines 104-122)
        let priority_target = match lower_mon.as_str() {
            "escherion" => Some("Staff of Inversion"),
            "commander gallaeon" => Some("hydra crew"),
            "vath" => Some("Stalagbite"),
            "ultra avatar tyndarius" => Some("Ultra Fire Orb"),
            _ => None,
        };
        if let Some(prio) = priority_target
            && ctx
                .flash
                .call_bool("isMonsterAvailable", &[FlashArg::from(prio)])
                .unwrap_or(false)
        {
            let _ = ctx
                .flash
                .call_bool("attackMonster", &[FlashArg::from(prio)]);
        }

        let (range_start, range_end) = active_skill_range(&ctx.config.skills, ctx.active_skill_set);
        let range_len = range_end.saturating_sub(range_start);

        if range_len > 0 {
            let offset = *ctx.skill_cursor % range_len;
            *ctx.skill_cursor = ctx.skill_cursor.wrapping_add(1);
            let skill = ctx.config.skills[range_start + offset].clone();
            if skill.skill_type != 2 && !skill.index.is_empty() && should_execute_skill(ctx, &skill)
            {
                let _ = ctx
                    .flash
                    .call_bool("useSkill", &[FlashArg::from(skill.index.as_str())]);
            }
        } else {
            let mut smart = ctx.bot.smart_combat.lock();
            cast_smart_skill(ctx.flash, &mut smart);
        }
        ctx.sleep(skill_delay);
    }

    if ctx.flash.call_bool("hasTarget", &[]).unwrap_or(false) {
        let _ = ctx.flash.call_bool("cancelTarget", &[]);
    }
}

/// Live combat snapshot returned in a single JSON call from Flash (`getCombatState`).
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CombatState {
    #[serde(default)]
    pub class_name: String,
    #[serde(default)]
    pub hp: i32,
    #[serde(default = "default_max_hp")]
    pub max_hp: i32,
    #[serde(default)]
    pub mp: i32,
    #[serde(default)]
    pub has_target: bool,
    #[serde(default)]
    pub counter_attack: bool,
    #[serde(default)]
    pub ready: Vec<bool>,
    #[serde(default)]
    pub self_auras: HashMap<String, i32>,
    #[serde(default)]
    pub target_auras: HashMap<String, i32>,
}

fn default_max_hp() -> i32 {
    1
}

fn normalize_aura_key(name: &str) -> String {
    name.chars()
        .filter(|c| !c.is_whitespace() && *c != '-' && *c != '_')
        .map(|c| c.to_ascii_lowercase())
        .collect()
}

impl CombatState {
    pub fn hp_pct(&self) -> i32 {
        let max_hp = self.max_hp.max(1) as f64;
        ((self.hp as f64 / max_hp) * 100.0) as i32
    }

    pub fn self_aura(&self, name: &str) -> i32 {
        let key = normalize_aura_key(name);
        self.self_auras.get(&key).copied().unwrap_or(0)
    }

    pub fn target_aura(&self, name: &str) -> i32 {
        if !self.has_target {
            return 0;
        }
        let key = normalize_aura_key(name);
        self.target_auras.get(&key).copied().unwrap_or(0)
    }

    pub fn normalized_class(&self) -> String {
        self.class_name.trim().to_ascii_lowercase()
    }

    /// Evaluates class-specific HP/MP/aura safety guards (ported from `AutoBattle.as::canUseSkill`).
    pub fn can_use_skill(&self, skill_idx: usize) -> bool {
        let my_hp = self.hp_pct();
        let my_mp = self.mp;
        match self.normalized_class().as_str() {
            "void highlord" => !(my_hp < 60 && (skill_idx == 1 || skill_idx == 3)),
            "scarlet sorceress" => !(my_hp < 50 && (skill_idx == 1 || skill_idx == 4)),
            "dragon of time" => !(my_hp < 50 && (skill_idx == 1 || skill_idx == 3)),
            "lich" => !((my_hp < 70 && skill_idx == 4) || (my_mp > 40 && skill_idx == 3)),
            "archpaladin" => !(my_hp > 60 && skill_idx == 2),
            "hollowborn vindicator" => {
                let has_safeguard = self.self_aura("Safeguard") > 0;
                let has_decay = self.self_aura("Decay") > 0;
                if (skill_idx == 1 || skill_idx == 3) && has_safeguard {
                    return true;
                }
                if skill_idx == 2 && !has_safeguard && has_decay {
                    return true;
                }
                if skill_idx == 4 && !has_safeguard && !has_decay {
                    return true;
                }
                false
            }
            "king's echo" | "kings echo" => {
                !(skill_idx == 3 && self.self_aura("Waiting For Corvak") > 0)
            }
            "master ranger" | "ranger" => {
                if skill_idx == 3 && self.self_aura("Marks").max(self.target_aura("Marks")) < 6 {
                    return false;
                }
                if skill_idx == 4 && self.self_aura("Vampiric Shot") > 0 {
                    return false;
                }
                true
            }
            "kathool adept" => !(skill_idx == 3 && self.self_aura("Feeding") < 5),
            _ => true,
        }
    }

    pub fn is_castable(&self, skill_idx: usize) -> bool {
        self.ready.get(skill_idx).copied().unwrap_or(false) && self.can_use_skill(skill_idx)
    }
}

/// Persistent per-bot smart class combo state (ported from `AutoBattle.as`).
#[derive(Clone, Debug, Default)]
pub struct SmartCombatState {
    pub skill_index: usize,
    pub temporal_rift: u32,
}

impl SmartCombatState {
    pub fn reset(&mut self) {
        self.skill_index = 0;
        self.temporal_rift = 0;
    }

    /// Picks the next skill index (`0..=4`) to cast based on equipped class, auras, HP/MP,
    /// cooldowns, and mana availability. Returns `None` when paused/counter-attacking or no skill
    /// is castable right now.
    pub fn choose_skill(&mut self, state: &CombatState) -> Option<usize> {
        if !state.has_target || state.counter_attack {
            return None;
        }

        match state.normalized_class().as_str() {
            "chrono shadowslayer" | "chrono shadowhunter" => self.choose_csh(state),
            "arcana invoker" => self.choose_arcana_invoker(state),
            "king's echo" | "kings echo" => self.choose_kings_echo(state),
            "master ranger" | "ranger" => self.choose_ranger(state),
            "kathool adept" => self.choose_kathool_adept(state),
            _ => self.choose_rotating(state, |_| false),
        }
    }

    fn choose_csh(&mut self, state: &CombatState) -> Option<usize> {
        if self.temporal_rift >= 4 {
            if state.is_castable(4) {
                self.temporal_rift = 0;
                return Some(4);
            }
            return None;
        }
        if state.self_aura("Rounds Empty") > 0 {
            return state.is_castable(1).then_some(1);
        }
        if state.self_aura("Temporal Rift") < 1 {
            return state.is_castable(2).then_some(2);
        }
        if state.is_castable(3) {
            self.temporal_rift += 1;
            return Some(3);
        }
        None
    }

    fn choose_arcana_invoker(&self, state: &CombatState) -> Option<usize> {
        let has_world = state.self_aura("XXI - The World") > 0;
        let has_judgement = state.self_aura("XX - Judgement") > 0;
        let end_of_the_world = state.self_aura("End of The World");
        let has_fool = state.self_aura("0 - The Fool") > 0;

        if !has_world {
            if !has_fool {
                if state.is_castable(1) {
                    return Some(1);
                }
            } else if has_judgement && state.is_castable(1) {
                return Some(1);
            }
        }

        if state.is_castable(2) {
            return Some(2);
        }
        if state.is_castable(4) {
            return Some(4);
        }
        if (has_judgement || state.mp > 50) && state.is_castable(3) {
            return Some(3);
        }
        if end_of_the_world > 10 && state.is_castable(1) {
            return Some(1);
        }
        None
    }

    fn choose_kings_echo(&mut self, state: &CombatState) -> Option<usize> {
        let has_waiting = state.self_aura("Waiting For Corvak") > 0;
        self.choose_rotating(state, |idx| idx == 3 && has_waiting)
    }

    fn choose_ranger(&mut self, state: &CombatState) -> Option<usize> {
        let marks = state.self_aura("Marks").max(state.target_aura("Marks"));
        let has_vampiric = state.self_aura("Vampiric Shot") > 0;
        if marks >= 6 && state.is_castable(3) {
            return Some(3);
        }
        self.choose_rotating(state, |idx| {
            (idx == 3 && marks < 6) || (idx == 4 && has_vampiric)
        })
    }

    fn choose_kathool_adept(&mut self, state: &CombatState) -> Option<usize> {
        let feeding = state.self_aura("Feeding");
        if feeding >= 5 && state.is_castable(3) {
            return Some(3);
        }
        self.choose_rotating(state, |idx| idx == 3 && feeding < 5)
    }

    fn choose_rotating(
        &mut self,
        state: &CombatState,
        skip: impl Fn(usize) -> bool,
    ) -> Option<usize> {
        for i in 0..5 {
            let idx = (self.skill_index + i) % 5;
            if skip(idx) {
                continue;
            }
            if state.is_castable(idx) {
                self.skill_index = (idx + 1) % 5;
                return Some(idx);
            }
        }
        None
    }
}

/// Fetches live combat state from Flash in one JSON call, chooses the optimal class-combo skill
/// in Rust, and casts it. Falls back to `1..=4` rotation if `getCombatState` is unavailable
/// (e.g. headless mode).
pub fn cast_smart_skill(flash: &Flash, smart: &mut SmartCombatState) -> bool {
    match flash.call_json::<CombatState>("getCombatState", &[]) {
        Ok(state) => {
            if let Some(idx) = smart.choose_skill(&state) {
                let _ = flash.call_bool("useSkill", &[FlashArg::from(idx.to_string())]);
                return true;
            }
            false
        }
        Err(_) => {
            let key = ((smart.skill_index % 4) + 1).to_string();
            smart.skill_index = smart.skill_index.wrapping_add(1);
            let _ = flash.call_bool("useSkill", &[FlashArg::from(key)]);
            true
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ready_state(class_name: &str) -> CombatState {
        CombatState {
            class_name: class_name.to_string(),
            hp: 4000,
            max_hp: 4000,
            mp: 100,
            has_target: true,
            counter_attack: false,
            ready: vec![true, true, true, true, true],
            self_auras: HashMap::new(),
            target_auras: HashMap::new(),
        }
    }

    fn with_self_aura(mut state: CombatState, name: &str, val: i32) -> CombatState {
        state.self_auras.insert(normalize_aura_key(name), val);
        state
    }

    fn with_target_aura(mut state: CombatState, name: &str, val: i32) -> CombatState {
        state.target_auras.insert(normalize_aura_key(name), val);
        state
    }

    #[test]
    fn counter_attack_blocks_all_skills() {
        let mut smart = SmartCombatState::default();
        let mut s = ready_state("Void Highlord");
        s.counter_attack = true;
        assert_eq!(smart.choose_skill(&s), None);
    }

    #[test]
    fn csh_combo_cycles_reload_rift_and_nuke() {
        let mut smart = SmartCombatState::default();

        // Reload when Rounds Empty is active
        let s_reload = with_self_aura(ready_state("Chrono ShadowSlayer"), "Rounds Empty", 1);
        assert_eq!(smart.choose_skill(&s_reload), Some(1));

        // Open with skill 2 when Temporal Rift < 1
        let s_no_rift = ready_state("Chrono ShadowHunter");
        assert_eq!(smart.choose_skill(&s_no_rift), Some(2));

        // Stack skill 3 four times, then nuke with skill 4 and reset counter
        let s_rift = with_self_aura(ready_state("Chrono ShadowSlayer"), "Temporal Rift", 1);
        for _ in 0..4 {
            assert_eq!(smart.choose_skill(&s_rift), Some(3));
        }
        assert_eq!(smart.temporal_rift, 4);
        assert_eq!(smart.choose_skill(&s_rift), Some(4));
        assert_eq!(smart.temporal_rift, 0);
    }

    #[test]
    fn arcana_invoker_phases_and_nuke() {
        let mut smart = SmartCombatState::default();

        // Pre-world without The Fool -> draw with skill 1
        let s1 = ready_state("Arcana Invoker");
        assert_eq!(smart.choose_skill(&s1), Some(1));

        // Pre-world with Fool + Judgement -> advance to The World with skill 1
        let s2 = with_self_aura(
            with_self_aura(ready_state("Arcana Invoker"), "0 - The Fool", 1),
            "XX - Judgement",
            1,
        );
        assert_eq!(smart.choose_skill(&s2), Some(1));

        // In The World with 2/4 on CD, MP <= 50, End of The World > 10 -> nuke with skill 1
        let mut s3 = with_self_aura(
            with_self_aura(ready_state("Arcana Invoker"), "XXI - The World", 1),
            "End of The World",
            11,
        );
        s3.ready = vec![true, true, false, true, false];
        s3.mp = 40;
        assert_eq!(smart.choose_skill(&s3), Some(1));
    }

    #[test]
    fn ranger_and_kathool_prioritize_stack_nukes() {
        let mut smart = SmartCombatState::default();

        // Master Ranger skips 3 when Marks < 6 and prioritizes 3 when target has 6 Marks
        let s_low = ready_state("Master Ranger");
        assert!(!s_low.can_use_skill(3));
        let s_six = with_target_aura(ready_state("Master Ranger"), "Marks", 6);
        assert_eq!(smart.choose_skill(&s_six), Some(3));

        // Kathool Adept skips 3 when Feeding < 5 and prioritizes 3 at 5 stacks
        let s_k_low = ready_state("Kathool Adept");
        assert!(!s_k_low.can_use_skill(3));
        let s_k_five = with_self_aura(ready_state("Kathool Adept"), "Feeding", 5);
        assert_eq!(smart.choose_skill(&s_k_five), Some(3));
    }

    #[test]
    fn class_guards_respect_hp_mp_and_auras() {
        // Void Highlord: skip 1 & 3 under 60% HP
        let mut vhl = ready_state("Void Highlord");
        vhl.hp = 2000;
        assert!(!vhl.can_use_skill(1));
        assert!(vhl.can_use_skill(2));
        assert!(!vhl.can_use_skill(3));
        assert!(vhl.can_use_skill(4));

        // Lich: skip 4 under 70% HP, skip 3 when MP > 40
        let mut lich = ready_state("Lich");
        lich.hp = 2500;
        lich.mp = 60;
        assert!(!lich.can_use_skill(3));
        assert!(!lich.can_use_skill(4));
        assert!(lich.can_use_skill(2));

        // ArchPaladin: skip 2 above 60% HP
        let ap = ready_state("ArchPaladin");
        assert!(!ap.can_use_skill(2));

        // Hollowborn Vindicator: Safeguard -> 1/3, Decay -> 2, neither -> 4
        let hbv_none = ready_state("Hollowborn Vindicator");
        assert!(!hbv_none.can_use_skill(1));
        assert!(!hbv_none.can_use_skill(2));
        assert!(hbv_none.can_use_skill(4));
        let hbv_decay = with_self_aura(ready_state("Hollowborn Vindicator"), "Decay", 1);
        assert!(hbv_decay.can_use_skill(2));
        assert!(!hbv_decay.can_use_skill(4));
        let hbv_safe = with_self_aura(ready_state("Hollowborn Vindicator"), "Safeguard", 1);
        assert!(hbv_safe.can_use_skill(1));
        assert!(hbv_safe.can_use_skill(3));
        assert!(!hbv_safe.can_use_skill(2));

        // King's Echo: skip 3 while Waiting For Corvak is active
        let ke = with_self_aura(ready_state("King's Echo"), "Waiting For Corvak", 1);
        assert!(!ke.can_use_skill(3));
    }
}
