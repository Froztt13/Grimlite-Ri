//! Thin router (`SpecialHandlerManager`) and shared packet/taunt utilities for all Special Handlers.
//! All packet interception, parsing, and boss-specific logic live inside each individual handler file.

use super::{
    AstralEmpyreanHandler, ColossalVordredHandler, DarkCarnaxHandler, GramielHandler,
    QueenIonaHandler, UltraDageHandler, UltraSpeakerHandler,
};
use crate::Bot;
use crate::capture::{Interceptor, MessageInfo};
use crate::gbot::SpecialMode;
use crate::host::FlashArg;
use crate::log::LogBuffer;
use parking_lot::RwLock;
use serde_json::Value;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Weak};
use std::time::Duration;

/// Implemented by every dedicated boss / auto-zone handler.
pub trait SpecialPacketHandler: Send + Sync {
    fn reset(&self) {}
    fn intercept(
        &self,
        bot: &Arc<Bot>,
        logs: &LogBuffer,
        zone: SpecialZone,
        message: &mut MessageInfo,
        outbound: bool,
    );
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpecialZone {
    UltraDage,
    DarkCarnax,
    AstralEmpyrean,
    AstralEmpyreanP1,
    AstralEmpyreanP2,
    QueenIona,
    ColossalVordred,
    GramielP1,
    GramielP2,
    GramielP3,
    GramielP4,
    UltraSpeakerDps,
    UltraSpeakerLr,
    UltraSpeakerLoo,
    UltraSpeakerAp,
}

impl SpecialZone {
    pub const ALL: &'static [SpecialZone] = &[
        Self::UltraDage,
        Self::DarkCarnax,
        Self::AstralEmpyrean,
        Self::AstralEmpyreanP1,
        Self::AstralEmpyreanP2,
        Self::QueenIona,
        Self::ColossalVordred,
        Self::GramielP1,
        Self::GramielP2,
        Self::GramielP3,
        Self::GramielP4,
        Self::UltraSpeakerDps,
        Self::UltraSpeakerLr,
        Self::UltraSpeakerLoo,
        Self::UltraSpeakerAp,
    ];

    /// Exact display name matching Grimlite-Li `cmbSpecials` & `CmdSpecialHandler.Zone`.
    pub fn display_name(self) -> &'static str {
        match self {
            Self::UltraDage => "Auto Zone - Ultradage",
            Self::DarkCarnax => "Auto Zone - Dark Carnax",
            Self::AstralEmpyrean => "Auto Zone - Astral Empyrean",
            Self::AstralEmpyreanP1 => "Auto Zone - Astral Empyrean P1",
            Self::AstralEmpyreanP2 => "Auto Zone - Astral Empyrean P2",
            Self::QueenIona => "Auto Zone - Queen Iona",
            Self::ColossalVordred => "Auto Zone - Colossal Vordred",
            Self::GramielP1 => "Gramiel P1",
            Self::GramielP2 => "Gramiel P2",
            Self::GramielP3 => "Gramiel P3",
            Self::GramielP4 => "Gramiel P4",
            Self::UltraSpeakerDps => "Ultra Speaker DPS",
            Self::UltraSpeakerLr => "Ultra Speaker LR",
            Self::UltraSpeakerLoo => "Ultra Speaker LOO",
            Self::UltraSpeakerAp => "Ultra Speaker AP",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        let trimmed = name.trim();
        Self::ALL
            .iter()
            .copied()
            .find(|z| z.display_name().eq_ignore_ascii_case(trimmed))
    }
}

pub struct SpecialHandlerManager {
    enabled: AtomicBool,
    zone: RwLock<SpecialZone>,
    bot: RwLock<Option<Weak<Bot>>>,
    logs: Arc<LogBuffer>,

    ultra_dage: UltraDageHandler,
    dark_carnax: DarkCarnaxHandler,
    astral_empyrean: AstralEmpyreanHandler,
    queen_iona: QueenIonaHandler,
    colossal_vordred: ColossalVordredHandler,
    gramiel: GramielHandler,
    ultra_speaker: UltraSpeakerHandler,
}

impl SpecialHandlerManager {
    pub fn new(logs: Arc<LogBuffer>) -> Self {
        Self {
            enabled: AtomicBool::new(false),
            zone: RwLock::new(SpecialZone::UltraDage),
            bot: RwLock::new(None),
            logs,
            ultra_dage: UltraDageHandler,
            dark_carnax: DarkCarnaxHandler,
            astral_empyrean: AstralEmpyreanHandler::default(),
            queen_iona: QueenIonaHandler,
            colossal_vordred: ColossalVordredHandler,
            gramiel: GramielHandler::default(),
            ultra_speaker: UltraSpeakerHandler::default(),
        }
    }

    pub fn attach_bot(&self, weak: Weak<Bot>) {
        *self.bot.write() = Some(weak);
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled.load(Ordering::Acquire)
    }

    pub fn zone(&self) -> SpecialZone {
        *self.zone.read()
    }

    pub fn set_zone(&self, z: SpecialZone) {
        let prev = *self.zone.read();
        *self.zone.write() = z;
        if prev != z {
            self.reset_counters();
        }
    }

    pub fn set_enabled(&self, on: bool) {
        let prev = self.enabled.swap(on, Ordering::AcqRel);
        if prev != on {
            self.reset_counters();
            let z = self.zone().display_name();
            self.logs.push_debug(format!(
                "[Special] {} ({z})",
                if on { "Enabled" } else { "Disabled" }
            ));
        }
    }

    /// Mirrors Grimlite-Li `BotManager.SetSpecialHandlerFromScript(zone, enabled)`.
    pub fn set_from_script(&self, zone_name: &str, mode: SpecialMode) {
        if let Some(z) = SpecialZone::from_name(zone_name) {
            self.set_zone(z);
        }
        match mode {
            SpecialMode::Start => self.set_enabled(true),
            SpecialMode::Stop => self.set_enabled(false),
            SpecialMode::Switch => {
                self.logs.push_debug(format!(
                    "[Special] Switched zone -> {}",
                    self.zone().display_name()
                ));
            }
        }
    }

    pub fn reset_counters(&self) {
        self.ultra_dage.reset();
        self.dark_carnax.reset();
        self.astral_empyrean.reset();
        self.queen_iona.reset();
        self.colossal_vordred.reset();
        self.gramiel.reset();
        self.ultra_speaker.reset();
    }

    fn active_handler(&self, zone: SpecialZone) -> &dyn SpecialPacketHandler {
        match zone {
            SpecialZone::UltraDage => &self.ultra_dage,
            SpecialZone::DarkCarnax => &self.dark_carnax,
            SpecialZone::AstralEmpyrean
            | SpecialZone::AstralEmpyreanP1
            | SpecialZone::AstralEmpyreanP2 => &self.astral_empyrean,
            SpecialZone::QueenIona => &self.queen_iona,
            SpecialZone::ColossalVordred => &self.colossal_vordred,
            SpecialZone::GramielP1
            | SpecialZone::GramielP2
            | SpecialZone::GramielP3
            | SpecialZone::GramielP4 => &self.gramiel,
            SpecialZone::UltraSpeakerDps
            | SpecialZone::UltraSpeakerLr
            | SpecialZone::UltraSpeakerLoo
            | SpecialZone::UltraSpeakerAp => &self.ultra_speaker,
        }
    }
}

impl Interceptor for SpecialHandlerManager {
    fn intercept(&self, message: &mut MessageInfo, outbound: bool) {
        if !self.is_enabled() {
            return;
        }
        let Some(bot) = self.bot.read().as_ref().and_then(Weak::upgrade) else {
            return;
        };
        let zone = self.zone();
        self.active_handler(zone)
            .intercept(&bot, &self.logs, zone, message, outbound);
    }
}

/// Static zone coordinates helper across auto-zone handlers.
pub fn static_zone_coords(zone: SpecialZone, zone_set: &str) -> Option<(i32, i32)> {
    let coords = match zone {
        SpecialZone::UltraDage => UltraDageHandler::zone_coords(zone_set),
        SpecialZone::DarkCarnax => DarkCarnaxHandler::zone_coords(zone_set),
        SpecialZone::AstralEmpyrean
        | SpecialZone::AstralEmpyreanP1
        | SpecialZone::AstralEmpyreanP2 => AstralEmpyreanHandler::zone_coords(zone_set),
        SpecialZone::ColossalVordred => ColossalVordredHandler::zone_coords(zone_set),
        _ => return None,
    };
    Some(coords)
}

/// Fast-path JSON extractor for `cmd == "event"` `args.zoneSet`.
pub(super) fn extract_zone_set(raw: &str) -> Option<String> {
    if !raw.starts_with('{') || !raw.contains("\"event\"") {
        return None;
    }
    let root: Value = serde_json::from_str(raw).ok()?;
    let data = root.pointer("/b/o")?;
    if data.get("cmd").and_then(Value::as_str) != Some("event") {
        return None;
    }
    data.get("args")?
        .get("zoneSet")?
        .as_str()
        .map(ToString::to_string)
}

/// Fast-path JSON extractor for `cmd == "ct"` `anims` array.
pub(super) fn extract_combat_anims(raw: &str) -> Option<Vec<Value>> {
    if !raw.starts_with('{') || !raw.contains("anims") {
        return None;
    }
    let root: Value = serde_json::from_str(raw).ok()?;
    let data = root.pointer("/b/o")?;
    if data.get("cmd").and_then(Value::as_str) != Some("ct") {
        return None;
    }
    data.get("anims")?.as_array().cloned()
}

pub(super) fn spawn_focus_taunt(bot: Arc<Bot>, target: String, delay_ms: u64) {
    let _ = std::thread::Builder::new()
        .name("grimlite-focus-taunt".into())
        .spawn(move || {
            run_focus_taunt(&bot, &target, delay_ms);
        });
}

pub(super) fn run_focus_taunt(bot: &Bot, target: &str, delay_ms: u64) {
    if delay_ms > 0 {
        std::thread::sleep(Duration::from_millis(delay_ms));
    }
    let Some(flash) = bot.flash() else { return };
    if target != "*" {
        let _ = flash.call_bool("attackMonster", &[FlashArg::from(target)]);
    }
    for _ in 0..3 {
        if !bot.is_logged_in() {
            break;
        }
        let has_focus = flash
            .call_str(
                "GetAurasValue",
                &[FlashArg::from("False"), FlashArg::from("Focus")],
            )
            .ok()
            .and_then(|s| s.trim().parse::<i32>().ok())
            .unwrap_or(0)
            > 0;
        if has_focus {
            break;
        }
        if let Ok(wait_str) = flash.call_str("skillAvailable", &[FlashArg::from("5")])
            && let Ok(wait_ms) = wait_str.trim().parse::<u64>()
            && wait_ms > 0
        {
            std::thread::sleep(Duration::from_millis(wait_ms.min(12_000)));
        }
        let _ = flash.call_bool("forceUseSkill", &[FlashArg::from("5")]);
        std::thread::sleep(Duration::from_millis(500));
        let _ = flash.call_bool("useSkill", &[FlashArg::from("5")]);
        std::thread::sleep(Duration::from_millis(300));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_static_zone_coordinates_and_speaker_table() {
        assert_eq!(
            static_zone_coords(SpecialZone::UltraDage, "A"),
            Some((107, 400))
        );
        assert_eq!(
            static_zone_coords(SpecialZone::DarkCarnax, "B"),
            Some((49, 400))
        );
        assert_eq!(
            static_zone_coords(SpecialZone::ColossalVordred, ""),
            Some((748, 372))
        );
        assert_eq!(
            SpecialZone::from_name("Auto Zone - Ultradage"),
            Some(SpecialZone::UltraDage)
        );
        let s1 = UltraSpeakerHandler::action_for_step(1);
        assert_eq!(s1.taunt_role, Some("LOO"));
        assert_eq!(s1.zone_role, Some("DPS"));
        assert_eq!(s1.zone_type, Some("IN"));
    }
}
