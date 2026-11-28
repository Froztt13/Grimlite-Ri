//! Quest commands: AcceptQuest, CompleteQuest (aligned with Grimlite-Li `CmdAcceptQuest` / `CmdCompleteQuest`).

use super::CommandContext;
use crate::Bot;
use crate::flash::Flash;
use crate::gbot::{BotCommand, QuestEntry};
use crate::host::FlashArg;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

pub fn execute(ctx: &mut CommandContext<'_>, cmd: &BotCommand) -> usize {
    match cmd {
        BotCommand::AcceptQuest { quest_id } => {
            accept_quest(ctx, *quest_id);
        }
        BotCommand::CompleteQuest {
            quest_id,
            item_id,
            qty,
        } => {
            complete_quest(ctx, *quest_id, *item_id, *qty);
        }
        BotCommand::AddQuestList { quest_id, item_id } => {
            if *quest_id > 0 {
                if !ctx.config.quests.iter().any(|q| q.id == *quest_id) {
                    ctx.config.quests.push(QuestEntry {
                        id: *quest_id,
                        item_id: (*item_id > 0).then(|| item_id.to_string()),
                        safe_relogin: false,
                    });
                    ctx.bot.runner.set_quests(ctx.config.quests.clone());
                }
                accept_quest(ctx, *quest_id);
            }
        }
        BotCommand::RemoveQuestList { quest_id } => {
            ctx.config.quests.retain(|q| q.id != *quest_id);
            ctx.bot.runner.set_quests(ctx.config.quests.clone());
        }
        _ => {}
    }
    ctx.idx + 1
}

/// Shared SSOT quest acceptance matching Grimlite-Li `CmdAcceptQuest.cs`:
/// 1. Skip immediately if already in progress.
/// 2. Load quest only if not yet loaded in `questTree`, waiting up to 3s.
/// 3. Skip non-repeatable quests whose progression requirement is already satisfied.
/// 4. Retry `acceptQuest` up to 4 times only while `!isQuestInProgress`.
pub fn accept_quest(ctx: &CommandContext<'_>, quest_id: i32) {
    accept_quest_direct(ctx.bot, ctx.flash, ctx.running, quest_id);
}

pub fn accept_quest_direct(bot: &Bot, flash: &Flash, running: &AtomicBool, quest_id: i32) {
    if quest_id <= 0 || !running.load(Ordering::Acquire) {
        return;
    }

    if flash
        .call_bool("isQuestInProgress", &[FlashArg::from(quest_id)])
        .unwrap_or(false)
    {
        return;
    }

    let loaded = flash
        .call_bool("isQuestLoaded", &[FlashArg::from(quest_id)])
        .unwrap_or(false);
    if !loaded {
        let _ = flash.call_bool("loadQuest", &[FlashArg::from(quest_id)]);
        let ok = wait_until(running, 3000, 150, || {
            flash
                .call_bool("isQuestLoaded", &[FlashArg::from(quest_id)])
                .unwrap_or(false)
        });
        if !ok {
            bot.logs.push_debug(format!(
                "[Quest] Failed to accept {quest_id}: quest not loaded"
            ));
            return;
        }
    }

    if flash
        .call_bool("isQuestCompletedOnce", &[FlashArg::from(quest_id)])
        .unwrap_or(false)
    {
        bot.logs.push_debug(format!(
            "[Quest] Skipping non-repeatable quest {quest_id} (already completed)"
        ));
        return;
    }

    for _ in 0..4 {
        if !running.load(Ordering::Acquire) {
            break;
        }
        if flash
            .call_bool("isQuestInProgress", &[FlashArg::from(quest_id)])
            .unwrap_or(false)
        {
            break;
        }
        let _ = flash.call_bool("acceptQuest", &[FlashArg::from(quest_id)]);
        if wait_until(running, 700, 140, || {
            flash
                .call_bool("isQuestInProgress", &[FlashArg::from(quest_id)])
                .unwrap_or(false)
        }) {
            break;
        }
    }
}

/// Shared SSOT quest turn-in:
/// 1. Verify `canCompleteQuest` before attempting turn-in.
/// 2. Call `completeQuest` directly in-combat (AQW supports accepting and turning in quests without
///    leaving combat) and respect the 1100ms server cooldown minus `bot_delay`.
pub fn complete_quest(ctx: &CommandContext<'_>, quest_id: i32, item_id: i32, qty: i32) {
    complete_quest_direct(
        ctx.bot,
        ctx.flash,
        ctx.running,
        quest_id,
        item_id,
        qty,
        ctx.config.exit_combat_before_quest,
        ctx.config.bot_delay,
    );
}

#[allow(clippy::too_many_arguments)]
pub fn complete_quest_direct(
    bot: &Bot,
    flash: &Flash,
    running: &AtomicBool,
    quest_id: i32,
    item_id: i32,
    qty: i32,
    _exit_combat_before_quest: bool,
    bot_delay: u64,
) {
    if quest_id <= 0 || !running.load(Ordering::Acquire) {
        return;
    }

    if !flash
        .call_bool("canCompleteQuest", &[FlashArg::from(quest_id)])
        .unwrap_or(false)
    {
        return;
    }

    let prev_provoke = bot.options.get().provoke_monsters;
    if prev_provoke {
        let mut opts = bot.options.get();
        opts.provoke_monsters = false;
        bot.apply_options(opts);
    }

    let turn_ins = if qty == 0 { 1 } else { qty };
    let _ = flash.call_bool(
        "completeQuest",
        &[
            FlashArg::from(quest_id),
            FlashArg::from(item_id),
            FlashArg::from(turn_ins),
        ],
    );

    if prev_provoke {
        let mut opts = bot.options.get();
        opts.provoke_monsters = true;
        bot.apply_options(opts);
    }

    let delay = 1100u64.saturating_sub(bot_delay);
    if delay > 0 {
        sleep_interruptible(running, delay);
    }
}

fn sleep_interruptible(running: &AtomicBool, ms: u64) {
    let mut slept = 0u64;
    while slept < ms && running.load(Ordering::Acquire) {
        let step = (ms - slept).min(50);
        std::thread::sleep(Duration::from_millis(step));
        slept += step;
    }
}

fn wait_until(
    running: &AtomicBool,
    timeout_ms: u64,
    poll_ms: u64,
    mut pred: impl FnMut() -> bool,
) -> bool {
    let deadline = Instant::now() + Duration::from_millis(timeout_ms);
    while running.load(Ordering::Acquire) {
        if pred() {
            return true;
        }
        if Instant::now() >= deadline {
            return false;
        }
        sleep_interruptible(running, poll_ms);
    }
    false
}
