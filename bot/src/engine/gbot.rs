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

/// Deserializes Grimlite-Li's `"Server": { "sName": "Artix", ... }` or a plain string `"Artix"`.
fn deserialize_server_name<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: Deserializer<'de>,
{
    let val = Option::<Value>::deserialize(deserializer)?;
    let name = match val {
        Some(Value::String(s)) if !s.trim().is_empty() => s,
        Some(Value::Object(map)) => map
            .get("sName")
            .or_else(|| map.get("Name"))
            .and_then(Value::as_str)
            .filter(|s| !s.trim().is_empty())
            .unwrap_or("Artix")
            .to_string(),
        _ => "Artix".to_string(),
    };
    Ok(name)
}

fn default_server_name() -> String {
    "Artix".to_string()
}

fn default_walk_speed() -> i32 {
    8
}

fn default_relog_delay() -> u64 {
    5000
}

fn default_true() -> bool {
    true
}

#[derive(Clone, Debug, Serialize, Deserialize)]
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
    #[serde(deserialize_with = "deserialize_net_list")]
    pub items: Vec<String>,
    pub skill_delay: u64,
    pub bot_delay: u64,
    pub enable_pickup: bool,
    pub enable_rejection: bool,
    pub enable_pickup_all: bool,
    pub enable_pickup_ac_tagged: bool,
    pub auto_relogin: bool,
    pub auto_relogin_any: bool,
    #[serde(
        rename = "Server",
        deserialize_with = "deserialize_server_name",
        default = "default_server_name"
    )]
    pub server: String,
    #[serde(default = "default_relog_delay")]
    pub relog_delay: u64,
    pub relog_retry_upon_failure: bool,
    pub infinite_attack_range: bool,
    pub provoke_monsters: bool,
    pub enemy_magnet: bool,
    pub lag_killer: bool,
    pub hide_players: bool,
    pub skip_cutscenes: bool,
    pub disable_animations: bool,
    #[serde(default = "default_walk_speed")]
    pub walk_speed: i32,
    #[serde(default)]
    pub walk_speed_enabled: bool,
    #[serde(default = "default_true")]
    pub anti_counter: bool,
    pub follow_check: bool,
    pub follow_name: String,
    pub restart_upon_death: bool,
    #[serde(rename = "RestartOnAFK", alias = "RestartOnAfk")]
    pub restart_on_afk: bool,
    pub bank_on_stop: bool,
    #[serde(default = "default_true")]
    pub exit_combat_before_rest: bool,
    pub exit_combat_before_quest: bool,
    #[serde(default = "default_true")]
    pub skip_delay_index_if: bool,
}

impl Default for BotConfig {
    fn default() -> Self {
        Self {
            author: String::new(),
            description: String::new(),
            commands: Vec::new(),
            skills: Vec::new(),
            quests: Vec::new(),
            drops: Vec::new(),
            items: Vec::new(),
            skill_delay: 0,
            bot_delay: 500,
            enable_pickup: true,
            enable_rejection: false,
            enable_pickup_all: false,
            enable_pickup_ac_tagged: false,
            auto_relogin: false,
            auto_relogin_any: false,
            server: default_server_name(),
            relog_delay: default_relog_delay(),
            relog_retry_upon_failure: false,
            infinite_attack_range: false,
            provoke_monsters: false,
            enemy_magnet: false,
            lag_killer: false,
            hide_players: false,
            skip_cutscenes: false,
            disable_animations: false,
            walk_speed: default_walk_speed(),
            walk_speed_enabled: false,
            anti_counter: true,
            follow_check: false,
            follow_name: String::new(),
            restart_upon_death: false,
            restart_on_afk: false,
            bank_on_stop: false,
            exit_combat_before_rest: true,
            exit_combat_before_quest: false,
            skip_delay_index_if: true,
        }
    }
}

impl BotConfig {
    pub fn from_gbot_json(json: &str) -> Result<Self, serde_json::Error> {
        let trimmed = json.trim_start_matches('\u{feff}');
        serde_json::from_str(trimmed)
    }

    /// Serializes into Grimlite-Li compatible `.gbot` JSON (`$type` + `$values`).
    pub fn to_gbot_json(&self) -> Result<String, serde_json::Error> {
        let commands: Vec<Value> = self
            .commands
            .iter()
            .map(BotCommand::to_gbot_value)
            .collect();
        let skills: Vec<Value> = self
            .skills
            .iter()
            .map(|s| {
                serde_json::json!({
                    "$type": "Grimoire.Game.Data.Skill, Grimoire",
                    "Text": s.text,
                    "Index": s.index,
                    "Type": s.skill_type,
                    "SType": s.safe_type,
                    "IsSafeMp": s.is_safe_mp || s.safe_mana,
                    "SafeValue": s.safe_value,
                    "SafeHealth": s.safe_health,
                    "SafeMana": s.safe_mana,
                })
            })
            .collect();
        let quests: Vec<Value> = self
            .quests
            .iter()
            .map(|q| {
                serde_json::json!({
                    "$type": "Grimoire.Game.Data.Quest, Grimoire",
                    "QuestID": q.id,
                    "ItemId": q.item_id,
                    "SafeRelogin": q.safe_relogin,
                })
            })
            .collect();

        let root = serde_json::json!({
            "$type": "Grimoire.Botting.Configuration, Grimoire",
            "Author": self.author,
            "Description": self.description,
            "Commands": {
                "$type": "System.Collections.Generic.List`1[[Grimoire.Botting.IBotCommand, Grimoire]], mscorlib",
                "$values": commands,
            },
            "Skills": {
                "$type": "System.Collections.Generic.List`1[[Grimoire.Game.Data.Skill, Grimoire]], mscorlib",
                "$values": skills,
            },
            "Quests": {
                "$type": "System.Collections.Generic.List`1[[Grimoire.Game.Data.Quest, Grimoire]], mscorlib",
                "$values": quests,
            },
            "Drops": {
                "$type": "System.Collections.Generic.List`1[[System.String, mscorlib]], mscorlib",
                "$values": self.drops,
            },
            "Items": {
                "$type": "System.Collections.Generic.List`1[[System.String, mscorlib]], mscorlib",
                "$values": self.items,
            },
            "SkillDelay": self.skill_delay,
            "BotDelay": self.bot_delay,
            "EnablePickup": self.enable_pickup,
            "EnableRejection": self.enable_rejection,
            "EnablePickupAll": self.enable_pickup_all,
            "EnablePickupAcTagged": self.enable_pickup_ac_tagged,
            "AutoRelogin": self.auto_relogin,
            "AutoReloginAny": self.auto_relogin_any,
            "Server": {
                "$type": "Grimoire.Game.Data.Server, Grimoire",
                "sName": self.server,
            },
            "RelogDelay": self.relog_delay,
            "RelogRetryUponFailure": self.relog_retry_upon_failure,
            "InfiniteAttackRange": self.infinite_attack_range,
            "ProvokeMonsters": self.provoke_monsters,
            "EnemyMagnet": self.enemy_magnet,
            "LagKiller": self.lag_killer,
            "HidePlayers": self.hide_players,
            "SkipCutscenes": self.skip_cutscenes,
            "DisableAnimations": self.disable_animations,
            "WalkSpeed": self.walk_speed,
            "WalkSpeedEnabled": self.walk_speed_enabled,
            "AntiCounter": self.anti_counter,
            "FollowCheck": self.follow_check,
            "FollowName": self.follow_name,
            "RestartUponDeath": self.restart_upon_death,
            "RestartOnAFK": self.restart_on_afk,
            "BankOnStop": self.bank_on_stop,
            "ExitCombatBeforeRest": true,
            "ExitCombatBeforeQuest": false,
            "SkipDelayIndexIf": true,
        });
        serde_json::to_string_pretty(&root)
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase", default)]
pub struct SkillEntry {
    pub text: String,
    pub index: String,
    #[serde(rename = "Type")]
    pub skill_type: i32,
    #[serde(rename = "SType")]
    pub safe_type: i32,
    #[serde(rename = "IsSafeMp")]
    pub is_safe_mp: bool,
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
    InHouse,
    NotInHouse,
    Equipped,
    NotEquipped,
    IsMaxStack,
    IsNotMaxStack,
    ItemPickupable,
    ItemNotPickupable,
    ListInInv,
    MapIs,
    MapIsNot,
    CellIs,
    CellIsNot,
    QuestInProgress,
    QuestNotInProgress,
    QuestCompleted,
    QuestNotCompleted,
    QuestAvailable,
    QuestNotAvailable,
    MonsterInRoom,
    MonsterNotInRoom,
    MonsterHealthGreaterThan,
    MonsterHealthLessThan,
    MonstersGreaterThan,
    MonstersLessThan,
    SpecialAnims,
    HasTarget,
    NotHasTarget,
    InCombat,
    NotInCombat,
    IsMember,
    IsNotMember,
    HealthLessThan,
    HealthGreaterThan,
    TargetHealthGreaterThan,
    TargetHealthLessThan,
    TargetHealthBetween,
    ManaLessThan,
    ManaGreaterThan,
    GoldGreaterThan,
    GoldLessThan,
    LevelGreaterThan,
    LevelLessThan,
    LevelIs,
    FactionRankGreaterThan,
    FactionRankLessThan,
    NameEquals,
    PlayerInRoom,
    PlayerNotInRoom,
    PlayerIsInCell,
    PlayerIsNotInCell,
    PlayerIsInMyCell,
    PlayerIsNotInMyCell,
    PlayersGreaterThan,
    PlayersLessThan,
    PlayerInMyCellGreaterThan,
    PlayerInMyCellLessThan,
    PlayerHPGreaterThan,
    PlayerHPLessThan,
    PlayerAuraEquals,
    PlayerAuraGreaterThan,
    PlayerAuraLessThan,
    TargetAuraEquals,
    TargetAuraGreaterThan,
    TargetAuraLessThan,
    TargetAuraDuration,
    IntIs,
    IntIsNot,
    IntGreaterThan,
    IntLessThan,
    IntEqualInt,
    IntNotEqualInt,
    VarEqualVar,
    VarNotEqualVar,
    UpdateVar,
}

#[derive(Clone, Debug, Serialize)]
pub enum BotCommand {
    // Combat
    Kill {
        monster: String,
    },
    KillFor {
        monster: String,
        item_name: String,
        quantity: String,
        is_temp: bool,
    },
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
    Attack {
        monster: String,
    },
    UseSkill {
        monster: String,
        index: String,
        wait: bool,
        force: bool,
    },
    SkillSet {
        name: String,
    },
    Rest {
        full: bool,
    },
    CancelTarget,

    // Map
    Join {
        map: String,
        cell: String,
        pad: String,
    },
    MoveToCell {
        cell: String,
        pad: String,
    },
    Walk {
        x: i32,
        y: i32,
    },
    SetSpawn,
    Yulgar,

    // Item
    GetDrop {
        item_name: String,
    },
    LoadShop {
        shop_id: i32,
    },
    Buy {
        shop_id: i32,
        item_name: String,
        qty: i32,
    },
    Sell {
        item_name: String,
    },
    Equip {
        item_name: String,
    },
    BankTransfer {
        item_name: String,
        to_bank: bool,
    },
    MapItem {
        item_id: i32,
    },
    Whitelist {
        item_name: String,
        add: bool,
    },

    // Quest
    AcceptQuest {
        quest_id: i32,
    },
    CompleteQuest {
        quest_id: i32,
        item_id: i32,
        qty: i32,
    },
    AddQuestList {
        quest_id: i32,
        item_id: i32,
    },
    RemoveQuestList {
        quest_id: i32,
    },

    // Misc
    Delay {
        ms: u64,
    },
    Label {
        name: String,
    },
    GotoLabel {
        name: String,
    },
    Index {
        index: i32,
        op: IndexOp,
    },
    Packet {
        packet: String,
        to_client: bool,
        spam_times: i32,
        delay_ms: u64,
    },
    Int {
        name: String,
        value: i32,
        op: IntOp,
    },
    SetVar {
        name: String,
        value: String,
    },
    Log {
        text: String,
        debug: bool,
    },
    Provoke {
        enabled: bool,
    },
    SetFPS {
        fps: i32,
    },
    Logout,
    Beep {
        times: i32,
    },
    GotoPlayer {
        player_name: String,
    },
    ClearTemp,
    ClientMessage {
        message: String,
        is_warning: bool,
    },
    LoadMap {
        swf: String,
    },
    SpecialHandler {
        zone: String,
        mode: SpecialMode,
    },
    Stop,
    Restart,
    Blank {
        text: String,
    },

    // Conditional Statement (skips next command if FALSE)
    Statement {
        kind: StatementKind,
        value1: String,
        value2: String,
    },

    // Unrecognized command preserved without failing `.gbot` load
    Unknown {
        type_name: String,
        raw: Value,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum IntOp {
    Set,
    Add,
    Sub,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum IndexOp {
    Up,
    Down,
    Goto,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SpecialMode {
    Start,
    Stop,
    Switch,
}

impl BotCommand {
    pub fn to_gbot_value(&self) -> Value {
        use serde_json::json;
        match self {
            Self::Kill { monster } => json!({
                "$type": "Grimoire.Botting.Commands.Combat.CmdKill, Grimoire",
                "Monster": monster,
            }),
            Self::KillFor {
                monster,
                item_name,
                quantity,
                is_temp,
            } => json!({
                "$type": "Grimoire.Botting.Commands.Combat.CmdKillFor, Grimoire",
                "Monster": monster,
                "ItemName": item_name,
                "Quantity": quantity,
                "ItemType": if *is_temp { 1 } else { 0 },
            }),
            Self::ShortHunt {
                map,
                cell,
                pad,
                monster,
                item_name,
                quantity,
                is_temp,
                delay_after_kill,
            } => json!({
                "$type": "Grimoire.Botting.Commands.Combat.CmdShortHunt, Grimoire",
                "Map": map,
                "Cell": cell,
                "Pad": pad,
                "Monster": monster,
                "ItemName": item_name,
                "Quantity": quantity,
                "ItemType": if *is_temp { 1 } else { 0 },
                "DelayAfterKill": delay_after_kill,
            }),
            Self::QuestHunt {
                map,
                cell,
                pad,
                monster,
                item_name,
                quantity,
                quest_id,
            } => json!({
                "$type": "Grimoire.Botting.Commands.Combat.CmdQuestHunt, Grimoire",
                "Map": map,
                "Cell": cell,
                "Pad": pad,
                "Monster": monster,
                "ItemName": item_name,
                "Quantity": quantity,
                "QID": quest_id,
            }),
            Self::Attack { monster } => json!({
                "$type": "Grimoire.Botting.Commands.Combat.CmdAttack, Grimoire",
                "Monster": monster,
            }),
            Self::UseSkill {
                monster,
                index,
                wait,
                force,
            } => json!({
                "$type": "Grimoire.Botting.Commands.Combat.CmdUseSkill, Grimoire",
                "Monster": monster,
                "Index": index,
                "Skill": index,
                "Wait": wait,
                "Force": force,
            }),
            Self::SkillSet { name } => json!({
                "$type": "Grimoire.Botting.Commands.Combat.CmdSkillSet, Grimoire",
                "Name": name,
            }),
            Self::Rest { full } => json!({
                "$type": "Grimoire.Botting.Commands.Combat.CmdRest, Grimoire",
                "Full": full,
            }),
            Self::CancelTarget => json!({
                "$type": "Grimoire.Botting.Commands.Combat.CmdCancelTarget, Grimoire",
            }),
            Self::Join { map, cell, pad } => json!({
                "$type": "Grimoire.Botting.Commands.Map.CmdJoin, Grimoire",
                "Map": map,
                "Cell": cell,
                "Pad": pad,
            }),
            Self::MoveToCell { cell, pad } => json!({
                "$type": "Grimoire.Botting.Commands.Map.CmdMoveToCell, Grimoire",
                "Cell": cell,
                "Pad": pad,
            }),
            Self::Walk { x, y } => json!({
                "$type": "Grimoire.Botting.Commands.Map.CmdWalk, Grimoire",
                "X": x,
                "Y": y,
            }),
            Self::SetSpawn => json!({
                "$type": "Grimoire.Botting.Commands.Map.CmdSetSpawn, Grimoire",
            }),
            Self::Yulgar => json!({
                "$type": "Grimoire.Botting.Commands.Map.CmdYulgar, Grimoire",
            }),
            Self::GetDrop { item_name } => json!({
                "$type": "Grimoire.Botting.Commands.Item.CmdGetDrop, Grimoire",
                "ItemName": item_name,
            }),
            Self::LoadShop { shop_id } => json!({
                "$type": "Grimoire.Botting.Commands.Item.CmdLoad, Grimoire",
                "ShopId": shop_id,
            }),
            Self::Buy {
                shop_id,
                item_name,
                qty,
            } => json!({
                "$type": "Grimoire.Botting.Commands.Item.CmdBuy, Grimoire",
                "ShopId": shop_id,
                "ItemName": item_name,
                "Qty": qty,
            }),
            Self::Sell { item_name } => json!({
                "$type": "Grimoire.Botting.Commands.Item.CmdSell, Grimoire",
                "ItemName": item_name,
            }),
            Self::Equip { item_name } => json!({
                "$type": "Grimoire.Botting.Commands.Item.CmdEquip, Grimoire",
                "ItemName": item_name,
            }),
            Self::BankTransfer { item_name, to_bank } => json!({
                "$type": "Grimoire.Botting.Commands.Item.CmdBankTransfer, Grimoire",
                "ItemName": item_name,
                "TransferFromBank": !*to_bank,
            }),
            Self::MapItem { item_id } => json!({
                "$type": "Grimoire.Botting.Commands.Item.CmdMapItem, Grimoire",
                "ItemId": item_id,
            }),
            Self::Whitelist { item_name, add } => json!({
                "$type": "Grimoire.Botting.Commands.Item.CmdWhitelist, Grimoire",
                "ItemName": item_name,
                "state": if *add { 0 } else { 1 },
            }),
            Self::AcceptQuest { quest_id } => json!({
                "$type": "Grimoire.Botting.Commands.Quest.CmdAcceptQuest, Grimoire",
                "Quest": {
                    "$type": "Grimoire.Game.Data.Quest, Grimoire",
                    "QuestID": quest_id,
                },
            }),
            Self::CompleteQuest {
                quest_id,
                item_id,
                qty,
            } => json!({
                "$type": "Grimoire.Botting.Commands.Quest.CmdCompleteQuest, Grimoire",
                "Quest": {
                    "$type": "Grimoire.Game.Data.Quest, Grimoire",
                    "QuestID": quest_id,
                    "ItemId": item_id,
                },
                "CompleteTry": qty,
            }),
            Self::AddQuestList { quest_id, item_id } => json!({
                "$type": "Grimoire.Botting.Commands.Quest.CmdAddQuestList, Grimoire",
                "Id": quest_id,
                "ItemId": if *item_id > 0 { Value::String(item_id.to_string()) } else { Value::Null },
            }),
            Self::RemoveQuestList { quest_id } => json!({
                "$type": "Grimoire.Botting.Commands.Quest.CmdRemoveQuestList, Grimoire",
                "Id": quest_id,
            }),
            Self::Delay { ms } => json!({
                "$type": "Grimoire.Botting.Commands.Misc.CmdDelay, Grimoire",
                "Delay": ms,
            }),
            Self::Label { name } => json!({
                "$type": "Grimoire.Botting.Commands.Misc.CmdLabel, Grimoire",
                "Name": name,
            }),
            Self::GotoLabel { name } => json!({
                "$type": "Grimoire.Botting.Commands.Misc.CmdGotoLabel, Grimoire",
                "Label": name,
            }),
            Self::Index { index, op } => json!({
                "$type": "Grimoire.Botting.Commands.Misc.CmdIndex, Grimoire",
                "Index": index,
                "Type": match op { IndexOp::Up => 0, IndexOp::Down => 1, IndexOp::Goto => 2 },
            }),
            Self::Packet {
                packet,
                to_client,
                spam_times,
                delay_ms,
            } => json!({
                "$type": "Grimoire.Botting.Commands.Misc.CmdPacket, Grimoire",
                "Packet": packet,
                "Client": to_client,
                "SpamTimes": spam_times,
                "Delay": delay_ms,
            }),
            Self::Int { name, value, op } => json!({
                "$type": "Grimoire.Botting.Commands.Misc.CmdInt, Grimoire",
                "Int": name,
                "Value": value,
                "type": match op { IntOp::Set => 0, IntOp::Add => 1, IntOp::Sub => 2 },
            }),
            Self::SetVar { name, value } => json!({
                "$type": "Grimoire.Botting.Commands.Misc.Statements.CmdSetVar, Grimoire",
                "Value1": name,
                "Value2": value,
            }),
            Self::Log { text, debug } => json!({
                "$type": "Grimoire.Botting.Commands.Misc.CmdLog, Grimoire",
                "Text": text,
                "Debug": debug,
            }),
            Self::Provoke { enabled } => json!({
                "$type": "Grimoire.Botting.Commands.Misc.CmdProvoke, Grimoire",
                "Set": enabled,
            }),
            Self::SetFPS { fps } => json!({
                "$type": "Grimoire.Botting.Commands.Misc.CmdSetFPS, Grimoire",
                "FPS": fps,
            }),
            Self::Logout => json!({
                "$type": "Grimoire.Botting.Commands.Misc.CmdLogout, Grimoire",
            }),
            Self::Beep { times } => json!({
                "$type": "Grimoire.Botting.Commands.Misc.CmdBeep, Grimoire",
                "Times": times,
            }),
            Self::GotoPlayer { player_name } => json!({
                "$type": "Grimoire.Botting.Commands.Misc.CmdGotoPlayer, Grimoire",
                "PlayerName": player_name,
            }),
            Self::ClearTemp => json!({
                "$type": "Grimoire.Botting.Commands.Misc.CmdClearTemp, Grimoire",
            }),
            Self::ClientMessage {
                message,
                is_warning,
            } => json!({
                "$type": "Grimoire.Botting.Commands.Misc.CmdClientMessage, Grimoire",
                "Messages": message,
                "IsWarning": is_warning,
            }),
            Self::LoadMap { swf } => json!({
                "$type": "Grimoire.Botting.Commands.Misc.CmdLoadMap, Grimoire",
                "Swf": swf,
            }),
            Self::SpecialHandler { zone, mode } => json!({
                "$type": "Grimoire.Botting.Commands.Misc.CmdSpecialHandler, Grimoire",
                "Zone": zone,
                "Mode": match mode { SpecialMode::Start => 0, SpecialMode::Stop => 1, SpecialMode::Switch => 2 },
            }),
            Self::Stop => json!({
                "$type": "Grimoire.Botting.Commands.Misc.CmdStop, Grimoire",
            }),
            Self::Restart => json!({
                "$type": "Grimoire.Botting.Commands.Misc.CmdRestart, Grimoire",
            }),
            Self::Blank { text } => json!({
                "$type": "Grimoire.Botting.Commands.Misc.CmdBlank3, Grimoire",
                "Text": text,
            }),
            Self::Statement {
                kind,
                value1,
                value2,
            } => json!({
                "$type": format!("Grimoire.Botting.Commands.Misc.Statements.Cmd{kind:?}, Grimoire"),
                "Value1": value1,
                "Value2": value2,
            }),
            Self::Unknown { raw, .. } => raw.clone(),
        }
    }
}

impl fmt::Display for BotCommand {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Kill { monster } => write!(f, "Kill {monster}"),
            Self::KillFor {
                monster,
                item_name,
                quantity,
                is_temp,
            } => {
                let kind = if *is_temp { "temp" } else { "item" };
                write!(f, "Kill for {kind}: {monster} -> {item_name} x{quantity}")
            }
            Self::ShortHunt {
                map,
                monster,
                item_name,
                quantity,
                ..
            } => {
                write!(
                    f,
                    "Short hunt [{map}]: {monster} -> {item_name} x{quantity}"
                )
            }
            Self::QuestHunt {
                map,
                monster,
                item_name,
                quantity,
                quest_id,
                ..
            } => {
                write!(
                    f,
                    "Quest hunt [{map}] (Q:{quest_id}): {monster} -> {item_name} x{quantity}"
                )
            }
            Self::Attack { monster } => write!(f, "Attack {monster}"),
            Self::UseSkill {
                monster,
                index,
                wait,
                force,
            } => {
                let w = if *wait { " [Wait]" } else { "" };
                let fc = if *force { " [Force]" } else { "" };
                write!(f, "Skill [{monster}]{w}{fc} {index}")
            }
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
            Self::Buy {
                shop_id,
                item_name,
                qty,
            } => {
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
                write!(
                    f,
                    "{} whitelist: {item_name}",
                    if *add { "Add" } else { "Remove" }
                )
            }
            Self::AcceptQuest { quest_id } => write!(f, "Accept quest {quest_id}"),
            Self::CompleteQuest {
                quest_id,
                item_id,
                qty,
            } => {
                let q_str = if *qty < 0 {
                    "Max".to_string()
                } else {
                    format!("{qty}x")
                };
                if *item_id > 0 {
                    write!(f, "Complete quest [{q_str}]: {quest_id} (item {item_id})")
                } else {
                    write!(f, "Complete quest [{q_str}]: {quest_id}")
                }
            }
            Self::AddQuestList { quest_id, item_id } => {
                if *item_id > 0 {
                    write!(f, "Add quest list: {quest_id} (item {item_id})")
                } else {
                    write!(f, "Add quest list: {quest_id}")
                }
            }
            Self::RemoveQuestList { quest_id } => {
                write!(f, "Remove quest list: {quest_id}")
            }
            Self::Delay { ms } => write!(f, "Delay {ms} ms"),
            Self::Label { name } => write!(f, "[{name}]"),
            Self::GotoLabel { name } => write!(f, "Goto label [{name}]"),
            Self::Index { index, op } => match op {
                IndexOp::Up => write!(f, "Index up: {index}"),
                IndexOp::Down => write!(f, "Index down: {index}"),
                IndexOp::Goto => write!(f, "Goto index: {index}"),
            },
            Self::Packet {
                packet,
                to_client,
                spam_times,
                ..
            } => {
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
            Self::Log { text, debug } => {
                let kind = if *debug { "Debug" } else { "Script" };
                let action = if text.contains("{CLEAR}") {
                    "Clear"
                } else {
                    "Log"
                };
                write!(f, "{action} {kind}: {text}")
            }
            Self::Provoke { enabled } => {
                write!(f, "Provoke {}", if *enabled { "ON" } else { "OFF" })
            }
            Self::SetFPS { fps } => write!(f, "Set FPS: {fps}"),
            Self::Logout => write!(f, "Logout"),
            Self::Beep { times } => write!(f, "Play sound ({times}x)"),
            Self::GotoPlayer { player_name } => write!(f, "Goto player: {player_name}"),
            Self::ClearTemp => write!(f, "Clear TempVar"),
            Self::ClientMessage {
                message,
                is_warning,
            } => {
                let kind = if *is_warning { "warning" } else { "info" };
                write!(f, "Send {kind}: {message}")
            }
            Self::LoadMap { swf } => write!(f, "Load map: {swf}"),
            Self::SpecialHandler { zone, mode } => {
                let prefix = match mode {
                    SpecialMode::Start => "Special Start",
                    SpecialMode::Stop => "Special Stop",
                    SpecialMode::Switch => "Special Switch",
                };
                write!(f, "{prefix}: {zone}")
            }
            Self::Stop => write!(f, "Stop bot"),
            Self::Restart => write!(f, "Restart bot"),
            Self::Blank { text } => write!(f, "{text}"),
            Self::Statement {
                kind,
                value1,
                value2,
            } => {
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
                monster: {
                    let m = str_field(&v, &["Monster"]);
                    if m.is_empty() { "*".to_string() } else { m }
                },
                index: str_field(&v, &["Index", "Skill"]),
                wait: bool_field(&v, &["Wait"], false),
                force: bool_field(&v, &["Force"], false),
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
                add: matches!(int_field(&v, &["state"], 3), 0 | 3),
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
                let qty = int_field(&v, &["CompleteTry"], 1);
                Self::CompleteQuest {
                    quest_id: id,
                    item_id,
                    qty: if qty == 0 { 1 } else { qty },
                }
            }
            "CmdAddQuestList" => Self::AddQuestList {
                quest_id: int_field(&v, &["Id", "QuestID", "QuestId"], 0),
                item_id: int_field(&v, &["ItemId", "ItemID"], -1),
            },
            "CmdRemoveQuestList" => Self::RemoveQuestList {
                quest_id: int_field(&v, &["Id", "QuestID", "QuestId"], 0),
            },
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
                op: match int_field(&v, &["Type"], 0) {
                    1 => IndexOp::Down,
                    2 => IndexOp::Goto,
                    _ => IndexOp::Up,
                },
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
                let raw_val = int_field(&v, &["Value"], 0);
                let value = if matches!(op, IntOp::Add | IntOp::Sub) && raw_val == 0 {
                    1
                } else {
                    raw_val
                };
                Self::Int {
                    name: str_field(&v, &["Int"]),
                    value,
                    op,
                }
            }
            "CmdSetVar" => Self::SetVar {
                name: str_field(&v, &["Value1"]),
                value: str_field(&v, &["Value2"]),
            },
            "CmdLog" => Self::Log {
                text: str_field(&v, &["Text"]),
                debug: bool_field(&v, &["Debug"], false),
            },
            "CmdProvoke" | "CmdToggleProvoke" => Self::Provoke {
                enabled: bool_field(&v, &["Set"], true),
            },
            "CmdSetFPS" => Self::SetFPS {
                fps: int_field(&v, &["FPS"], 30),
            },
            "CmdLogout" => Self::Logout,
            "CmdBeep" => Self::Beep {
                times: int_field(&v, &["Times"], 10).max(1),
            },
            "CmdGotoPlayer" => Self::GotoPlayer {
                player_name: str_field(&v, &["PlayerName"]),
            },
            "CmdClearTemp" => Self::ClearTemp,
            "CmdClientMessage" => Self::ClientMessage {
                message: str_field(&v, &["Messages", "Message"]),
                is_warning: bool_field(&v, &["IsWarning"], false),
            },
            "CmdLoadMap" => Self::LoadMap {
                swf: str_field(&v, &["Swf", "Map", "File"]),
            },
            "CmdSpecialHandler" => Self::SpecialHandler {
                zone: str_field(&v, &["Zone"]),
                mode: match int_field(&v, &["Mode"], 0) {
                    1 => SpecialMode::Stop,
                    2 => SpecialMode::Switch,
                    _ => SpecialMode::Start,
                },
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
        "CmdInHouse" => S::InHouse,
        "CmdNotInHouse" => S::NotInHouse,
        "CmdEquipped" => S::Equipped,
        "CmdNotEquipped" => S::NotEquipped,
        "CmdIsMaxStack" => S::IsMaxStack,
        "CmdIsNotMaxStack" => S::IsNotMaxStack,
        "CmdItemPickupable" => S::ItemPickupable,
        "CmdItemNotPickupable" => S::ItemNotPickupable,
        "CmdListInInv" => S::ListInInv,
        "CmdMapIs" => S::MapIs,
        "CmdMapIsNot" => S::MapIsNot,
        "CmdCellIs" => S::CellIs,
        "CmdCellIsNot" => S::CellIsNot,
        "CmdQuestInProgress" => S::QuestInProgress,
        "CmdQuestNotInProgress" => S::QuestNotInProgress,
        "CmdQuestCompleted" => S::QuestCompleted,
        "CmdQuestNotCompleted" => S::QuestNotCompleted,
        "CmdQuestAvailable" => S::QuestAvailable,
        "CmdQuestNotAvailable" => S::QuestNotAvailable,
        "CmdMonsterInRoom" => S::MonsterInRoom,
        "CmdMonsterNotInRoom" => S::MonsterNotInRoom,
        "CmdMonsterHealthGreaterThan" => S::MonsterHealthGreaterThan,
        "CmdMonsterHealthLessThan" => S::MonsterHealthLessThan,
        "CmdMonstersGreaterThan" => S::MonstersGreaterThan,
        "CmdMonstersLessThan" => S::MonstersLessThan,
        "CmdSpecialAnims" => S::SpecialAnims,
        "CmdHasTarget" => S::HasTarget,
        "CmdNotHasTarget" => S::NotHasTarget,
        "CmdInCombat" => S::InCombat,
        "CmdNotInCombat" => S::NotInCombat,
        "CmdIsMember" => S::IsMember,
        "CmdIsNotMember" => S::IsNotMember,
        "CmdHealthLessThan" => S::HealthLessThan,
        "CmdHealthGreaterThan" => S::HealthGreaterThan,
        "CmdTargetHealthGreaterThan" => S::TargetHealthGreaterThan,
        "CmdTargetHealthLessThan" => S::TargetHealthLessThan,
        "CmdTargetHealthBetween" => S::TargetHealthBetween,
        "CmdManaLessThan" => S::ManaLessThan,
        "CmdManaGreaterThan" => S::ManaGreaterThan,
        "CmdGoldGreaterThan" => S::GoldGreaterThan,
        "CmdGoldLessThan" => S::GoldLessThan,
        "CmdLevelGreaterThan" => S::LevelGreaterThan,
        "CmdLevelLessThan" => S::LevelLessThan,
        "CmdLevelIs" => S::LevelIs,
        "CmdFactionRankGreaterThan" => S::FactionRankGreaterThan,
        "CmdFactionRankLessThan" => S::FactionRankLessThan,
        "CmdNameEquals" => S::NameEquals,
        "CmdPlayerInRoom" => S::PlayerInRoom,
        "CmdPlayerNotInRoom" => S::PlayerNotInRoom,
        "CmdPlayerIsInCell" => S::PlayerIsInCell,
        "CmdPlayerIsNotInCell" => S::PlayerIsNotInCell,
        "CmdPlayerIsInMyCell" => S::PlayerIsInMyCell,
        "CmdPlayerIsNotInMyCell" => S::PlayerIsNotInMyCell,
        "CmdPlayersGreaterThan" => S::PlayersGreaterThan,
        "CmdPlayersLessThan" => S::PlayersLessThan,
        "CmdPlayerInMyCellGreaterThan" => S::PlayerInMyCellGreaterThan,
        "CmdPlayerInMyCellLessThan" => S::PlayerInMyCellLessThan,
        "CmdPlayerHPGreaterThan" | "CmdPlayersHPGreaterThan" => S::PlayerHPGreaterThan,
        "CmdPlayerHPLessThan" | "CmdPlayersHPLessThan" => S::PlayerHPLessThan,
        "CmdPlayerAuraEquals" => S::PlayerAuraEquals,
        "CmdPlayerAuraGreaterThan" => S::PlayerAuraGreaterThan,
        "CmdPlayerAuraLessThan" => S::PlayerAuraLessThan,
        "CmdTargetAuraEquals" => S::TargetAuraEquals,
        "CmdTargetAuraGreaterThan" => S::TargetAuraGreaterThan,
        "CmdTargetAuraLessThan" => S::TargetAuraLessThan,
        "CmdTargetAuraDuration" => S::TargetAuraDuration,
        "CmdIntIs" => S::IntIs,
        "CmdIntIsNot" => S::IntIsNot,
        "CmdIntGreaterThan" => S::IntGreaterThan,
        "CmdIntLessThan" => S::IntLessThan,
        "CmdIntEqualInt" => S::IntEqualInt,
        "CmdIntNotEqualInt" => S::IntNotEqualInt,
        "CmdVarEqualVar" => S::VarEqualVar,
        "CmdVarNotEqualVar" => S::VarNotEqualVar,
        "CmdUpdateVar" => S::UpdateVar,
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

        // Round-trip serialize -> deserialize
        let exported = cfg.to_gbot_json().expect("serialize .gbot");
        let reparsed = BotConfig::from_gbot_json(&exported).expect("reparse .gbot");
        assert_eq!(reparsed.commands.len(), 5);
        assert_eq!(reparsed.drops, vec!["Frogzard Tooth"]);
    }

    #[test]
    fn parses_equipped_statements() {
        let sample = r#"{
            "$type": "Grimoire.Botting.Configuration, Grimoire",
            "Commands": {
                "$type": "System.Collections.Generic.List`1[[Grimoire.Botting.IBotCommand, Grimoire]], mscorlib",
                "$values": [
                    { "$type": "Grimoire.Botting.Commands.Misc.Statements.CmdEquipped, Grimoire", "Value1": "Chaos Avenger", "Value2": "" },
                    { "$type": "Grimoire.Botting.Commands.Misc.Statements.CmdNotEquipped, Grimoire", "Value1": "Scroll of Life Steal", "Value2": "" }
                ]
            }
        }"#;
        let cfg = BotConfig::from_gbot_json(sample).expect("parse equipped statements");
        assert_eq!(cfg.commands.len(), 2);
        assert!(matches!(
            &cfg.commands[0],
            BotCommand::Statement {
                kind: StatementKind::Equipped,
                value1,
                ..
            } if value1 == "Chaos Avenger"
        ));
        assert!(matches!(
            &cfg.commands[1],
            BotCommand::Statement {
                kind: StatementKind::NotEquipped,
                value1,
                ..
            } if value1 == "Scroll of Life Steal"
        ));
    }

    #[test]
    fn parses_all_extended_statements() {
        let names = [
            "CmdInHouse", "CmdNotInHouse", "CmdIsMaxStack", "CmdIsNotMaxStack",
            "CmdItemPickupable", "CmdItemNotPickupable", "CmdListInInv",
            "CmdQuestAvailable", "CmdQuestNotAvailable",
            "CmdMonsterHealthGreaterThan", "CmdMonsterHealthLessThan",
            "CmdMonstersGreaterThan", "CmdMonstersLessThan", "CmdSpecialAnims",
            "CmdInCombat", "CmdNotInCombat", "CmdIsMember", "CmdIsNotMember",
            "CmdTargetHealthGreaterThan", "CmdTargetHealthLessThan", "CmdTargetHealthBetween",
            "CmdLevelIs", "CmdFactionRankGreaterThan", "CmdFactionRankLessThan",
            "CmdPlayerInRoom", "CmdPlayerNotInRoom",
            "CmdPlayerIsInCell", "CmdPlayerIsNotInCell",
            "CmdPlayerIsInMyCell", "CmdPlayerIsNotInMyCell",
            "CmdPlayersGreaterThan", "CmdPlayersLessThan",
            "CmdPlayerInMyCellGreaterThan", "CmdPlayerInMyCellLessThan",
            "CmdPlayerHPGreaterThan", "CmdPlayerHPLessThan",
            "CmdPlayerAuraEquals", "CmdPlayerAuraGreaterThan", "CmdPlayerAuraLessThan",
            "CmdTargetAuraEquals", "CmdTargetAuraGreaterThan", "CmdTargetAuraLessThan",
            "CmdTargetAuraDuration", "CmdUpdateVar",
        ];
        for name in names {
            assert!(
                parse_statement_kind(name).is_some(),
                "missing parser mapping for {name}"
            );
        }
    }
}
