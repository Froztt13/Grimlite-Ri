//! Bot Manager — Options tab.

use super::{BotManagerWindow, group_box, two_columns};
use crate::theme;
use egui::{TextEdit, Ui};
use grimlite_bot::gbot::BotConfig;
use grimlite_bot::options::AQW_SERVERS;

impl BotManagerWindow {
    // ---- Tab 8: Options ----

    pub(super) fn show_options_tab(&mut self, ui: &mut Ui, cfg: &mut BotConfig) -> bool {
        let mut dirty = false;
        let mut live_opt_changed = false;

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

            // Column 2: Game Options (mirrors Grimlite-Li tabOptions)
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
                let c6 = ui
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
                        // Immediate walk speed toggle like Grimlite-Li chkWalkSpeed_CheckedChanged
                        let mut opts = self.bot.options.get();
                        opts.walk_speed_enabled = cfg.walk_speed_enabled;
                        opts.walk_speed = cfg.walk_speed.clamp(1, 99);
                        self.bot.apply_options(opts);
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
                    ui.add_enabled_ui(!self.enable_options_live, |ui| {
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

                if c1 || c2 || c3 || c4 || c5 || c6 || c8 || c9 || c10 {
                    dirty = true;
                    live_opt_changed = true;
                }

                ui.add_space(3.0);
                ui.separator();
                if ui
                    .checkbox(
                        &mut self.enable_options_live,
                        "Enable options without starting",
                    )
                    .changed()
                {
                    live_opt_changed = true;
                }
            });
        });

        if live_opt_changed {
            let mut opts = self.bot.options.get();
            if self.enable_options_live {
                opts.infinite_range = cfg.infinite_attack_range;
                opts.provoke_monsters = cfg.provoke_monsters;
                opts.enemy_magnet = cfg.enemy_magnet;
                opts.lag_killer = cfg.lag_killer;
                opts.hide_players = cfg.hide_players;
                opts.skip_cutscenes = cfg.skip_cutscenes;
                opts.disable_animations = cfg.disable_animations;
                opts.walk_speed_enabled = cfg.walk_speed_enabled;
                opts.walk_speed = cfg.walk_speed.clamp(1, 99);
                opts.anti_counter = cfg.anti_counter;
                opts.follow_enabled = cfg.follow_check && !cfg.follow_name.trim().is_empty();
                opts.follow_player = cfg.follow_name.clone();
            } else if !self.bot.runner.is_running() {
                opts.infinite_range = false;
                opts.provoke_monsters = false;
                opts.enemy_magnet = false;
                opts.lag_killer = false;
                opts.hide_players = false;
                opts.skip_cutscenes = false;
                opts.disable_animations = false;
                opts.anti_counter = cfg.anti_counter;
                opts.follow_enabled = false;
            }
            self.bot.apply_options(opts);
        }

        dirty
    }

}
