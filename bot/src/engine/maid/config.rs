//! Maid Remake configuration (`MaidConfig` JSON compatible with Grimlite-Li),
//! class presets (`ClassPreset.cs`), Ultra Boss extra modes, and map list parsers.

use serde::{Deserialize, Serialize};

/// Exact JSON-compatible config matching `Grimlite-Li/Grimoire/Tools/Maid/MaidConfig.cs`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct MaidConfig {
    #[serde(rename = "Target")]
    pub target: String,
    #[serde(rename = "SkillList")]
    pub skill_list: String,
    #[serde(rename = "LockedZoneHandlerMaps")]
    pub locked_zone_handler_maps: String,
    #[serde(rename = "SafeSkillList")]
    pub safe_skill_list: String,
    #[serde(rename = "BuffStopAttackList")]
    pub buff_stop_attack_list: String,
    #[serde(rename = "AttackPriorityMonster")]
    pub attack_priority_monster: String,
    #[serde(rename = "SpecialMsg")]
    pub special_msg: String,
    #[serde(rename = "WhitelistMapMaps")]
    pub whitelist_map_maps: String,

    #[serde(rename = "RelogDelay")]
    pub relog_delay: u32,
    #[serde(rename = "SkillDelay")]
    pub skill_delay: u32,
    #[serde(rename = "SafeSkillHP")]
    pub safe_skill_hp: u32,
    #[serde(rename = "SpecialAct")]
    pub special_act: u32,
    #[serde(rename = "UltraBossExtra")]
    pub ultra_boss_extra: usize,

    #[serde(rename = "AntiCounter")]
    pub anti_counter: bool,
    #[serde(rename = "CopyWalk")]
    pub copy_walk: bool,
    #[serde(rename = "AttackPriority")]
    pub attack_priority: bool,
    #[serde(rename = "BuffStopAttack")]
    pub buff_stop_attack: bool,
    #[serde(rename = "WaitSkill")]
    pub wait_skill: bool,
    #[serde(rename = "StopFailedGoto")]
    pub stop_failed_goto: bool,
    #[serde(rename = "LockedZoneHandler")]
    pub locked_zone_handler: bool,
    #[serde(rename = "WhitelistMap")]
    pub whitelist_map: bool,
    #[serde(rename = "GlobalHotkey")]
    pub global_hotkey: bool,
    #[serde(rename = "SafeSkill")]
    pub safe_skill: bool,

    // Extended Grimlite-Ri fields (defaulted when loading legacy Grimlite-Li JSONs)
    #[serde(rename = "StopFailedGotoLimit")]
    pub stop_failed_goto_limit: u32,
    #[serde(rename = "SpecialAnims")]
    pub special_anims: bool,
    #[serde(rename = "PartyCmd")]
    pub party_cmd: bool,
    #[serde(rename = "ClassPreset")]
    pub class_preset: MaidPreset,
}

impl Default for MaidConfig {
    fn default() -> Self {
        Self {
            target: String::new(),
            skill_list: "1,2,3,4".to_string(),
            locked_zone_handler_maps: "whitemap-100000;Enter;Spawn".to_string(),
            safe_skill_list: String::new(),
            buff_stop_attack_list: String::new(),
            attack_priority_monster: String::new(),
            special_msg: String::new(),
            whitelist_map_maps: String::new(),
            relog_delay: 5000,
            skill_delay: 200,
            safe_skill_hp: 60,
            special_act: 5,
            ultra_boss_extra: 0,
            anti_counter: true,
            copy_walk: false,
            attack_priority: false,
            buff_stop_attack: false,
            wait_skill: false,
            stop_failed_goto: false,
            locked_zone_handler: false,
            whitelist_map: false,
            global_hotkey: true,
            safe_skill: false,
            stop_failed_goto_limit: 5,
            special_anims: false,
            party_cmd: true,
            class_preset: MaidPreset::None,
        }
    }
}

impl MaidConfig {
    pub fn from_json(raw: &str) -> Result<Self, String> {
        let mut cfg = serde_json::from_str::<Self>(raw)
            .map_err(|e| format!("Invalid MaidConfig JSON: {e}"))?;
        if !cfg.special_msg.trim().is_empty() {
            cfg.special_anims = true;
        }
        if cfg.stop_failed_goto_limit == 0 {
            cfg.stop_failed_goto_limit = 5;
        }
        Ok(cfg)
    }

    pub fn to_json_pretty(&self) -> Result<String, String> {
        serde_json::to_string_pretty(self).map_err(|e| format!("Serialize MaidConfig failed: {e}"))
    }

    pub fn ultra_boss_mode(&self) -> UltraBossMode {
        UltraBossMode::from_index(self.ultra_boss_extra)
    }

    pub fn set_ultra_boss_mode(&mut self, mode: UltraBossMode) {
        self.ultra_boss_extra = mode.as_index();
    }

    pub fn parsed_skills(&self) -> Vec<String> {
        split_csv(&self.skill_list)
    }

    pub fn parsed_safe_skills(&self) -> Vec<String> {
        split_csv(&self.safe_skill_list)
    }

    pub fn parsed_buff_skills(&self) -> Vec<String> {
        split_csv(&self.buff_stop_attack_list)
    }

    pub fn parsed_attack_priority(&self) -> Vec<String> {
        split_csv(&self.attack_priority_monster)
    }

    pub fn parsed_special_msgs(&self) -> Vec<String> {
        split_csv(&self.special_msg)
    }
}

fn split_csv(s: &str) -> Vec<String> {
    s.split(',')
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(ToString::to_string)
        .collect()
}

/// Class presets mirroring `Grimlite-Li/Grimoire/Tools/Maid/ClassPreset.cs`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum MaidPreset {
    #[default]
    None,
    EC,
    DOT,
    LC,
    LOO,
    SC,
    AP,
    CCMD,
    SSOT,
    NCM,
    TK,
    AI,
    AM,
    CSH,
    CSHv2,
}

impl MaidPreset {
    pub const ALL: &'static [Self] = &[
        Self::None,
        Self::EC,
        Self::DOT,
        Self::LC,
        Self::LOO,
        Self::SC,
        Self::AP,
        Self::CCMD,
        Self::SSOT,
        Self::NCM,
        Self::TK,
        Self::AI,
        Self::AM,
        Self::CSH,
        Self::CSHv2,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::None => "None",
            Self::EC => "EC (Eternal Chronomancer)",
            Self::DOT => "DOT (Dragon of Time)",
            Self::LC => "LC (LightCaster)",
            Self::LOO => "LOO (Lord of Order)",
            Self::SC => "SC (StoneCrusher)",
            Self::AP => "AP (ArchPaladin)",
            Self::CCMD => "CCMD (Chrono Commandant)",
            Self::SSOT => "SSOT (ShadowStalker of Time)",
            Self::NCM => "NCM (NeChronomancer)",
            Self::TK => "TK (TimeKeeper)",
            Self::AI => "AI (Arcana Invoker)",
            Self::AM => "AM (ArchMage)",
            Self::CSH => "CSH (Gunslinger)",
            Self::CSHv2 => "CSH v2 (Normal)",
        }
    }

    pub fn short_name(self) -> &'static str {
        match self {
            Self::None => "None",
            Self::EC => "EC",
            Self::DOT => "DOT",
            Self::LC => "LC",
            Self::LOO => "LOO",
            Self::SC => "SC",
            Self::AP => "AP",
            Self::CCMD => "CCMD",
            Self::SSOT => "SSOT",
            Self::NCM => "NCM",
            Self::TK => "TK",
            Self::AI => "AI",
            Self::AM => "AM",
            Self::CSH => "CSH",
            Self::CSHv2 => "CSH v2",
        }
    }

    /// Applies this preset's exact Grimlite-Li skill/heal/buff/wait settings onto `cfg`.
    pub fn apply_to(self, cfg: &mut MaidConfig) {
        cfg.class_preset = self;
        match self {
            Self::None => return,
            Self::EC => {
                cfg.skill_list = "4,3,2,1,2,1,2,1,2".into();
                cfg.safe_skill_list.clear();
                cfg.safe_skill_hp = 60;
                cfg.buff_stop_attack_list = "3".into();
                cfg.wait_skill = true;
                cfg.safe_skill = false;
                cfg.buff_stop_attack = true;
            }
            Self::LC => {
                cfg.skill_list = "1,2,4,1,2,4,3".into();
                cfg.safe_skill_list = "3".into();
                cfg.safe_skill_hp = 60;
                cfg.buff_stop_attack_list.clear();
                cfg.wait_skill = false;
                cfg.safe_skill = true;
                cfg.buff_stop_attack = false;
            }
            Self::LOO => {
                cfg.skill_list = "2,1,3,4".into();
                cfg.safe_skill_list.clear();
                cfg.safe_skill_hp = 60;
                cfg.buff_stop_attack_list = "1,2,3".into();
                cfg.wait_skill = false;
                cfg.safe_skill = false;
                cfg.buff_stop_attack = true;
            }
            Self::AI => {
                cfg.skill_list = "2,3,4".into();
                cfg.safe_skill_list.clear();
                cfg.buff_stop_attack_list = "5".into();
                cfg.wait_skill = false;
                cfg.safe_skill = false;
                cfg.buff_stop_attack = true;
            }
            Self::AM => {
                cfg.skill_list = "2,3,1".into();
                cfg.safe_skill_list.clear();
                cfg.buff_stop_attack_list = "5".into();
                cfg.wait_skill = false;
                cfg.safe_skill = false;
                cfg.buff_stop_attack = true;
            }
            Self::CSH => {
                cfg.skill_list = "3,2,3,3,0,3,3,4,1".into();
                cfg.safe_skill_list.clear();
                cfg.safe_skill_hp = 80;
                cfg.buff_stop_attack_list = "5".into();
                cfg.wait_skill = true;
                cfg.safe_skill = false;
                cfg.buff_stop_attack = true;
            }
            Self::CSHv2 => {
                cfg.skill_list = "3,2,3,2,3,4,1".into();
                cfg.safe_skill_list.clear();
                cfg.safe_skill_hp = 80;
                cfg.buff_stop_attack_list = "5".into();
                cfg.wait_skill = true;
                cfg.safe_skill = false;
                cfg.buff_stop_attack = true;
            }
            Self::SC => {
                cfg.skill_list = "3,1,2,4".into();
                cfg.safe_skill_list.clear();
                cfg.safe_skill_hp = 60;
                cfg.buff_stop_attack_list = "2,3".into();
                cfg.wait_skill = false;
                cfg.safe_skill = false;
                cfg.buff_stop_attack = true;
            }
            Self::AP => {
                cfg.skill_list = "1,2,3".into();
                cfg.safe_skill_list = "2".into();
                cfg.safe_skill_hp = 60;
                cfg.buff_stop_attack_list = "2".into();
                cfg.wait_skill = false;
                cfg.safe_skill = true;
                cfg.buff_stop_attack = true;
            }
            Self::CCMD => {
                cfg.skill_list = "1,1,1,1,3".into();
                cfg.safe_skill_list = "2".into();
                cfg.safe_skill_hp = 75;
                cfg.buff_stop_attack_list.clear();
                cfg.wait_skill = true;
                cfg.safe_skill = true;
                cfg.buff_stop_attack = false;
            }
            Self::DOT => {
                cfg.skill_list = "3,2,1,2,4,2".into();
                cfg.safe_skill_list.clear();
                cfg.safe_skill_hp = 75;
                cfg.buff_stop_attack_list.clear();
                cfg.wait_skill = true;
                cfg.safe_skill = false;
                cfg.buff_stop_attack = false;
            }
            Self::SSOT => {
                cfg.skill_list = "4,2,3,1,2".into();
                cfg.safe_skill_list.clear();
                cfg.safe_skill_hp = 60;
                cfg.buff_stop_attack_list.clear();
                cfg.wait_skill = true;
                cfg.safe_skill = false;
                cfg.buff_stop_attack = false;
            }
            Self::NCM => {
                cfg.skill_list = "1,2,4,1,2,1,3".into();
                cfg.safe_skill_list.clear();
                cfg.safe_skill_hp = 60;
                cfg.buff_stop_attack_list.clear();
                cfg.wait_skill = true;
                cfg.safe_skill = false;
                cfg.buff_stop_attack = false;
            }
            Self::TK => {
                cfg.skill_list = "1,2,1,4,1,3".into();
                cfg.safe_skill_list.clear();
                cfg.safe_skill_hp = 60;
                cfg.buff_stop_attack_list.clear();
                cfg.wait_skill = true;
                cfg.safe_skill = false;
                cfg.buff_stop_attack = false;
            }
        }
    }
}

/// Ultra Boss Extra modes matching `cmbUltraBoss` indices in Grimlite-Li `MaidRemake.cs`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum UltraBossMode {
    #[default]
    None = 0,
    AutoPotion = 1,
    AscSolsticeP1 = 2,
    AscSolsticeP2 = 3,
    AscMidnightP1 = 4,
    AscMidnightP2 = 5,
    AstEmpyreanP1 = 6,
    AstEmpyreanP2 = 7,
    GramielL1 = 8,
    GramielL2 = 9,
    GramielR1 = 10,
    GramielR2 = 11,
}

impl UltraBossMode {
    pub const ALL: &'static [Self] = &[
        Self::None,
        Self::AutoPotion,
        Self::AscSolsticeP1,
        Self::AscSolsticeP2,
        Self::AscMidnightP1,
        Self::AscMidnightP2,
        Self::AstEmpyreanP1,
        Self::AstEmpyreanP2,
        Self::GramielL1,
        Self::GramielL2,
        Self::GramielR1,
        Self::GramielR2,
    ];

    /// Dropdown display order with `Auto Potion` placed at the bottom (preserves Grimlite-Li JSON indices).
    pub const DISPLAY_ORDER: &'static [Self] = &[
        Self::None,
        Self::AscSolsticeP1,
        Self::AscSolsticeP2,
        Self::AscMidnightP1,
        Self::AscMidnightP2,
        Self::AstEmpyreanP1,
        Self::AstEmpyreanP2,
        Self::GramielL1,
        Self::GramielL2,
        Self::GramielR1,
        Self::GramielR2,
        Self::AutoPotion,
    ];

    pub fn from_index(idx: usize) -> Self {
        Self::ALL.get(idx).copied().unwrap_or(Self::None)
    }

    pub fn as_index(self) -> usize {
        self as usize
    }

    pub fn display_name(self) -> &'static str {
        match self {
            Self::None => "None",
            Self::AutoPotion => "Auto Potion",
            Self::AscSolsticeP1 => "Asc.Solstice P1",
            Self::AscSolsticeP2 => "Asc.Solstice P2",
            Self::AscMidnightP1 => "Asc.Midnight P1",
            Self::AscMidnightP2 => "Asc.Midnight P2",
            Self::AstEmpyreanP1 => "Ast.Empyrean P1",
            Self::AstEmpyreanP2 => "Ast.Empyrean P2",
            Self::GramielL1 => "Gramiel L1",
            Self::GramielL2 => "Gramiel L2",
            Self::GramielR1 => "Gramiel R1",
            Self::GramielR2 => "Gramiel R2",
        }
    }

    /// True when Maid should auto-equip `Scroll of Enrage` (matches `MaidRemake.cs`).
    pub fn uses_enrage(self) -> bool {
        !matches!(self, Self::None | Self::AutoPotion)
    }

    pub fn is_gramiel(self) -> bool {
        matches!(
            self,
            Self::GramielL1 | Self::GramielL2 | Self::GramielR1 | Self::GramielR2
        )
    }

    /// Returns `(my_crystal_id, other_crystal_id)` for Gramiel crystal HP balancing.
    pub fn gramiel_crystal_pair(self) -> Option<(&'static str, &'static str)> {
        match self {
            Self::GramielL1 | Self::GramielL2 => Some(("2", "3")),
            Self::GramielR1 | Self::GramielR2 => Some(("3", "2")),
            _ => None,
        }
    }
}

/// Parsed entry from `LockedZoneHandlerMaps` (`map-room;cell;pad`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LockedMapEntry {
    pub map: String,
    pub cell: String,
    pub pad: String,
}

pub fn parse_locked_map_entries(raw: &str) -> Vec<LockedMapEntry> {
    raw.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(|line| {
            let mut parts = line.split(';').map(str::trim);
            let map = parts.next().unwrap_or_default().to_string();
            let cell = parts
                .next()
                .filter(|s| !s.is_empty())
                .unwrap_or("Enter")
                .to_string();
            let pad = parts
                .next()
                .filter(|s| !s.is_empty())
                .unwrap_or("Spawn")
                .to_string();
            LockedMapEntry { map, cell, pad }
        })
        .filter(|e| !e.map.is_empty())
        .collect()
}

/// Parsed entry from `WhitelistMapMaps` (`map;cell` or `map` -> `all`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WhitelistMapEntry {
    pub map: String,
    pub cell: String,
}

pub fn parse_whitelist_entries(raw: &str) -> Vec<WhitelistMapEntry> {
    raw.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(|line| {
            let mut parts = line.split(';').map(str::trim);
            let map = parts.next().unwrap_or_default().to_ascii_lowercase();
            let cell = parts
                .next()
                .filter(|s| !s.is_empty())
                .unwrap_or("all")
                .to_ascii_lowercase();
            WhitelistMapEntry { map, cell }
        })
        .filter(|e| !e.map.is_empty())
        .collect()
}
