//! Bot Manager — Misc tab.

use super::{BotManagerWindow, group_box, two_columns};
use crate::theme;
use egui::{Color32, RichText, ScrollArea, TextEdit, Ui};
use grimlite_bot::gbot::{BotCommand, BotConfig};

impl BotManagerWindow {
    // ---- Tab 6: Misc ----

    pub(super) fn show_misc_tab(&mut self, ui: &mut Ui, cfg: &mut BotConfig) -> bool {
        let mut dirty = false;
        two_columns(ui, |cols| {
            // Column 1: Label (actions + zebra list) + Bot Controls + Log & Comment
            group_box(&mut cols[0], "Label", |ui| {
                ui.add(
                    TextEdit::singleline(&mut self.label_name)
                        .hint_text("Label name (e.g. LOOP)")
                        .desired_width(f32::INFINITY),
                );
                ui.add_space(4.0);
                let half = ((ui.available_width() - 5.0) * 0.5).max(60.0);
                ui.horizontal(|ui| {
                    if theme::primary_button(ui, "+ Label", [half, 21.0]).clicked()
                        && !self.label_name.trim().is_empty()
                    {
                        self.push_command(
                            cfg,
                            BotCommand::Label {
                                name: self.label_name.trim().to_string(),
                            },
                        );
                        dirty = true;
                    }
                    if theme::primary_button(ui, "Goto Label", [half, 21.0]).clicked()
                        && !self.label_name.trim().is_empty()
                    {
                        self.push_command(
                            cfg,
                            BotCommand::GotoLabel {
                                name: self.label_name.trim().to_string(),
                            },
                        );
                        dirty = true;
                    }
                });

                let existing_labels: Vec<String> = cfg
                    .commands
                    .iter()
                    .filter_map(|c| match c {
                        BotCommand::Label { name } if !name.trim().is_empty() => Some(name.clone()),
                        _ => None,
                    })
                    .collect();

                ui.add_space(4.0);
                egui::Frame::group(ui.style())
                    .inner_margin(egui::Margin::symmetric(2, 2))
                    .show(ui, |ui| {
                        ui.set_min_width(ui.available_width());
                        ScrollArea::vertical()
                            .id_salt("misc_existing_labels")
                            .max_height(66.0)
                            .auto_shrink([false, true])
                            .show(ui, |ui| {
                                ui.spacing_mut().item_spacing.y = 1.0;
                                if existing_labels.is_empty() {
                                    ui.label(
                                        RichText::new("No labels in commands")
                                            .small()
                                            .weak()
                                            .italics(),
                                    );
                                    return;
                                }
                                let mut goto_clicked: Option<String> = None;
                                for (row_idx, name) in existing_labels.iter().enumerate() {
                                    let is_cur = self.label_name.eq_ignore_ascii_case(name);
                                    let row_fill = if is_cur {
                                        Color32::from_rgb(16, 82, 118)
                                    } else if row_idx % 2 == 1 {
                                        Color32::from_rgba_unmultiplied(255, 255, 255, 6)
                                    } else {
                                        Color32::TRANSPARENT
                                    };
                                    let mut rt =
                                        RichText::new(format!("[{name}]")).monospace().size(11.0);
                                    if is_cur {
                                        rt = rt.color(Color32::WHITE);
                                    }
                                    let resp = egui::Frame::new()
                                        .fill(row_fill)
                                        .corner_radius(2.5)
                                        .inner_margin(egui::Margin::symmetric(5, 2))
                                        .show(ui, |ui| {
                                            ui.set_min_width(ui.available_width());
                                            ui.add(
                                                egui::Label::new(rt).truncate().selectable(false),
                                            )
                                        })
                                        .response
                                        .interact(egui::Sense::click())
                                        .on_hover_text(
                                            "Click to select • Double-click to add Goto Label",
                                        );
                                    if resp.double_clicked() {
                                        self.label_name = name.clone();
                                        goto_clicked = Some(name.clone());
                                    } else if resp.clicked() {
                                        self.label_name = name.clone();
                                    }
                                }
                                if let Some(target) = goto_clicked {
                                    self.push_command(cfg, BotCommand::GotoLabel { name: target });
                                    dirty = true;
                                }
                            });
                    });
            });

            cols[0].add_space(6.0);
            group_box(&mut cols[0], "Bot Controls", |ui| {
                let third = ((ui.available_width() - 8.0) / 3.0).max(48.0);
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 4.0;
                    if theme::primary_button(ui, "Restart", [third, 21.0]).clicked() {
                        self.push_command(cfg, BotCommand::Restart);
                        dirty = true;
                    }
                    if theme::primary_button(ui, "Stop", [third, 21.0]).clicked() {
                        self.push_command(cfg, BotCommand::Stop);
                        dirty = true;
                    }
                    if theme::primary_button(ui, "Logout", [third, 21.0]).clicked() {
                        self.push_command(cfg, BotCommand::Logout);
                        dirty = true;
                    }
                });
            });

            cols[0].add_space(6.0);
            group_box(&mut cols[0], "Logs", |ui| {
                ui.add(
                    TextEdit::multiline(&mut self.log_text)
                        .hint_text("Text")
                        .desired_rows(2)
                        .desired_width(f32::INFINITY),
                );
                ui.add_space(4.0);
                let half = ((ui.available_width() - 5.0) * 0.5).max(60.0);
                ui.horizontal(|ui| {
                    if theme::primary_button(ui, "Log Script", [half, 21.0]).clicked() {
                        self.push_command(
                            cfg,
                            BotCommand::Log {
                                text: self.log_text.clone(),
                                debug: false,
                            },
                        );
                        dirty = true;
                    }
                    if theme::primary_button(ui, "Log Debug", [half, 21.0]).clicked() {
                        self.push_command(
                            cfg,
                            BotCommand::Log {
                                text: self.log_text.clone(),
                                debug: true,
                            },
                        );
                        dirty = true;
                    }
                });
                ui.add_space(4.0);
                ui.label(RichText::new("Viable Log References").small().weak());
                egui::Frame::group(ui.style())
                    .inner_margin(egui::Margin::symmetric(2, 2))
                    .show(ui, |ui| {
                        ui.set_min_width(ui.available_width());
                        ScrollArea::vertical()
                            .id_salt("misc_log_references")
                            .max_height(64.0)
                            .auto_shrink([false, true])
                            .show(ui, |ui| {
                                ui.spacing_mut().item_spacing.y = 1.0;
                                for (row_idx, &token) in LOG_REFERENCES.iter().enumerate() {
                                    let row_fill = if row_idx % 2 == 1 {
                                        Color32::from_rgba_unmultiplied(255, 255, 255, 6)
                                    } else {
                                        Color32::TRANSPARENT
                                    };
                                    let resp = egui::Frame::new()
                                        .fill(row_fill)
                                        .corner_radius(2.5)
                                        .inner_margin(egui::Margin::symmetric(5, 1))
                                        .show(ui, |ui| {
                                            ui.set_min_width(ui.available_width());
                                            ui.add(
                                                egui::Label::new(
                                                    RichText::new(token).monospace().size(10.5),
                                                )
                                                .truncate()
                                                .selectable(false),
                                            )
                                        })
                                        .response
                                        .interact(egui::Sense::click())
                                        .on_hover_text("Click to insert into log text");
                                    if resp.clicked() {
                                        self.log_text.push_str(token);
                                    }
                                }
                            });
                    });
            });

            // Column 2: Miscellaneous (Delay, Play Sound, Provoke, Logout, Goto Player) + Packet
            group_box(&mut cols[1], "Miscellaneous", |ui| {
                let btn_w = 86.0;

                ui.horizontal(|ui| {
                    ui.label("Delay");
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if theme::primary_button(ui, "+ Delay", [btn_w, 21.0]).clicked()
                            && let Ok(ms) = self.delay_ms.trim().parse::<u64>()
                        {
                            self.push_command(cfg, BotCommand::Delay { ms });
                            dirty = true;
                        }
                        ui.label("ms");
                        ui.add(
                            TextEdit::singleline(&mut self.delay_ms)
                                .hint_text("1000")
                                .desired_width(ui.available_width().max(28.0)),
                        );
                    });
                });

                ui.add_space(3.0);
                ui.horizontal(|ui| {
                    ui.label("Sound");
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if theme::primary_button(ui, "Play Sound", [btn_w, 21.0]).clicked() {
                            self.push_command(
                                cfg,
                                BotCommand::Beep {
                                    times: self.beep_times.max(1),
                                },
                            );
                            dirty = true;
                        }
                        ui.label("x");
                        let num_w = ui.available_width().max(28.0);
                        theme::num_input(ui, &mut self.beep_times, 1..=50, num_w);
                    });
                });

                ui.add_space(3.0);
                ui.horizontal(|ui| {
                    ui.label("Level");
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if theme::primary_button(ui, "Set Level", [btn_w, 21.0]).clicked()
                            && let Ok(lvl) = self.misc_level.trim().parse::<i32>()
                        {
                            let pkt = format!(
                                r#"{{"t":"xt","b":{{"r":-1,"o":{{"cmd":"levelUp","intExpToLevel":"2000","intLevel":{lvl}}}}}}}"#
                            );
                            self.push_command(
                                cfg,
                                BotCommand::Packet {
                                    packet: pkt,
                                    to_client: true,
                                    spam_times: 1,
                                    delay_ms: 500,
                                },
                            );
                            dirty = true;
                        }
                        ui.add(
                            TextEdit::singleline(&mut self.misc_level)
                                .hint_text("100")
                                .desired_width(ui.available_width().max(28.0)),
                        );
                    });
                });

                ui.add_space(3.0);
                ui.horizontal(|ui| {
                    ui.label("FPS");
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if theme::primary_button(ui, "Set FPS", [btn_w, 21.0]).clicked() {
                            self.push_command(
                                cfg,
                                BotCommand::SetFPS {
                                    fps: self.misc_fps.clamp(1, 60),
                                },
                            );
                            dirty = true;
                        }
                        let num_w = ui.available_width().max(28.0);
                        theme::num_input(ui, &mut self.misc_fps, 1..=60, num_w);
                    });
                });

                ui.add_space(4.0);
                let half = ((ui.available_width() - 5.0) * 0.5).floor().max(48.0);
                ui.horizontal(|ui| {
                    if theme::primary_button(ui, "Provoke ON", [half, 21.0]).clicked() {
                        self.push_command(cfg, BotCommand::Provoke { enabled: true });
                        dirty = true;
                    }
                    if theme::primary_button(ui, "Provoke OFF", [half, 21.0]).clicked() {
                        self.push_command(cfg, BotCommand::Provoke { enabled: false });
                        dirty = true;
                    }
                });

                ui.add_space(4.0);
                ui.separator();
                ui.add_space(2.0);
                ui.horizontal(|ui| {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if theme::primary_button(ui, "Goto Player", [btn_w, 21.0]).clicked()
                            && !self.goto_player_name.trim().is_empty()
                        {
                            self.push_command(
                                cfg,
                                BotCommand::GotoPlayer {
                                    player_name: self.goto_player_name.trim().to_string(),
                                },
                            );
                            dirty = true;
                        }
                        ui.add(
                            TextEdit::singleline(&mut self.goto_player_name)
                                .hint_text("Player name")
                                .desired_width(ui.available_width().max(32.0)),
                        );
                    });
                });
            });

            cols[1].add_space(6.0);
            group_box(&mut cols[1], "Packet", |ui| {
                ui.add(
                    TextEdit::singleline(&mut self.packet_text)
                        .hint_text("%xt%zm%...%")
                        .desired_width(f32::INFINITY),
                );
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    let switch_w = 96.0;
                    theme::segmented_switch(
                        ui,
                        &mut self.packet_to_client,
                        "Server",
                        "Client",
                        switch_w,
                    );
                    let rem = ui.available_width().floor().max(48.0);
                    if theme::primary_button(ui, "+ Packet", [rem, 21.0]).clicked()
                        && !self.packet_text.trim().is_empty()
                    {
                        self.push_command(
                            cfg,
                            BotCommand::Packet {
                                packet: self.packet_text.trim().to_string(),
                                to_client: self.packet_to_client,
                                spam_times: 1,
                                delay_ms: 0,
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

const LOG_REFERENCES: &[&str] = &[
    "{USERNAME}",
    "{MAP}",
    "{GOLD}",
    "{LEVEL}",
    "{CELL}",
    "{PAD}",
    "{HEALTH}",
    "{MANA}",
    "{TIME: 12}",
    "{TIME: 24}",
    "{CLEAR}",
    "{ITEM: item name}",
    "{ITEM MAX: item name}",
    "{INT VALUE: int name}",
];
