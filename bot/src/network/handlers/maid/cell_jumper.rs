//! Mirrors `Grimlite-Li/Grimoire/Networking/Handlers/Maid/CellJumperHandler.cs`.
//! Follows the Maid target player's `uotls` cell/pad jumps and enforces `WhitelistMap`.

use crate::Bot;
use crate::engine::maid::{MaidConfig, parse_whitelist_entries};
use crate::host::FlashArg;
use std::sync::Arc;

pub fn handle(bot: &Arc<Bot>, cfg: &MaidConfig, parts: &[&str]) {
    // %xt%uotls%-1%username%mvts:-1,px:0,strPad:Spawn,py:0,mvtd:0,tx:0,strFrame:Enter,ty:0%
    if parts.len() < 6 {
        return;
    }

    let target = cfg.target.trim();
    if target.is_empty() {
        return;
    }

    let curr_user = parts[4].trim();
    if !curr_user.eq_ignore_ascii_case(target) {
        return;
    }

    let movement = parts[5];
    let mut cell: Option<&str> = None;
    let mut pad: Option<&str> = None;
    for pair in movement.split(',') {
        if let Some((k, v)) = pair.split_once(':') {
            match k {
                "strFrame" => cell = Some(v),
                "strPad" => pad = Some(v),
                _ => {}
            }
        }
    }

    let Some(cell_val) = cell else {
        return;
    };

    bot.maid.reset_specials();

    // Immediately stop attacking and jump to target's cell/pad unless unfollowed or paused
    if !bot.maid.is_unfollow() && !bot.maid.is_on_pause() {
        let target_cell = cell_val.to_string();
        let target_pad = pad.unwrap_or("Left").to_string();
        let b = Arc::clone(bot);
        let _ = std::thread::Builder::new()
            .name("maid-follow-cell".into())
            .spawn(move || {
                if let Some(flash) = b.flash() {
                    let _ = flash.call_str("CancelAutoAttack", &[]);
                    let _ = flash.call_str("CancelTarget", &[]);
                    let _ = flash.call_str(
                        "moveToCell",
                        &[
                            FlashArg::from(target_cell.as_str()),
                            FlashArg::from(target_pad.as_str()),
                        ],
                    );
                }
            });
    }

    // Check WhitelistMap rules
    if cfg.whitelist_map {
        let map_name = bot.player_snapshot().map.to_ascii_lowercase();
        let wl = parse_whitelist_entries(&cfg.whitelist_map_maps);
        if let Some(entry) = wl.iter().find(|e| e.map == map_name) {
            if entry.cell == "all" || entry.cell.eq_ignore_ascii_case(cell_val) {
                bot.maid.pause_follow();
                bot.maid.set_stop_attack(true, Some(bot));
            } else {
                bot.maid.resume_follow();
                bot.maid.set_stop_attack(false, None);
            }
        } else {
            bot.maid.resume_follow();
        }
    }
}
