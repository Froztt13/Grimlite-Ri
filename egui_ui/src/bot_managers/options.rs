//! Bot Manager — Options tab.
//! Shares the unified `bot.options` (`OptionsStore`) engine with the standalone Game Options window.

use super::{BotManagerWindow, group_box, two_columns};
use crate::theme;
use egui::{RichText, TextEdit, Ui};
use grimlite_bot::gbot::BotConfig;
use grimlite_bot::host::FlashArg;
use grimlite_bot::options::AQW_SERVERS;

impl BotManagerWindow {
    // ---- Tab 8: Options ----

    pub(super) fn show_options_tab(&mut self, ui: &mut Ui, cfg: &mut BotConfig) -> bool {
        let mut dirty = false;
        let mut opt_changed = false;
        let mut hide_players_toggled = false;
        let bot_running = self.bot.runner.is_running();
        let mut live_enabled = self.bot.options.is_live_without_starting();

        if cfg.server.trim().is_empty() {
            cfg.server = AQW_SERVERS[0].to_string();
        }

        two_columns(ui, |cols| {
            // Column 1: Auto Relogin + Bot Behavior
            group_box(&mut cols[0], "Auto Relogin", |ui| {
                ui.horizontal(|ui| {
                    ui.label("Server");
                    let prev_srv = cfg.server.clone();
                    let cb_w = ui.available_width().max(110.0);
                    theme::dropdown(ui, "bot_opt_server_cb", &prev_srv, cb_w, |ui| {
                        for &srv in AQW_SERVERS {
                            if ui
                                .selectable_value(&mut cfg.server, srv.to_string(), srv)
                                .changed()
                            {
                                dirty = true;
                            }
                        }
                    });
                });
                ui.add_space(2.0);
                if ui.checkbox(&mut cfg.auto_relogin, "Auto relogin").changed() {
                    dirty = true;
                    if !cfg.auto_relogin {
                        cfg.auto_relogin_any = false;
                    }
                }
                if ui
                    .add_enabled(
                        cfg.auto_relogin,
                        egui::Checkbox::new(&mut cfg.auto_relogin_any, "Relogin on AFK"),
                    )
                    .changed()
                {
                    dirty = true;
                }
                if ui
                    .add_enabled(
                        cfg.auto_relogin,
                        egui::Checkbox::new(
                            &mut cfg.relog_retry_upon_failure,
                            "Relog again if in battleon",
                        ),
                    )
                    .changed()
                {
                    dirty = true;
                }
                ui.horizontal(|ui| {
                    ui.label("Delay");
                    dirty |= theme::num_input(ui, &mut cfg.relog_delay, 1000..=20_000, 62.0)
                        .changed();
                    ui.label("ms");
                });
            });

            cols[0].add_space(6.0);
            group_box(&mut cols[0], "Bot Behavior", |ui| {
                ui.horizontal(|ui| {
                    ui.label("Bot");
                    dirty |= theme::num_input(ui, &mut cfg.bot_delay, 0..=5000, 50.0).changed();
                    ui.label("ms");
                    ui.add_space(4.0);
                    ui.label("Skill");
                    dirty |= theme::num_input(ui, &mut cfg.skill_delay, 0..=5000, 50.0).changed();
                    ui.label("ms");
                });
                ui.add_space(3.0);
                ui.separator();
                dirty |= ui
                    .checkbox(&mut cfg.restart_upon_death, "Restart upon death")
                    .changed();
                dirty |= ui
                    .checkbox(&mut cfg.restart_on_afk, "Restart bot on AFK")
                    .changed();
            });

            // Column 2: Game Options (unified with standalone Game Options window via `bot.options`)
            group_box(&mut cols[1], "Game Options", |ui| {
                let c1 = ui
                    .checkbox(&mut cfg.infinite_attack_range, "Infinite attack range")
                    .changed();
                let c2 = ui
                    .checkbox(&mut cfg.provoke_monsters, "Provoke monsters")
                    .changed();
                let c3 = ui.checkbox(&mut cfg.enemy_magnet, "Enemy magnet").changed();
                let c4 = ui.checkbox(&mut cfg.lag_killer, "Lag killer").changed();
                let c5 = ui.checkbox(&mut cfg.hide_players, "Hide players").changed();
                if c5 {
                    hide_players_toggled = true;
                }
                let c6 = ui
                    .checkbox(&mut cfg.disable_animations, "Disable FX / Anims")
                    .changed();
                let c7 = ui
                    .checkbox(&mut cfg.skip_cutscenes, "Skip cutscenes")
                    .changed();

                // Walk speed (checkbox locks numeric input while active, matching chkWalkSpeed)
                let mut c8 = false;
                ui.horizontal(|ui| {
                    if ui
                        .checkbox(&mut cfg.walk_speed_enabled, "Walk speed")
                        .changed()
                    {
                        c8 = true;
                    }
                    ui.add_enabled_ui(!cfg.walk_speed_enabled, |ui| {
                        if theme::num_input(ui, &mut cfg.walk_speed, 1..=99, 46.0).changed() {
                            c8 = true;
                        }
                    });
                });

                // Follow player (locked while live options are active, matching chkEnableSettings)
                let mut c9 = false;
                ui.horizontal(|ui| {
                    ui.add_enabled_ui(!live_enabled, |ui| {
                        c9 |= ui.checkbox(&mut cfg.follow_check, "Follow").changed();
                        c9 |= ui
                            .add(
                                TextEdit::singleline(&mut cfg.follow_name)
                                    .hint_text("Player name")
                                    .desired_width(ui.available_width().max(80.0)),
                            )
                            .changed();
                    });
                });

                // Anti counter attack (checked by default)
                let c10 = ui
                    .checkbox(&mut cfg.anti_counter, "Anti counter attack")
                    .changed();

                if c1 || c2 || c3 || c4 || c5 || c6 || c7 || c8 || c9 || c10 {
                    dirty = true;
                    opt_changed = true;
                }

                ui.add_space(3.0);
                ui.separator();
                ui.horizontal(|ui| {
                    if ui
                        .checkbox(&mut live_enabled, "Enable options without starting")
                        .changed()
                    {
                        self.bot.options.set_live_without_starting(live_enabled);
                        if live_enabled {
                            self.bot.options.sync_from_bot_config(cfg);
                            self.bot.spawn_flash_action(
                                "setHidePlayers",
                                vec![FlashArg::Bool(cfg.hide_players)],
                            );
                        } else if !bot_running {
                            self.bot.stop_active_options();
                        }
                    }
                    let active_now = bot_running || live_enabled;
                    ui.label(
                        RichText::new(if active_now { "Active" } else { "Idle" })
                            .small()
                            .color(if active_now {
                                theme::Palette::SUCCESS
                            } else {
                                theme::Palette::TEXT_MUTED
                            }),
                    );
                });
            });
        });

        if opt_changed && (bot_running || live_enabled) {
            self.bot.options.sync_from_bot_config(cfg);
            if hide_players_toggled {
                self.bot.spawn_flash_action(
                    "setHidePlayers",
                    vec![FlashArg::Bool(cfg.hide_players)],
                );
            }
        }

        dirty
    }
}
