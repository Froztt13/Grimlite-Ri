//! Anti-Mod / Staff Detection packet handler (mirrors Grimlite-Li `BotManager.AntiMODHandler`).
//! Listens to incoming AQW server JSON packets via Ruffle's `CaptureProxy` (`initUserDatas`,
//! `initUserData`, `moveToArea`, `uotls`) and triggers an emergency callback when a player with
//! `intAccessLevel >= min_access_level` (default `30`) is detected.

use crate::capture::{Interceptor, MessageInfo};
use crate::log::LogBuffer;
use parking_lot::RwLock;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};

/// Default minimum AQW `intAccessLevel` treated as Game Staff / Moderator (matches Grimlite-Li).
pub const DEFAULT_MIN_STAFF_ACCESS_LEVEL: i32 = 30;

/// Active alert payload presented in the UI when a Game Staff member is encountered while botting.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StaffAlert {
    pub username: String,
    pub access_level: i32,
    pub color: &'static str,
    pub map: String,
    pub timestamp: String,
}

type StaffCallback = Arc<dyn Fn(&str, i32, &'static str) + Send + Sync>;

/// Maps AQW staff `intAccessLevel` to its nametag color (matches Grimlite-Li `showLog`).
pub fn staff_color(access_level: i32) -> &'static str {
    match access_level {
        30 | 100 => "Dark Green",
        40 => "Light Green",
        50 => "Purple",
        60 => "Yellow",
        _ => "Unknown",
    }
}

pub struct AntiModHandler {
    enabled: AtomicBool,
    min_access_level: AtomicI32,
    known_levels: RwLock<HashMap<String, i32>>,
    active_alert: RwLock<Option<StaffAlert>>,
    logs: Arc<LogBuffer>,
    on_staff_detected: RwLock<Option<StaffCallback>>,
}

impl AntiModHandler {
    pub fn new(logs: Arc<LogBuffer>) -> Self {
        Self {
            enabled: AtomicBool::new(true),
            min_access_level: AtomicI32::new(DEFAULT_MIN_STAFF_ACCESS_LEVEL),
            known_levels: RwLock::new(HashMap::new()),
            active_alert: RwLock::new(None),
            logs,
            on_staff_detected: RwLock::new(None),
        }
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled.load(Ordering::Acquire)
    }

    pub fn set_enabled(&self, enabled: bool) {
        self.enabled.store(enabled, Ordering::Release);
    }

    pub fn min_access_level(&self) -> i32 {
        self.min_access_level.load(Ordering::Acquire)
    }

    pub fn set_min_access_level(&self, level: i32) {
        self.min_access_level.store(level.max(1), Ordering::Release);
    }

    /// Registers a non-blocking callback invoked when a staff player is spotted.
    pub fn set_on_staff_detected(&self, cb: StaffCallback) {
        *self.on_staff_detected.write() = Some(cb);
    }

    /// Returns the currently pending Game Staff alert for the UI window, if any.
    pub fn active_alert(&self) -> Option<StaffAlert> {
        self.active_alert.read().clone()
    }

    /// Sets or updates the active UI alert window payload.
    pub fn set_alert(&self, alert: StaffAlert) {
        *self.active_alert.write() = Some(alert);
    }

    /// Dismisses the active UI alert window.
    pub fn dismiss_alert(&self) {
        *self.active_alert.write() = None;
    }

    /// Formats and logs the staff detection banner (matches Grimlite-Li `showLog`) and invokes
    /// the registered detection callback.
    pub fn check_player(&self, username: &str, access_level: i32) {
        let name = username.trim();
        if name.is_empty() {
            return;
        }
        self.known_levels
            .write()
            .insert(name.to_ascii_lowercase(), access_level);

        if !self.is_enabled() || access_level < self.min_access_level() {
            return;
        }

        let color = staff_color(access_level);
        let banner = format!("==> [STAFF | {access_level}/{color} | {name}] <==");
        let strip = "-".repeat(banner.len());
        self.logs
            .push_debug(format!("{strip}\n{banner}\n{strip}"));

        if let Some(cb) = self.on_staff_detected.read().as_ref() {
            cb(name, access_level, color);
        }
    }

    fn handle_json(&self, root: &Value) {
        let Some(data) = root.pointer("/b/o") else {
            return;
        };
        let cmd = data.get("cmd").and_then(Value::as_str).unwrap_or("");
        match cmd {
            "initUserDatas" => {
                if let Some(arr) = data.get("a").and_then(Value::as_array) {
                    for entry in arr {
                        if let Some(pdata) = entry.get("data")
                            && let Some(name) = pdata.get("strUsername").and_then(Value::as_str)
                            && let Some(level) = json_i32(pdata.get("intAccessLevel"))
                        {
                            self.check_player(name, level);
                        }
                    }
                }
            }
            "initUserData" => {
                if let Some(pdata) = data.get("data")
                    && let Some(name) = pdata.get("strUsername").and_then(Value::as_str)
                    && let Some(level) = json_i32(pdata.get("intAccessLevel"))
                {
                    self.check_player(name, level);
                }
            }
            "moveToArea" => {
                if let Some(uo_list) = data.get("uoBranch").and_then(Value::as_array) {
                    for uo in uo_list {
                        if let Some(name) = uo.get("uoName").and_then(Value::as_str)
                            && let Some(level) = json_i32(uo.get("intAccessLevel"))
                        {
                            self.check_player(name, level);
                        }
                    }
                }
            }
            "uotls" => {
                if let Some(unm) = data.get("unm").and_then(Value::as_str) {
                    let level = data
                        .get("o")
                        .and_then(|o| json_i32(o.get("intAccessLevel")))
                        .or_else(|| {
                            self.known_levels
                                .read()
                                .get(&unm.trim().to_ascii_lowercase())
                                .copied()
                        });
                    if let Some(level) = level {
                        self.check_player(unm, level);
                    }
                }
            }
            _ => {}
        }
    }
}

fn json_i32(v: Option<&Value>) -> Option<i32> {
    let v = v?;
    v.as_i64()
        .map(|n| n as i32)
        .or_else(|| v.as_str().and_then(|s| s.trim().parse::<i32>().ok()))
}

impl Interceptor for AntiModHandler {
    fn priority(&self) -> i32 {
        -90
    }

    fn intercept(&self, message: &mut MessageInfo, outbound: bool) {
        if outbound || !message.content.starts_with('{') {
            return;
        }
        // Fast path: skip JSON parsing unless packet is one of the player presence commands.
        if !message.content.contains("initUserData")
            && !message.content.contains("uotls")
            && !message.content.contains("moveToArea")
        {
            return;
        }
        if let Ok(root) = serde_json::from_str::<Value>(&message.content) {
            self.handle_json(&root);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use parking_lot::Mutex;

    #[test]
    fn detects_staff_in_init_user_datas_and_ignores_normal_players() {
        let logs = Arc::new(LogBuffer::default());
        let handler = AntiModHandler::new(logs);
        let hits: Arc<Mutex<Vec<(String, i32, &'static str)>>> = Arc::new(Mutex::new(Vec::new()));
        let hits_clone = hits.clone();
        handler.set_on_staff_detected(Arc::new(move |name, level, color| {
            hits_clone.lock().push((name.to_string(), level, color));
        }));

        let mut pkt = MessageInfo::new(
            r#"{"t":"xt","b":{"r":-1,"o":{"cmd":"initUserDatas","a":[{"uid":1,"data":{"strUsername":"NormalPlayer","intAccessLevel":10}},{"uid":2,"data":{"strUsername":"Alina","intAccessLevel":60}}]}}}"#,
        );
        handler.intercept(&mut pkt, false);

        let recorded = hits.lock().clone();
        assert_eq!(recorded, vec![("Alina".to_string(), 60, "Yellow")]);
    }

    #[test]
    fn detects_staff_in_uotls_using_cached_access_level() {
        let logs = Arc::new(LogBuffer::default());
        let handler = AntiModHandler::new(logs);
        let hits: Arc<Mutex<Vec<(String, i32, &'static str)>>> = Arc::new(Mutex::new(Vec::new()));
        let hits_clone = hits.clone();
        handler.set_on_staff_detected(Arc::new(move |name, level, color| {
            hits_clone.lock().push((name.to_string(), level, color));
        }));

        let mut init = MessageInfo::new(
            r#"{"t":"xt","b":{"r":-1,"o":{"cmd":"initUserData","uid":7,"data":{"strUsername":"Yoshino","intAccessLevel":"50"}}}}"#,
        );
        handler.intercept(&mut init, false);

        let mut uotls = MessageInfo::new(
            r#"{"t":"xt","b":{"r":-1,"o":{"cmd":"uotls","unm":"Yoshino","o":{"strFrame":"Enter"}}}}"#,
        );
        handler.intercept(&mut uotls, false);

        let recorded = hits.lock().clone();
        assert_eq!(recorded.len(), 2);
        assert_eq!(recorded[0], ("Yoshino".to_string(), 50, "Purple"));
        assert_eq!(recorded[1], ("Yoshino".to_string(), 50, "Purple"));
    }

    #[test]
    fn respects_disabled_flag_and_custom_threshold() {
        let logs = Arc::new(LogBuffer::default());
        let handler = AntiModHandler::new(logs);
        let hits: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
        let hits_clone = hits.clone();
        handler.set_on_staff_detected(Arc::new(move |name, _, _| {
            hits_clone.lock().push(name.to_string());
        }));

        handler.set_enabled(false);
        handler.check_player("Artix", 60);
        assert!(hits.lock().is_empty());

        handler.set_enabled(true);
        handler.set_min_access_level(50);
        handler.check_player("TesterMod", 40);
        assert!(hits.lock().is_empty());
        handler.check_player("Artix", 60);
        assert_eq!(*hits.lock(), vec!["Artix".to_string()]);
    }
}
