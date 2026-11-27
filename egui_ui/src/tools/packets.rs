//! Packets tool windows: Packet Logger, Packet Spammer & Sender, and Packet Tamperer.

use crate::show_detached_window;
use egui::{Color32, ComboBox, Context, RichText, ScrollArea, Slider, TextEdit, Ui};
use grimlite_bot::Bot;
use grimlite_bot::tamper::{TamperAction, TamperDirection};
use std::sync::Arc;

pub struct PacketsWindows {
    bot: Arc<Bot>,

    pub show_logger: bool,
    pub show_spammer: bool,
    pub show_tamperer: bool,

    // Packet Logger state
    packet_filter: String,
    last_packet_version: u64,
    cached_packets: Vec<(String, String, String)>,

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

impl PacketsWindows {
    pub fn new(bot: Arc<Bot>) -> Self {
        Self {
            bot,
            show_logger: false,
            show_spammer: false,
            show_tamperer: false,
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

    pub fn menu_items(&mut self, ui: &mut Ui) {
        if ui
            .selectable_label(self.show_logger, "Packet Logger")
            .clicked()
        {
            self.show_logger = !self.show_logger;
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
    }

    pub fn show(&mut self, ctx: &Context) {
        if self.show_logger {
            self.show_packet_logger_window(ctx);
        }
        if self.show_spammer {
            self.show_spammer_window(ctx);
        }
        if self.show_tamperer {
            self.show_tamperer_window(ctx);
        }
    }

    fn show_packet_logger_window(&mut self, ctx: &Context) {
        let ver = self.bot.capture.version();
        if ver != self.last_packet_version {
            let (_, packets) = self.bot.capture.snapshot();
            self.cached_packets = packets
                .into_iter()
                .map(|p| (p.timestamp.clone(), p.direction().to_string(), p.packet))
                .collect();
            self.last_packet_version = ver;
        }

        let mut open = self.show_logger;
        show_detached_window(
            ctx,
            "grimlite_packet_logger",
            "Packet Logger (Ruffle Socket Capture)",
            [680.0, 400.0],
            &mut open,
            |ui| {
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
                        for (ts, dir, pkt) in &self.cached_packets {
                            if !filter.is_empty() && !pkt.to_lowercase().contains(&filter) {
                                continue;
                            }
                            let color = match dir.as_str() {
                                "OUT" => Color32::from_rgb(120, 190, 255),
                                "IN" => Color32::from_rgb(130, 225, 150),
                                _ => Color32::from_rgb(255, 110, 110),
                            };
                            ui.horizontal_wrapped(|ui| {
                                ui.label(
                                    RichText::new(format!("[{ts}]"))
                                        .monospace()
                                        .color(Color32::from_rgb(130, 140, 160)),
                                );
                                ui.label(
                                    RichText::new(format!("[{dir}]")).monospace().color(color),
                                );
                                ui.label(RichText::new(pkt).monospace());
                            });
                        }
                    });
            },
        );
        self.show_logger = open;
    }

    fn show_spammer_window(&mut self, ctx: &Context) {
        let mut open = self.show_spammer;
        show_detached_window(
            ctx,
            "grimlite_packet_spammer",
            "Packet Spammer & Sender",
            [520.0, 340.0],
            &mut open,
            |ui| {
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
            },
        );
        self.show_spammer = open;
    }

    fn show_tamperer_window(&mut self, ctx: &Context) {
        let mut open = self.show_tamperer;
        show_detached_window(
            ctx,
            "grimlite_packet_tamperer",
            "Packet Tamperer / Interceptor",
            [560.0, 360.0],
            &mut open,
            |ui| {
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
            },
        );
        self.show_tamperer = open;
    }
}
