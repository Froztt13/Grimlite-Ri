//! Quest commands: AcceptQuest, CompleteQuest (aligned with Grimlite-Li `CmdAcceptQuest` / `CmdCompleteQuest`).

use super::CommandContext;
use crate::gbot::{BotCommand, QuestEntry};
use crate::host::FlashArg;

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
                let _ = ctx
                    .bot
                    .capture
                    .send_to_server(&format!("%xt%zm%acceptQuest%1%{quest_id}%"));
                if !ctx.config.quests.iter().any(|q| q.id == *quest_id) {
                    ctx.config.quests.push(QuestEntry {
                        id: *quest_id,
                        item_id: (*item_id > 0).then(|| item_id.to_string()),
                        safe_relogin: false,
                    });
                }
            }
        }
        BotCommand::RemoveQuestList { quest_id } => {
            ctx.config.quests.retain(|q| q.id != *quest_id);
        }
        _ => {}
    }
    ctx.idx + 1
}

/// Shared SSOT quest acceptance matching Grimlite-Li `CmdAcceptQuest.cs`:
/// 1. Skip immediately if already in progress.
/// 2. Load quest only if not yet loaded in `questTree`, waiting up to 3s.
/// 3. Skip non-repeatable quests whose progression requirement is already satisfied.
/// 4. Retry `acceptQuest` up to 2 times (600ms interval) only while `!isQuestInProgress`.
pub fn accept_quest(ctx: &CommandContext<'_>, quest_id: i32) {
    if quest_id <= 0 {
        return;
    }

    if ctx
        .flash
        .call_bool("isQuestInProgress", &[FlashArg::from(quest_id)])
        .unwrap_or(false)
    {
        return;
    }

    let loaded = ctx
        .flash
        .call_bool("isQuestLoaded", &[FlashArg::from(quest_id)])
        .unwrap_or(false);
    if !loaded {
        let _ = ctx
            .flash
            .call_bool("loadQuest", &[FlashArg::from(quest_id)]);
        let ok = ctx.wait_until(3000, 150, || {
            ctx.flash
                .call_bool("isQuestLoaded", &[FlashArg::from(quest_id)])
                .unwrap_or(false)
        });
        if !ok {
            ctx.bot.logs.push_debug(format!(
                "[Quest] Failed to accept {quest_id}: quest not loaded"
            ));
            return;
        }
    }

    if ctx
        .flash
        .call_bool("isQuestCompletedOnce", &[FlashArg::from(quest_id)])
        .unwrap_or(false)
    {
        ctx.bot.logs.push_debug(format!(
            "[Quest] Skipping non-repeatable quest {quest_id} (already completed)"
        ));
        return;
    }

    for _ in 0..2 {
        if !ctx.is_running() {
            break;
        }
        if ctx
            .flash
            .call_bool("isQuestInProgress", &[FlashArg::from(quest_id)])
            .unwrap_or(false)
        {
            break;
        }
        let _ = ctx
            .flash
            .call_bool("acceptQuest", &[FlashArg::from(quest_id)]);
        ctx.sleep(600);
    }
}

/// Shared SSOT quest turn-in matching Grimlite-Li `CmdCompleteQuest.cs`:
/// 1. Verify `canCompleteQuest` before attempting turn-in.
/// 2. Optionally exit combat if `config.exit_combat_before_quest` is enabled.
/// 3. Call `completeQuest` and respect the 1100ms server cooldown minus `bot_delay`.
pub fn complete_quest(ctx: &CommandContext<'_>, quest_id: i32, item_id: i32, qty: i32) {
    if quest_id <= 0 {
        return;
    }

    if !ctx
        .flash
        .call_bool("canCompleteQuest", &[FlashArg::from(quest_id)])
        .unwrap_or(false)
    {
        return;
    }

    let prev_provoke = ctx.bot.options.get().provoke_monsters;
    if prev_provoke {
        let mut opts = ctx.bot.options.get();
        opts.provoke_monsters = false;
        ctx.bot.apply_options(opts);
    }

    let turn_ins = if qty == 0 { 1 } else { qty };
    let _ = ctx.flash.call_bool(
        "completeQuest",
        &[
            FlashArg::from(quest_id),
            FlashArg::from(item_id),
            FlashArg::from(turn_ins),
        ],
    );

    if prev_provoke {
        let mut opts = ctx.bot.options.get();
        opts.provoke_monsters = true;
        ctx.bot.apply_options(opts);
    }

    let delay = 1100u64.saturating_sub(ctx.config.bot_delay);
    if delay > 0 {
        ctx.sleep(delay);
    }
}
