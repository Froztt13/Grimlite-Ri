//! Live game state updated from both batched Flash snapshots and real-time socket packets.

use crate::capture::{Interceptor, MessageInfo};
use crate::log::LogBuffer;
use parking_lot::RwLock;
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::Arc;

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlayerSnapshot {
    pub logged_in: bool,
    pub username: String,
    pub hp: i32,
    pub max_hp: i32,
    pub mp: i32,
    pub max_mp: i32,
    pub level: i32,
    pub gold: i64,
    pub map: String,
    pub cell: String,
    pub pad: String,
    pub state: i32,
    pub alive: bool,
    pub target_name: String,
    pub target_hp: i32,
    pub target_max_hp: i32,
}

#[derive(Clone, Debug)]
pub struct DropItem {
    pub id: i32,
    pub name: String,
    pub qty: i32,
    pub is_ac: bool,
}

#[derive(Default)]
pub struct GameState {
    pub player: RwLock<PlayerSnapshot>,
    pub room_id: RwLock<i32>,
    pub drops: RwLock<HashMap<i32, DropItem>>,
}

impl GameState {
    pub fn snapshot(&self) -> PlayerSnapshot {
        self.player.read().clone()
    }

    pub fn update_player(&self, snap: PlayerSnapshot) {
        *self.player.write() = snap;
    }

    pub fn room_id(&self) -> i32 {
        *self.room_id.read()
    }

    pub fn drop_list(&self) -> Vec<DropItem> {
        self.drops.read().values().cloned().collect()
    }
}

/// Passive packet listener attached to `CaptureProxy` that updates `GameState` from AQW packets.
pub struct StatePacketInterceptor {
    state: Arc<GameState>,
    logs: Arc<LogBuffer>,
}

impl StatePacketInterceptor {
    pub fn new(state: Arc<GameState>, logs: Arc<LogBuffer>) -> Self {
        Self { state, logs }
    }
}

impl Interceptor for StatePacketInterceptor {
    fn priority(&self) -> i32 {
        -100
    }

    fn intercept(&self, message: &mut MessageInfo, outbound: bool) {
        if outbound || !message.send {
            return;
        }
        let text = message.content.trim();
        if text.starts_with('{') {
            self.handle_json_packet(text);
        } else if text.starts_with("%xt%") {
            self.handle_xt_packet(text);
        }
    }
}

impl StatePacketInterceptor {
    fn handle_json_packet(&self, text: &str) {
        let Ok(v) = serde_json::from_str::<serde_json::Value>(text) else {
            return;
        };
        let Some(o) = v.get("b").and_then(|b| b.get("o")) else {
            return;
        };
        let cmd = o.get("cmd").and_then(|c| c.as_str()).unwrap_or("");
        match cmd {
            "moveToArea" => {
                if let Some(area) = o.get("areaId").and_then(|a| a.as_i64()) {
                    *self.state.room_id.write() = area as i32;
                }
                if let Some(map) = o.get("strMapName").and_then(|m| m.as_str()) {
                    let mut p = self.state.player.write();
                    p.map = map.to_string();
                }
            }
            "dropItem" => {
                if let Some(items) = o.get("items").and_then(|i| i.as_object()) {
                    let mut drops = self.state.drops.write();
                    for item in items.values() {
                        let id = item.get("ItemID").and_then(|v| v.as_i64()).unwrap_or(0) as i32;
                        let name = item
                            .get("sName")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string();
                        let qty = item.get("iQty").and_then(|v| v.as_i64()).unwrap_or(1) as i32;
                        let is_ac = item.get("bCoins").and_then(|v| v.as_i64()).unwrap_or(0) == 1;
                        if id > 0 && !name.is_empty() {
                            drops.insert(id, DropItem { id, name, qty, is_ac });
                        }
                    }
                }
            }
            "getDrop" => {
                if let Some(id) = o.get("ItemID").and_then(|v| v.as_i64()) {
                    self.state.drops.write().remove(&(id as i32));
                }
            }
            _ => {}
        }
    }

    fn handle_xt_packet(&self, text: &str) {
        // Example: %xt%warning%-1%...%
        let parts: Vec<&str> = text.split('%').collect();
        if parts.len() >= 5 && parts[2] == "warning" {
            self.logs.push_debug(format!("[Server Warning] {}", parts[4]));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_drop_and_move_packets() {
        let state = Arc::new(GameState::default());
        let logs = Arc::new(LogBuffer::default());
        let interceptor = StatePacketInterceptor::new(state.clone(), logs);

        let mut msg = MessageInfo::new(
            r#"{"t":"xt","b":{"r":-1,"o":{"cmd":"moveToArea","areaId":12345,"strMapName":"yulgar"}}}"#,
        );
        interceptor.intercept(&mut msg, false);
        assert_eq!(state.room_id(), 12345);
        assert_eq!(state.snapshot().map, "yulgar");

        let mut drop_msg = MessageInfo::new(
            r#"{"t":"xt","b":{"r":-1,"o":{"cmd":"dropItem","items":{"42":{"ItemID":42,"sName":"Diamond of Nulgath","iQty":2,"bCoins":1}}}}}"#,
        );
        interceptor.intercept(&mut drop_msg, false);
        let drops = state.drop_list();
        assert_eq!(drops.len(), 1);
        assert_eq!(drops[0].name, "Diamond of Nulgath");
        assert!(drops[0].is_ac);
    }
}
