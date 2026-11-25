//! Grimlite Ri cross-platform egui overlay & tool windows.

use egui::{Color32, ComboBox, Context, Panel, RichText, ScrollArea, Slider, TextEdit, Ui, Window};
use grimlite_bot::Bot;
use grimlite_bot::options::BotOptions;
use grimlite_bot::tamper::{TamperAction, TamperDirection};
use std::path::PathBuf;
use std::sync::Arc;

pub const NAV_BAR_HEIGHT: f32 = 34.0;

pub type FilePickerFn = Box<dyn Fn() -> Option<PathBuf> + Send + Sync>;

pub struct GrimliteUi {
    bot: Arc<Bot>,
    pick_gbot: Option<FilePickerFn>,

    show_bot_manager: bool,
    show_options: bool,
    show_packet_logger: bool,
    show_spammer: bool,
    show_tamperer: bool,
    show_logs: bool,

    // Bot Manager state
    gbot_path_input: String,
    gbot_status: String,

    // Options draft state
    options: BotOptions,

    // Packet Logger state
    packet_filter: String,
    last_packet_version: u64,
    cached_packets: Vec<(String, String)>,

    // Packet Spammer state
    spammer_text: String,
    spammer_delay_ms: u64,
    raw_packet_input: String,
    raw_send_status: String,

    // Packet Tamperer state
    tamper_dir: TamperDirection,
    tamper_contains: String,
    tamper_is_block: bool,
    tamper_from: String,
    tamper_to: String,
}

impl GrimliteUi {
    pub fn new(bot: Arc<Bot>, pick_gbot: Option<FilePickerFn>) -> Self {
        let options = bot.options.get();
        Self {
            bot,
            pick_gbot,
            show_bot_manager: false,
            show_options: false,
            show_packet_logger: false,
            show_spammer: false,
            show_tamperer: false,
            show_logs: false,
            gbot_path_input: String::new(),
            gbot_status: String::new(),
            options,
            packet_filter: String::new(),
            last_packet_version: 0,
            cached_packets: Vec::new(),
            spammer_text: String::new(),
            spammer_delay_ms: 1000,
            raw_packet_input: String::new(),
            raw_send_status: String::new(),
            tamper_dir: TamperDirection::Outbound,
            tamper_contains: String::new(),
            tamper_is_block: false,
            tamper_from: String::new(),
            tamper_to: String::new(),
        }
    }

    pub fn show(&mut self, ctx: &Context, egui_ui: &mut Ui) {
        Panel::top("grimlite_nav_bar")
            .exact_size(NAV_BAR_HEIGHT)
            .show(egui_ui, |ui| {
                ui.horizontal_centered(|ui| {
                    ui.label(
                        RichText::new("Grimlite Ri")
                            .strong()
                            .color(Color32::from_rgb(230, 57, 70)),
                    );
                    ui.separator();

                    if ui
                        .selectable_label(self.show_bot_manager, "Bot")
                        .clicked()
                    {
                        self.show_bot_manager = !self.show_bot_manager;
                    }
                    if ui.selectable_label(self.show_options, "Options").clicked() {
                        self.show_options = !self.show_options;
                    }

                    ui.menu_button("Tools", |ui| {
                        if ui
                            .selectable_label(self.show_packet_logger, "Packet Logger")
                            .clicked()
                        {
                            self.show_packet_logger = !self.show_packet_logger;
                            ui.close();
                        }
                        if ui
                            .selectable_label(self.show_spammer, "Packet Spammer & Sender")
                            .clicked()
                        {
                            self.show_spammer = !self.show_spammer;
                            ui.close();
                        }
                        if ui
                            .selectable_label(self.show_tamperer, "Packet Tamperer / Interceptor")
                            .clicked()
                        {
                            self.show_tamperer = !self.show_tamperer;
                            ui.close();
                        }
                    });

                    if ui.selectable_label(self.show_logs, "Logs").clicked() {
                        self.show_logs = !self.show_logs;
                    }

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let snap = self.bot.player_snapshot();
                        if snap.logged_in {
                            let bot_badge = if self.bot.runner.is_running() {
                                if self.bot.runner.is_paused() {
                                    " [PAUSED]"
                                } else {
                                    " [BOT ON]"
                                }
                            } else {
                                ""
                            };
                            let info = format!(
                                "{} | {}-{} ({}) | HP {}/{}{}",
                                snap.username,
                                snap.map,
                                self.bot.state.room_id(),
                                snap.cell,
                                snap.hp,
                                snap.max_hp,
                                bot_badge
                            );
                            ui.label(RichText::new(info).color(Color32::from_rgb(120, 220, 150)));
                        } else if self.bot.is_game_loaded() {
                            ui.label(
                                RichText::new("● AQW Ready")
                                    .color(Color32::from_rgb(80, 200, 120)),
                            );
                        } else {
                            ui.label(
                                RichText::new("○ Loading...")
                                    .color(Color32::from_rgb(240, 180, 70)),
                            );
                        }
                    });
                });
            });

        if self.show_bot_manager {
            self.show_bot_manager_window(ctx);
        }
        if self.show_options {
            self.show_options_window(ctx);
        }
        if self.show_packet_logger {
            self.show_packet_logger_window(ctx);
        }
        if self.show_spammer {
            self.show_spammer_window(ctx);
        }
        if self.show_tamperer {
            self.show_tamperer_window(ctx);
        }
        if self.show_logs {
            self.show_logs_window(ctx);
        }
    }

    fn show_bot_manager_window(&mut self, ctx: &Context) {
        let mut open = self.show_bot_manager;
        Window::new("Bot Manager (.gbot)")
            .open(&mut open)
            .default_size([560.0, 420.0])
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    if let Some(picker) = &self.pick_gbot
                        && ui.button("Browse .gbot...").clicked()
                        && let Some(path) = picker()
                    {
                        self.gbot_path_input = path.display().to_string();
                        self.gbot_status = match self.bot.load_gbot_file(&path) {
                            Ok(()) => format!("Loaded {}", self.bot.runner.loaded_name()),
                            Err(e) => format!("Error: {e}"),
                        };
                    }
                    ui.add(
                        TextEdit::singleline(&mut self.gbot_path_input)
                            .desired_width(260.0)
                            .hint_text("/path/to/bot.gbot"),
                    );
                    if ui.button("Load").clicked() {
                        let p = PathBuf::from(self.gbot_path_input.trim());
                        self.gbot_status = match self.bot.load_gbot_file(&p) {
                            Ok(()) => format!("Loaded {}", self.bot.runner.loaded_name()),
                            Err(e) => format!("Error: {e}"),
                        };
                    }
                });

                if !self.gbot_status.is_empty() {
                    ui.label(RichText::new(&self.gbot_status).small());
                }

                ui.separator();
                let running = self.bot.runner.is_running();
                let paused = self.bot.runner.is_paused();
                let cfg = self.bot.runner.config();

                ui.horizontal(|ui| {
                    if !running {
                        if ui
                            .add_enabled(!cfg.commands.is_empty(), egui::Button::new("▶ Start"))
                            .clicked()
                        {
                            self.bot.start_bot();
                        }
                    } else {
                        if ui.button("⏹ Stop").clicked() {
                            self.bot.stop_bot();
                        }
                        if ui
                            .button(if paused { "▶ Resume" } else { "⏸ Pause" })
                            .clicked()
                        {
                            self.bot.runner.set_paused(!paused);
                        }
                    }
                    ui.separator();
                    let name = self.bot.runner.loaded_name();
                    let title = if name.is_empty() {
                        "No bot loaded".to_string()
                    } else {
                        format!(
                            "{name} ({} cmds, {} drops, {} skills)",
                            cfg.commands.len(),
                            cfg.drops.len(),
                            cfg.skills.len()
                        )
                    };
                    ui.label(title);
                });

                ui.separator();
                let active_idx = self.bot.runner.current_index();
                ScrollArea::vertical().show(ui, |ui| {
                    for (i, cmd) in cfg.commands.iter().enumerate() {
                        let is_cur = running && i == active_idx;
                        let prefix = if is_cur { "▶ " } else { "  " };
                        let text = format!("{prefix}[{i:02}] {cmd}");
                        let rt = if is_cur {
                            RichText::new(text)
                                .monospace()
                                .strong()
                                .color(Color32::from_rgb(120, 225, 150))
                        } else {
                            RichText::new(text).monospace()
                        };
                        ui.label(rt);
                    }
                });
            });
        self.show_bot_manager = open;
    }

    fn show_options_window(&mut self, ctx: &Context) {
        let mut open = self.show_options;
        let mut changed = false;
        Window::new("Game Options")
            .open(&mut open)
            .default_size([320.0, 280.0])
            .show(ctx, |ui| {
                changed |= ui
                    .checkbox(&mut self.options.infinite_range, "Infinite Attack Range")
                    .changed();
                changed |= ui
                    .checkbox(&mut self.options.provoke_monsters, "Provoke Monsters")
                    .changed();
                changed |= ui
                    .checkbox(&mut self.options.enemy_magnet, "Enemy Magnet")
                    .changed();
                changed |= ui
                    .checkbox(&mut self.options.lag_killer, "Lag Killer (Hide World)")
                    .changed();
                changed |= ui
                    .checkbox(&mut self.options.disable_animations, "Disable Animations")
                    .changed();
                changed |= ui
                    .checkbox(&mut self.options.skip_cutscenes, "Skip Cutscenes")
                    .changed();

                ui.separator();
                changed |= ui
                    .add(Slider::new(&mut self.options.walk_speed, 4..=40).text("Walk Speed"))
                    .changed();
                changed |= ui
                    .add(Slider::new(&mut self.options.fps, 10..=60).text("Stage FPS"))
                    .changed();
            });
        if changed {
            self.bot.apply_options(self.options.clone());
        }
        self.show_options = open;
    }

    fn show_packet_logger_window(&mut self, ctx: &Context) {
        let ver = self.bot.capture.version();
        if ver != self.last_packet_version {
            let (_, packets) = self.bot.capture.snapshot();
            self.cached_packets = packets
                .into_iter()
                .map(|p| (p.direction().to_string(), p.packet))
                .collect();
            self.last_packet_version = ver;
        }

        let mut open = self.show_packet_logger;
        Window::new("Packet Logger (Ruffle Socket Capture)")
            .open(&mut open)
            .default_size([680.0, 400.0])
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let mut logging = self.bot.capture.is_logging();
                    if ui.checkbox(&mut logging, "Log packets").changed() {
                        self.bot.capture.set_logging(logging);
                    }
                    if ui.button("Clear").clicked() {
                        self.bot.capture.clear();
                    }
                    ui.separator();
                    ui.label("Filter:");
                    ui.text_edit_singleline(&mut self.packet_filter);
                });
                ui.separator();
                ScrollArea::vertical()
                    .stick_to_bottom(true)
                    .show(ui, |ui| {
                        let filter = self.packet_filter.to_lowercase();
                        for (dir, pkt) in &self.cached_packets {
                            if !filter.is_empty() && !pkt.to_lowercase().contains(&filter) {
                                continue;
                            }
                            let color = match dir.as_str() {
                                "OUT" => Color32::from_rgb(120, 190, 255),
                                "IN" => Color32::from_rgb(130, 225, 150),
                                _ => Color32::from_rgb(255, 110, 110),
                            };
                            ui.horizontal_wrapped(|ui| {
                                ui.label(RichText::new(format!("[{dir}]")).monospace().color(color));
                                ui.label(RichText::new(pkt).monospace());
                            });
                        }
                    });
            });
        self.show_packet_logger = open;
    }

    fn show_spammer_window(&mut self, ctx: &Context) {
        let mut open = self.show_spammer;
        Window::new("Packet Spammer & Sender")
            .open(&mut open)
            .default_size([520.0, 340.0])
            .show(ctx, |ui| {
                ui.label("Single Packet (Direct Socket Injection):");
                ui.add(
                    TextEdit::singleline(&mut self.raw_packet_input)
                        .desired_width(f32::INFINITY)
                        .hint_text("%xt%zm%...%"),
                );
                ui.horizontal(|ui| {
                    if ui.button("Send to Server").clicked() {
                        self.raw_send_status = match self
                            .bot
                            .capture
                            .send_to_server(&self.raw_packet_input)
                        {
                            Ok(()) => "Sent to server".to_string(),
                            Err(e) => format!("Error: {e}"),
                        };
                    }
                    if ui.button("Send to Client").clicked() {
                        self.raw_send_status = match self
                            .bot
                            .capture
                            .send_to_client(&self.raw_packet_input)
                        {
                            Ok(()) => "Injected to client".to_string(),
                            Err(e) => format!("Error: {e}"),
                        };
                    }
                    if !self.raw_send_status.is_empty() {
                        ui.label(&self.raw_send_status);
                    }
                });

                ui.separator();
                ui.label("Spammer Queue (one packet per line):");
                ui.add(
                    TextEdit::multiline(&mut self.spammer_text)
                        .desired_rows(6)
                        .desired_width(f32::INFINITY)
                        .code_editor(),
                );
                ui.horizontal(|ui| {
                    ui.add(Slider::new(&mut self.spammer_delay_ms, 100..=5000).text("Delay (ms)"));
                    let running = self.bot.spammer.is_running();
                    if running {
                        if ui.button("Stop Spammer").clicked() {
                            self.bot.stop_spammer();
                        }
                    } else if ui.button("Start Spammer").clicked() {
                        let lines: Vec<String> = self
                            .spammer_text
                            .lines()
                            .map(str::to_string)
                            .collect();
                        self.bot.start_spammer(lines, self.spammer_delay_ms);
                    }
                });
            });
        self.show_spammer = open;
    }

    fn show_tamperer_window(&mut self, ctx: &Context) {
        let mut open = self.show_tamperer;
        Window::new("Packet Tamperer / Interceptor")
            .open(&mut open)
            .default_size([560.0, 360.0])
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ComboBox::from_id_salt("tamper_dir")
                        .selected_text(self.tamper_dir.label())
                        .show_ui(ui, |ui| {
                            ui.selectable_value(
                                &mut self.tamper_dir,
                                TamperDirection::Outbound,
                                "OUT",
                            );
                            ui.selectable_value(
                                &mut self.tamper_dir,
                                TamperDirection::Inbound,
                                "IN",
                            );
                            ui.selectable_value(&mut self.tamper_dir, TamperDirection::Both, "BOTH");
                        });
                    ui.checkbox(&mut self.tamper_is_block, "Block");
                    ui.label("If contains:");
                    ui.text_edit_singleline(&mut self.tamper_contains);
                });
                if !self.tamper_is_block {
                    ui.horizontal(|ui| {
                        ui.label("Replace:");
                        ui.text_edit_singleline(&mut self.tamper_from);
                        ui.label("With:");
                        ui.text_edit_singleline(&mut self.tamper_to);
                    });
                }
                if ui.button("+ Add Rule").clicked() {
                    let action = if self.tamper_is_block {
                        TamperAction::Block
                    } else {
                        TamperAction::Replace {
                            from: self.tamper_from.clone(),
                            to: self.tamper_to.clone(),
                        }
                    };
                    self.bot
                        .tamper
                        .add_rule(self.tamper_dir, self.tamper_contains.clone(), action);
                }

                ui.separator();
                ScrollArea::vertical().show(ui, |ui| {
                    for rule in self.bot.tamper.rules() {
                        ui.horizontal(|ui| {
                            let mut enabled = rule.enabled;
                            if ui.checkbox(&mut enabled, "").changed() {
                                self.bot.tamper.set_enabled(rule.id, enabled);
                            }
                            let desc = match &rule.action {
                                TamperAction::Block => format!(
                                    "[{}] BLOCK if contains `{}`",
                                    rule.direction.label(),
                                    rule.contains
                                ),
                                TamperAction::Replace { from, to } => format!(
                                    "[{}] REPLACE `{from}` -> `{to}` (match `{}`)",
                                    rule.direction.label(),
                                    rule.contains
                                ),
                            };
                            ui.label(RichText::new(desc).monospace());
                            if ui.button("✕").clicked() {
                                self.bot.tamper.remove_rule(rule.id);
                            }
                        });
                    }
                });
            });
        self.show_tamperer = open;
    }

    fn show_logs_window(&mut self, ctx: &Context) {
        let mut open = self.show_logs;
        Window::new("Debug Logs")
            .open(&mut open)
            .default_size([520.0, 300.0])
            .show(ctx, |ui| {
                if ui.button("Clear").clicked() {
                    self.bot.logs.clear_debug();
                }
                ui.separator();
                ScrollArea::vertical()
                    .stick_to_bottom(true)
                    .show(ui, |ui| {
                        for line in self.bot.logs.debug_lines() {
                            ui.label(RichText::new(line).monospace());
                        }
                    });
            });
        self.show_logs = open;
    }
}
