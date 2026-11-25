//! Grimlite-Li `.gbot` JSON parser, serializer, and command model.
//! Supports both Newtonsoft.Json `{ "$type": "...", "$values": [...] }` and plain JSON arrays.

use serde::de::DeserializeOwned;
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;
use std::fmt;

/// Deserializes either a plain JSON array `[...]` or Newtonsoft.Json `{ "$values": [...] }`.
fn deserialize_net_list<'de, D, T>(deserializer: D) -> Result<Vec<T>, D::Error>
where
    D: Deserializer<'de>,
    T: DeserializeOwned,
{
    let val = Option::<Value>::deserialize(deserializer)?;
    let Some(val) = val else {
        return Ok(Vec::new());
    };
    let arr = match val {
        Value::Array(a) => a,
        Value::Object(mut map) => match map.remove("$values") {
            Some(Value::Array(a)) => a,
            _ => Vec::new(),
        },
        _ => Vec::new(),
    };
    arr.into_iter()
        .map(serde_json::from_value)
        .collect::<Result<Vec<T>, _>>()
        .map_err(serde::de::Error::custom)
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase", default)]
pub struct BotConfig {
    pub author: String,
    pub description: String,
    #[serde(deserialize_with = "deserialize_net_list")]
    pub commands: Vec<BotCommand>,
    #[serde(deserialize_with = "deserialize_net_list")]
    pub skills: Vec<SkillEntry>,
    #[serde(deserialize_with = "deserialize_net_list")]
    pub quests: Vec<QuestEntry>,
    #[serde(deserialize_with = "deserialize_net_list")]
    pub drops: Vec<String>,
    pub skill_delay: u64,
    pub bot_delay: u64,
    pub enable_pickup: bool,
    pub enable_rejection: bool,
    pub enable_pickup_all: bool,
    pub enable_pickup_ac_tagged: bool,
    pub auto_relogin: bool,
    pub infinite_attack_range: bool,
    pub provoke_monsters: bool,
    pub enemy_magnet: bool,
    pub lag_killer: bool,
    pub skip_cutscenes: bool,
    pub disable_animations: bool,
    pub restart_upon_death: bool,
    pub exit_combat_before_rest: bool,
    pub exit_combat_before_quest: bool,
    pub skip_delay_index_if: bool,
}

impl BotConfig {
    pub fn from_gbot_json(json: &str) -> Result<Self, serde_json::Error> {
        let trimmed = json.trim_start_matches('\u{feff}');
        serde_json::from_str(trimmed)
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase", default)]
pub struct SkillEntry {
    pub text: String,
    pub index: String,
    #[serde(rename = "Type")]
    pub skill_type: i32,
    pub safe_value: i32,
    pub safe_health: bool,
    pub safe_mana: bool,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase", default)]
pub struct QuestEntry {
    #[serde(alias = "QuestID")]
    pub id: i32,
    pub item_id: Option<String>,
    pub safe_relogin: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum StatementKind {
    InInventory,
    NotInInventory,
    InTemp,
    NotInTemp,
    InBank,
    NotInBank,
    InBankOrInvent,
    NotInBankAndInvent,
    MapIs,
    MapIsNot,
    CellIs,
    CellIsNot,
    QuestInProgress,
    QuestNotInProgress,
    QuestCompleted,
    QuestNotCompleted,
    MonsterInRoom,
    MonsterNotInRoom,
    HasTarget,
    NotHasTarget,
    HealthLessThan,
    HealthGreaterThan,
    ManaLessThan,
    ManaGreaterThan,
    GoldGreaterThan,
    GoldLessThan,
    LevelGreaterThan,
    LevelLessThan,
    IntIs,
    IntIsNot,
    IntGreaterThan,
    IntLessThan,
    IntEqualInt,
    IntNotEqualInt,
    VarEqualVar,
    VarNotEqualVar,
    NameEquals,
}

#[derive(Clone, Debug, Serialize)]
pub enum BotCommand {
    // Combat
    Kill { monster: String },
    KillFor { monster: String, item_name: String, quantity: String, is_temp: bool },
    ShortHunt {
        map: String,
        cell: String,
        pad: String,
        monster: String,
        item_name: String,
        quantity: String,
        is_temp: bool,
        delay_after_kill: u64,
    },
    QuestHunt {
        map: String,
        cell: String,
        pad: String,
        monster: String,
        item_name: String,
        quantity: String,
        quest_id: i32,
    },
    Attack { monster: String },
    UseSkill { index: String },
    SkillSet { name: String },
    Rest { full: bool },
    CancelTarget,

    // Map
    Join { map: String, cell: String, pad: String },
    MoveToCell { cell: String, pad: String },
    Walk { x: i32, y: i32 },
    SetSpawn,
    Yulgar,

    // Item
    GetDrop { item_name: String },
    LoadShop { shop_id: i32 },
    Buy { shop_id: i32, item_name: String, qty: i32 },
    Sell { item_name: String },
    Equip { item_name: String },
    BankTransfer { item_name: String, to_bank: bool },
    MapItem { item_id: i32 },
    Whitelist { item_name: String, add: bool },

    // Quest
    AcceptQuest { quest_id: i32 },
    CompleteQuest { quest_id: i32, item_id: i32 },

    // Misc
    Delay { ms: u64 },
    Label { name: String },
    GotoLabel { name: String },
    Index { index: i32, is_up: bool },
    Packet { packet: String, to_client: bool, spam_times: i32, delay_ms: u64 },
    Int { name: String, value: i32, op: IntOp },
    SetVar { name: String, value: String },
    Log { text: String },
    Provoke { enabled: bool },
    SetFPS { fps: i32 },
    Stop,
    Restart,
    Blank { text: String },

    // Conditional Statement (skips next command if FALSE)
    Statement { kind: StatementKind, value1: String, value2: String },

    // Unrecognized command preserved without failing `.gbot` load
    Unknown { type_name: String, raw: Value },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum IntOp {
    Set,
    Add,
    Sub,
}

impl fmt::Display for BotCommand {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Kill { monster } => write!(f, "Kill {monster}"),
            Self::KillFor { monster, item_name, quantity, is_temp } => {
                let kind = if *is_temp { "temp" } else { "item" };
                write!(f, "Kill for {kind}: {monster} -> {item_name} x{quantity}")
            }
            Self::ShortHunt { map, monster, item_name, quantity, .. } => {
                write!(f, "Short hunt [{map}]: {monster} -> {item_name} x{quantity}")
            }
            Self::QuestHunt { map, monster, item_name, quantity, quest_id, .. } => {
                write!(f, "Quest hunt [{map}] (Q:{quest_id}): {monster} -> {item_name} x{quantity}")
            }
            Self::Attack { monster } => write!(f, "Attack {monster}"),
            Self::UseSkill { index } => write!(f, "Use skill [{index}]"),
            Self::SkillSet { name } => write!(f, "Skill set: [{name}]"),
            Self::Rest { full } => write!(f, "Rest{}", if *full { " (full)" } else { "" }),
            Self::CancelTarget => write!(f, "Cancel target"),
            Self::Join { map, cell, pad } => write!(f, "Join {map}, {cell}, {pad}"),
            Self::MoveToCell { cell, pad } => write!(f, "Move to cell {cell}, {pad}"),
            Self::Walk { x, y } => write!(f, "Walk to ({x}, {y})"),
            Self::SetSpawn => write!(f, "Set spawn point"),
            Self::Yulgar => write!(f, "Join Yulgar"),
            Self::GetDrop { item_name } => write!(f, "Get drop: {item_name}"),
            Self::LoadShop { shop_id } => write!(f, "Load shop {shop_id}"),
            Self::Buy { shop_id, item_name, qty } => {
                if *qty > 1 {
                    write!(f, "Buy {item_name} x{qty} (shop {shop_id})")
                } else {
                    write!(f, "Buy {item_name} (shop {shop_id})")
                }
            }
            Self::Sell { item_name } => write!(f, "Sell {item_name}"),
            Self::Equip { item_name } => write!(f, "Equip {item_name}"),
            Self::BankTransfer { item_name, to_bank } => {
                if *to_bank {
                    write!(f, "Transfer to bank: {item_name}")
                } else {
                    write!(f, "Transfer from bank: {item_name}")
                }
            }
            Self::MapItem { item_id } => write!(f, "Get map item {item_id}"),
            Self::Whitelist { item_name, add } => {
                write!(f, "{} whitelist: {item_name}", if *add { "Add" } else { "Remove" })
            }
            Self::AcceptQuest { quest_id } => write!(f, "Accept quest {quest_id}"),
            Self::CompleteQuest { quest_id, item_id } => {
                if *item_id > 0 {
                    write!(f, "Complete quest {quest_id} (item {item_id})")
                } else {
                    write!(f, "Complete quest {quest_id}")
                }
            }
            Self::Delay { ms } => write!(f, "Delay {ms} ms"),
            Self::Label { name } => write!(f, "[{name}]"),
            Self::GotoLabel { name } => write!(f, "Goto label [{name}]"),
            Self::Index { index, is_up } => {
                write!(f, "Index {} {index}", if *is_up { "up" } else { "down" })
            }
            Self::Packet { packet, to_client, spam_times, .. } => {
                let dir = if *to_client { "client" } else { "server" };
                if *spam_times > 1 {
                    write!(f, "Send {dir} packet (x{spam_times}): {packet}")
                } else {
                    write!(f, "Send {dir} packet: {packet}")
                }
            }
            Self::Int { name, value, op } => match op {
                IntOp::Set => write!(f, "Int {name} = {value}"),
                IntOp::Add => write!(f, "Int {name} += {value}"),
                IntOp::Sub => write!(f, "Int {name} -= {value}"),
            },
            Self::SetVar { name, value } => write!(f, "Set var [{name}] = {value}"),
            Self::Log { text } => write!(f, "Log: {text}"),
            Self::Provoke { enabled } => write!(f, "Provoke: {enabled}"),
            Self::SetFPS { fps } => write!(f, "Set FPS: {fps}"),
            Self::Stop => write!(f, "Stop bot"),
            Self::Restart => write!(f, "Restart bot"),
            Self::Blank { text } => write!(f, "{text}"),
            Self::Statement { kind, value1, value2 } => {
                if value2.is_empty() {
                    write!(f, "If {kind:?}: {value1}")
                } else {
                    write!(f, "If {kind:?}: {value1}, {value2}")
                }
            }
            Self::Unknown { type_name, .. } => write!(f, "[Unsupported: {type_name}]"),
        }
    }
}

fn str_field(v: &Value, keys: &[&str]) -> String {
    for k in keys {
        if let Some(val) = v.get(*k) {
            if let Some(s) = val.as_str() {
                return s.to_string();
            }
            if let Some(n) = val.as_i64() {
                return n.to_string();
            }
        }
    }
    String::new()
}

fn int_field(v: &Value, keys: &[&str], default: i32) -> i32 {
    for k in keys {
        if let Some(val) = v.get(*k) {
            if let Some(n) = val.as_i64() {
                return n as i32;
            }
            if let Some(s) = val.as_str()
                && let Ok(n) = s.trim().parse::<i32>()
            {
                return n;
            }
        }
    }
    default
}

fn bool_field(v: &Value, keys: &[&str], default: bool) -> bool {
    for k in keys {
        if let Some(val) = v.get(*k)
            && let Some(b) = val.as_bool()
        {
            return b;
        }
    }
    default
}

impl<'de> Deserialize<'de> for BotCommand {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let v = Value::deserialize(deserializer)?;
        let full_type = v.get("$type").and_then(|t| t.as_str()).unwrap_or("");
        let short = full_type
            .split(',')
            .next()
            .unwrap_or("")
            .rsplit('.')
            .next()
            .unwrap_or(full_type);

        let cmd = match short {
            "CmdKill" => Self::Kill {
                monster: str_field(&v, &["Monster"]),
            },
            "CmdKillFor" => Self::KillFor {
                monster: str_field(&v, &["Monster"]),
                item_name: str_field(&v, &["ItemName"]),
                quantity: str_field(&v, &["Quantity"]),
                is_temp: int_field(&v, &["ItemType"], 0) == 1,
            },
            "CmdShortHunt" => Self::ShortHunt {
                map: str_field(&v, &["Map"]),
                cell: {
                    let c = str_field(&v, &["Cell"]);
                    if c.is_empty() { "Enter".to_string() } else { c }
                },
                pad: {
                    let p = str_field(&v, &["Pad"]);
                    if p.is_empty() { "Spawn".to_string() } else { p }
                },
                monster: str_field(&v, &["Monster"]),
                item_name: str_field(&v, &["ItemName"]),
                quantity: str_field(&v, &["Quantity"]),
                is_temp: int_field(&v, &["ItemType"], 0) == 1,
                delay_after_kill: int_field(&v, &["DelayAfterKill"], 50).max(0) as u64,
            },
            "CmdQuestHunt" => Self::QuestHunt {
                map: str_field(&v, &["Map"]),
                cell: {
                    let c = str_field(&v, &["Cell"]);
                    if c.is_empty() { "Enter".to_string() } else { c }
                },
                pad: {
                    let p = str_field(&v, &["Pad"]);
                    if p.is_empty() { "Spawn".to_string() } else { p }
                },
                monster: str_field(&v, &["Monster"]),
                item_name: str_field(&v, &["ItemName"]),
                quantity: str_field(&v, &["Quantity"]),
                quest_id: int_field(&v, &["QID", "QuestId"], 0),
            },
            "CmdAttack" => Self::Attack {
                monster: str_field(&v, &["Monster"]),
            },
            "CmdUseSkill" => Self::UseSkill {
                index: str_field(&v, &["Skill", "Index"]),
            },
            "CmdSkillSet" => Self::SkillSet {
                name: str_field(&v, &["Name"]),
            },
            "CmdRest" => Self::Rest {
                full: bool_field(&v, &["Full"], false),
            },
            "CmdCancelTarget" => Self::CancelTarget,
            "CmdJoin" => Self::Join {
                map: str_field(&v, &["Map"]),
                cell: {
                    let c = str_field(&v, &["Cell"]);
                    if c.is_empty() { "Enter".to_string() } else { c }
                },
                pad: {
                    let p = str_field(&v, &["Pad"]);
                    if p.is_empty() { "Spawn".to_string() } else { p }
                },
            },
            "CmdMoveToCell" => Self::MoveToCell {
                cell: str_field(&v, &["Cell"]),
                pad: {
                    let p = str_field(&v, &["Pad"]);
                    if p.is_empty() { "Spawn".to_string() } else { p }
                },
            },
            "CmdWalk" => Self::Walk {
                x: int_field(&v, &["X"], 0),
                y: int_field(&v, &["Y"], 0),
            },
            "CmdSetSpawn" => Self::SetSpawn,
            "CmdYulgar" => Self::Yulgar,
            "CmdGetDrop" => Self::GetDrop {
                item_name: str_field(&v, &["ItemName"]),
            },
            "CmdLoad" => Self::LoadShop {
                shop_id: int_field(&v, &["ShopId"], 0),
            },
            "CmdBuy" | "CmdBuyFast" => Self::Buy {
                shop_id: int_field(&v, &["ShopId"], 0),
                item_name: str_field(&v, &["ItemName"]),
                qty: int_field(&v, &["Qty"], 1).max(1),
            },
            "CmdSell" => Self::Sell {
                item_name: str_field(&v, &["ItemName"]),
            },
            "CmdEquip" => Self::Equip {
                item_name: str_field(&v, &["ItemName"]),
            },
            "CmdBankTransfer" => Self::BankTransfer {
                item_name: str_field(&v, &["ItemName"]),
                to_bank: !bool_field(&v, &["TransferFromBank"], false),
            },
            "CmdMapItem" => Self::MapItem {
                item_id: int_field(&v, &["ItemId"], 0),
            },
            "CmdWhitelist" => Self::Whitelist {
                item_name: str_field(&v, &["ItemName"]),
                add: int_field(&v, &["state"], 0) == 0,
            },
            "CmdAcceptQuest" => {
                let id = v
                    .get("Quest")
                    .map(|q| int_field(q, &["QuestID", "Id"], 0))
                    .unwrap_or_else(|| int_field(&v, &["QuestID", "QuestId", "Id"], 0));
                Self::AcceptQuest { quest_id: id }
            }
            "CmdCompleteQuest" => {
                let id = v
                    .get("Quest")
                    .map(|q| int_field(q, &["QuestID", "Id"], 0))
                    .unwrap_or_else(|| int_field(&v, &["QuestID", "QuestId", "Id"], 0));
                let item_id = v
                    .get("Quest")
                    .map(|q| int_field(q, &["ItemId", "ItemID"], -1))
                    .unwrap_or(-1);
                Self::CompleteQuest {
                    quest_id: id,
                    item_id,
                }
            }
            "CmdDelay" => Self::Delay {
                ms: int_field(&v, &["Delay"], 1000).max(0) as u64,
            },
            "CmdLabel" => Self::Label {
                name: str_field(&v, &["Name"]),
            },
            "CmdGotoLabel" => Self::GotoLabel {
                name: str_field(&v, &["Label"]),
            },
            "CmdIndex" => Self::Index {
                index: int_field(&v, &["Index"], 1),
                is_up: int_field(&v, &["Type"], 0) == 0,
            },
            "CmdPacket" => Self::Packet {
                packet: str_field(&v, &["Packet"]),
                to_client: bool_field(&v, &["Client"], false),
                spam_times: int_field(&v, &["SpamTimes"], 1).max(1),
                delay_ms: int_field(&v, &["Delay"], 0).max(0) as u64,
            },
            "CmdInt" => {
                let op = match int_field(&v, &["type"], 0) {
                    1 => IntOp::Add,
                    2 => IntOp::Sub,
                    _ => IntOp::Set,
                };
                Self::Int {
                    name: str_field(&v, &["Int"]),
                    value: int_field(&v, &["Value"], 0),
                    op,
                }
            }
            "CmdSetVar" => Self::SetVar {
                name: str_field(&v, &["Value1"]),
                value: str_field(&v, &["Value2"]),
            },
            "CmdLog" => Self::Log {
                text: str_field(&v, &["Text"]),
            },
            "CmdProvoke" | "CmdToggleProvoke" => Self::Provoke {
                enabled: bool_field(&v, &["Set"], true),
            },
            "CmdSetFPS" => Self::SetFPS {
                fps: int_field(&v, &["FPS"], 30),
            },
            "CmdStop" => Self::Stop,
            "CmdRestart" => Self::Restart,
            "CmdBlank" | "CmdBlank2" | "CmdBlank3" => Self::Blank {
                text: str_field(&v, &["Text"]),
            },
            other => {
                if let Some(kind) = parse_statement_kind(other) {
                    Self::Statement {
                        kind,
                        value1: str_field(&v, &["Value1"]),
                        value2: str_field(&v, &["Value2"]),
                    }
                } else {
                    Self::Unknown {
                        type_name: other.to_string(),
                        raw: v,
                    }
                }
            }
        };
        Ok(cmd)
    }
}

fn parse_statement_kind(name: &str) -> Option<StatementKind> {
    use StatementKind as S;
    Some(match name {
        "CmdInInventory" => S::InInventory,
        "CmdNotInInventory" => S::NotInInventory,
        "CmdInTemp" => S::InTemp,
        "CmdNotInTemp" => S::NotInTemp,
        "CmdInBank" => S::InBank,
        "CmdNotInBank" => S::NotInBank,
        "CmdInBankOrInvent" => S::InBankOrInvent,
        "CmdNotInBankAndInvent" => S::NotInBankAndInvent,
        "CmdMapIs" => S::MapIs,
        "CmdMapIsNot" => S::MapIsNot,
        "CmdCellIs" => S::CellIs,
        "CmdCellIsNot" => S::CellIsNot,
        "CmdQuestInProgress" => S::QuestInProgress,
        "CmdQuestNotInProgress" => S::QuestNotInProgress,
        "CmdQuestCompleted" => S::QuestCompleted,
        "CmdQuestNotCompleted" => S::QuestNotCompleted,
        "CmdMonsterInRoom" => S::MonsterInRoom,
        "CmdMonsterNotInRoom" => S::MonsterNotInRoom,
        "CmdHasTarget" => S::HasTarget,
        "CmdNotHasTarget" => S::NotHasTarget,
        "CmdHealthLessThan" => S::HealthLessThan,
        "CmdHealthGreaterThan" => S::HealthGreaterThan,
        "CmdManaLessThan" => S::ManaLessThan,
        "CmdManaGreaterThan" => S::ManaGreaterThan,
        "CmdGoldGreaterThan" => S::GoldGreaterThan,
        "CmdGoldLessThan" => S::GoldLessThan,
        "CmdLevelGreaterThan" => S::LevelGreaterThan,
        "CmdLevelLessThan" => S::LevelLessThan,
        "CmdIntIs" => S::IntIs,
        "CmdIntIsNot" => S::IntIsNot,
        "CmdIntGreaterThan" => S::IntGreaterThan,
        "CmdIntLessThan" => S::IntLessThan,
        "CmdIntEqualInt" => S::IntEqualInt,
        "CmdIntNotEqualInt" => S::IntNotEqualInt,
        "CmdVarEqualVar" => S::VarEqualVar,
        "CmdVarNotEqualVar" => S::VarNotEqualVar,
        "CmdNameEquals" => S::NameEquals,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_grimlite_li_gbot_json() {
        let sample = r#"{
            "$type": "Grimoire.Botting.Configuration, Grimoire",
            "Author": "Questwalker",
            "Description": "Sample farm",
            "Commands": {
                "$type": "System.Collections.Generic.List`1[[Grimoire.Botting.IBotCommand, Grimoire]], mscorlib",
                "$values": [
                    { "$type": "Grimoire.Botting.Commands.Map.CmdJoin, Grimoire", "Map": "greenguardwest-9999", "Cell": "West4", "Pad": "Left" },
                    { "$type": "Grimoire.Botting.Commands.Misc.CmdLabel, Grimoire", "Name": "LOOP" },
                    { "$type": "Grimoire.Botting.Commands.Combat.CmdKill, Grimoire", "Monster": "Frogzard" },
                    { "$type": "Grimoire.Botting.Commands.Misc.Statements.CmdNotInInventory, Grimoire", "Value1": "Frogzard Tooth", "Value2": "10" },
                    { "$type": "Grimoire.Botting.Commands.Misc.CmdGotoLabel, Grimoire", "Label": "LOOP" }
                ]
            },
            "Drops": {
                "$values": ["Frogzard Tooth"]
            }
        }"#;
        let cfg = BotConfig::from_gbot_json(sample).expect("parse .gbot");
        assert_eq!(cfg.author, "Questwalker");
        assert_eq!(cfg.commands.len(), 5);
        assert_eq!(cfg.drops, vec!["Frogzard Tooth"]);
        assert!(matches!(cfg.commands[0], BotCommand::Join { .. }));
        assert!(matches!(
            cfg.commands[3],
            BotCommand::Statement {
                kind: StatementKind::NotInInventory,
                ..
            }
        ));
    }
}
