//! Misc commands: Delay, Label, GotoLabel, Index, Packet, Int, SetVar, Log, Provoke, SetFPS, Stop, Restart
//! (aligned with Grimlite-Li `Grimoire/Botting/Commands/Misc/`).

use super::CommandContext;
use crate::gbot::{BotCommand, IndexOp, IntOp};
use crate::host::FlashArg;
use std::sync::atomic::Ordering;

pub fn execute(ctx: &mut CommandContext<'_>, cmd: &BotCommand) -> usize {
    match cmd {
        BotCommand::Delay { ms } => {
            ctx.sleep(*ms);
        }
        BotCommand::Label { .. } | BotCommand::Blank { .. } => {}
        BotCommand::GotoLabel { name } => {
            let target = ctx.resolve(name);
            if let Some(pos) = ctx.config.commands.iter().position(|c| {
                matches!(c, BotCommand::Label { name: n } if n.eq_ignore_ascii_case(&target))
            }) {
                return pos;
            }
        }
        BotCommand::Index { index, op } => {
            let n = (*index).max(0) as usize;
            return match op {
                IndexOp::Up => ctx.idx.saturating_sub(n),
                IndexOp::Down => (ctx.idx + n).min(ctx.config.commands.len()),
                IndexOp::Goto => n.min(ctx.config.commands.len()),
            };
        }
        BotCommand::Int { name, value, op } => {
            let entry = ctx.vars.ints.entry(name.clone()).or_insert(0);
            match op {
                IntOp::Set => *entry = *value,
                IntOp::Add => *entry += *value,
                IntOp::Sub => *entry -= *value,
            }
        }
        BotCommand::SetVar { name, value } => {
            let resolved = ctx.resolve(value);
            ctx.vars.strings.insert(name.clone(), resolved);
        }
        BotCommand::Log { text, debug } => {
            if text.contains("{CLEAR}") {
                if *debug {
                    ctx.bot.logs.clear_debug();
                } else {
                    ctx.bot.logs.clear_script();
                }
            } else {
                let msg = format_log_text(ctx, text);
                if *debug {
                    ctx.bot.logs.push_debug(msg);
                } else {
                    ctx.bot.logs.push_script(&msg);
                    ctx.bot.logs.push_debug(format!("[Script] {msg}"));
                }
            }
        }
        BotCommand::Stop => {
            ctx.running.store(false, Ordering::Release);
            return ctx.idx;
        }
        BotCommand::Restart => {
            return 0;
        }
        BotCommand::Provoke { enabled } => {
            let mut opts = ctx.bot.options.get();
            opts.provoke_monsters = *enabled;
            ctx.bot.apply_options(opts);
        }
        BotCommand::SetFPS { fps } => {
            let mut opts = ctx.bot.options.get();
            opts.fps = *fps;
            ctx.bot.apply_options(opts);
        }
        BotCommand::Logout => {
            let _ = ctx.flash.call_str("Logout", &[]);
            ctx.sleep(500);
        }
        BotCommand::Beep { times } => {
            let count = (*times).clamp(1, 50);
            for _ in 0..count {
                if !ctx.is_running() {
                    break;
                }
                play_alert_sound();
                ctx.sleep(250);
            }
        }
        BotCommand::GotoPlayer { player_name } => {
            let target = ctx.resolve(player_name);
            if !target.trim().is_empty() {
                let _ = ctx
                    .flash
                    .call_str("GoTo", &[FlashArg::from(target.trim())]);
                ctx.sleep(500);
                ctx.wait_until(40_000, 250, || {
                    !ctx.flash.call_bool("isMapLoading", &[]).unwrap_or(false)
                });
            }
        }
        BotCommand::ClearTemp => {
            ctx.vars.ints.clear();
            ctx.vars.strings.clear();
        }
        BotCommand::ClientMessage {
            message,
            is_warning,
        } => {
            let msg = ctx.resolve(message);
            let msg_type = if *is_warning { "warning" } else { "server" };
            let _ = ctx.flash.call_game_function(
                "chatF.pushMsg",
                &[
                    FlashArg::from(msg_type),
                    FlashArg::from(msg.as_str()),
                    FlashArg::from("SERVER"),
                    FlashArg::from(""),
                    FlashArg::from(""),
                    FlashArg::from("0"),
                ],
            );
        }
        BotCommand::LoadMap { swf } => {
            let file = ctx.resolve(swf);
            let trimmed = file.trim();
            if !trimmed.is_empty() {
                let _ = ctx
                    .flash
                    .call_game_function("world.loadMap", &[FlashArg::from(trimmed)]);
                ctx.sleep(1000);
            }
        }
        BotCommand::SpecialHandler { zone, mode } => {
            let resolved = ctx.resolve(zone);
            if !resolved.trim().is_empty() {
                ctx.bot.special_handler.set_from_script(&resolved, *mode);
            }
        }
        BotCommand::Packet {
            packet,
            to_client,
            spam_times,
            delay_ms,
        } => {
            let count = (*spam_times).max(1);
            for i in 0..count {
                if !ctx.is_running() {
                    break;
                }
                let resolved = resolve_packet_placeholders(ctx, packet);
                let res = if *to_client {
                    ctx.bot.capture.send_to_client(&resolved)
                } else {
                    ctx.bot.capture.send_to_server(&resolved)
                };
                if let Err(e) = res {
                    ctx.bot.logs.push_debug(format!("[Packet Cmd] {e}"));
                    break;
                }
                if i + 1 < count && *delay_ms > 0 {
                    ctx.sleep(*delay_ms);
                }
            }
        }
        BotCommand::Unknown { type_name, .. } => {
            ctx.bot
                .logs
                .push_debug(format!("[Bot] Skipping unsupported command: {type_name}"));
        }
        _ => {}
    }
    ctx.idx + 1
}

/// Resolves Grimlite-Li `CmdLog` tokens (`{USERNAME}`, `{MAP}`, `{GOLD}`, `{LEVEL}`, `{CELL}`,
/// `{PAD}`, `{HEALTH}`, `{MANA}`, `{TIME}`, `{TIME: 12}`, `{TIME: 24}`, `{ITEM: ...}`, `{ITEM MAX: ...}`, `{INT VALUE: ...}`).
fn format_log_text(ctx: &CommandContext<'_>, raw: &str) -> String {
    let snap = ctx.bot.player_snapshot();
    let room_id = ctx.bot.state.room_id().to_string();
    let now = chrono::Local::now();

    let mut out = ctx.resolve(raw);
    out = out
        .replace("{USERNAME}", &snap.username)
        .replace("{MAP}", &snap.map)
        .replace("{ROOM_ID}", &room_id)
        .replace("{ROOM_NUM}", &room_id)
        .replace("{GOLD}", &snap.gold.to_string())
        .replace("{LEVEL}", &snap.level.to_string())
        .replace("{CELL}", &snap.cell)
        .replace("{PAD}", &snap.pad)
        .replace("{HEALTH}", &snap.hp.to_string())
        .replace("{MANA}", &snap.mp.to_string())
        .replace("{TIME: 12}", &now.format("%I:%M:%S %p").to_string())
        .replace("{TIME: 24}", &now.format("%H:%M:%S").to_string())
        .replace("{TIME}", &now.format("%H:%M:%S").to_string());

    // Replace parameterized tokens: {ITEM: name}, {ITEM MAX: name}, {INT VALUE: key}
    out = replace_param_token(&out, "{ITEM:", |name| {
        ctx.bot
            .state
            .inventory_list()
            .into_iter()
            .find(|it| it.name.eq_ignore_ascii_case(name))
            .map(|it| it.qty)
            .unwrap_or(0)
            .to_string()
    });
    out = replace_param_token(&out, "{ITEM MAX:", |name| {
        ctx.flash
            .call_json::<Vec<serde_json::Value>>("getGameObject", &[FlashArg::from("world.myAvatar.items")])
            .ok()
            .and_then(|items| {
                items.into_iter().find_map(|it| {
                    let sname = it.get("sName").and_then(|v| v.as_str())?;
                    if sname.eq_ignore_ascii_case(name) {
                        it.get("iStk").and_then(|v| v.as_i64())
                    } else {
                        None
                    }
                })
            })
            .unwrap_or(0)
            .to_string()
    });
    out = replace_param_token(&out, "{INT VALUE:", |key| {
        ctx.vars.ints.get(key).copied().unwrap_or(0).to_string()
    });

    out
}

fn replace_param_token(input: &str, prefix: &str, lookup: impl Fn(&str) -> String) -> String {
    let mut result = String::with_capacity(input.len());
    let mut rest = input;
    let prefix_upper = prefix.to_ascii_uppercase();
    while let Some(pos) = rest.to_ascii_uppercase().find(&prefix_upper) {
        result.push_str(&rest[..pos]);
        let after = &rest[pos + prefix.len()..];
        if let Some(end) = after.find('}') {
            let arg = after[..end].trim();
            result.push_str(&lookup(arg));
            rest = &after[end + 1..];
        } else {
            result.push_str(&rest[pos..]);
            return result;
        }
    }
    result.push_str(rest);
    result
}

/// Plays a short alert sound (macOS system sound with terminal bell fallback).
fn play_alert_sound() {
    #[cfg(target_os = "macos")]
    {
        if std::process::Command::new("afplay")
            .arg("/System/Library/Sounds/Ping.aiff")
            .spawn()
            .is_ok()
        {
            return;
        }
    }
    print!("\x07");
}

/// Resolves Grimlite-Li packet placeholders: `{ROOM_ID}`, `{ROOM_NUMBER}`, `PLAYERNAME`, `{GETMAP}`.
fn resolve_packet_placeholders(ctx: &CommandContext<'_>, raw: &str) -> String {
    let snap = ctx.bot.player_snapshot();
    let room_id = ctx.bot.state.room_id().to_string();
    let mut text = ctx.resolve(raw);
    text = text
        .replace("{ROOM_ID}", &room_id)
        .replace("{ROOM_NUMBER}", &room_id)
        .replace("PLAYERNAME", &snap.username)
        .replace("{GETMAP}", &snap.map);
    while text.contains("--") {
        text = text.replace("--", "-");
    }
    text
}
