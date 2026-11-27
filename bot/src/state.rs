//! Live game state updated from both batched Flash snapshots (Ruffle mode) and real-time socket
//! packets (Ruffle + Headless Zero-SWF mode) — Single Source of Truth (SSOT).

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
    pub category: String,
}

#[derive(Clone, Debug)]
pub struct SimpleItem {
    pub id: i32,
    pub char_item_id: i32,
    pub name: String,
    pub qty: i32,
    pub category: String,
    pub is_ac: bool,
    pub equipped: bool,
}

#[derive(Clone, Debug)]
pub struct QuestReq {
    pub item_id: i32,
    pub name: String,
    pub required: i32,
    pub is_temp: bool,
}

#[derive(Clone, Debug)]
pub struct QuestInfo {
    pub id: i32,
    pub name: String,
    pub accepted: bool,
    pub reqs: Vec<QuestReq>,
}

#[derive(Clone, Debug)]
pub struct RoomMonster {
    pub mon_map_id: i32,
    pub name: String,
    pub cell: String,
    pub hp: i32,
    pub max_hp: i32,
}

#[derive(Clone, Debug)]
pub struct SkillSlot {
    pub skill_ref: String,
    pub cd_ms: u64,
    pub mp: i32,
    pub tgt: String,
    pub next_use: Option<std::time::Instant>,
}

#[derive(Default)]
pub struct GameState {
    pub player: RwLock<PlayerSnapshot>,
    pub user_id: RwLock<i32>,
    pub room_id: RwLock<i32>,
    pub drops: RwLock<HashMap<i32, DropItem>>,
    pub inventory: RwLock<Vec<SimpleItem>>,
    pub temp_inventory: RwLock<Vec<SimpleItem>>,
    pub bank: RwLock<Vec<SimpleItem>>,
    pub quests: RwLock<HashMap<i32, QuestInfo>>,
    pub monsters: RwLock<Vec<RoomMonster>>,
    pub map_cells: RwLock<Vec<String>>,
    pub skills: RwLock<Vec<SkillSlot>>,
    pub haste: RwLock<f64>,
    pub last_skill_cast: RwLock<Option<std::time::Instant>>,
}

impl GameState {
    pub fn snapshot(&self) -> PlayerSnapshot {
        self.player.read().clone()
    }

    pub fn update_player(&self, snap: PlayerSnapshot) {
        *self.player.write() = snap;
    }

    pub fn user_id(&self) -> i32 {
        *self.user_id.read()
    }

    pub fn room_id(&self) -> i32 {
        *self.room_id.read()
    }

    pub fn drop_list(&self) -> Vec<DropItem> {
        self.drops.read().values().cloned().collect()
    }

    pub fn inventory_list(&self) -> Vec<SimpleItem> {
        self.inventory.read().clone()
    }

    pub fn temp_inventory_list(&self) -> Vec<SimpleItem> {
        self.temp_inventory.read().clone()
    }

    pub fn bank_list(&self) -> Vec<SimpleItem> {
        self.bank.read().clone()
    }

    pub fn quest_list(&self) -> Vec<QuestInfo> {
        let mut list: Vec<QuestInfo> = self.quests.read().values().cloned().collect();
        list.sort_by_key(|q| (!q.accepted, q.id));
        list
    }

    pub fn cell_list(&self) -> Vec<String> {
        let mut cells = Vec::new();
        let mut push_unique = |c: &str| {
            let trimmed = c.trim();
            if !trimmed.is_empty() && !cells.iter().any(|x: &String| x.eq_ignore_ascii_case(trimmed))
            {
                cells.push(trimmed.to_string());
            }
        };
        for c in self.map_cells.read().iter() {
            push_unique(c);
        }
        let cur = self.player.read().cell.clone();
        push_unique(&cur);
        for m in self.monsters.read().iter() {
            push_unique(&m.cell);
        }
        push_unique("Enter");
        push_unique("Blank");
        cells
    }

    pub fn item_qty(&self, item_id: i32, name: &str, is_temp: bool) -> i32 {
        let lookup = |items: &[SimpleItem]| -> i32 {
            items
                .iter()
                .find(|i| {
                    (item_id > 0 && i.id == item_id)
                        || (!name.is_empty() && i.name.eq_ignore_ascii_case(name))
                })
                .map_or(0, |i| i.qty)
        };
        if is_temp {
            let q = lookup(&self.temp_inventory.read());
            if q > 0 {
                return q;
            }
        }
        let q = lookup(&self.inventory.read());
        if q > 0 {
            return q;
        }
        lookup(&self.temp_inventory.read())
    }

    pub fn has_item(&self, name: &str, qty: i32) -> bool {
        self.inventory
            .read()
            .iter()
            .any(|i| i.name.eq_ignore_ascii_case(name) && (qty <= 0 || i.qty >= qty))
    }

    pub fn has_temp_item(&self, name: &str, qty: i32) -> bool {
        self.temp_inventory
            .read()
            .iter()
            .any(|i| i.name.eq_ignore_ascii_case(name) && (qty <= 0 || i.qty >= qty))
    }

    pub fn has_bank_item(&self, name: &str, qty: i32) -> bool {
        self.bank
            .read()
            .iter()
            .any(|i| i.name.eq_ignore_ascii_case(name) && (qty <= 0 || i.qty >= qty))
    }

    pub fn find_monster_in_cell(&self, name: &str) -> Option<RoomMonster> {
        let cell = self.player.read().cell.clone();
        let lower = name.to_lowercase();
        self.monsters
            .read()
            .iter()
            .find(|m| {
                m.hp > 0
                    && (cell.is_empty() || m.cell.eq_ignore_ascii_case(&cell))
                    && (lower == "*"
                        || lower.is_empty()
                        || m.name.to_lowercase().contains(&lower))
            })
            .cloned()
    }

    pub fn reset_session(&self) {
        *self.player.write() = PlayerSnapshot::default();
        *self.user_id.write() = 0;
        *self.room_id.write() = 0;
        self.drops.write().clear();
        self.inventory.write().clear();
        self.temp_inventory.write().clear();
        self.bank.write().clear();
        self.quests.write().clear();
        self.monsters.write().clear();
        self.map_cells.write().clear();
        self.skills.write().clear();
        *self.haste.write() = 0.0;
        *self.last_skill_cast.write() = None;
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
                if let Some(area) = json_i64(o.get("areaId")) {
                    *self.state.room_id.write() = area as i32;
                }
                let my_user = self.state.player.read().username.clone();
                if let Some(map) = o.get("strMapName").and_then(|m| m.as_str()) {
                    let mut p = self.state.player.write();
                    p.map = map.to_string();
                    if p.cell.is_empty() {
                        p.cell = "Enter".to_string();
                        p.pad = "Spawn".to_string();
                    }
                }
                // Update player live stats & cell from uoBranch
                if let Some(uo_list) = o.get("uoBranch").and_then(|u| u.as_array()) {
                    for uo in uo_list {
                        let uo_name = uo.get("uoName").and_then(|s| s.as_str()).unwrap_or("");
                        if !my_user.is_empty() && uo_name.eq_ignore_ascii_case(&my_user) {
                            let mut p = self.state.player.write();
                            if let Some(hp) = json_i64(uo.get("intHP")) {
                                p.hp = hp as i32;
                                p.alive = hp > 0;
                            }
                            if let Some(mhp) = json_i64(uo.get("intHPMax")) {
                                p.max_hp = mhp as i32;
                            }
                            if let Some(mp) = json_i64(uo.get("intMP")) {
                                p.mp = mp as i32;
                            }
                            if let Some(mmp) = json_i64(uo.get("intMPMax")) {
                                p.max_mp = mmp as i32;
                            }
                            if let Some(lvl) = json_i64(uo.get("intLevel")) {
                                p.level = lvl as i32;
                            }
                            if let Some(frame) = uo.get("strFrame").and_then(|s| s.as_str()) {
                                p.cell = frame.to_string();
                            }
                            if let Some(pad) = uo.get("strPad").and_then(|s| s.as_str()) {
                                p.pad = pad.to_string();
                            }
                        }
                    }
                }
                // Parse room monsters: mondef (MonID -> name), monmap (MonMapID -> strFrame), monBranch (MonMapID -> HP)
                let mut defs: HashMap<i32, String> = HashMap::new();
                if let Some(mondefs) = o.get("mondef").and_then(|m| m.as_array()) {
                    for d in mondefs {
                        let id = json_i64(d.get("MonID")).unwrap_or(0) as i32;
                        let name = d
                            .get("strMonName")
                            .and_then(|s| s.as_str())
                            .unwrap_or("")
                            .to_string();
                        if id > 0 {
                            defs.insert(id, name);
                        }
                    }
                }
                let mut frames: HashMap<i32, String> = HashMap::new();
                if let Some(monmaps) = o.get("monmap").and_then(|m| m.as_array()) {
                    for mm in monmaps {
                        let map_id = json_i64(mm.get("MonMapID")).unwrap_or(0) as i32;
                        let frame = mm
                            .get("strFrame")
                            .and_then(|s| s.as_str())
                            .unwrap_or("")
                            .to_string();
                        if map_id > 0 && !frame.is_empty() {
                            frames.insert(map_id, frame);
                        }
                    }
                }
                let mut mon_list = Vec::new();
                if let Some(branches) = o.get("monBranch").and_then(|m| m.as_array()) {
                    for b in branches {
                        let map_id = json_i64(b.get("MonMapID")).unwrap_or(0) as i32;
                        let mon_id = json_i64(b.get("MonID")).unwrap_or(0) as i32;
                        let hp = json_i64(b.get("intHP")).unwrap_or(100) as i32;
                        let max_hp = json_i64(b.get("intHPMax")).unwrap_or(hp as i64) as i32;
                        let name = defs
                            .get(&mon_id)
                            .cloned()
                            .unwrap_or_else(|| format!("Monster#{mon_id}"));
                        let cell = frames
                            .get(&map_id)
                            .cloned()
                            .or_else(|| {
                                b.get("strFrame")
                                    .and_then(|s| s.as_str())
                                    .map(str::to_string)
                            })
                            .unwrap_or_default();
                        if map_id > 0 {
                            mon_list.push(RoomMonster {
                                mon_map_id: map_id,
                                name,
                                cell,
                                hp,
                                max_hp,
                            });
                        }
                    }
                }
                if !mon_list.is_empty() {
                    *self.state.monsters.write() = mon_list;
                }
            }
            "uotls" => {
                let my_user = self.state.player.read().username.clone();
                let uo_name = o.get("unm").and_then(|s| s.as_str()).unwrap_or("");
                if !my_user.is_empty()
                    && uo_name.eq_ignore_ascii_case(&my_user)
                    && let Some(obj) = o.get("o")
                {
                    let mut p = self.state.player.write();
                    if let Some(hp) = json_i64(obj.get("intHP")) {
                        p.hp = hp as i32;
                        p.alive = hp > 0;
                    }
                    if let Some(mhp) = json_i64(obj.get("intHPMax")) {
                        p.max_hp = mhp as i32;
                    }
                    if let Some(mp) = json_i64(obj.get("intMP")) {
                        p.mp = mp as i32;
                    }
                    if let Some(mmp) = json_i64(obj.get("intMPMax")) {
                        p.max_mp = mmp as i32;
                    }
                    if let Some(lvl) = json_i64(obj.get("intLevel")) {
                        p.level = lvl as i32;
                    }
                    if let Some(frame) = obj.get("strFrame").and_then(|s| s.as_str()) {
                        p.cell = frame.to_string();
                    }
                    if let Some(pad) = obj.get("strPad").and_then(|s| s.as_str()) {
                        p.pad = pad.to_string();
                    }
                }
            }
            "sAct" => {
                if let Some(active) = o
                    .get("actions")
                    .and_then(|a| a.get("active"))
                    .and_then(|a| a.as_array())
                {
                    let parsed: Vec<SkillSlot> = active
                        .iter()
                        .enumerate()
                        .map(|(i, s)| {
                            let default_ref = if i == 0 {
                                "aa".to_string()
                            } else {
                                format!("a{i}")
                            };
                            let skill_ref = s
                                .get("ref")
                                .and_then(|x| x.as_str())
                                .unwrap_or(&default_ref)
                                .to_string();
                            let cd_ms = json_i64(s.get("cd")).unwrap_or(2000).max(500) as u64;
                            let mp = json_i64(s.get("mp")).unwrap_or(0) as i32;
                            let tgt = s
                                .get("tgt")
                                .and_then(|x| x.as_str())
                                .unwrap_or("h")
                                .to_string();
                            SkillSlot {
                                skill_ref,
                                cd_ms,
                                mp,
                                tgt,
                                next_use: None,
                            }
                        })
                        .collect();
                    if !parsed.is_empty() {
                        *self.state.skills.write() = parsed;
                    }
                }
            }
            "stu" => {
                if let Some(sta) = o.get("sta") {
                    let mut p = self.state.player.write();
                    if let Some(gold) = json_i64(sta.get("intGold")) {
                        p.gold = gold;
                    }
                    if let Some(lvl) = json_i64(sta.get("intLevel")) {
                        p.level = lvl as i32;
                    }
                    if let Some(tha) = sta.get("$tha").and_then(|v| {
                        v.as_f64()
                            .or_else(|| v.as_str().and_then(|s| s.parse::<f64>().ok()))
                    }) {
                        *self.state.haste.write() = tha.clamp(0.0, 0.5);
                    }
                }
            }
            "initUserData" => {
                let my_id = self.state.user_id();
                let uid = json_i64(o.get("uid")).unwrap_or(-1) as i32;
                if my_id <= 0 || uid == my_id {
                    let mut p = self.state.player.write();
                    if let Some(frame) = o.get("strFrame").and_then(|s| s.as_str()) {
                        p.cell = frame.to_string();
                    }
                    if let Some(pad) = o.get("strPad").and_then(|s| s.as_str()) {
                        p.pad = pad.to_string();
                    }
                    if let Some(data) = o.get("data") {
                        if let Some(name) = data.get("strUsername").and_then(|s| s.as_str()) {
                            p.username = name.to_string();
                        }
                        if let Some(hp) = json_i64(data.get("intHP")) {
                            p.hp = hp as i32;
                            p.alive = hp > 0;
                        }
                        if let Some(mhp) = json_i64(data.get("intHPMax")) {
                            p.max_hp = mhp as i32;
                        }
                        if let Some(mp) = json_i64(data.get("intMP")) {
                            p.mp = mp as i32;
                        }
                        if let Some(mmp) = json_i64(data.get("intMPMax")) {
                            p.max_mp = mmp as i32;
                        }
                        if let Some(lvl) = json_i64(data.get("intLevel")) {
                            p.level = lvl as i32;
                        }
                        if let Some(gold) = json_i64(data.get("intGold")) {
                            p.gold = gold;
                        }
                    }
                }
            }
            "initUserDatas" => {
                let my_id = self.state.user_id();
                let my_user = self.state.player.read().username.clone();
                if let Some(arr) = o.get("a").and_then(|a| a.as_array()) {
                    for entry in arr {
                        let uid = json_i64(entry.get("uid")).unwrap_or(-1) as i32;
                        let entry_name = entry
                            .get("data")
                            .and_then(|d| d.get("strUsername"))
                            .and_then(|s| s.as_str())
                            .unwrap_or("");
                        let is_me = (my_id > 0 && uid == my_id)
                            || (!my_user.is_empty() && entry_name.eq_ignore_ascii_case(&my_user));
                        if !is_me {
                            continue;
                        }
                        if let Some(data) = entry.get("data") {
                            let mut p = self.state.player.write();
                            if !entry_name.is_empty() {
                                p.username = entry_name.to_string();
                            }
                            if let Some(hp) = json_i64(data.get("intHP")) {
                                p.hp = hp as i32;
                                p.alive = hp > 0;
                            }
                            if let Some(mhp) = json_i64(data.get("intHPMax")) {
                                p.max_hp = mhp as i32;
                            }
                            if let Some(mp) = json_i64(data.get("intMP")) {
                                p.mp = mp as i32;
                            }
                            if let Some(mmp) = json_i64(data.get("intMPMax")) {
                                p.max_mp = mmp as i32;
                            }
                            if let Some(lvl) = json_i64(data.get("intLevel")) {
                                p.level = lvl as i32;
                            }
                            if let Some(gold) = json_i64(data.get("intGold")) {
                                p.gold = gold;
                            }
                        }
                    }
                }
            }
            "loadInventoryBig" => {
                if let Some(items) = o.get("items").and_then(|i| i.as_array()) {
                    let parsed: Vec<SimpleItem> =
                        items.iter().filter_map(parse_simple_item).collect();
                    *self.state.inventory.write() = parsed;
                }
                if let Some(temps) = o.get("tempitems").and_then(|i| i.as_array()) {
                    let parsed: Vec<SimpleItem> =
                        temps.iter().filter_map(parse_simple_item).collect();
                    *self.state.temp_inventory.write() = parsed;
                }
            }
            "loadBank" => {
                if let Some(items) = o.get("items").and_then(|i| i.as_array()) {
                    let parsed: Vec<SimpleItem> =
                        items.iter().filter_map(parse_simple_item).collect();
                    *self.state.bank.write() = parsed;
                }
            }
            "addItems" => {
                if let Some(items) = o.get("items").and_then(|i| i.as_object()) {
                    let mut inv = self.state.inventory.write();
                    let mut temp = self.state.temp_inventory.write();
                    for item in items.values() {
                        if let Some(mut si) = parse_simple_item(item) {
                            let is_temp = json_i64(item.get("bTemp")).unwrap_or(0) == 1;
                            if let Some(now) = json_i64(item.get("iQtyNow")) {
                                si.qty = now as i32;
                            }
                            let target = if is_temp { &mut *temp } else { &mut *inv };
                            if let Some(existing) = target.iter_mut().find(|x| x.id == si.id) {
                                existing.qty = si.qty;
                                if !si.name.is_empty() {
                                    existing.name = si.name;
                                }
                            } else {
                                target.push(si);
                            }
                        }
                    }
                }
            }
            "bankFromInv" => {
                if let Some(item_id) = json_i64(o.get("ItemID")) {
                    let item_id = item_id as i32;
                    let mut inv = self.state.inventory.write();
                    if let Some(pos) = inv.iter().position(|x| x.id == item_id) {
                        let moved = inv.remove(pos);
                        let mut bank = self.state.bank.write();
                        if !bank.iter().any(|x| x.id == moved.id) {
                            bank.push(moved);
                        }
                    }
                }
            }
            "bankToInv" => {
                if let Some(item_id) = json_i64(o.get("ItemID")) {
                    let item_id = item_id as i32;
                    let mut bank = self.state.bank.write();
                    if let Some(pos) = bank.iter().position(|x| x.id == item_id) {
                        let moved = bank.remove(pos);
                        let mut inv = self.state.inventory.write();
                        if !inv.iter().any(|x| x.id == moved.id) {
                            inv.push(moved);
                        }
                    }
                }
            }
            "getQuests" => {
                if let Some(qmap) = o.get("quests").and_then(|q| q.as_object()) {
                    let mut quests = self.state.quests.write();
                    for qv in qmap.values() {
                        let Some(qid) = json_i64(qv.get("QuestID")).map(|v| v as i32) else {
                            continue;
                        };
                        let name = qv
                            .get("sName")
                            .and_then(|s| s.as_str())
                            .unwrap_or("")
                            .to_string();
                        let status = qv.get("status").and_then(|s| s.as_str()).unwrap_or("");
                        let prev_accepted = quests.get(&qid).is_some_and(|q| q.accepted);
                        let accepted = prev_accepted || status.eq_ignore_ascii_case("p");

                        // Map ItemID -> (sName, bTemp) from oItems if available
                        let mut item_meta: HashMap<i32, (String, bool)> = HashMap::new();
                        if let Some(oitems) = qv.get("oItems").and_then(|m| m.as_object()) {
                            for iv in oitems.values() {
                                if let Some(iid) = json_i64(iv.get("ItemID")).map(|v| v as i32) {
                                    let iname = iv
                                        .get("sName")
                                        .and_then(|s| s.as_str())
                                        .unwrap_or("")
                                        .to_string();
                                    let is_temp = json_i64(iv.get("bTemp")).unwrap_or(0) == 1;
                                    item_meta.insert(iid, (iname, is_temp));
                                }
                            }
                        }

                        let mut reqs = Vec::new();
                        if let Some(turnins) = qv.get("turnin").and_then(|t| t.as_array()) {
                            for tv in turnins {
                                let iid = json_i64(tv.get("ItemID")).unwrap_or(0) as i32;
                                let required = json_i64(tv.get("iQty")).unwrap_or(1) as i32;
                                let (meta_name, is_temp) = item_meta
                                    .get(&iid)
                                    .cloned()
                                    .unwrap_or_else(|| (String::new(), false));
                                let rname = tv
                                    .get("sName")
                                    .and_then(|s| s.as_str())
                                    .filter(|s| !s.is_empty())
                                    .map(str::to_string)
                                    .unwrap_or_else(|| {
                                        if meta_name.is_empty() {
                                            format!("Item #{iid}")
                                        } else {
                                            meta_name
                                        }
                                    });
                                reqs.push(QuestReq {
                                    item_id: iid,
                                    name: rname,
                                    required,
                                    is_temp,
                                });
                            }
                        }

                        quests.insert(
                            qid,
                            QuestInfo {
                                id: qid,
                                name,
                                accepted,
                                reqs,
                            },
                        );
                    }
                }
            }
            "acceptQuest" => {
                if let Some(qid) = json_i64(o.get("QuestID")).map(|v| v as i32) {
                    let ok = json_i64(o.get("bSuccess")).unwrap_or(1) == 1;
                    if ok {
                        let mut quests = self.state.quests.write();
                        quests
                            .entry(qid)
                            .and_modify(|q| q.accepted = true)
                            .or_insert_with(|| QuestInfo {
                                id: qid,
                                name: format!("Quest #{qid}"),
                                accepted: true,
                                reqs: Vec::new(),
                            });
                    }
                }
            }
            "ccqr" | "abandonQuest" => {
                if let Some(qid) = json_i64(o.get("QuestID")).map(|v| v as i32) {
                    if let Some(q) = self.state.quests.write().get_mut(&qid) {
                        q.accepted = false;
                    }
                }
            }
            "dropItem" => {
                if let Some(items) = o.get("items").and_then(|i| i.as_object()) {
                    let mut drops = self.state.drops.write();
                    for item in items.values() {
                        let id = json_i64(item.get("ItemID")).unwrap_or(0) as i32;
                        let name = item
                            .get("sName")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string();
                        let qty = json_i64(item.get("iQty")).unwrap_or(1) as i32;
                        let is_ac = json_i64(item.get("bCoins")).unwrap_or(0) == 1;
                        let category = item
                            .get("sType")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string();
                        if id > 0 && !name.is_empty() {
                            drops.insert(
                                id,
                                DropItem {
                                    id,
                                    name,
                                    qty,
                                    is_ac,
                                    category,
                                },
                            );
                        }
                    }
                }
            }
            "equipItem" => {
                let my_id = self.state.user_id();
                let uid = json_i64(o.get("uid")).unwrap_or(-1) as i32;
                if (my_id <= 0 || uid == my_id)
                    && let Some(item_id) = json_i64(o.get("ItemID")).map(|v| v as i32)
                {
                    let mut inv = self.state.inventory.write();
                    let target_slot = inv
                        .iter()
                        .find(|x| x.id == item_id)
                        .map(|x| equip_slot_group(&x.category));
                    for it in inv.iter_mut() {
                        if it.id == item_id {
                            it.equipped = true;
                        } else if let Some(slot) = target_slot
                            && !slot.is_empty()
                            && equip_slot_group(&it.category) == slot
                        {
                            it.equipped = false;
                        }
                    }
                }
            }
            "unequipItem" => {
                let my_id = self.state.user_id();
                let uid = json_i64(o.get("uid")).unwrap_or(-1) as i32;
                if (my_id <= 0 || uid == my_id)
                    && let Some(item_id) = json_i64(o.get("ItemID")).map(|v| v as i32)
                {
                    let mut inv = self.state.inventory.write();
                    if let Some(it) = inv.iter_mut().find(|x| x.id == item_id) {
                        it.equipped = false;
                    }
                }
            }
            "getDrop" => {
                if let Some(id) = o.get("ItemID").and_then(|v| v.as_i64()) {
                    self.state.drops.write().remove(&(id as i32));
                }
            }
            "ct" => {
                // Update player HP/MP from combat ticks
                let uname = self.state.player.read().username.to_lowercase();
                if let Some(p_map) = o.get("p").and_then(|p| p.as_object()) {
                    if let Some(me) = p_map.get(&uname) {
                        let mut p = self.state.player.write();
                        if let Some(hp) = me.get("intHP").and_then(|x| x.as_i64()) {
                            p.hp = hp as i32;
                            p.alive = hp > 0;
                        }
                        if let Some(mp) = me.get("intMP").and_then(|x| x.as_i64()) {
                            p.mp = mp as i32;
                        }
                    }
                }
                if let Some(m_map) = o.get("m").and_then(|m| m.as_object()) {
                    let mut mons = self.state.monsters.write();
                    for (mid_str, mval) in m_map {
                        if let Ok(mid) = mid_str.parse::<i32>()
                            && let Some(mon) = mons.iter_mut().find(|x| x.mon_map_id == mid)
                            && let Some(hp) = mval.get("intHP").and_then(|x| x.as_i64())
                        {
                            mon.hp = hp as i32;
                        }
                    }
                }
            }
            _ => {}
        }
    }

    fn handle_xt_packet(&self, text: &str) {
        let parts: Vec<&str> = text.split('%').collect();
        if parts.len() < 4 {
            return;
        }
        match parts[2] {
            "loginResponse" => {
                // %xt%loginResponse%-1%true%<userId>%<username>%...
                if parts.len() >= 7 && parts[4].eq_ignore_ascii_case("true") {
                    if let Ok(uid) = parts[5].parse::<i32>() {
                        *self.state.user_id.write() = uid;
                    }
                    let mut p = self.state.player.write();
                    p.logged_in = true;
                    p.username = parts[6].to_string();
                    p.alive = true;
                    self.logs
                        .push_debug(format!("[Login] Connected as {}", p.username));
                }
            }
            "warning" if parts.len() >= 5 => {
                self.logs.push_debug(format!("[Server Warning] {}", parts[4]));
            }
            "server" if parts.len() >= 5 => {
                self.logs.push_debug(format!("[Server] {}", parts[4]));
            }
            _ => {}
        }
    }
}

fn json_i64(v: Option<&serde_json::Value>) -> Option<i64> {
    match v? {
        serde_json::Value::Number(n) => n.as_i64(),
        serde_json::Value::String(s) => s.trim().parse::<i64>().ok(),
        _ => None,
    }
}

fn equip_slot_group(category: &str) -> &'static str {
    match category.trim().to_lowercase().as_str() {
        "sword" | "axe" | "dagger" | "gun" | "handgun" | "rifle" | "bow" | "mace"
        | "gauntlet" | "polearm" | "staff" | "wand" | "whip" | "weapon" => "weapon",
        "class" => "class",
        "armor" => "armor",
        "helm" => "helm",
        "cape" => "cape",
        "pet" => "pet",
        "necklace" | "amulet" => "amulet",
        "ground" | "ground item" | "floor item" => "ground",
        _ => "",
    }
}

fn parse_simple_item(v: &serde_json::Value) -> Option<SimpleItem> {
    let id = json_i64(v.get("ItemID"))? as i32;
    let char_item_id = json_i64(v.get("CharItemID")).unwrap_or(0) as i32;
    let name = v.get("sName").and_then(|x| x.as_str())?.to_string();
    let qty = json_i64(v.get("iQty")).unwrap_or(1) as i32;
    let category = v
        .get("sType")
        .and_then(|x| x.as_str())
        .unwrap_or("")
        .to_string();
    let is_ac = json_i64(v.get("bCoins")).unwrap_or(0) == 1;
    let equipped = json_i64(v.get("bEquip")).unwrap_or(0) == 1;
    Some(SimpleItem {
        id,
        char_item_id,
        name,
        qty,
        category,
        is_ac,
        equipped,
    })
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
