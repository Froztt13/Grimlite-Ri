//! Grimlite Ri cross-platform egui overlay & tool windows.

use egui::{Color32, Context, Panel, RichText, ScrollArea, Ui, Window};
use grimlite_bot::Bot;
use std::sync::Arc;

pub const NAV_BAR_HEIGHT: f32 = 34.0;

pub struct GrimliteUi {
    bot: Arc<Bot>,
    show_packet_logger: bool,
    show_logs: bool,
    packet_filter: String,
    last_packet_version: u64,
    cached_packets: Vec<(String, String)>,
}

impl GrimliteUi {
    pub fn new(bot: Arc<Bot>) -> Self {
        Self {
            bot,
            show_packet_logger: false,
            show_logs: false,
            packet_filter: String::new(),
            last_packet_version: 0,
            cached_packets: Vec::new(),
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
                        .selectable_label(self.show_packet_logger, "Packets")
                        .clicked()
                    {
                        self.show_packet_logger = !self.show_packet_logger;
                    }
                    if ui.selectable_label(self.show_logs, "Logs").clicked() {
                        self.show_logs = !self.show_logs;
                    }

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let status = if self.bot.is_game_loaded() {
                            RichText::new("● AQW Ready").color(Color32::from_rgb(80, 200, 120))
                        } else {
                            RichText::new("○ Loading...").color(Color32::from_rgb(240, 180, 70))
                        };
                        ui.label(status);
                    });
                });
            });

        if self.show_packet_logger {
            self.show_packet_logger_window(ctx);
        }
        if self.show_logs {
            self.show_logs_window(ctx);
        }
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
            .default_size([640.0, 380.0])
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
