//! Mirrors `Grimlite-Li/Grimoire/Networking/Handlers/Maid/CellJumperHandler.cs`.
//! Follows the Maid target player's `uotls` cell/pad jumps and enforces `WhitelistMap`.

use crate::Bot;
use crate::engine::maid::{MaidConfig, parse_whitelist_entries};
use crate::host::FlashArg;
use std::sync::Arc;

/// Parses `strFrame` and `strPad` from an AQW `uotls` movement payload.
///
/// AQW sends `strFrame` WITHOUT `strPad` when a player merely walks within a cell
/// (`sp:8,tx:714,ty:356,strFrame:Enter`), and sends BOTH `strFrame` AND `strPad` on an actual
/// cell jump (`mvts:-1,px:0,strPad:Spawn,py:0,mvtd:0,tx:0,strFrame:Enter,ty:0`).
pub(crate) fn parse_uotls_cell_pad(movement: &str) -> (Option<&str>, Option<&str>) {
    let mut cell: Option<&str> = None;
    let mut pad: Option<&str> = None;
    for pair in movement.split(',') {
        if let Some((k, v)) = pair.split_once(':') {
            match k {
                "strFrame" if !v.is_empty() => cell = Some(v),
                "strPad" if !v.is_empty() => pad = Some(v),
                _ => {}
            }
        }
    }
    (cell, pad)
}

pub fn handle(bot: &Arc<Bot>, cfg: &MaidConfig, parts: &[&str]) {
    // %xt%uotls%-1%username%mvts:-1,px:0,strPad:Spawn,py:0,mvtd:0,tx:0,strFrame:Enter,ty:0%
    if parts.len() < 6 || bot.maid.is_unfollow() {
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

    let (cell, pad) = parse_uotls_cell_pad(parts[5]);

    // Match Grimlite-Li `CellJumperHandler.cs`: only treat as a cell jump when BOTH `strFrame`
    // and `strPad` are present. Walk-only `uotls` packets omit `strPad` and must NOT trigger a
    // `moveToCell` jump or interrupt combat.
    if let (Some(cell_val), Some(pad_val)) = (cell, pad) {
        bot.maid.reset_specials();

        if !bot.maid.is_on_pause() {
            let target_cell = cell_val.to_string();
            let target_pad = pad_val.to_string();
            let b = Arc::clone(bot);
            let _ = std::thread::Builder::new()
                .name("maid-follow-cell".into())
                .spawn(move || {
                    if let Some(flash) = b.flash() {
                        if flash.call_bool("isMapLoading", &[]).unwrap_or(false) {
                            return;
                        }
                        let (same_cell, in_combat) = b.read_player(|snap| {
                            (
                                snap.cell.eq_ignore_ascii_case(&target_cell),
                                snap.state == 2,
                            )
                        });
                        if same_cell {
                            return;
                        }
                        if in_combat {
                            let _ = flash.call_str("CancelAutoAttack", &[]);
                            let _ = flash.call_str("CancelTarget", &[]);
                        }
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
    }

    // Check WhitelistMap rules (only when a cell jump packet arrived or cell is known)
    if cfg.whitelist_map
        && let (Some(cell_val), Some(_)) = (cell, pad)
    {
        let map_name = bot.read_player(|s| s.map.to_ascii_lowercase());
        let wl = parse_whitelist_entries(&cfg.whitelist_map_maps);
        if let Some(entry) = wl.iter().find(|e| e.map == map_name) {
            if entry.cell == "all" || entry.cell.eq_ignore_ascii_case(cell_val) {
                bot.maid.pause_follow();
                if !bot.maid.is_stop_attack() {
                    bot.maid.set_stop_attack(true, Some(bot));
                }
            } else {
                bot.maid.resume_follow();
                if bot.maid.is_stop_attack() {
                    bot.maid.set_stop_attack(false, None);
                }
            }
        } else {
            bot.maid.resume_follow();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn walk_uotls_without_pad_does_not_produce_cell_jump() {
        let (cell, pad) = parse_uotls_cell_pad("sp:8,tx:714,ty:356,strFrame:Enter");
        assert_eq!(cell, Some("Enter"));
        assert_eq!(pad, None);

        let (cell2, pad2) =
            parse_uotls_cell_pad("mvts:1646524997575,sp:9,px:759,py:384,mvtd:503,tx:662,ty:346,strFrame:Enter");
        assert_eq!(cell2, Some("Enter"));
        assert_eq!(pad2, None);
    }

    #[test]
    fn jump_uotls_with_frame_and_pad_produces_cell_jump() {
        let (cell, pad) =
            parse_uotls_cell_pad("mvts:-1,px:0,strPad:Spawn,py:0,mvtd:0,tx:0,strFrame:r2,ty:0");
        assert_eq!(cell, Some("r2"));
        assert_eq!(pad, Some("Spawn"));
    }
}
