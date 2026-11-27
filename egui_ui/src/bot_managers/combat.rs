//! Bot Manager — Combat tab.

use super::{BotManagerWindow, group_box, non_empty_or, two_columns};
use crate::theme;
use egui::{TextEdit, Ui};
use grimlite_bot::gbot::{BotCommand, BotConfig};

impl BotManagerWindow {
    // ---- Tab 1: Combat (2 clean columns) ----

    pub(super) fn show_combat_tab(&mut self, ui: &mut Ui, cfg: &mut BotConfig) -> bool {
        let mut dirty = false;

        two_columns(ui, |cols| {
            // Column 1: Kill/attack once + Kill for items
            group_box(&mut cols[0], "Kill/attack once", |ui| {
                ui.add(
                    TextEdit::singleline(&mut self.combat_monster)
                        .hint_text("Monster (* = random)")
                        .desired_width(f32::INFINITY),
                );
                ui.add_space(3.0);
                ui.horizontal(|ui| {
                    let half = ((ui.available_width() - 5.0) * 0.5).max(50.0);
                    if theme::primary_button(ui, "Kill", [half, 21.0]).clicked() {
                        let m = non_empty_or(&self.combat_monster, "*");
                        self.push_command(cfg, BotCommand::Kill { monster: m });
                        dirty = true;
                    }
                    if theme::primary_button(ui, "Attack", [half, 21.0]).clicked() {
                        let m = non_empty_or(&self.combat_monster, "*");
                        self.push_command(cfg, BotCommand::Attack { monster: m });
                        dirty = true;
                    }
                });
            });

            cols[0].add_space(8.0);
            group_box(&mut cols[0], "Kill for items", |ui| {
                theme::segmented_switch(
                    ui,
                    &mut self.kf_is_temp,
                    "Items",
                    "Temp items",
                    ui.available_width(),
                );
                ui.add_space(2.0);
                ui.checkbox(&mut self.kf_add_whitelist, "Add to Whitelist");
                ui.add(
                    TextEdit::singleline(&mut self.kf_monster)
                        .hint_text("Monster (* = random)")
                        .desired_width(f32::INFINITY),
                );
                ui.add_space(2.0);
                ui.add(
                    TextEdit::singleline(&mut self.kf_item)
                        .hint_text("Item name")
                        .desired_width(f32::INFINITY),
                );
                ui.add_space(2.0);
                ui.add(
                    TextEdit::singleline(&mut self.kf_qty)
                        .hint_text("Quantity (* = any)")
                        .desired_width(f32::INFINITY),
                );
                ui.add_space(4.0);
                if theme::primary_button(ui, "Kill for [KF]", [ui.available_width(), 22.0])
                    .clicked()
                    && !self.kf_item.trim().is_empty()
                {
                    let item = self.kf_item.trim().to_string();
                    if self.kf_add_whitelist
                        && !self.kf_is_temp
                        && !cfg.drops.iter().any(|d| d.eq_ignore_ascii_case(&item))
                    {
                        cfg.drops.push(item.clone());
                    }
                    self.push_command(
                        cfg,
                        BotCommand::KillFor {
                            monster: non_empty_or(&self.kf_monster, "*"),
                            item_name: item,
                            quantity: non_empty_or(&self.kf_qty, "*"),
                            is_temp: self.kf_is_temp,
                        },
                    );
                    dirty = true;
                }
            });

            // Column 2: Rest + Combat Controls
            group_box(&mut cols[1], "Rest", |ui| {
                ui.horizontal(|ui| {
                    let half = ((ui.available_width() - 5.0) * 0.5).max(60.0);
                    if theme::primary_button(ui, "Rest", [half, 22.0]).clicked() {
                        self.push_command(cfg, BotCommand::Rest { full: false });
                        dirty = true;
                    }
                    if theme::primary_button(ui, "Rest fully", [half, 22.0]).clicked() {
                        self.push_command(cfg, BotCommand::Rest { full: true });
                        dirty = true;
                    }
                });
            });

            cols[1].add_space(8.0);
            group_box(&mut cols[1], "Combat Controls", |ui| {
                ui.horizontal(|ui| {
                    let half = ((ui.available_width() - 5.0) * 0.5).max(60.0);
                    if theme::primary_button(ui, "Stop attack", [half, 21.0]).clicked() {
                        self.push_command(cfg, BotCommand::CancelTarget);
                        dirty = true;
                    }
                    if theme::primary_button(ui, "Leave combat", [half, 21.0]).clicked() {
                        self.push_command(
                            cfg,
                            BotCommand::MoveToCell {
                                cell: "Blank".into(),
                                pad: "Spawn".into(),
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
