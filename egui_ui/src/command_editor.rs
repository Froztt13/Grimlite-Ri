//! Detached Command Editor window opened by double-clicking any item in the Bot Manager list.

use crate::show_detached_window_sized;
use crate::theme::{self, Palette};
use egui::{Context, Key, RichText, TextEdit, Ui};
use grimlite_bot::gbot::{
    BotCommand, BotConfig, IndexOp, IntOp, QuestEntry, SkillEntry, SpecialMode, StatementKind,
};
use grimlite_bot::handlers::SpecialZone;

#[derive(Clone)]
pub enum EditTarget {
    Command {
        index: usize,
        draft: BotCommand,
        unknown_json: String,
    },
    Skill {
        index: usize,
        draft: SkillEntry,
    },
    Quest {
        index: usize,
        draft: QuestEntry,
        item_id_text: String,
    },
    Drop {
        index: usize,
        draft: String,
    },
}

#[derive(Default)]
pub struct CommandEditorWindow {
    target: Option<EditTarget>,
    error: String,
}

impl CommandEditorWindow {
    pub fn open_command(&mut self, index: usize, cmd: BotCommand) {
        let unknown_json = match &cmd {
            BotCommand::Unknown { raw, .. } => {
                serde_json::to_string_pretty(raw).unwrap_or_default()
            }
            _ => String::new(),
        };
        self.error.clear();
        self.target = Some(EditTarget::Command {
            index,
            draft: cmd,
            unknown_json,
        });
    }

    pub fn open_skill(&mut self, index: usize, skill: SkillEntry) {
        self.error.clear();
        self.target = Some(EditTarget::Skill {
            index,
            draft: skill,
        });
    }

    pub fn open_quest(&mut self, index: usize, quest: QuestEntry) {
        let item_id_text = quest.item_id.clone().unwrap_or_default();
        self.error.clear();
        self.target = Some(EditTarget::Quest {
            index,
            draft: quest,
            item_id_text,
        });
    }

    pub fn open_drop(&mut self, index: usize, name: String) {
        self.error.clear();
        self.target = Some(EditTarget::Drop { index, draft: name });
    }

    pub fn close(&mut self) {
        self.target = None;
        self.error.clear();
    }

    /// Shows the detached Command Editor window if active. Returns `true` if `cfg` was modified.
    pub fn show(&mut self, ctx: &Context, cfg: &mut BotConfig) -> bool {
        let Some(mut target) = self.target.take() else {
            return false;
        };

        let title = match &target {
            EditTarget::Command { index, .. } => format!("Command Editor — #{index:02}"),
            EditTarget::Skill { index, .. } => format!("Skill Editor — #{index:02}"),
            EditTarget::Quest { index, .. } => format!("Quest Editor — #{index:02}"),
            EditTarget::Drop { index, .. } => format!("Drop Editor — #{index:02}"),
        };

        let mut open = true;
        let mut saved = false;
        let mut cancelled = false;
        let mut err_msg = std::mem::take(&mut self.error);

        show_detached_window_sized(
            ctx,
            "grimlite_command_editor",
            &title,
            [400.0, 260.0],
            Some([340.0, 190.0]),
            &mut open,
            |ui| {
                ui.spacing_mut().item_spacing = egui::vec2(6.0, 5.0);
                ui.spacing_mut().button_padding = egui::vec2(8.0, 3.0);

                if ui.input(|i| i.key_pressed(Key::Escape)) {
                    cancelled = true;
                }

                egui::Panel::bottom("cmd_editor_footer")
                    .resizable(false)
                    .show_separator_line(true)
                    .show(ui, |ui| {
                        ui.add_space(3.0);
                        ui.horizontal(|ui| {
                            if !err_msg.is_empty() {
                                ui.label(
                                    RichText::new(&err_msg)
                                        .small()
                                        .color(Palette::DANGER),
                                );
                            }
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    if ui.button("Cancel").clicked() {
                                        cancelled = true;
                                    }
                                    if theme::primary_button(ui, "Save", [64.0, 22.0]).clicked() {
                                        saved = true;
                                    }
                                },
                            );
                        });
                    });

                egui::CentralPanel::default()
                    .frame(egui::Frame::NONE)
                    .show(ui, |ui| {
                        egui::ScrollArea::vertical()
                            .auto_shrink([false, false])
                            .show(ui, |ui| match &mut target {
                                EditTarget::Command {
                                    draft,
                                    unknown_json,
                                    ..
                                } => render_command_fields(ui, draft, unknown_json),
                                EditTarget::Skill { draft, .. } => render_skill_fields(ui, draft),
                                EditTarget::Quest {
                                    draft,
                                    item_id_text,
                                    ..
                                } => render_quest_fields(ui, draft, item_id_text),
                                EditTarget::Drop { draft, .. } => {
                                    ui.label("Drop Item Name:");
                                    ui.add(
                                        TextEdit::singleline(draft)
                                            .desired_width(f32::INFINITY),
                                    );
                                }
                            });
                    });
            },
        );

        let mut dirty = false;
        if saved {
            match apply_target(&mut target, cfg) {
                Ok(()) => {
                    dirty = true;
                    open = false;
                }
                Err(e) => {
                    err_msg = e;
                }
            }
        }

        self.error = err_msg;
        if open && !cancelled && !dirty {
            self.target = Some(target);
        }
        dirty
    }
}

fn apply_target(target: &mut EditTarget, cfg: &mut BotConfig) -> Result<(), String> {
    match target {
        EditTarget::Command {
            index,
            draft,
            unknown_json,
        } => {
            if let BotCommand::Unknown { raw, .. } = draft {
                let parsed: serde_json::Value = serde_json::from_str(unknown_json)
                    .map_err(|e| format!("Invalid JSON: {e}"))?;
                *raw = parsed;
            }
            if *index < cfg.commands.len() {
                cfg.commands[*index] = draft.clone();
                Ok(())
            } else {
                Err("Command index out of range".into())
            }
        }
        EditTarget::Skill { index, draft } => {
            if *index < cfg.skills.len() {
                cfg.skills[*index] = draft.clone();
                Ok(())
            } else {
                Err("Skill index out of range".into())
            }
        }
        EditTarget::Quest {
            index,
            draft,
            item_id_text,
        } => {
            let trimmed = item_id_text.trim();
            draft.item_id = if trimmed.is_empty() || trimmed == "-1" {
                None
            } else {
                Some(trimmed.to_string())
            };
            if *index < cfg.quests.len() {
                cfg.quests[*index] = draft.clone();
                Ok(())
            } else {
                Err("Quest index out of range".into())
            }
        }
        EditTarget::Drop { index, draft } => {
            if *index < cfg.drops.len() {
                cfg.drops[*index] = draft.trim().to_string();
                Ok(())
            } else {
                Err("Drop index out of range".into())
            }
        }
    }
}

fn render_skill_fields(ui: &mut Ui, s: &mut SkillEntry) {
    egui::Grid::new("skill_editor_grid")
        .num_columns(2)
        .spacing([10.0, 5.0])
        .show(ui, |ui| {
            ui.label("Text / Name:");
            ui.add(TextEdit::singleline(&mut s.text).desired_width(220.0));
            ui.end_row();

            ui.label("Index:");
            ui.add(TextEdit::singleline(&mut s.index).desired_width(80.0));
            ui.end_row();

            ui.label("Type:");
            theme::dropdown(
                ui,
                "edit_skill_type",
                match s.skill_type {
                    1 => "Safe Skill (1)",
                    2 => "Skill Set Label (2)",
                    _ => "Normal (0)",
                },
                150.0,
                |ui| {
                    ui.selectable_value(&mut s.skill_type, 0, "Normal (0)");
                    ui.selectable_value(&mut s.skill_type, 1, "Safe Skill (1)");
                    ui.selectable_value(&mut s.skill_type, 2, "Skill Set Label (2)");
                },
            );
            ui.end_row();

            ui.label("Safe %:");
            theme::num_input(ui, &mut s.safe_value, 0..=100, 60.0);
            ui.end_row();

            ui.label("Safe Target:");
            ui.horizontal(|ui| {
                ui.checkbox(&mut s.safe_health, "HP");
                ui.checkbox(&mut s.safe_mana, "MP");
            });
            ui.end_row();
        });
}

fn render_quest_fields(ui: &mut Ui, q: &mut QuestEntry, item_id_text: &mut String) {
    egui::Grid::new("quest_editor_grid")
        .num_columns(2)
        .spacing([10.0, 5.0])
        .show(ui, |ui| {
            ui.label("Quest ID:");
            theme::num_input(ui, &mut q.id, 0..=999_999, 90.0);
            ui.end_row();

            ui.label("Reward Item ID:");
            ui.add(
                TextEdit::singleline(item_id_text)
                    .hint_text("Empty or -1 = any")
                    .desired_width(140.0),
            );
            ui.end_row();

            ui.label("Safe Relogin:");
            ui.checkbox(&mut q.safe_relogin, "");
            ui.end_row();
        });
}

fn render_command_fields(ui: &mut Ui, cmd: &mut BotCommand, unknown_json: &mut String) {
    egui::Grid::new("cmd_editor_grid")
        .num_columns(2)
        .spacing([10.0, 5.0])
        .show(ui, |ui| match cmd {
            BotCommand::Kill { monster } | BotCommand::Attack { monster } => {
                ui.label("Monster:");
                ui.add(TextEdit::singleline(monster).desired_width(230.0));
                ui.end_row();
            }
            BotCommand::KillFor {
                monster,
                item_name,
                quantity,
                is_temp,
            } => {
                ui.label("Monster:");
                ui.add(TextEdit::singleline(monster).desired_width(230.0));
                ui.end_row();
                ui.label("Item Name:");
                ui.add(TextEdit::singleline(item_name).desired_width(230.0));
                ui.end_row();
                ui.label("Quantity:");
                ui.add(TextEdit::singleline(quantity).desired_width(120.0));
                ui.end_row();
                ui.label("Temp Item:");
                ui.checkbox(is_temp, "Temp inventory");
                ui.end_row();
            }
            BotCommand::ShortHunt {
                map,
                cell,
                pad,
                monster,
                item_name,
                quantity,
                is_temp,
                delay_after_kill,
            } => {
                ui.label("Map:");
                ui.add(TextEdit::singleline(map).desired_width(230.0));
                ui.end_row();
                ui.label("Cell / Pad:");
                ui.horizontal(|ui| {
                    ui.add(TextEdit::singleline(cell).desired_width(105.0));
                    ui.add(TextEdit::singleline(pad).desired_width(105.0));
                });
                ui.end_row();
                ui.label("Monster:");
                ui.add(TextEdit::singleline(monster).desired_width(230.0));
                ui.end_row();
                ui.label("Item Name:");
                ui.add(TextEdit::singleline(item_name).desired_width(230.0));
                ui.end_row();
                ui.label("Quantity:");
                ui.add(TextEdit::singleline(quantity).desired_width(100.0));
                ui.end_row();
                ui.label("Temp / Delay:");
                ui.horizontal(|ui| {
                    ui.checkbox(is_temp, "Temp");
                    ui.label("Delay:");
                    theme::num_input(ui, delay_after_kill, 0..=30_000, 65.0);
                    ui.label("ms");
                });
                ui.end_row();
            }
            BotCommand::QuestHunt {
                map,
                cell,
                pad,
                monster,
                item_name,
                quantity,
                quest_id,
            } => {
                ui.label("Map:");
                ui.add(TextEdit::singleline(map).desired_width(230.0));
                ui.end_row();
                ui.label("Cell / Pad:");
                ui.horizontal(|ui| {
                    ui.add(TextEdit::singleline(cell).desired_width(105.0));
                    ui.add(TextEdit::singleline(pad).desired_width(105.0));
                });
                ui.end_row();
                ui.label("Monster:");
                ui.add(TextEdit::singleline(monster).desired_width(230.0));
                ui.end_row();
                ui.label("Item Name:");
                ui.add(TextEdit::singleline(item_name).desired_width(230.0));
                ui.end_row();
                ui.label("Quantity:");
                ui.add(TextEdit::singleline(quantity).desired_width(100.0));
                ui.end_row();
                ui.label("Quest ID:");
                theme::num_input(ui, quest_id, 0..=999_999, 90.0);
                ui.end_row();
            }
            BotCommand::UseSkill {
                monster,
                index,
                wait,
                force,
            } => {
                ui.label("Monster:");
                ui.add(TextEdit::singleline(monster).desired_width(220.0));
                ui.end_row();
                ui.label("Skill Index:");
                ui.add(TextEdit::singleline(index).desired_width(100.0));
                ui.end_row();
                ui.label("Options:");
                ui.horizontal(|ui| {
                    ui.checkbox(wait, "Wait CD");
                    ui.checkbox(force, "Force");
                });
                ui.end_row();
            }
            BotCommand::SkillSet { name }
            | BotCommand::Label { name }
            | BotCommand::GotoLabel { name } => {
                ui.label("Name:");
                ui.add(TextEdit::singleline(name).desired_width(230.0));
                ui.end_row();
            }
            BotCommand::Rest { full } => {
                ui.label("Full Rest:");
                ui.checkbox(full, "Wait until full HP & MP");
                ui.end_row();
            }
            BotCommand::Join { map, cell, pad } => {
                ui.label("Map:");
                ui.add(TextEdit::singleline(map).desired_width(230.0));
                ui.end_row();
                ui.label("Cell:");
                ui.add(TextEdit::singleline(cell).desired_width(160.0));
                ui.end_row();
                ui.label("Pad:");
                ui.add(TextEdit::singleline(pad).desired_width(160.0));
                ui.end_row();
            }
            BotCommand::MoveToCell { cell, pad } => {
                ui.label("Cell:");
                ui.add(TextEdit::singleline(cell).desired_width(180.0));
                ui.end_row();
                ui.label("Pad:");
                ui.add(TextEdit::singleline(pad).desired_width(180.0));
                ui.end_row();
            }
            BotCommand::Walk { x, y } => {
                ui.label("X:");
                theme::num_input(ui, x, 0..=960, 70.0);
                ui.end_row();
                ui.label("Y:");
                theme::num_input(ui, y, 0..=550, 70.0);
                ui.end_row();
            }
            BotCommand::GetDrop { item_name }
            | BotCommand::Sell { item_name }
            | BotCommand::Equip { item_name } => {
                ui.label("Item Name:");
                ui.add(TextEdit::singleline(item_name).desired_width(230.0));
                ui.end_row();
            }
            BotCommand::LoadShop { shop_id } => {
                ui.label("Shop ID:");
                theme::num_input(ui, shop_id, 0..=999_999, 90.0);
                ui.end_row();
            }
            BotCommand::Buy {
                shop_id,
                item_name,
                qty,
            } => {
                ui.label("Shop ID:");
                theme::num_input(ui, shop_id, 0..=999_999, 90.0);
                ui.end_row();
                ui.label("Item Name:");
                ui.add(TextEdit::singleline(item_name).desired_width(230.0));
                ui.end_row();
                ui.label("Quantity:");
                theme::num_input(ui, qty, 1..=9999, 70.0);
                ui.end_row();
            }
            BotCommand::BankTransfer { item_name, to_bank } => {
                ui.label("Item Name:");
                ui.add(TextEdit::singleline(item_name).desired_width(230.0));
                ui.end_row();
                ui.label("Direction:");
                ui.checkbox(to_bank, "Transfer to Bank (unchecked = to Inventory)");
                ui.end_row();
            }
            BotCommand::MapItem { item_id } => {
                ui.label("Map Item ID:");
                theme::num_input(ui, item_id, 0..=999_999, 90.0);
                ui.end_row();
            }
            BotCommand::Whitelist { item_name, add } => {
                ui.label("Item Name:");
                ui.add(TextEdit::singleline(item_name).desired_width(230.0));
                ui.end_row();
                ui.label("Action:");
                ui.checkbox(add, "Add to whitelist (unchecked = remove)");
                ui.end_row();
            }
            BotCommand::AcceptQuest { quest_id } | BotCommand::RemoveQuestList { quest_id } => {
                ui.label("Quest ID:");
                theme::num_input(ui, quest_id, 0..=999_999, 90.0);
                ui.end_row();
            }
            BotCommand::AddQuestList { quest_id, item_id } => {
                ui.label("Quest ID:");
                theme::num_input(ui, quest_id, 0..=999_999, 90.0);
                ui.end_row();
                ui.label("Reward Item ID:");
                theme::num_input(ui, item_id, -1..=999_999, 90.0);
                ui.end_row();
            }
            BotCommand::CompleteQuest {
                quest_id,
                item_id,
                qty,
            } => {
                ui.label("Quest ID:");
                theme::num_input(ui, quest_id, 0..=999_999, 90.0);
                ui.end_row();
                ui.label("Reward Item ID:");
                theme::num_input(ui, item_id, -1..=999_999, 90.0);
                ui.end_row();
                ui.label("Qty (-1 = Max):");
                theme::num_input(ui, qty, -1..=999, 90.0);
                ui.end_row();
            }
            BotCommand::Delay { ms } => {
                ui.label("Delay (ms):");
                theme::num_input(ui, ms, 0..=600_000, 90.0);
                ui.end_row();
            }
            BotCommand::Index { index, op } => {
                ui.label("Mode:");
                theme::dropdown(
                    ui,
                    "edit_index_op",
                    match op {
                        IndexOp::Up => "Up (++)",
                        IndexOp::Down => "Down (--)",
                        IndexOp::Goto => "Goto",
                    },
                    130.0,
                    |ui| {
                        ui.selectable_value(op, IndexOp::Up, "Up (++)");
                        ui.selectable_value(op, IndexOp::Down, "Down (--)");
                        ui.selectable_value(op, IndexOp::Goto, "Goto");
                    },
                );
                ui.end_row();
                ui.label("Index:");
                theme::num_input(ui, index, 0..=9999, 80.0);
                ui.end_row();
            }
            BotCommand::Packet {
                packet,
                to_client,
                spam_times,
                delay_ms,
            } => {
                ui.label("Packet:");
                ui.add(TextEdit::singleline(packet).desired_width(240.0));
                ui.end_row();
                ui.label("To Client:");
                ui.checkbox(to_client, "Inject to client");
                ui.end_row();
                ui.label("Spam Times:");
                theme::num_input(ui, spam_times, 1..=1000, 70.0);
                ui.end_row();
                ui.label("Delay (ms):");
                theme::num_input(ui, delay_ms, 0..=60_000, 80.0);
                ui.end_row();
            }
            BotCommand::Int { name, value, op } => {
                ui.label("Var Name:");
                ui.add(TextEdit::singleline(name).desired_width(180.0));
                ui.end_row();
                ui.label("Value:");
                theme::num_input(ui, value, i32::MIN..=i32::MAX, 90.0);
                ui.end_row();
                ui.label("Operation:");
                theme::dropdown(ui, "edit_int_op", format!("{op:?}"), 90.0, |ui| {
                    ui.selectable_value(op, IntOp::Set, "Set");
                    ui.selectable_value(op, IntOp::Add, "Add");
                    ui.selectable_value(op, IntOp::Sub, "Sub");
                });
                ui.end_row();
            }
            BotCommand::SetVar { name, value } => {
                ui.label("Var Name:");
                ui.add(TextEdit::singleline(name).desired_width(180.0));
                ui.end_row();
                ui.label("Value:");
                ui.add(TextEdit::singleline(value).desired_width(220.0));
                ui.end_row();
            }
            BotCommand::Log { text, debug } => {
                ui.label("Text:");
                ui.add(TextEdit::singleline(text).desired_width(240.0));
                ui.end_row();
                ui.label("Target:");
                ui.checkbox(debug, "Log Debug (unchecked = Log Script)");
                ui.end_row();
            }
            BotCommand::Blank { text } => {
                ui.label("Text:");
                ui.add(TextEdit::singleline(text).desired_width(240.0));
                ui.end_row();
            }
            BotCommand::ClientMessage {
                message,
                is_warning,
            } => {
                ui.label("Message:");
                ui.add(TextEdit::singleline(message).desired_width(240.0));
                ui.end_row();
                ui.label("Type:");
                ui.checkbox(is_warning, "Warning (unchecked = Info)");
                ui.end_row();
            }
            BotCommand::LoadMap { swf } => {
                ui.label("Map SWF:");
                ui.add(TextEdit::singleline(swf).desired_width(240.0));
                ui.end_row();
            }
            BotCommand::Provoke { enabled } => {
                ui.label("Provoke:");
                ui.checkbox(enabled, "Enable provoke monsters");
                ui.end_row();
            }
            BotCommand::SetFPS { fps } => {
                ui.label("Stage FPS:");
                theme::num_input(ui, fps, 1..=60, 60.0);
                ui.end_row();
            }
            BotCommand::Beep { times } => {
                ui.label("Times:");
                theme::num_input(ui, times, 1..=50, 70.0);
                ui.end_row();
            }
            BotCommand::GotoPlayer { player_name } => {
                ui.label("Player Name:");
                ui.add(TextEdit::singleline(player_name).desired_width(220.0));
                ui.end_row();
            }
            BotCommand::SpecialHandler { zone, mode } => {
                ui.label("Zone:");
                let cur_zone = zone.clone();
                theme::dropdown(ui, "edit_special_zone", cur_zone, 220.0, |ui| {
                    for &z in SpecialZone::ALL {
                        let name = z.display_name().to_string();
                        ui.selectable_value(zone, name.clone(), name);
                    }
                });
                ui.end_row();
                ui.label("Mode:");
                theme::dropdown(
                    ui,
                    "edit_special_mode",
                    format!("{mode:?}"),
                    120.0,
                    |ui| {
                        ui.selectable_value(mode, SpecialMode::Start, "Start");
                        ui.selectable_value(mode, SpecialMode::Stop, "Stop");
                        ui.selectable_value(mode, SpecialMode::Switch, "Switch");
                    },
                );
                ui.end_row();
            }
            BotCommand::Statement {
                kind,
                value1,
                value2,
            } => {
                ui.label("Condition:");
                theme::dropdown(ui, "edit_stmt_kind", format!("{kind:?}"), 210.0, |ui| {
                    for k in all_statement_kinds() {
                        ui.selectable_value(kind, k.clone(), format!("{k:?}"));
                    }
                });
                ui.end_row();
                ui.label("Value 1:");
                ui.add(TextEdit::singleline(value1).desired_width(220.0));
                ui.end_row();
                ui.label("Value 2:");
                ui.add(TextEdit::singleline(value2).desired_width(220.0));
                ui.end_row();
            }
            BotCommand::Unknown { type_name, .. } => {
                ui.label("Raw Type:");
                ui.label(RichText::new(type_name.as_str()).small().weak());
                ui.end_row();
                ui.label("JSON:");
                ui.add(
                    TextEdit::multiline(unknown_json)
                        .desired_rows(6)
                        .desired_width(250.0)
                        .code_editor(),
                );
                ui.end_row();
            }
            BotCommand::CancelTarget
            | BotCommand::SetSpawn
            | BotCommand::Yulgar
            | BotCommand::Logout
            | BotCommand::ClearTemp
            | BotCommand::Stop
            | BotCommand::Restart => {
                ui.label(RichText::new("No editable parameters for this command.").weak());
                ui.end_row();
            }
        });
}

pub fn all_statement_kinds() -> Vec<StatementKind> {
    vec![
        StatementKind::InInventory,
        StatementKind::NotInInventory,
        StatementKind::InTemp,
        StatementKind::NotInTemp,
        StatementKind::InBank,
        StatementKind::NotInBank,
        StatementKind::InBankOrInvent,
        StatementKind::NotInBankAndInvent,
        StatementKind::Equipped,
        StatementKind::NotEquipped,
        StatementKind::MapIs,
        StatementKind::MapIsNot,
        StatementKind::CellIs,
        StatementKind::CellIsNot,
        StatementKind::QuestInProgress,
        StatementKind::QuestNotInProgress,
        StatementKind::QuestCompleted,
        StatementKind::QuestNotCompleted,
        StatementKind::MonsterInRoom,
        StatementKind::MonsterNotInRoom,
        StatementKind::HasTarget,
        StatementKind::NotHasTarget,
        StatementKind::HealthLessThan,
        StatementKind::HealthGreaterThan,
        StatementKind::ManaLessThan,
        StatementKind::ManaGreaterThan,
        StatementKind::GoldGreaterThan,
        StatementKind::GoldLessThan,
        StatementKind::LevelGreaterThan,
        StatementKind::LevelLessThan,
        StatementKind::IntIs,
        StatementKind::IntIsNot,
        StatementKind::IntGreaterThan,
        StatementKind::IntLessThan,
        StatementKind::IntEqualInt,
        StatementKind::IntNotEqualInt,
        StatementKind::VarEqualVar,
        StatementKind::VarNotEqualVar,
        StatementKind::NameEquals,
    ]
}
