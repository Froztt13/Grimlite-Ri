//! Astral Empyrean Auto-Zone & Starfire Taunt packet handler (Base, P1, P2).

use super::special::{
    SpecialPacketHandler, SpecialZone, extract_combat_anims, extract_zone_set, spawn_focus_taunt,
};
use crate::Bot;
use crate::capture::MessageInfo;
use crate::host::FlashArg;
use crate::log::LogBuffer;
use serde_json::Value;
use std::sync::Arc;
use std::sync::atomic::{AtomicI32, Ordering};

#[derive(Default)]
pub struct AstralEmpyreanHandler {
    starfire_count: AtomicI32,
}

impl AstralEmpyreanHandler {
    pub fn zone_coords(zone_set: &str) -> (i32, i32) {
        match zone_set {
            "A" => (708, 447),
            "B" => (287, 191),
            _ => (461, 329),
        }
    }
}

impl SpecialPacketHandler for AstralEmpyreanHandler {
    fn reset(&self) {
        self.starfire_count.store(0, Ordering::Release);
    }

    fn intercept(
        &self,
        bot: &Arc<Bot>,
        logs: &LogBuffer,
        zone: SpecialZone,
        message: &mut MessageInfo,
        outbound: bool,
    ) {
        if outbound {
            return;
        }

        // 1. Zone event -> walk to safe zone
        if let Some(zone_set) = extract_zone_set(&message.content) {
            let (x, y) = Self::zone_coords(&zone_set);
            bot.spawn_flash_action("walkTo", vec![FlashArg::from(x), FlashArg::from(y)]);
            return;
        }

        // 2. Combat event -> Starfire taunt rotation (P1 = odd, P2 = even)
        if !matches!(
            zone,
            SpecialZone::AstralEmpyreanP1 | SpecialZone::AstralEmpyreanP2
        ) {
            return;
        }
        let Some(anims) = extract_combat_anims(&message.content) else {
            return;
        };
        let found = anims.iter().any(|a| {
            a.get("msg")
                .and_then(Value::as_str)
                .is_some_and(|m| m.to_ascii_lowercase().contains("behold our starfire"))
        });
        if !found {
            return;
        }

        let count = self.starfire_count.fetch_add(1, Ordering::AcqRel) + 1;
        let should_act = if zone == SpecialZone::AstralEmpyreanP1 {
            count % 2 == 1
        } else {
            count % 2 == 0
        };
        logs.push_debug(format!(
            "[Astral Empyrean] Starfire #{count} -> {}",
            if should_act { "TAUNT" } else { "skip" }
        ));
        if should_act {
            spawn_focus_taunt(bot.clone(), "Astral Empyrean".to_string(), 0);
        }
    }
}
