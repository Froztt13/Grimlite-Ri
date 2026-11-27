//! Combat commands: Kill, KillFor, ShortHunt, QuestHunt, Attack, UseSkill, SkillSet, Rest, CancelTarget
//! (aligned with Grimlite-Li `Grimoire/Botting/Commands/Combat/`).

use super::{CommandContext, map, quest};
use crate::gbot::{BotCommand, SkillEntry};
use crate::host::FlashArg;
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
            let key = ((*ctx.skill_cursor % 4) + 1).to_string();
            *ctx.skill_cursor = ctx.skill_cursor.wrapping_add(1);
            let _ = ctx.flash.call_bool("useSkill", &[FlashArg::from(key)]);
        }
        ctx.sleep(skill_delay);
    }

    if ctx.flash.call_bool("hasTarget", &[]).unwrap_or(false) {
        let _ = ctx.flash.call_bool("cancelTarget", &[]);
    }
}
