//! Bot Manager — Misc 2 tab (Conditional Statements, Messages, Load Map, Temp Integer, Index, Special Handlers).

use super::{BotManagerWindow, group_box, two_columns};
use crate::theme;
use egui::{TextEdit, Ui};
use grimlite_bot::gbot::{BotCommand, BotConfig, IndexOp, IntOp, SpecialMode, StatementKind};
use grimlite_bot::handlers::SpecialZone;

impl BotManagerWindow {
    // ---- Tab 7: Misc 2 (Statements, Temp Integer, Index) ----

    pub(super) fn show_misc2_tab(&mut self, ui: &mut Ui, cfg: &mut BotConfig) -> bool {
        let mut dirty = false;
        two_columns(ui, |cols| {
            // Column 1: Conditional Statement + dynamic parameters (Grimlite-Li BotManager.cs behavior)
            group_box(&mut cols[0], "Conditional Statement", |ui| {
                let full_w = ui.available_width();

                // Category dropdown (Tag)
                let prev_cat = self.stmt_category;
                theme::dropdown(ui, "stmt_cat_cb", self.stmt_category, full_w, |ui| {
                    for &cat in STATEMENT_CATEGORIES {
                        ui.selectable_value(&mut self.stmt_category, cat, cat);
                    }
                });
                let filtered: Vec<StatementMeta> = statement_catalog()
                    .into_iter()
                    .filter(|m| m.category == self.stmt_category)
                    .collect();
                if self.stmt_category != prev_cat
                    || !filtered.iter().any(|m| m.kind == self.stmt_kind)
                {
                    if let Some(first) = filtered.first() {
                        self.stmt_kind = first.kind.clone();
                        self.stmt_v1.clear();
                        self.stmt_v2 = first.default_v2.to_string();
                    }
                }

                ui.add_space(3.0);
                // Statement dropdown filtered by category
                let cur_meta = filtered
                    .iter()
                    .find(|m| m.kind == self.stmt_kind)
                    .cloned()
                    .unwrap_or_else(|| filtered[0].clone());
                let prev_kind = self.stmt_kind.clone();
                theme::dropdown(ui, "stmt_kind_cb", cur_meta.label, full_w, |ui| {
                    for m in &filtered {
                        ui.selectable_value(&mut self.stmt_kind, m.kind.clone(), m.label);
                    }
                });
                if self.stmt_kind != prev_kind
                    && let Some(m) = filtered.iter().find(|m| m.kind == self.stmt_kind)
                {
                    self.stmt_v1.clear();
                    self.stmt_v2 = m.default_v2.to_string();
                }

                let active_meta = filtered
                    .iter()
                    .find(|m| m.kind == self.stmt_kind)
                    .cloned()
                    .unwrap_or(cur_meta);

                if let Some(d1) = active_meta.desc1 {
                    ui.add_space(4.0);
                    ui.add(
                        TextEdit::singleline(&mut self.stmt_v1)
                            .hint_text(d1)
                            .desired_width(f32::INFINITY),
                    );
                }
                if let Some(d2) = active_meta.desc2 {
                    ui.add_space(3.0);
                    ui.add(
                        TextEdit::singleline(&mut self.stmt_v2)
                            .hint_text(d2)
                            .desired_width(f32::INFINITY),
                    );
                }

                ui.add_space(5.0);
                if theme::primary_button(ui, "+ Add Statement", [ui.available_width(), 22.0])
                    .clicked()
                {
                    let v1 = if active_meta.desc1.is_some() {
                        self.stmt_v1.trim().to_string()
                    } else {
                        String::new()
                    };
                    let v2 = if active_meta.desc2.is_some() {
                        self.stmt_v2.trim().to_string()
                    } else {
                        String::new()
                    };
                    self.push_command(
                        cfg,
                        BotCommand::Statement {
                            kind: self.stmt_kind.clone(),
                            value1: v1,
                            value2: v2,
                        },
                    );
                    dirty = true;
                }
            });

            cols[0].add_space(6.0);
            group_box(&mut cols[0], "Messages", |ui| {
                ui.add(
                    TextEdit::singleline(&mut self.client_msg_text)
                        .hint_text("Hello World!")
                        .desired_width(f32::INFINITY),
                );
                ui.add_space(4.0);
                let half = ((ui.available_width() - 5.0) * 0.5).max(60.0);
                ui.horizontal(|ui| {
                    if theme::primary_button(ui, "Add Warning", [half, 21.0]).clicked()
                        && !self.client_msg_text.trim().is_empty()
                    {
                        self.push_command(
                            cfg,
                            BotCommand::ClientMessage {
                                message: self.client_msg_text.trim().to_string(),
                                is_warning: true,
                            },
                        );
                        dirty = true;
                    }
                    if theme::primary_button(ui, "Add Info", [half, 21.0]).clicked()
                        && !self.client_msg_text.trim().is_empty()
                    {
                        self.push_command(
                            cfg,
                            BotCommand::ClientMessage {
                                message: self.client_msg_text.trim().to_string(),
                                is_warning: false,
                            },
                        );
                        dirty = true;
                    }
                });
            });

            cols[0].add_space(6.0);
            group_box(&mut cols[0], "Load Map", |ui| {
                ui.horizontal(|ui| {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if theme::primary_button(ui, "Load", [56.0, 21.0]).clicked() {
                            let swf = self.load_map_swf.trim();
                            if !swf.is_empty() && swf.to_ascii_lowercase().ends_with(".swf") {
                                self.push_command(
                                    cfg,
                                    BotCommand::LoadMap {
                                        swf: swf.to_string(),
                                    },
                                );
                                dirty = true;
                            }
                        }
                        ui.add(
                            TextEdit::singleline(&mut self.load_map_swf)
                                .hint_text("Map file (.swf)")
                                .desired_width(ui.available_width().max(36.0)),
                        );
                    });
                });
            });

            // Column 2: Temp Integer + Index
            group_box(&mut cols[1], "Temp Integer", |ui| {
                ui.horizontal(|ui| {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        theme::num_input(ui, &mut self.temp_int_val, -999_999..=999_999, 56.0);
                        ui.add(
                            TextEdit::singleline(&mut self.temp_int_name)
                                .hint_text("Int name")
                                .desired_width(ui.available_width().max(36.0)),
                        );
                    });
                });
                ui.add_space(4.0);
                let third = ((ui.available_width() - 8.0) / 3.0).max(46.0);
                let name = self.temp_int_name.trim().to_string();
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 4.0;
                    if theme::primary_button(ui, "Set", [third, 21.0]).clicked() && !name.is_empty()
                    {
                        self.push_command(
                            cfg,
                            BotCommand::Int {
                                name: name.clone(),
                                value: self.temp_int_val,
                                op: IntOp::Set,
                            },
                        );
                        dirty = true;
                    }
                    if theme::primary_button(ui, "++", [third, 21.0]).clicked() && !name.is_empty()
                    {
                        self.push_command(
                            cfg,
                            BotCommand::Int {
                                name: name.clone(),
                                value: 1,
                                op: IntOp::Add,
                            },
                        );
                        dirty = true;
                    }
                    if theme::primary_button(ui, "--", [third, 21.0]).clicked() && !name.is_empty()
                    {
                        self.push_command(
                            cfg,
                            BotCommand::Int {
                                name: name.clone(),
                                value: 1,
                                op: IntOp::Sub,
                            },
                        );
                        dirty = true;
                    }
                });
                ui.add_space(3.0);
                if theme::primary_button(ui, "Clear Var&Int", [ui.available_width(), 21.0])
                    .clicked()
                {
                    self.push_command(cfg, BotCommand::ClearTemp);
                    dirty = true;
                }
            });

            cols[1].add_space(6.0);
            group_box(&mut cols[1], "Index", |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 4.0;
                    let num_w = 52.0;
                    theme::num_input(ui, &mut self.index_val, 0..=9999, num_w);
                    let btn_w = ((ui.available_width() - 8.0) / 3.0).max(38.0);
                    if theme::primary_button(ui, "Goto", [btn_w, 21.0]).clicked() {
                        self.push_command(
                            cfg,
                            BotCommand::Index {
                                index: self.index_val,
                                op: IndexOp::Goto,
                            },
                        );
                        dirty = true;
                    }
                    if theme::primary_button(ui, "++", [btn_w, 21.0]).clicked() {
                        self.push_command(
                            cfg,
                            BotCommand::Index {
                                index: self.index_val,
                                op: IndexOp::Up,
                            },
                        );
                        dirty = true;
                    }
                    if theme::primary_button(ui, "--", [btn_w, 21.0]).clicked() {
                        self.push_command(
                            cfg,
                            BotCommand::Index {
                                index: self.index_val,
                                op: IndexOp::Down,
                            },
                        );
                        dirty = true;
                    }
                });
            });

            cols[1].add_space(6.0);
            group_box(&mut cols[1], "Special Handlers", |ui| {
                let mut sp_on = self.bot.special_handler.is_enabled();
                self.special_zone = self.bot.special_handler.zone();
                ui.horizontal(|ui| {
                    if ui.checkbox(&mut sp_on, "On").changed() {
                        self.bot.special_handler.set_enabled(sp_on);
                    }
                    let cb_w = ui.available_width().max(60.0);
                    let prev = self.special_zone;
                    theme::dropdown(
                        ui,
                        "special_zone_cb",
                        self.special_zone.display_name(),
                        cb_w,
                        |ui| {
                            for &z in SpecialZone::ALL {
                                ui.selectable_value(&mut self.special_zone, z, z.display_name());
                            }
                        },
                    );
                    if self.special_zone != prev {
                        self.bot.special_handler.set_zone(self.special_zone);
                    }
                });
                ui.add_space(4.0);
                let third = ((ui.available_width() - 8.0) / 3.0).max(46.0);
                let zone_name = self.special_zone.display_name().to_string();
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 4.0;
                    if theme::primary_button(ui, "Start", [third, 21.0]).clicked() {
                        self.push_command(
                            cfg,
                            BotCommand::SpecialHandler {
                                zone: zone_name.clone(),
                                mode: SpecialMode::Start,
                            },
                        );
                        dirty = true;
                    }
                    if theme::primary_button(ui, "Stop", [third, 21.0]).clicked() {
                        self.push_command(
                            cfg,
                            BotCommand::SpecialHandler {
                                zone: zone_name.clone(),
                                mode: SpecialMode::Stop,
                            },
                        );
                        dirty = true;
                    }
                    if theme::primary_button(ui, "Switch", [third, 21.0]).clicked() {
                        self.push_command(
                            cfg,
                            BotCommand::SpecialHandler {
                                zone: zone_name,
                                mode: SpecialMode::Switch,
                            },
                        );
                        dirty = true;
                    }
                });
            });
        });
        dirty
    }
}

const STATEMENT_CATEGORIES: &[&str] = &[
    "Item",
    "Map",
    "Quest",
    "Monster",
    "This player",
    "Player",
    "Aura",
    "Misc",
];

#[derive(Clone)]
struct StatementMeta {
    category: &'static str,
    kind: StatementKind,
    label: &'static str,
    desc1: Option<&'static str>,
    desc2: Option<&'static str>,
    default_v2: &'static str,
}

fn statement_catalog() -> Vec<StatementMeta> {
    use StatementKind as S;
    let m = |category, kind, label, desc1, desc2, default_v2| StatementMeta {
        category,
        kind,
        label,
        desc1,
        desc2,
        default_v2,
    };
    vec![
        // Item
        m(
            "Item",
            S::InInventory,
            "Item is in inventory",
            Some("Item name"),
            Some("Quantity"),
            "1",
        ),
        m(
            "Item",
            S::NotInInventory,
            "Item is not in inventory",
            Some("Item name"),
            Some("Quantity"),
            "1",
        ),
        m(
            "Item",
            S::InTemp,
            "Item is in temp inventory",
            Some("Item name"),
            Some("Quantity"),
            "1",
        ),
        m(
            "Item",
            S::NotInTemp,
            "Item is not in temp inventory",
            Some("Item name"),
            Some("Quantity"),
            "1",
        ),
        m(
            "Item",
            S::InBank,
            "Item is in bank",
            Some("Item name"),
            Some("Quantity"),
            "1",
        ),
        m(
            "Item",
            S::NotInBank,
            "Item is not in bank",
            Some("Item name"),
            Some("Quantity"),
            "1",
        ),
        m(
            "Item",
            S::InBankOrInvent,
            "Item is in bank or inventory",
            Some("Item name"),
            Some("Quantity"),
            "1",
        ),
        m(
            "Item",
            S::NotInBankAndInvent,
            "Item not in bank and inventory",
            Some("Item name"),
            Some("Quantity"),
            "1",
        ),
        m(
            "Item",
            S::Equipped,
            "Is equipped",
            Some("Item name"),
            None,
            "",
        ),
        m(
            "Item",
            S::NotEquipped,
            "Is not equipped",
            Some("Item name"),
            None,
            "",
        ),
        m(
            "Item",
            S::InHouse,
            "Is in house",
            Some("Item name"),
            Some("Quantity (* = any)"),
            "1",
        ),
        m(
            "Item",
            S::NotInHouse,
            "Is not in house",
            Some("Item name"),
            Some("Quantity (* = any)"),
            "1",
        ),
        m(
            "Item",
            S::IsMaxStack,
            "Item is max in inventory",
            Some("Item name"),
            None,
            "",
        ),
        m(
            "Item",
            S::IsNotMaxStack,
            "Item is not max in inventory",
            Some("Item name"),
            None,
            "",
        ),
        m(
            "Item",
            S::ItemPickupable,
            "Has dropped",
            Some("Item name"),
            None,
            "",
        ),
        m(
            "Item",
            S::ItemNotPickupable,
            "Has not dropped",
            Some("Item name"),
            None,
            "",
        ),
        m(
            "Item",
            S::ListInInv,
            "All in List is in inventory",
            None,
            None,
            "",
        ),
        // Map
        m("Map", S::MapIs, "Map is", Some("Map name"), None, ""),
        m("Map", S::MapIsNot, "Map is not", Some("Map name"), None, ""),
        m("Map", S::CellIs, "Cell is", Some("Cell"), None, ""),
        m("Map", S::CellIsNot, "Cell is not", Some("Cell"), None, ""),
        // Quest
        m(
            "Quest",
            S::QuestInProgress,
            "Quest is in progress",
            Some("Quest ID"),
            None,
            "",
        ),
        m(
            "Quest",
            S::QuestNotInProgress,
            "Quest is not in progress",
            Some("Quest ID"),
            None,
            "",
        ),
        m(
            "Quest",
            S::QuestCompleted,
            "Quest can be turned in",
            Some("Quest ID"),
            None,
            "",
        ),
        m(
            "Quest",
            S::QuestNotCompleted,
            "Quest can not be turned in",
            Some("Quest ID"),
            None,
            "",
        ),
        m(
            "Quest",
            S::QuestAvailable,
            "Quest is available",
            Some("Quest ID"),
            None,
            "",
        ),
        m(
            "Quest",
            S::QuestNotAvailable,
            "Quest is not available",
            Some("Quest ID"),
            None,
            "",
        ),
        // Monster
        m(
            "Monster",
            S::MonsterInRoom,
            "Is in room",
            Some("Monster name"),
            None,
            "",
        ),
        m(
            "Monster",
            S::MonsterNotInRoom,
            "Is not in room",
            Some("Monster name"),
            None,
            "",
        ),
        m(
            "Monster",
            S::MonsterHealthGreaterThan,
            "Monster HP greater than",
            Some("Monster name"),
            Some("Health"),
            "0",
        ),
        m(
            "Monster",
            S::MonsterHealthLessThan,
            "Monster HP less than",
            Some("Monster name"),
            Some("Health"),
            "0",
        ),
        m(
            "Monster",
            S::MonstersGreaterThan,
            "Count is greater than",
            Some("Monster count"),
            None,
            "",
        ),
        m(
            "Monster",
            S::MonstersLessThan,
            "Count is less than",
            Some("Monster count"),
            None,
            "",
        ),
        m(
            "Monster",
            S::SpecialAnims,
            "Special Anims",
            Some("Animation message"),
            Some("Skill to use"),
            "",
        ),
        // This player
        m("This player", S::InCombat, "Is in combat", None, None, ""),
        m(
            "This player",
            S::NotInCombat,
            "Not in combat",
            None,
            None,
            "",
        ),
        m("This player", S::HasTarget, "Is has target", None, None, ""),
        m(
            "This player",
            S::NotHasTarget,
            "Is not has target",
            None,
            None,
            "",
        ),
        m("This player", S::IsMember, "Is Member", None, None, ""),
        m(
            "This player",
            S::IsNotMember,
            "Is Not Member",
            None,
            None,
            "",
        ),
        m(
            "This player",
            S::HealthLessThan,
            "Health is less than",
            Some("Health"),
            None,
            "",
        ),
        m(
            "This player",
            S::HealthGreaterThan,
            "Health is greater than",
            Some("Health"),
            None,
            "",
        ),
        m(
            "This player",
            S::TargetHealthGreaterThan,
            "Target health greater than",
            Some("Health"),
            Some("Default true/false"),
            "false",
        ),
        m(
            "This player",
            S::TargetHealthLessThan,
            "Target health less than",
            Some("Health"),
            Some("Default true/false"),
            "false",
        ),
        m(
            "This player",
            S::TargetHealthBetween,
            "Target health between",
            Some("StartHP,EndHP"),
            Some("Default true/false"),
            "false",
        ),
        m(
            "This player",
            S::ManaLessThan,
            "Mana is less than",
            Some("Mana"),
            None,
            "",
        ),
        m(
            "This player",
            S::ManaGreaterThan,
            "Mana is greater than",
            Some("Mana"),
            None,
            "",
        ),
        m(
            "This player",
            S::GoldGreaterThan,
            "Gold is greater than",
            Some("Gold"),
            None,
            "",
        ),
        m(
            "This player",
            S::GoldLessThan,
            "Gold is less than",
            Some("Gold"),
            None,
            "",
        ),
        m(
            "This player",
            S::LevelGreaterThan,
            "Level is greater than",
            Some("Level"),
            None,
            "",
        ),
        m(
            "This player",
            S::LevelIs,
            "Level is",
            Some("Level"),
            None,
            "",
        ),
        m(
            "This player",
            S::LevelLessThan,
            "Level is less than",
            Some("Level"),
            None,
            "",
        ),
        m(
            "This player",
            S::FactionRankGreaterThan,
            "Faction Rank is greater than",
            Some("Faction"),
            Some("Rank"),
            "1",
        ),
        m(
            "This player",
            S::FactionRankLessThan,
            "Faction Rank is less than",
            Some("Faction"),
            Some("Rank"),
            "10",
        ),
        m(
            "This player",
            S::NameEquals,
            "Name equals",
            Some("Player name"),
            None,
            "",
        ),
        // Player
        m(
            "Player",
            S::PlayerInRoom,
            "Player is in room",
            Some("Player name"),
            None,
            "",
        ),
        m(
            "Player",
            S::PlayerNotInRoom,
            "Player is not in room",
            Some("Player name"),
            None,
            "",
        ),
        m(
            "Player",
            S::PlayerIsInCell,
            "Player is in cell",
            Some("Player name"),
            Some("Cell name"),
            "",
        ),
        m(
            "Player",
            S::PlayerIsNotInCell,
            "Player is not in cell",
            Some("Player name"),
            Some("Cell name"),
            "",
        ),
        m(
            "Player",
            S::PlayerIsInMyCell,
            "Player is in my cell",
            Some("Player name"),
            None,
            "",
        ),
        m(
            "Player",
            S::PlayerIsNotInMyCell,
            "Player is not in my cell",
            Some("Player name"),
            None,
            "",
        ),
        m(
            "Player",
            S::PlayersGreaterThan,
            "Count is greater than",
            Some("Player count"),
            None,
            "",
        ),
        m(
            "Player",
            S::PlayersLessThan,
            "Count is less than",
            Some("Player count"),
            None,
            "",
        ),
        m(
            "Player",
            S::PlayerInMyCellGreaterThan,
            "Player count is greater than [cell]",
            Some("Player count"),
            None,
            "",
        ),
        m(
            "Player",
            S::PlayerInMyCellLessThan,
            "Player count is Less than [cell]",
            Some("Player count"),
            None,
            "",
        ),
        m(
            "Player",
            S::PlayerHPGreaterThan,
            "Health is greater than",
            Some("Player name"),
            Some("Health in percentage"),
            "50",
        ),
        m(
            "Player",
            S::PlayerHPLessThan,
            "Health is less than",
            Some("Player name"),
            Some("Health in percentage"),
            "50",
        ),
        // Aura
        m(
            "Aura",
            S::PlayerAuraEquals,
            "Player aura value equals",
            Some("Aura name"),
            Some("Aura value"),
            "0",
        ),
        m(
            "Aura",
            S::PlayerAuraGreaterThan,
            "Player aura value greater than",
            Some("Aura name"),
            Some("Aura value"),
            "0",
        ),
        m(
            "Aura",
            S::PlayerAuraLessThan,
            "Player aura value less than",
            Some("Aura name"),
            Some("Aura value"),
            "0",
        ),
        m(
            "Aura",
            S::TargetAuraEquals,
            "Target aura value equals",
            Some("Aura name"),
            Some("Aura value"),
            "0",
        ),
        m(
            "Aura",
            S::TargetAuraGreaterThan,
            "Target aura value greater than",
            Some("Aura name"),
            Some("Aura value"),
            "0",
        ),
        m(
            "Aura",
            S::TargetAuraLessThan,
            "Target aura value less than",
            Some("Aura name"),
            Some("Aura value"),
            "0",
        ),
        m(
            "Aura",
            S::TargetAuraDuration,
            "Monster (ID) Aura duration is less than",
            Some("Aura Name"),
            Some("Duration remaining (in ms)"),
            "2000",
        ),
        // Misc
        m("Misc", S::IntIs, "Int is", Some("Int"), Some("Value"), "0"),
        m(
            "Misc",
            S::IntIsNot,
            "Int is not",
            Some("Int"),
            Some("Value"),
            "0",
        ),
        m(
            "Misc",
            S::IntGreaterThan,
            "Int greater than",
            Some("Int"),
            Some("Value"),
            "0",
        ),
        m(
            "Misc",
            S::IntLessThan,
            "Int less than",
            Some("Int"),
            Some("Value"),
            "0",
        ),
        m(
            "Misc",
            S::IntEqualInt,
            "Int is equal to Int",
            Some("Int1"),
            Some("Int2"),
            "",
        ),
        m(
            "Misc",
            S::IntNotEqualInt,
            "Int is not equal to Int",
            Some("Int1"),
            Some("Int2"),
            "",
        ),
        m(
            "Misc",
            S::VarEqualVar,
            "Var is equal to Var",
            Some("Var1"),
            Some("Var2"),
            "",
        ),
        m(
            "Misc",
            S::VarNotEqualVar,
            "Var is not equal to Var",
            Some("Var1"),
            Some("Var2"),
            "",
        ),
        m(
            "Misc",
            S::UpdateVar,
            "Update Temporary Variable",
            Some("Variable Name"),
            Some("Value"),
            "",
        ),
    ]
}
