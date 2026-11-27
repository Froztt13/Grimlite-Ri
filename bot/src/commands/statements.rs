//! Conditional Statement evaluators (`Grimoire/Botting/Commands/Misc/Statements/`).

use super::CommandContext;
use crate::gbot::StatementKind;
use crate::host::FlashArg;
use serde_json::Value;

pub fn evaluate(ctx: &mut CommandContext<'_>, kind: &StatementKind, v1: &str, v2: &str) -> bool {
    let snap = ctx.bot.player_snapshot();
    let qty = if v2.trim() == "*" {
        1
    } else {
        v2.parse::<i32>().unwrap_or(1)
    };
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
        StatementKind::InHouse => has_house_item(ctx, v1, qty),
        StatementKind::NotInHouse => !has_house_item(ctx, v1, qty),
        StatementKind::Equipped => ctx
            .flash
            .call_bool("isEquipped", &[FlashArg::from(v1)])
            .unwrap_or(false),
        StatementKind::NotEquipped => !ctx
            .flash
            .call_bool("isEquipped", &[FlashArg::from(v1)])
            .unwrap_or(false),
        StatementKind::IsMaxStack => is_max_stack(ctx, v1),
        StatementKind::IsNotMaxStack => !is_max_stack(ctx, v1),
        StatementKind::ItemPickupable => ctx
            .bot
            .state
            .drop_list()
            .iter()
            .any(|d| d.name.eq_ignore_ascii_case(v1)),
        StatementKind::ItemNotPickupable => !ctx
            .bot
            .state
            .drop_list()
            .iter()
            .any(|d| d.name.eq_ignore_ascii_case(v1)),
        StatementKind::ListInInv => ctx.config.items.iter().all(|item| {
            ctx.flash
                .call_bool("hasItem", &[FlashArg::from(item.as_str()), FlashArg::from(1)])
                .unwrap_or(false)
        }),
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
        StatementKind::QuestAvailable => ctx
            .flash
            .call_bool("IsAvailable", &[FlashArg::from(n1)])
            .unwrap_or(false),
        StatementKind::QuestNotAvailable => !ctx
            .flash
            .call_bool("IsAvailable", &[FlashArg::from(n1)])
            .unwrap_or(false),
        StatementKind::MonsterInRoom => ctx
            .flash
            .call_bool("isMonsterAvailable", &[FlashArg::from(v1)])
            .unwrap_or(false),
        StatementKind::MonsterNotInRoom => !ctx
            .flash
            .call_bool("isMonsterAvailable", &[FlashArg::from(v1)])
            .unwrap_or(false),
        StatementKind::MonsterHealthGreaterThan => monster_health(ctx, v1) >= n2,
        StatementKind::MonsterHealthLessThan => monster_health(ctx, v1) <= n2,
        StatementKind::MonstersGreaterThan => visible_monsters_count(ctx) > n1,
        StatementKind::MonstersLessThan => visible_monsters_count(ctx) < n1,
        StatementKind::SpecialAnims => {
            let last_msg = ctx.bot.state.last_animation_message().to_lowercase();
            let matched = !last_msg.is_empty()
                && v1
                    .split(',')
                    .map(|s| s.trim().to_lowercase())
                    .filter(|s| !s.is_empty())
                    .any(|kw| last_msg.contains(&kw));
            if matched {
                let skill = v2.trim();
                if !skill.is_empty() {
                    let _ = ctx
                        .flash
                        .call_bool("forceUseSkill", &[FlashArg::from(skill)]);
                }
            }
            matched
        }
        StatementKind::HasTarget => ctx.flash.call_bool("hasTarget", &[]).unwrap_or(false),
        StatementKind::NotHasTarget => !ctx.flash.call_bool("hasTarget", &[]).unwrap_or(false),
        StatementKind::InCombat => ctx.is_in_combat(),
        StatementKind::NotInCombat => !ctx.is_in_combat(),
        StatementKind::IsMember => is_member(ctx),
        StatementKind::IsNotMember => !is_member(ctx),
        StatementKind::HealthLessThan => snap.hp < n1,
        StatementKind::HealthGreaterThan => snap.hp > n1,
        StatementKind::TargetHealthGreaterThan => {
            if !ctx.flash.call_bool("hasTarget", &[]).unwrap_or(false) {
                v2.eq_ignore_ascii_case("true")
            } else {
                target_health(ctx) >= n1
            }
        }
        StatementKind::TargetHealthLessThan => {
            if !ctx.flash.call_bool("hasTarget", &[]).unwrap_or(false) {
                v2.eq_ignore_ascii_case("true")
            } else {
                target_health(ctx) <= n1
            }
        }
        StatementKind::TargetHealthBetween => {
            if !ctx.flash.call_bool("hasTarget", &[]).unwrap_or(false) {
                v2.eq_ignore_ascii_case("true")
            } else if let Some((lo_s, hi_s)) = v1.split_once(',') {
                let lo = lo_s.trim().parse::<i32>().unwrap_or(0);
                let hi = hi_s.trim().parse::<i32>().unwrap_or(0);
                let hp = target_health(ctx);
                hp >= lo && hp <= hi
            } else {
                false
            }
        }
        StatementKind::ManaLessThan => snap.mp < n1,
        StatementKind::ManaGreaterThan => snap.mp > n1,
        StatementKind::GoldGreaterThan => snap.gold > i64::from(n1),
        StatementKind::GoldLessThan => snap.gold < i64::from(n1),
        StatementKind::LevelGreaterThan => snap.level > n1,
        StatementKind::LevelLessThan => snap.level < n1,
        StatementKind::LevelIs => snap.level == n1,
        StatementKind::FactionRankGreaterThan => faction_rank(ctx, v1) >= n2,
        StatementKind::FactionRankLessThan => faction_rank(ctx, v1) < n2,
        StatementKind::NameEquals => snap.username.eq_ignore_ascii_case(v1),
        StatementKind::PlayerInRoom => players_in_map(ctx)
            .iter()
            .any(|p| p.eq_ignore_ascii_case(v1)),
        StatementKind::PlayerNotInRoom => !players_in_map(ctx)
            .iter()
            .any(|p| p.eq_ignore_ascii_case(v1)),
        StatementKind::PlayerIsInCell => player_in_cell(ctx, v1, v2),
        StatementKind::PlayerIsNotInCell => !player_in_cell(ctx, v1, v2),
        StatementKind::PlayerIsInMyCell => player_in_cell(ctx, v1, &snap.cell),
        StatementKind::PlayerIsNotInMyCell => !player_in_cell(ctx, v1, &snap.cell),
        StatementKind::PlayersGreaterThan => (players_in_map(ctx).len() as i32) > n1,
        StatementKind::PlayersLessThan => (players_in_map(ctx).len() as i32) < n1,
        StatementKind::PlayerInMyCellGreaterThan => {
            players_in_cell_count(ctx, &snap.cell) > n1
        }
        StatementKind::PlayerInMyCellLessThan => {
            players_in_cell_count(ctx, &snap.cell) < n1
        }
        StatementKind::PlayerHPGreaterThan => player_hp_pct(ctx, v1) > n2,
        StatementKind::PlayerHPLessThan => player_hp_pct(ctx, v1) < n2,
        StatementKind::PlayerAuraEquals => aura_value(ctx, true, v1) == n2,
        StatementKind::PlayerAuraGreaterThan => aura_value(ctx, true, v1) > n2,
        StatementKind::PlayerAuraLessThan => aura_value(ctx, true, v1) < n2,
        StatementKind::TargetAuraEquals => aura_value(ctx, false, v1) == n2,
        StatementKind::TargetAuraGreaterThan => aura_value(ctx, false, v1) > n2,
        StatementKind::TargetAuraLessThan => aura_value(ctx, false, v1) < n2,
        StatementKind::TargetAuraDuration => {
            let mon_id = ctx
                .flash
                .call_str("getGameObject", &[FlashArg::from("world.myAvatar.target.objData.MonMapID")])
                .ok()
                .and_then(|s| s.trim().trim_matches('"').parse::<i32>().ok())
                .unwrap_or(0);
            if mon_id <= 0 {
                false
            } else {
                let rem = ctx.bot.state.enemy_aura_remaining_ms(mon_id, v1);
                rem > 0 && rem <= n2
            }
        }
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
        StatementKind::UpdateVar => {
            if ctx.vars.strings.contains_key(v1) {
                ctx.vars.strings.insert(v1.to_string(), v2.to_string());
                true
            } else {
                false
            }
        }
    }
}

fn has_house_item(ctx: &CommandContext<'_>, name: &str, qty: i32) -> bool {
    let Ok(items) = ctx
        .flash
        .call_json::<Vec<Value>>("getGameObject", &[FlashArg::from("world.myAvatar.houseitems")])
    else {
        return false;
    };
    items.iter().any(|it| {
        let sname = it.get("sName").and_then(Value::as_str).unwrap_or("");
        let iqty = it.get("iQty").and_then(Value::as_i64).unwrap_or(1) as i32;
        sname.eq_ignore_ascii_case(name) && iqty >= qty
    })
}

fn is_max_stack(ctx: &CommandContext<'_>, name: &str) -> bool {
    let Ok(items) = ctx
        .flash
        .call_json::<Vec<Value>>("getGameObject", &[FlashArg::from("world.myAvatar.items")])
    else {
        return false;
    };
    items.iter().any(|it| {
        let sname = it.get("sName").and_then(Value::as_str).unwrap_or("");
        let qty = it.get("iQty").and_then(Value::as_i64).unwrap_or(0);
        let stk = it.get("iStk").and_then(Value::as_i64).unwrap_or(0);
        sname.eq_ignore_ascii_case(name) && stk > 0 && qty >= stk
    })
}

fn is_member(ctx: &CommandContext<'_>) -> bool {
    ctx.flash
        .call_json::<i32>("getGameObject", &[FlashArg::from("world.myAvatar.objData.iUpgDays")])
        .map(|days| days > 0)
        .unwrap_or(false)
}

fn faction_rank(ctx: &CommandContext<'_>, name: &str) -> i32 {
    let Ok(factions) = ctx
        .flash
        .call_json::<Vec<Value>>("getGameObject", &[FlashArg::from("world.myAvatar.factions")])
    else {
        return 0;
    };
    factions
        .iter()
        .find(|f| {
            f.get("sName")
                .and_then(Value::as_str)
                .is_some_and(|n| n.eq_ignore_ascii_case(name))
        })
        .and_then(|f| f.get("iRank").and_then(Value::as_i64))
        .unwrap_or(0) as i32
}

fn target_health(ctx: &CommandContext<'_>) -> i32 {
    ctx.flash
        .call_str("GetTargetHealth", &[])
        .ok()
        .and_then(|s| s.trim().trim_matches('"').parse::<i32>().ok())
        .unwrap_or(0)
}

fn monster_health(ctx: &CommandContext<'_>, monster: &str) -> i32 {
    if let Ok(mons) = ctx.flash.call_json::<Vec<Value>>("GetMonstersInCell", &[]) {
        let target = monster.trim().to_lowercase();
        if let Some(hp) = mons.iter().find_map(|m| {
            let name = m.get("sName").and_then(Value::as_str).unwrap_or("");
            let matches = target.is_empty() || target == "*" || name.to_lowercase().contains(&target);
            matches.then(|| m.get("intHP").and_then(Value::as_i64).unwrap_or(0) as i32)
        }) {
            return hp;
        }
    }
    ctx.bot
        .state
        .find_monster_in_cell(monster)
        .map_or(0, |m| m.hp)
}

fn visible_monsters_count(ctx: &CommandContext<'_>) -> i32 {
    if let Ok(mons) = ctx
        .flash
        .call_json::<Vec<Value>>("GetVisibleMonstersInCell", &[])
    {
        return mons.len() as i32;
    }
    if let Ok(mons) = ctx.flash.call_json::<Vec<Value>>("GetMonstersInCell", &[]) {
        return mons.len() as i32;
    }
    0
}

fn players_in_map(ctx: &CommandContext<'_>) -> Vec<String> {
    ctx.flash
        .call_json::<Vec<String>>("PlayersInMap", &[])
        .unwrap_or_default()
}

fn player_in_cell(ctx: &CommandContext<'_>, player: &str, cell: &str) -> bool {
    let Ok(tree) = ctx
        .flash
        .call_json::<Value>("getGameObject", &[FlashArg::from("world.uoTree")])
    else {
        return false;
    };
    let key = player.trim().to_lowercase();
    tree.get(&key)
        .and_then(|u| u.get("strFrame"))
        .and_then(Value::as_str)
        .is_some_and(|frame| frame.eq_ignore_ascii_case(cell.trim()))
}

fn players_in_cell_count(ctx: &CommandContext<'_>, cell: &str) -> i32 {
    let Ok(tree) = ctx
        .flash
        .call_json::<Value>("getGameObject", &[FlashArg::from("world.uoTree")])
    else {
        return 0;
    };
    let Some(obj) = tree.as_object() else {
        return 0;
    };
    obj.values()
        .filter(|u| {
            u.get("strFrame")
                .and_then(Value::as_str)
                .is_some_and(|frame| frame.eq_ignore_ascii_case(cell.trim()))
        })
        .count() as i32
}

fn player_hp_pct(ctx: &CommandContext<'_>, name: &str) -> i32 {
    let Ok(tree) = ctx
        .flash
        .call_json::<Value>("getGameObject", &[FlashArg::from("world.uoTree")])
    else {
        return 0;
    };
    let key = name.trim().to_lowercase();
    let Some(u) = tree.get(&key) else {
        return 0;
    };
    let hp = u.get("intHP").and_then(Value::as_i64).unwrap_or(0);
    let max_hp = u.get("intHPMax").and_then(Value::as_i64).unwrap_or(0);
    if max_hp > 0 {
        ((hp * 100) / max_hp) as i32
    } else {
        0
    }
}

fn aura_value(ctx: &CommandContext<'_>, is_self: bool, aura_name: &str) -> i32 {
    let flag = if is_self { "true" } else { "false" };
    ctx.flash
        .call_str("GetAurasValue", &[FlashArg::from(flag), FlashArg::from(aura_name)])
        .ok()
        .and_then(|s| s.trim().trim_matches('"').parse::<f64>().ok())
        .unwrap_or(0.0) as i32
}
