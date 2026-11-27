//! Bot Manager — Quest tab.

use super::{BotManagerWindow, group_box, two_columns};
use crate::theme;
use egui::{TextEdit, Ui};
use grimlite_bot::gbot::{BotCommand, BotConfig, QuestEntry};

impl BotManagerWindow {
    // ---- Tab 5: Quest ----

    pub(super) fn show_quest_tab(&mut self, ui: &mut Ui, cfg: &mut BotConfig) -> bool {
        let mut dirty = false;
        two_columns(ui, |cols| {
            group_box(&mut cols[0], "Quest Commands", |ui| {
                ui.horizontal(|ui| {
                    ui.label("Quest ID");
                    ui.add(
                        TextEdit::singleline(&mut self.quest_id)
                            .hint_text("e.g. 555")
                            .desired_width(ui.available_width().max(70.0)),
                    );
                });
                ui.add_space(3.0);
                ui.horizontal(|ui| {
                    ui.label("Reward");
                    ui.add(
                        TextEdit::singleline(&mut self.quest_item_id)
                            .hint_text("-1 = any")
                            .desired_width(68.0),
                    );
                    ui.add_space(4.0);
                    ui.label("Qty");
                    theme::num_input(ui, &mut self.quest_complete_qty, -1..=999, 44.0)
                        .on_hover_text("-1 = Max turn-in");
                });
                ui.add_space(5.0);
                let half = ((ui.available_width() - 5.0) * 0.5).max(60.0);
                ui.horizontal(|ui| {
                    if theme::primary_button(ui, "Accept Quest", [half, 22.0]).clicked()
                        && let Ok(id) = self.quest_id.trim().parse::<i32>()
                    {
                        self.push_command(cfg, BotCommand::AcceptQuest { quest_id: id });
                        dirty = true;
                    }
                    if theme::primary_button(ui, "Complete", [half, 22.0]).clicked()
                        && let Ok(id) = self.quest_id.trim().parse::<i32>()
                    {
                        let item_id = self.quest_item_id.trim().parse::<i32>().unwrap_or(-1);
                        let qty = if self.quest_complete_qty == 0 {
                            1
                        } else {
                            self.quest_complete_qty
                        };
                        self.push_command(
                            cfg,
                            BotCommand::CompleteQuest {
                                quest_id: id,
                                item_id,
                                qty,
                            },
                        );
                        dirty = true;
                    }
                });
            });

            group_box(&mut cols[1], "Auto-Quest List", |ui| {
                ui.horizontal(|ui| {
                    ui.label("Quest ID");
                    ui.add(
                        TextEdit::singleline(&mut self.auto_quest_id)
                            .hint_text("e.g. 555")
                            .desired_width(ui.available_width().max(70.0)),
                    );
                });
                ui.add_space(3.0);
                ui.horizontal(|ui| {
                    ui.label("Reward ID");
                    ui.add(
                        TextEdit::singleline(&mut self.auto_quest_item_id)
                            .hint_text("-1 = any")
                            .desired_width(ui.available_width().max(70.0)),
                    );
                });
                ui.add_space(5.0);
                if theme::primary_button(ui, "+ Add to Quest List", [ui.available_width(), 22.0])
                    .clicked()
                    && let Ok(id) = self.auto_quest_id.trim().parse::<i32>()
                {
                    let item = self.auto_quest_item_id.trim();
                    let item_opt = if item.is_empty() || item == "-1" {
                        None
                    } else {
                        Some(item.to_string())
                    };
                    if !cfg.quests.iter().any(|q| q.id == id) {
                        cfg.quests.push(QuestEntry {
                            id,
                            item_id: item_opt,
                            safe_relogin: false,
                        });
                        dirty = true;
                    }
                }
                ui.add_space(3.0);
                let half = ((ui.available_width() - 5.0) * 0.5).max(60.0);
                ui.horizontal(|ui| {
                    if theme::primary_button(ui, "+ Add Cmd", [half, 22.0]).clicked()
                        && let Ok(id) = self.auto_quest_id.trim().parse::<i32>()
                    {
                        let item_id = self.auto_quest_item_id.trim().parse::<i32>().unwrap_or(-1);
                        self.push_command(
                            cfg,
                            BotCommand::AddQuestList {
                                quest_id: id,
                                item_id,
                            },
                        );
                        dirty = true;
                    }
                    if theme::primary_button(ui, "+ Remove Cmd", [half, 22.0]).clicked()
                        && let Ok(id) = self.auto_quest_id.trim().parse::<i32>()
                    {
                        self.push_command(cfg, BotCommand::RemoveQuestList { quest_id: id });
                        dirty = true;
                    }
                });
            });
        });
        dirty
    }

}
