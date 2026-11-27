//! Ultra Dage Auto-Zone packet handler (mirrors Grimlite-Li `HandlerAutoZoneUltraDage`).

use super::special::{SpecialPacketHandler, SpecialZone, extract_zone_set};
use crate::Bot;
use crate::capture::MessageInfo;
use crate::host::FlashArg;
use crate::log::LogBuffer;
use std::sync::Arc;

#[derive(Default)]
pub struct UltraDageHandler;

impl UltraDageHandler {
    pub fn zone_coords(zone_set: &str) -> (i32, i32) {
        match zone_set {
            "A" => (107, 400),
            "B" => (843, 400),
            _ => (503, 276),
        }
    }
}

impl SpecialPacketHandler for UltraDageHandler {
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
        if let Some(zone_set) = extract_zone_set(&message.content) {
            let (x, y) = Self::zone_coords(&zone_set);
            bot.spawn_flash_action("walkTo", vec![FlashArg::from(x), FlashArg::from(y)]);
        }
    }
}
