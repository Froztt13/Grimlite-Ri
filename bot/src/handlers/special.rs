//! Ultra Boss & Auto-Zone Special Handlers driven by Ruffle's `CaptureProxy`.
//! Ports all 15 Grimlite-Li special handlers (`BotManager.UpdateSpecialHandler` / `CmdSpecialHandler`).

use crate::Bot;
use crate::capture::{Interceptor, MessageInfo};
use crate::gbot::SpecialMode;
use crate::host::FlashArg;
use crate::log::LogBuffer;
use parking_lot::RwLock;
use serde_json::Value;
use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};
use std::sync::{Arc, Weak};
use std::time::Duration;

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

    // Per-fight mechanic counters
    starfire_count: AtomicI32,
    shatter_count: AtomicI32,
    speaker_counter: AtomicI32,
    taunt_busy: AtomicBool,
}

impl SpecialHandlerManager {
    pub fn new(logs: Arc<LogBuffer>) -> Self {
        Self {
            enabled: AtomicBool::new(false),
            zone: RwLock::new(SpecialZone::UltraDage),
            bot: RwLock::new(None),
            logs,
            starfire_count: AtomicI32::new(0),
            shatter_count: AtomicI32::new(0),
            speaker_counter: AtomicI32::new(-1),
            taunt_busy: AtomicBool::new(false),
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
        self.starfire_count.store(0, Ordering::Release);
        self.shatter_count.store(0, Ordering::Release);
        self.speaker_counter.store(-1, Ordering::Release);
        self.taunt_busy.store(false, Ordering::Release);
    }

    fn upgrade_bot(&self) -> Option<Arc<Bot>> {
        self.bot.read().as_ref().and_then(Weak::upgrade)
    }

    fn handle_json(&self, root: &Value) {
        let Some(data) = root.pointer("/b/o") else {
            return;
        };
        let cmd = data.get("cmd").and_then(Value::as_str).unwrap_or("");
        let zone = self.zone();

        match cmd {
            "event" => self.handle_zone_event(zone, data),
            "ct" => self.handle_combat_event(zone, data),
            _ => {}
        }
    }

    fn handle_zone_event(&self, zone: SpecialZone, data: &Value) {
        let Some(zone_set) = data
            .get("args")
            .and_then(|a| a.get("zoneSet"))
            .and_then(Value::as_str)
        else {
            return;
        };

        if zone == SpecialZone::QueenIona {
            let Some(bot) = self.upgrade_bot() else {
                return;
            };
            let z = zone_set.to_string();
            let _ = std::thread::Builder::new()
                .name("grimlite-iona-zone".into())
                .spawn(move || {
                    std::thread::sleep(Duration::from_millis(500));
                    let Some(flash) = bot.flash() else { return };
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
                    let (x, y) = match z.as_str() {
                        "A" if pos => (679, 339),
                        "A" if neg => (272, 379),
                        "B" if pos => (272, 379),
                        "B" if neg => (679, 339),
                        _ => (490, 320),
                    };
                    let _ = flash.call_bool("walkTo", &[FlashArg::from(x), FlashArg::from(y)]);
                });
            return;
        }

        if let Some((x, y)) = static_zone_coords(zone, zone_set)
            && let Some(bot) = self.upgrade_bot()
        {
            bot.spawn_flash_action("walkTo", vec![FlashArg::from(x), FlashArg::from(y)]);
        }
    }

    fn handle_combat_event(&self, zone: SpecialZone, data: &Value) {
        let Some(anims) = data.get("anims").and_then(Value::as_array) else {
            return;
        };

        match zone {
            SpecialZone::AstralEmpyreanP1 | SpecialZone::AstralEmpyreanP2 => {
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
                self.logs.push_debug(format!(
                    "[Astral Empyrean] Starfire #{count} -> {}",
                    if should_act { "TAUNT" } else { "skip" }
                ));
                if should_act && let Some(bot) = self.upgrade_bot() {
                    spawn_focus_taunt(bot, "Astral Empyrean".to_string(), 0);
                }
            }
            SpecialZone::GramielP1
            | SpecialZone::GramielP2
            | SpecialZone::GramielP3
            | SpecialZone::GramielP4 => {
                for anim in anims {
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
                    let is_left =
                        matches!(zone, SpecialZone::GramielP1 | SpecialZone::GramielP2);
                    if mon_id != 0 && ((is_left && mon_id != 2) || (!is_left && mon_id != 3)) {
                        continue;
                    }
                    let count = self.shatter_count.fetch_add(1, Ordering::AcqRel) + 1;
                    let is_odd_preset =
                        matches!(zone, SpecialZone::GramielP1 | SpecialZone::GramielP3);
                    let act = if is_odd_preset {
                        count % 2 == 1
                    } else {
                        count % 2 == 0
                    };
                    if !act {
                        continue;
                    }
                    let orb = if is_left { "id:2" } else { "id:3" };
                    if let Some(bot) = self.upgrade_bot() {
                        let orb_target = orb.to_string();
                        let _ = std::thread::Builder::new()
                            .name("grimlite-gramiel-taunt".into())
                            .spawn(move || {
                                let Some(flash) = bot.flash() else { return };
                                if flash
                                    .call_bool("isMonsterAvailable", &[FlashArg::from(orb_target.as_str())])
                                    .unwrap_or(false)
                                {
                                    run_focus_taunt(&bot, &orb_target, 0);
                                } else if flash
                                    .call_bool("isMonsterAvailable", &[FlashArg::from("Gramiel")])
                                    .unwrap_or(false)
                                    && !flash
                                        .call_bool("isMonsterAvailable", &[FlashArg::from("Grace Crystal")])
                                        .unwrap_or(false)
                                {
                                    run_focus_taunt(&bot, "Gramiel", 0);
                                }
                            });
                    }
                }
            }
            SpecialZone::UltraSpeakerDps
            | SpecialZone::UltraSpeakerLr
            | SpecialZone::UltraSpeakerLoo
            | SpecialZone::UltraSpeakerAp => {
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
                let next = self.speaker_counter.fetch_add(1, Ordering::AcqRel) + 1;
                let step = speaker_action(next);
                if next == 15 {
                    self.speaker_counter.store(1, Ordering::Release);
                }
                let role = match zone {
                    SpecialZone::UltraSpeakerDps => "DPS",
                    SpecialZone::UltraSpeakerLr => "LR",
                    SpecialZone::UltraSpeakerLoo => "LOO",
                    SpecialZone::UltraSpeakerAp => "AP",
                    _ => "",
                };
                let Some(bot) = self.upgrade_bot() else {
                    return;
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
                    let busy_flag = Arc::new(AtomicBool::new(true));
                    let _ = busy_flag;
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
            _ => {}
        }
    }
}

impl Interceptor for SpecialHandlerManager {
    fn intercept(&self, message: &mut MessageInfo, outbound: bool) {
        if outbound || !self.is_enabled() || !message.content.starts_with('{') {
            return;
        }
        if !message.content.contains("\"event\"") && !message.content.contains("anims") {
            return;
        }
        if let Ok(root) = serde_json::from_str::<Value>(&message.content) {
            self.handle_json(&root);
        }
    }
}

/// Static zone coordinates matching Grimlite-Li `HandlerAutoZone*.cs`.
pub fn static_zone_coords(zone: SpecialZone, zone_set: &str) -> Option<(i32, i32)> {
    let coords = match zone {
        SpecialZone::UltraDage => match zone_set {
            "A" => (107, 400),
            "B" => (843, 400),
            _ => (503, 276),
        },
        SpecialZone::DarkCarnax => match zone_set {
            "A" => (860, 400),
            "B" => (49, 400),
            _ => (426, 372),
        },
        SpecialZone::AstralEmpyrean
        | SpecialZone::AstralEmpyreanP1
        | SpecialZone::AstralEmpyreanP2 => match zone_set {
            "A" => (708, 447),
            "B" => (287, 191),
            _ => (461, 329),
        },
        SpecialZone::ColossalVordred => match zone_set {
            "A" => (731, 461),
            "B" => (700, 321),
            _ => (748, 372),
        },
        _ => return None,
    };
    Some(coords)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct SpeakerStep {
    taunt_role: Option<&'static str>,
    zone_role: Option<&'static str>,
    zone_type: Option<&'static str>,
    delay_ms: u64,
}

/// Exact 16-step Ultra Speaker rotation table from `HandlerUltraSpeakerMechanics.WhatAction`.
fn speaker_action(count: i32) -> SpeakerStep {
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

fn spawn_focus_taunt(bot: Arc<Bot>, target: String, delay_ms: u64) {
    let _ = std::thread::Builder::new()
        .name("grimlite-focus-taunt".into())
        .spawn(move || {
            run_focus_taunt(&bot, &target, delay_ms);
        });
}

fn run_focus_taunt(bot: &Bot, target: &str, delay_ms: u64) {
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
            .call_str("GetAurasValue", &[FlashArg::from("False"), FlashArg::from("Focus")])
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
        assert_eq!(static_zone_coords(SpecialZone::UltraDage, "A"), Some((107, 400)));
        assert_eq!(static_zone_coords(SpecialZone::DarkCarnax, "B"), Some((49, 400)));
        assert_eq!(static_zone_coords(SpecialZone::ColossalVordred, ""), Some((748, 372)));
        assert_eq!(
            SpecialZone::from_name("Auto Zone - Ultradage"),
            Some(SpecialZone::UltraDage)
        );
        let s1 = speaker_action(1);
        assert_eq!(s1.taunt_role, Some("LOO"));
        assert_eq!(s1.zone_role, Some("DPS"));
        assert_eq!(s1.zone_type, Some("IN"));
    }
}
