//! Bot Manager — Map tab.

use super::{BotManagerWindow, group_box, non_empty_or, two_columns};
use crate::theme;
use egui::{RichText, TextEdit, Ui};
use grimlite_bot::gbot::{BotCommand, BotConfig};

impl BotManagerWindow {
    // ---- Tab 3: Map (Join Map, Jump Cell + Set Spawn, Walk To, Map Item) ----

    pub(super) fn show_map_tab(&mut self, ui: &mut Ui, cfg: &mut BotConfig) -> bool {
        let mut dirty = false;

        two_columns(ui, |cols| {
            group_box(&mut cols[0], "Join Map", |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Map Name").small().weak());
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if theme::primary_btn(ui, "Use Current")
                            .on_hover_text("Fill map, cell, and pad from current player location")
                            .clicked()
                        {
                            let snap = self.bot.player_snapshot();
                            if !snap.map.is_empty() {
                                self.join_map = format!("{}-99999", snap.map);
                            }
                            if !snap.cell.is_empty() {
                                self.join_cell = snap.cell;
                            }
                            if !snap.pad.is_empty() {
                                self.join_pad = snap.pad;
                            }
                        }
                    });
                });
                ui.add(
                    TextEdit::singleline(&mut self.join_map)
                        .hint_text("e.g. battleon-99999")
                        .desired_width(f32::INFINITY),
                );

                ui.add_space(4.0);
                let half = ((ui.available_width() - 6.0) * 0.5).max(70.0);
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 6.0;
                    ui.vertical(|ui| {
                        ui.label(RichText::new("Cell").small().weak());
                        ui.add(
                            TextEdit::singleline(&mut self.join_cell)
                                .hint_text("Enter")
                                .desired_width(half),
                        );
                    });
                    ui.vertical(|ui| {
                        ui.label(RichText::new("Pad").small().weak());
                        ui.add(
                            TextEdit::singleline(&mut self.join_pad)
                                .hint_text("Spawn")
                                .desired_width(half),
                        );
                    });
                });

                ui.add_space(6.0);
                if theme::primary_button(ui, "+ Join Map", [ui.available_width(), 22.0]).clicked()
                    && !self.join_map.trim().is_empty()
                {
                    self.push_command(
                        cfg,
                        BotCommand::Join {
                            map: self.join_map.trim().to_string(),
                            cell: non_empty_or(&self.join_cell, "Enter"),
                            pad: non_empty_or(&self.join_pad, "Spawn"),
                        },
                    );
                    dirty = true;
                }
            });

            group_box(&mut cols[1], "Walk To Position", |ui| {
                ui.horizontal(|ui| {
                    ui.label("X");
                    theme::num_input(ui, &mut self.walk_x, 0..=960, 58.0);
                    ui.add_space(6.0);
                    ui.label("Y");
                    theme::num_input(ui, &mut self.walk_y, 0..=550, 58.0);
                });
                ui.add_space(4.0);
                if theme::primary_button(ui, "+ Walk To", [ui.available_width(), 22.0]).clicked() {
                    self.push_command(
                        cfg,
                        BotCommand::Walk {
                            x: self.walk_x,
                            y: self.walk_y,
                        },
                    );
                    dirty = true;
                }
            });

            cols[1].add_space(8.0);
            group_box(&mut cols[1], "Jump Cell", |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Cell / Pad").small().weak());
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if theme::primary_btn(ui, "Use Current")
                            .on_hover_text("Fill cell and pad from current player location")
                            .clicked()
                        {
                            let snap = self.bot.player_snapshot();
                            if !snap.cell.is_empty() {
                                self.jump_cell = snap.cell;
                            }
                            if !snap.pad.is_empty() {
                                self.jump_pad = snap.pad;
                            }
                        }
                    });
                });
                let half = ((ui.available_width() - 6.0) * 0.5).max(70.0);
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 6.0;
                    ui.add(
                        TextEdit::singleline(&mut self.jump_cell)
                            .hint_text("Cell (e.g. Enter)")
                            .desired_width(half),
                    );
                    ui.add(
                        TextEdit::singleline(&mut self.jump_pad)
                            .hint_text("Pad (e.g. Spawn)")
                            .desired_width(half),
                    );
                });
                ui.add_space(5.0);
                ui.horizontal(|ui| {
                    let btn_w = ((ui.available_width() - 5.0) * 0.5).max(60.0);
                    if theme::primary_button(ui, "Jump Cell", [btn_w, 22.0]).clicked() {
                        self.push_command(
                            cfg,
                            BotCommand::MoveToCell {
                                cell: non_empty_or(&self.jump_cell, "Enter"),
                                pad: non_empty_or(&self.jump_pad, "Spawn"),
                            },
                        );
                        dirty = true;
                    }
                    if theme::primary_button(ui, "Set Spawn", [btn_w, 22.0]).clicked() {
                        self.push_command(cfg, BotCommand::SetSpawn);
                        dirty = true;
                    }
                });
            });
        });
        dirty
    }

}
