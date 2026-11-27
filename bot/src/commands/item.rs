//! Item, Shop, Bank, and Drop commands
//! (aligned with Grimlite-Li `Grimoire/Botting/Commands/Item/`).

use super::CommandContext;
use crate::gbot::BotCommand;
use crate::host::FlashArg;

pub fn execute(ctx: &mut CommandContext<'_>, cmd: &BotCommand) -> usize {
    match cmd {
        BotCommand::GetDrop { item_name } => {
            let name = ctx.resolve(item_name);
            let room_id = ctx.bot.state.room_id();
            for drop in ctx.bot.state.drop_list() {
                if drop.name.eq_ignore_ascii_case(&name) {
                    let _ = ctx
                        .bot
                        .capture
                        .send_to_server(&format!("%xt%zm%getDrop%{room_id}%{}%", drop.id));
                }
            }
            ctx.sleep(500);
        }
        BotCommand::LoadShop { shop_id } => {
            ctx.exit_combat_in_place(500);
            let _ = ctx
                .flash
                .call_bool("loadShop", &[FlashArg::from(*shop_id)]);
            ctx.wait_until(3000, 150, || {
                ctx.flash
                    .call_bool("isShopLoaded", &[FlashArg::from(*shop_id)])
                    .unwrap_or(false)
            });
        }
        BotCommand::Buy {
            shop_id,
            item_name,
            qty,
        } => {
            let item = ctx.resolve(item_name);
            ctx.exit_combat_in_place(500);
            if *shop_id > 0
                && !ctx
                    .flash
                    .call_bool("isShopLoaded", &[FlashArg::from(*shop_id)])
                    .unwrap_or(false)
            {
                let _ = ctx
                    .flash
                    .call_bool("loadShop", &[FlashArg::from(*shop_id)]);
                ctx.wait_until(3000, 150, || {
                    ctx.flash
                        .call_bool("isShopLoaded", &[FlashArg::from(*shop_id)])
                        .unwrap_or(false)
                });
            }
            for _ in 0..(*qty).max(1) {
                if !ctx.is_running() {
                    break;
                }
                let _ = ctx.flash.call_bool(
                    "buyItem",
                    &[FlashArg::from(*shop_id), FlashArg::from(item.as_str())],
                );
                ctx.sleep(600);
            }
        }
        BotCommand::Sell { item_name } => {
            let item = ctx.resolve(item_name);
            if ctx
                .flash
                .call_bool("hasItem", &[FlashArg::from(item.as_str()), FlashArg::from(1)])
                .unwrap_or(false)
            {
                ctx.exit_combat_in_place(500);
                let _ = ctx
                    .flash
                    .call_bool("sellItem", &[FlashArg::from(item.as_str())]);
                ctx.sleep(600);
            }
        }
        BotCommand::Equip { item_name } => {
            let item = ctx.resolve(item_name);
            ctx.exit_combat_in_place(500);
            for _ in 0..3 {
                if !ctx.is_running() {
                    break;
                }
                if ctx
                    .flash
                    .call_bool("isEquipped", &[FlashArg::from(item.as_str())])
                    .unwrap_or(false)
                {
                    break;
                }
                let _ = ctx
                    .flash
                    .call_bool("equipItem", &[FlashArg::from(item.as_str())]);
                ctx.sleep(500);
            }
        }
        BotCommand::BankTransfer { item_name, to_bank } => {
            let item = ctx.resolve(item_name);
            ctx.exit_combat_in_place(500);
            if *to_bank {
                let in_inv = ctx
                    .flash
                    .call_bool("hasItem", &[FlashArg::from(item.as_str()), FlashArg::from(1)])
                    .unwrap_or(false);
                if in_inv {
                    let _ = ctx
                        .flash
                        .call_bool("transferToBank", &[FlashArg::from(item.as_str())]);
                    ctx.wait_until(3000, 200, || {
                        !ctx.flash
                            .call_bool(
                                "hasItem",
                                &[FlashArg::from(item.as_str()), FlashArg::from(1)],
                            )
                            .unwrap_or(false)
                    });
                }
            } else {
                let in_bank = ctx
                    .flash
                    .call_bool(
                        "hasBankItem",
                        &[FlashArg::from(item.as_str()), FlashArg::from(1)],
                    )
                    .unwrap_or(false);
                if in_bank {
                    let _ = ctx
                        .flash
                        .call_bool("transferToInv", &[FlashArg::from(item.as_str())]);
                    ctx.wait_until(3000, 200, || {
                        ctx.flash
                            .call_bool(
                                "hasItem",
                                &[FlashArg::from(item.as_str()), FlashArg::from(1)],
                            )
                            .unwrap_or(false)
                    });
                }
            }
        }
        BotCommand::MapItem { item_id } => {
            let _ = ctx
                .flash
                .call_bool("getMapItem", &[FlashArg::from(*item_id)]);
            // Grimlite-Li CmdMapItem.cs uses 1500ms delay to avoid server rate limit
            ctx.sleep(1500);
        }
        BotCommand::Whitelist { item_name, add } => {
            let name = ctx.resolve(item_name);
            let trimmed = name.trim();
            if trimmed.is_empty() {
                ctx.config.enable_pickup = *add;
            } else if *add {
                if !ctx
                    .config
                    .drops
                    .iter()
                    .any(|d| d.eq_ignore_ascii_case(trimmed))
                {
                    ctx.config.drops.push(trimmed.to_string());
                }
            } else {
                ctx.config
                    .drops
                    .retain(|d| !d.eq_ignore_ascii_case(trimmed));
            }
        }
        _ => {}
    }
    ctx.idx + 1
}
