//! Mirrors `Grimlite-Li/Grimoire/Networking/Handlers/Maid/PartyInvitationHandler.cs`
//! and `PartyChatHandler.cs`.

use crate::Bot;
use crate::host::FlashArg;
use serde_json::Value;
use std::sync::Arc;
use std::time::Duration;

/// Auto-accepts incoming party invitations (`{"t":"xt","b":{"o":{"cmd":"pi","pid":...}}}`).
pub fn handle_party_invitation(bot: &Arc<Bot>, raw: &str) {
    if !raw.contains("\"pi\"") {
        return;
    }
    let Ok(json) = serde_json::from_str::<Value>(raw) else {
        return;
    };
    let Some(obj) = json.pointer("/b/o") else {
        return;
    };
    if obj.get("cmd").and_then(Value::as_str) != Some("pi") {
        return;
    }
    let pid = match obj.get("pid") {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Number(n)) => n.to_string(),
        _ => return,
    };
    let pkt = format!("%xt%zm%gp%1%pa%{pid}%");
    let _ = bot.capture.send_to_server(&pkt);
    bot.logs
        .push_debug(format!("[Maid] Auto-accepted party invite (pid={pid})"));
}

/// Handles `/p` party chat commands: `.tp`, `.join`, `.acc`, `.turnin`, `.target`, `.start`, `.stop`.
pub fn handle_party_chat(bot: &Arc<Bot>, raw: &str) {
    if !raw.starts_with("%xt%chatm%") {
        return;
    }
    // %xt%chatm%32123%party~message%sender%...%
    let parts: Vec<&str> = raw.split('%').collect();
    if parts.len() < 5 {
        return;
    }
    let Some((chat_type, msg)) = parts[4].split_once('~') else {
        return;
    };
    if chat_type != "party" || !msg.starts_with('.') {
        return;
    }

    let mut split = msg.splitn(2, ' ');
    let cmd = split.next().unwrap_or_default().trim();
    let arg = split.next().unwrap_or_default().trim().to_string();

    match cmd {
        ".tp" if !arg.is_empty() => {
            let bot = Arc::clone(bot);
            let _ = std::thread::Builder::new()
                .name("maid-party-tp".into())
                .spawn(move || {
                    if arg.to_ascii_lowercase().contains("tercessuinotlim") {
                        bot.spawn_flash_action(
                            "moveToCell",
                            vec![FlashArg::from("m22"), FlashArg::from("Left")],
                        );
                        std::thread::sleep(Duration::from_millis(600));
                    }
                    bot.spawn_flash_action(
                        "joinMap",
                        vec![
                            FlashArg::from(arg),
                            FlashArg::from("Enter"),
                            FlashArg::from("Spawn"),
                        ],
                    );
                });
        }
        ".join" if !arg.is_empty() => {
            bot.spawn_flash_action(
                "joinMap",
                vec![
                    FlashArg::from(arg),
                    FlashArg::from("Enter"),
                    FlashArg::from("Spawn"),
                ],
            );
        }
        ".acc" if !arg.is_empty() => {
            let bot = Arc::clone(bot);
            let _ = std::thread::Builder::new()
                .name("maid-party-acc".into())
                .spawn(move || {
                    let ids: Vec<String> = arg
                        .split(',')
                        .map(str::trim)
                        .filter(|s| !s.is_empty())
                        .map(ToString::to_string)
                        .collect();
                    for q in &ids {
                        if let Ok(id) = q.parse::<i32>() {
                            bot.spawn_flash_action("loadQuest", vec![FlashArg::from(id)]);
                        }
                    }
                    std::thread::sleep(Duration::from_millis(1000));
                    for q in &ids {
                        if let Ok(id) = q.parse::<i32>() {
                            bot.spawn_flash_action("acceptQuest", vec![FlashArg::from(id)]);
                            std::thread::sleep(Duration::from_millis(1000));
                        }
                    }
                });
        }
        ".turnin" if !arg.is_empty() => {
            let bot = Arc::clone(bot);
            let _ = std::thread::Builder::new()
                .name("maid-party-turnin".into())
                .spawn(move || {
                    let ids: Vec<String> = arg
                        .split(',')
                        .map(str::trim)
                        .filter(|s| !s.is_empty())
                        .map(ToString::to_string)
                        .collect();
                    for q in &ids {
                        if let Ok(id) = q.parse::<i32>() {
                            bot.spawn_flash_action("completeQuest", vec![FlashArg::from(id)]);
                            std::thread::sleep(Duration::from_millis(1000));
                        }
                    }
                });
        }
        ".target" if !arg.is_empty() => {
            bot.maid.update_config(|c| c.target = arg.clone());
            bot.logs
                .push_debug(format!("[Maid] Party cmd .target -> `{arg}`"));
        }
        ".stop" => {
            bot.maid.stop(bot);
        }
        ".start" => {
            bot.maid.start(Arc::clone(bot));
        }
        _ => {}
    }
}
