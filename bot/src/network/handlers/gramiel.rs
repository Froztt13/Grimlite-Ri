//! Gramiel P1–P4 Defense Shattering orb & boss taunt packet handler.

use super::special::{SpecialPacketHandler, SpecialZone, extract_combat_anims, run_focus_taunt};
use crate::Bot;
use crate::capture::MessageInfo;
use crate::host::FlashArg;
use crate::log::LogBuffer;
use serde_json::Value;
use std::sync::Arc;
use std::sync::atomic::{AtomicI32, Ordering};

#[derive(Default)]
pub struct GramielHandler {
    shatter_count: AtomicI32,
}

impl SpecialPacketHandler for GramielHandler {
    fn reset(&self) {
        self.shatter_count.store(0, Ordering::Release);
    }

    fn intercept(
        &self,
        bot: &Arc<Bot>,
        _logs: &LogBuffer,
        zone: SpecialZone,
        message: &mut MessageInfo,
        outbound: bool,
    ) {
        if outbound {
            return;
        }
        let Some(anims) = extract_combat_anims(&message.content) else {
            return;
        };

        for anim in &anims {
            let Some(msg) = anim.get("msg").and_then(Value::as_str) else {
                continue;
            };
            if !msg.to_ascii_lowercase().contains("defense shattering") {
                continue;
            }
            let mon_id: i32 = anim
                .get("tInf")
                .and_then(Value::as_str)
                .and_then(|s| s.split(':').nth(1))
                .and_then(|n| n.parse().ok())
                .unwrap_or(0);
            let is_left = matches!(zone, SpecialZone::GramielP1 | SpecialZone::GramielP2);
            if mon_id != 0 && ((is_left && mon_id != 2) || (!is_left && mon_id != 3)) {
                continue;
            }
            let count = self.shatter_count.fetch_add(1, Ordering::AcqRel) + 1;
            let is_odd_preset = matches!(zone, SpecialZone::GramielP1 | SpecialZone::GramielP3);
            let act = if is_odd_preset {
                count % 2 == 1
            } else {
                count % 2 == 0
            };
            if !act {
                continue;
            }

            let orb = if is_left { "id:2" } else { "id:3" }.to_string();
            let b = bot.clone();
            let _ = std::thread::Builder::new()
                .name("grimlite-gramiel-taunt".into())
                .spawn(move || {
                    let Some(flash) = b.flash() else { return };
                    if flash
                        .call_bool("isMonsterAvailable", &[FlashArg::from(orb.as_str())])
                        .unwrap_or(false)
                    {
                        run_focus_taunt(&b, &orb, 0);
                    } else if flash
                        .call_bool("isMonsterAvailable", &[FlashArg::from("Gramiel")])
                        .unwrap_or(false)
                        && !flash
                            .call_bool("isMonsterAvailable", &[FlashArg::from("Grace Crystal")])
                            .unwrap_or(false)
                    {
                        run_focus_taunt(&b, "Gramiel", 0);
                    }
                });
        }
    }
}
