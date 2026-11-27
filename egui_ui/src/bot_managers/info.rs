//! Bot Manager — Info tab.

use super::{BotManagerWindow, group_box, two_columns};
use egui::{RichText, Ui};
use grimlite_bot::gbot::BotConfig;

impl BotManagerWindow {
    // ---- Tab 10: Info ----

    pub(super) fn show_info_tab(&self, ui: &mut Ui, cfg: &BotConfig) {
        two_columns(ui, |cols| {
            group_box(&mut cols[0], "Bot Summary", |ui| {
                let loaded = self.bot.runner.loaded_name();
                egui::Grid::new("bot_info_summary_grid")
                    .num_columns(2)
                    .spacing([12.0, 4.0])
                    .show(ui, |ui| {
                        ui.label(RichText::new("Loaded File").weak());
                        ui.label(if loaded.is_empty() {
                            "(unsaved)"
                        } else {
                            &loaded
                        });
                        ui.end_row();

                        ui.label(RichText::new("Commands").weak());
                        ui.label(cfg.commands.len().to_string());
                        ui.end_row();

                        ui.label(RichText::new("Skills").weak());
                        ui.label(cfg.skills.len().to_string());
                        ui.end_row();

                        ui.label(RichText::new("Quests").weak());
                        ui.label(cfg.quests.len().to_string());
                        ui.end_row();

                        ui.label(RichText::new("Drops").weak());
                        ui.label(cfg.drops.len().to_string());
                        ui.end_row();
                    });
            });

            group_box(&mut cols[1], "Shortcuts", |ui| {
                for (key, desc) in [
                    ("Double-click", "Edit item"),
                    ("Shift + Click", "Select range"),
                    ("Cmd/Ctrl + Click", "Toggle select / Insert below"),
                    ("Cmd/Ctrl + Up/Down", "Reorder selected"),
                    ("Cmd/Ctrl + C / X / V", "Copy / Cut / Paste (.gbot)"),
                    ("Cmd/Ctrl + D", "Duplicate selected"),
                    ("Delete / Backspace", "Remove selected"),
                ] {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(key).small().monospace().strong());
                        ui.label(RichText::new(format!("— {desc}")).small().weak());
                    });
                }
            });
        });
    }
}
