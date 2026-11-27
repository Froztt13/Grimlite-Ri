//! Mirrors `Grimlite-Li/Grimoire/Networking/Handlers/Maid/CopyWalkHandler.cs`.
//! Copies the target player's `tx`/`ty` movement coordinates from `uotls` packets.

use crate::Bot;
use crate::engine::maid::MaidConfig;
use crate::host::FlashArg;
use std::sync::Arc;

pub fn handle(bot: &Arc<Bot>, cfg: &MaidConfig, parts: &[&str]) {
    if parts.len() < 6 || bot.maid.is_unfollow() || bot.maid.is_on_pause() {
        return;
    }

    let target = cfg.target.trim();
    if target.is_empty() || !parts[4].trim().eq_ignore_ascii_case(target) {
        return;
    }

    let mut tx: Option<f32> = None;
    let mut ty: Option<f32> = None;
    for pair in parts[5].split(',') {
        if let Some((k, v)) = pair.split_once(':') {
            match k {
                "tx" => tx = v.parse::<f32>().ok(),
                "ty" => ty = v.parse::<f32>().ok(),
                _ => {}
            }
        }
    }

    if let (Some(x), Some(y)) = (tx, ty)
        && x != 0.0
        && y != 0.0
    {
        bot.spawn_flash_action(
            "WalkToPoint",
            vec![FlashArg::from(x.to_string()), FlashArg::from(y.to_string())],
        );
    }
}
