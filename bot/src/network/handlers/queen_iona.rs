//! Queen Iona Auto-Zone packet handler (mirrors Grimlite-Li `HandlerAutoZoneQueenIona`).
//! Reads `Positive Charge` / `Negative Charge` auras to walk into the safe zone.

use super::special::{SpecialPacketHandler, SpecialZone, extract_zone_set};
use crate::Bot;
use crate::capture::MessageInfo;
use crate::host::FlashArg;
use crate::log::LogBuffer;
use std::sync::Arc;
use std::time::Duration;

#[derive(Default)]
pub struct QueenIonaHandler;

impl SpecialPacketHandler for QueenIonaHandler {
    fn intercept(
        &self,
        bot: &Arc<Bot>,
        _logs: &LogBuffer,
        _zone: SpecialZone,
        message: &mut MessageInfo,
        outbound: bool,
    ) {
        if outbound {
            return;
        }
        let Some(zone_set) = extract_zone_set(&message.content) else {
            return;
        };
        let b = bot.clone();
        let _ = std::thread::Builder::new()
            .name("grimlite-iona-zone".into())
            .spawn(move || {
                std::thread::sleep(Duration::from_millis(500));
                let Some(flash) = b.flash() else { return };
                let has_aura = |name: &str| -> bool {
                    flash
                        .call_str(
                            "GetAurasValue",
                            &[FlashArg::from("True"), FlashArg::from(name)],
                        )
                        .ok()
                        .and_then(|s| s.trim().parse::<i32>().ok())
                        .unwrap_or(0)
                        > 0
                };
                let pos = has_aura("Positive Charge") || has_aura("Negative Charge?");
                let neg = has_aura("Negative Charge") || has_aura("Positive Charge?");
                let (x, y) = match zone_set.as_str() {
                    "A" if pos => (679, 339),
                    "A" if neg => (272, 379),
                    "B" if pos => (272, 379),
                    "B" if neg => (679, 339),
                    _ => (490, 320),
                };
                let _ = flash.call_bool("walkTo", &[FlashArg::from(x), FlashArg::from(y)]);
            });
    }
}
