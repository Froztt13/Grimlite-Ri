//! Mirrors `Grimlite-Li/Grimoire/Networking/Handlers/Maid/JoinMapHandler.cs`.
//! Evaluates `WhitelistMap` when joining a map (`%xt%server%...%You joined ...%`).

use crate::Bot;
use crate::engine::maid::{MaidConfig, parse_whitelist_entries};
use std::sync::Arc;

pub fn handle(bot: &Arc<Bot>, cfg: &MaidConfig, raw: &str) {
    if !cfg.whitelist_map || !raw.contains("You joined ") {
        return;
    }

    let snap = bot.player_snapshot();
    let map = snap.map.to_ascii_lowercase();
    let cell = snap.cell.to_ascii_lowercase();

    let wl = parse_whitelist_entries(&cfg.whitelist_map_maps);
    if let Some(entry) = wl.iter().find(|e| e.map == map) {
        if entry.cell == "all" || entry.cell == cell {
            bot.maid.pause_follow();
        } else {
            bot.maid.resume_follow();
        }
    } else {
        bot.maid.set_stop_attack(false, None);
        bot.maid.resume_follow();
    }
}
