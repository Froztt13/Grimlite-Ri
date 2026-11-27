//! Conditional Statement evaluators (`Grimoire/Botting/Commands/Misc/Statements/`).

use super::CommandContext;
use crate::gbot::StatementKind;
use crate::host::FlashArg;

pub fn evaluate(ctx: &CommandContext<'_>, kind: &StatementKind, v1: &str, v2: &str) -> bool {
    let snap = ctx.bot.player_snapshot();
    let qty = v2.parse::<i32>().unwrap_or(1);
    let n1 = v1.parse::<i32>().unwrap_or(0);
    let n2 = v2.parse::<i32>().unwrap_or(0);

    match kind {
        StatementKind::InInventory => ctx
            .flash
            .call_bool("hasItem", &[FlashArg::from(v1), FlashArg::from(qty)])
            .unwrap_or(false),
        StatementKind::NotInInventory => !ctx
            .flash
            .call_bool("hasItem", &[FlashArg::from(v1), FlashArg::from(qty)])
            .unwrap_or(false),
        StatementKind::InTemp => ctx
            .flash
            .call_bool("hasTempItem", &[FlashArg::from(v1), FlashArg::from(qty)])
            .unwrap_or(false),
        StatementKind::NotInTemp => !ctx
            .flash
            .call_bool("hasTempItem", &[FlashArg::from(v1), FlashArg::from(qty)])
            .unwrap_or(false),
        StatementKind::InBank => ctx
            .flash
            .call_bool("hasBankItem", &[FlashArg::from(v1), FlashArg::from(qty)])
            .unwrap_or(false),
        StatementKind::NotInBank => !ctx
            .flash
            .call_bool("hasBankItem", &[FlashArg::from(v1), FlashArg::from(qty)])
            .unwrap_or(false),
        StatementKind::InBankOrInvent => {
            ctx.flash
                .call_bool("hasItem", &[FlashArg::from(v1), FlashArg::from(qty)])
                .unwrap_or(false)
                || ctx
                    .flash
                    .call_bool("hasBankItem", &[FlashArg::from(v1), FlashArg::from(qty)])
                    .unwrap_or(false)
        }
        StatementKind::NotInBankAndInvent => {
            !ctx.flash
                .call_bool("hasItem", &[FlashArg::from(v1), FlashArg::from(qty)])
                .unwrap_or(false)
                && !ctx
                    .flash
                    .call_bool("hasBankItem", &[FlashArg::from(v1), FlashArg::from(qty)])
                    .unwrap_or(false)
        }
        StatementKind::Equipped => ctx
            .flash
            .call_bool("isEquipped", &[FlashArg::from(v1)])
            .unwrap_or(false),
        StatementKind::NotEquipped => !ctx
            .flash
            .call_bool("isEquipped", &[FlashArg::from(v1)])
            .unwrap_or(false),
        StatementKind::MapIs => snap.map.eq_ignore_ascii_case(v1),
        StatementKind::MapIsNot => !snap.map.eq_ignore_ascii_case(v1),
        StatementKind::CellIs => snap.cell.eq_ignore_ascii_case(v1),
        StatementKind::CellIsNot => !snap.cell.eq_ignore_ascii_case(v1),
        StatementKind::QuestInProgress => ctx
            .flash
            .call_bool("isQuestInProgress", &[FlashArg::from(n1)])
            .unwrap_or(false),
        StatementKind::QuestNotInProgress => !ctx
            .flash
            .call_bool("isQuestInProgress", &[FlashArg::from(n1)])
            .unwrap_or(false),
        StatementKind::QuestCompleted => ctx
            .flash
            .call_bool("canCompleteQuest", &[FlashArg::from(n1)])
            .unwrap_or(false),
        StatementKind::QuestNotCompleted => !ctx
            .flash
            .call_bool("canCompleteQuest", &[FlashArg::from(n1)])
            .unwrap_or(false),
        StatementKind::MonsterInRoom => ctx
            .flash
            .call_bool("isMonsterAvailable", &[FlashArg::from(v1)])
            .unwrap_or(false),
        StatementKind::MonsterNotInRoom => !ctx
            .flash
            .call_bool("isMonsterAvailable", &[FlashArg::from(v1)])
            .unwrap_or(false),
        StatementKind::HasTarget => ctx.flash.call_bool("hasTarget", &[]).unwrap_or(false),
        StatementKind::NotHasTarget => !ctx.flash.call_bool("hasTarget", &[]).unwrap_or(false),
        StatementKind::HealthLessThan => snap.hp < n1,
        StatementKind::HealthGreaterThan => snap.hp > n1,
        StatementKind::ManaLessThan => snap.mp < n1,
        StatementKind::ManaGreaterThan => snap.mp > n1,
        StatementKind::GoldGreaterThan => snap.gold > i64::from(n1),
        StatementKind::GoldLessThan => snap.gold < i64::from(n1),
        StatementKind::LevelGreaterThan => snap.level > n1,
        StatementKind::LevelLessThan => snap.level < n1,
        StatementKind::IntIs => ctx.vars.ints.get(v1).copied().unwrap_or(0) == n2,
        StatementKind::IntIsNot => ctx.vars.ints.get(v1).copied().unwrap_or(0) != n2,
        StatementKind::IntGreaterThan => ctx.vars.ints.get(v1).copied().unwrap_or(0) > n2,
        StatementKind::IntLessThan => ctx.vars.ints.get(v1).copied().unwrap_or(0) < n2,
        StatementKind::IntEqualInt => {
            ctx.vars.ints.get(v1).copied().unwrap_or(0)
                == ctx.vars.ints.get(v2).copied().unwrap_or(0)
        }
        StatementKind::IntNotEqualInt => {
            ctx.vars.ints.get(v1).copied().unwrap_or(0)
                != ctx.vars.ints.get(v2).copied().unwrap_or(0)
        }
        StatementKind::VarEqualVar => v1.eq_ignore_ascii_case(v2),
        StatementKind::VarNotEqualVar => !v1.eq_ignore_ascii_case(v2),
        StatementKind::NameEquals => snap.username.eq_ignore_ascii_case(v1),
    }
}
