//! Bot Manager — Bots tab (Load / Save `.gbot`).

use super::{BotManagerWindow, group_box, two_columns};
use crate::theme;
use egui::{RichText, TextEdit, Ui};
use grimlite_bot::gbot::BotConfig;

impl BotManagerWindow {
    // ---- Tab 9: Bots (Load / Save .gbot) ----

    pub(super) fn show_bots_tab(&mut self, ui: &mut Ui, cfg: &mut BotConfig, dirty: &mut bool) {
        two_columns(ui, |cols| {
            group_box(&mut cols[0], "Bot File (.gbot)", |ui| {
                let half = ((ui.available_width() - 5.0) * 0.5).max(60.0);
                ui.horizontal(|ui| {
                    if theme::primary_button(ui, "Load .gbot...", [half, 22.0]).clicked()
                        && let Some(pick) = &self.pick_gbot
                        && let Some(path) = pick()
                    {
                        match self.bot.load_gbot_file(&path) {
                            Ok(()) => {
                                *cfg = self.bot.runner.config();
                                self.selected.clear();
                                self.anchor = None;
                                self.status = format!("Loaded: {}", path.display());
                            }
                            Err(e) => self.status = format!("Load error: {e}"),
                        }
                    }
                    if theme::primary_button(ui, "Save .gbot...", [half, 22.0]).clicked()
                        && let Some(save) = &self.save_gbot
                        && let Some(path) = save()
                    {
                        self.bot.runner.update_config(cfg.clone());
                        match self.bot.save_gbot_file(&path) {
                            Ok(()) => self.status = format!("Saved: {}", path.display()),
                            Err(e) => self.status = format!("Save error: {e}"),
                        }
                    }
                });

                ui.add_space(6.0);
                ui.label(RichText::new("Author").small().weak());
                *dirty |= ui
                    .add(
                        TextEdit::singleline(&mut cfg.author)
                            .hint_text("Bot author")
                            .desired_width(f32::INFINITY),
                    )
                    .changed();

                if !self.status.is_empty() {
                    ui.add_space(5.0);
                    ui.label(RichText::new(&self.status).small().weak());
                }
            });

            group_box(&mut cols[1], "Description", |ui| {
                *dirty |= ui
                    .add(
                        TextEdit::multiline(&mut cfg.description)
                            .hint_text("Notes or requirements for this bot...")
                            .desired_rows(5)
                            .desired_width(f32::INFINITY),
                    )
                    .changed();
            });
        });
    }

}
