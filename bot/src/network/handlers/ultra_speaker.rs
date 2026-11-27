//! Ultra Speaker DPS / LR / LOO / AP 16-step mechanics packet handler
//! (mirrors Grimlite-Li `HandlerUltraSpeakerMechanics`).

use super::special::{SpecialPacketHandler, SpecialZone, extract_combat_anims, run_focus_taunt};
use crate::Bot;
use crate::capture::MessageInfo;
use crate::host::FlashArg;
use crate::log::LogBuffer;
use serde_json::Value;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};
use std::time::Duration;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SpeakerStep {
    pub taunt_role: Option<&'static str>,
    pub zone_role: Option<&'static str>,
    pub zone_type: Option<&'static str>,
    pub delay_ms: u64,
}

pub struct UltraSpeakerHandler {
    counter: AtomicI32,
    taunt_busy: AtomicBool,
}

impl Default for UltraSpeakerHandler {
    fn default() -> Self {
        Self {
            counter: AtomicI32::new(-1),
            taunt_busy: AtomicBool::new(false),
        }
    }
}

impl UltraSpeakerHandler {
    /// Exact 16-step Ultra Speaker rotation table from `HandlerUltraSpeakerMechanics.WhatAction`.
    pub fn action_for_step(count: i32) -> SpeakerStep {
        let s = |t, z, zt, d| SpeakerStep {
            taunt_role: t,
            zone_role: z,
            zone_type: zt,
            delay_ms: d,
        };
        match count {
            0 => s(Some("LR"), None, None, 0),
            1 => s(Some("LOO"), Some("DPS"), Some("IN"), 0),
            2 => s(Some("AP"), Some("DPS"), Some("OUT"), 0),
            3 => s(Some("LOO"), None, None, 100),
            4 => s(None, Some("LR"), Some("IN"), 0),
            5 => s(Some("LR"), Some("LR"), Some("OUT"), 100),
            6 => s(None, None, None, 0),
            7 => s(Some("LOO"), None, None, 100),
            8 => s(None, Some("AP"), Some("IN"), 0),
            9 => s(Some("AP"), Some("AP"), Some("OUT"), 0),
            10 => s(Some("LR"), None, None, 100),
            11 => s(None, Some("LOO"), Some("IN"), 0),
            12 => s(Some("LOO"), Some("LOO"), Some("OUT"), 100),
            13 => s(None, None, None, 0),
            14 => s(Some("LR"), None, None, 0),
            15 => s(Some("LOO"), Some("DPS"), Some("IN"), 100),
            _ => s(None, None, None, 0),
        }
    }
}

impl SpecialPacketHandler for UltraSpeakerHandler {
    fn reset(&self) {
        self.counter.store(-1, Ordering::Release);
        self.taunt_busy.store(false, Ordering::Release);
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
        let Some(msg) = anims
            .first()
            .and_then(|a| a.get("msg"))
            .and_then(Value::as_str)
        else {
            return;
        };
        let lower = msg.to_ascii_lowercase();
        if !lower.contains("truth") && !lower.contains("listen") {
            return;
        }

        let next = self.counter.fetch_add(1, Ordering::AcqRel) + 1;
        let step = Self::action_for_step(next);
        if next == 15 {
            self.counter.store(1, Ordering::Release);
        }
        let role = match zone {
            SpecialZone::UltraSpeakerDps => "DPS",
            SpecialZone::UltraSpeakerLr => "LR",
            SpecialZone::UltraSpeakerLoo => "LOO",
            SpecialZone::UltraSpeakerAp => "AP",
            _ => return,
        };

        if step.zone_role == Some(role) {
            if step.zone_type == Some("IN") {
                bot.spawn_flash_action("walkTo", vec![FlashArg::from(203), FlashArg::from(301)]);
            } else if step.zone_type == Some("OUT") {
                bot.spawn_flash_action("walkTo", vec![FlashArg::from(100), FlashArg::from(321)]);
            }
        }
        if step.zone_type == Some("OUT") && role == "AP" {
            let b = bot.clone();
            let _ = std::thread::Builder::new()
                .name("grimlite-speaker-ap".into())
                .spawn(move || {
                    std::thread::sleep(Duration::from_millis(300));
                    if let Some(flash) = b.flash() {
                        for _ in 0..3 {
                            let _ = flash.call_bool("useSkill", &[FlashArg::from("2")]);
                            std::thread::sleep(Duration::from_millis(100));
                        }
                    }
                });
        }
        if role == "LR" {
            bot.spawn_flash_action("useSkill", vec![FlashArg::from("1")]);
        }
        if step.taunt_role == Some(role) && !self.taunt_busy.swap(true, Ordering::AcqRel) {
            let delay = step.delay_ms;
            let b = bot.clone();
            let _ = std::thread::Builder::new()
                .name("grimlite-speaker-taunt".into())
                .spawn(move || {
                    run_focus_taunt(&b, "*", delay);
                });
            self.taunt_busy.store(false, Ordering::Release);
        }
    }
}
