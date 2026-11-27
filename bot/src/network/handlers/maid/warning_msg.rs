//! Mirrors `Grimlite-Li/Grimoire/Networking/Handlers/Maid/WarningMsgHandler.cs`.
//! Handles locked zones, full rooms, and target-not-found warnings by joining an alternative or safe map.

use crate::Bot;
use crate::engine::maid::MaidConfig;
use crate::host::FlashArg;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub fn handle(bot: &Arc<Bot>, cfg: &MaidConfig, parts: &[&str]) {
    // %xt%warning%-1%Cannot goto to player in a Locked zone.%
    if parts.len() < 5 {
        return;
    }
    let msg = parts[4];
    let snap = bot.player_snapshot();
    if !snap.logged_in {
        return;
    }

    if cfg.locked_zone_handler && msg.contains("Cannot goto to player in a Locked zone.") {
        if let Some(alt) = bot.maid.next_locked_map() {
            let bot = Arc::clone(bot);
            let _ = std::thread::Builder::new()
                .name("maid-alt-map".into())
                .spawn(move || {
                    std::thread::sleep(Duration::from_millis(950));
                    bot.logs.push_debug(format!(
                        "[Maid] Locked zone -> joining `{}` ({}/{})",
                        alt.map, alt.cell, alt.pad
                    ));
                    bot.spawn_flash_action(
                        "joinMap",
                        vec![
                            FlashArg::from(alt.map),
                            FlashArg::from(alt.cell),
                            FlashArg::from(alt.pad),
                        ],
                    );
                });
        }
        return;
    }

    if !snap.map.eq_ignore_ascii_case("whitemap") {
        let target_lower = cfg.target.trim().to_ascii_lowercase();
        let not_found_msg = format!("Player '{target_lower}' could not be found.");
        let should_safe_map = msg.contains("Cannot goto to player in a Locked zone.")
            || msg.contains("Room join failed, destination room is full.")
            || (!target_lower.is_empty() && msg.to_ascii_lowercase().contains(&not_found_msg));

        if should_safe_map {
            let bot = Arc::clone(bot);
            let _ = std::thread::Builder::new()
                .name("maid-safe-map".into())
                .spawn(move || {
                    std::thread::sleep(Duration::from_millis(950));
                    let nanos = SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .map(|d| d.subsec_nanos())
                        .unwrap_or(42424);
                    let room = 10000 + (nanos % 89999);
                    let map_str = format!("whitemap-{room}");
                    bot.logs
                        .push_debug(format!("[Maid] Goto failed -> joining safe `{map_str}`"));
                    bot.spawn_flash_action(
                        "joinMap",
                        vec![
                            FlashArg::from(map_str),
                            FlashArg::from("Enter"),
                            FlashArg::from("Spawn"),
                        ],
                    );
                });
        }
    }
}
