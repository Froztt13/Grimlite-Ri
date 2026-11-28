//! Packets tool windows: Packet Logger (+ Simple Packet Spammer), Packet Spammer & Sender, and Packet Tamperer.

use crate::show_detached_window_sized;
use egui::{Color32, Context, RichText, ScrollArea, TextEdit, Ui};
use grimlite_bot::Bot;
use grimlite_bot::spammer::{MIN_SPAMMER_DELAY_MS, QueuedPacket};
use grimlite_bot::tamper::{TamperAction, TamperDirection};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

pub struct PacketsWindows {
    bot: Arc<Bot>,

    pub show_logger: bool,
    pub show_spammer: bool,
    pub show_tamperer: bool,

    // Packet Logger state
    packet_filter: String,
    last_packet_version: u64,
    cached_packets: Vec<(String, String, String)>,
    logger_selected: Option<usize>,

    // Simple Packet Spammer inside Packet Logger (mirrors Grimlite-Li `PacketLogger.cs`)
    simple_spam_packet: String,
    simple_spam_to_client: bool,
    simple_spam_delay_ms: u64,
    simple_spam_times: u32,
    simple_spam_running: Arc<AtomicBool>,

    // Packet Spammer & Sender state
    spammer_queue: Vec<QueuedPacket>,
    spammer_selected: Option<usize>,
    spammer_to_client: bool,
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
            logger_selected: None,
            simple_spam_packet: String::new(),
            simple_spam_to_client: false,
            simple_spam_delay_ms: 1000,
            simple_spam_times: 0,
            simple_spam_running: Arc::new(AtomicBool::new(false)),
            spammer_queue: Vec::new(),
            spammer_selected: None,
            spammer_to_client: false,
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
        if ui.selectable_label(self.show_logger, "Sniffer").clicked() {
            self.show_logger = !self.show_logger;
            ui.close();
        }
        if ui.selectable_label(self.show_spammer, "Spammer").clicked() {
            self.show_spammer = !self.show_spammer;
            ui.close();
        }
        if ui
            .selectable_label(self.show_tamperer, "Tamperer / Interceptor")
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

    fn start_simple_spam(&self) {
        let pkt = self.simple_spam_packet.trim().to_string();
        if pkt.is_empty() {
            return;
        }
        if self.simple_spam_running.swap(true, Ordering::AcqRel) {
            return;
        }
        let bot = Arc::clone(&self.bot);
        let running = Arc::clone(&self.simple_spam_running);
        let to_client = self.simple_spam_to_client;
        let delay_ms = self.simple_spam_delay_ms.max(MIN_SPAMMER_DELAY_MS);
        let times = self.simple_spam_times;

        let _ = std::thread::Builder::new()
            .name("grimlite-simple-spammer".into())
            .spawn(move || {
                let mut sent = 0u32;
                while running.load(Ordering::Acquire) {
                    if to_client {
                        let _ = bot.capture.send_to_client(&pkt);
                    } else {
                        let _ = bot.capture.send_to_server(&pkt);
                    }
                    sent = sent.saturating_add(1);
                    if times > 0 && sent >= times {
                        break;
                    }
                    let mut slept = 0u64;
                    while slept < delay_ms && running.load(Ordering::Acquire) {
                        let step = (delay_ms - slept).min(50);
                        std::thread::sleep(Duration::from_millis(step));
                        slept += step;
                    }
                }
                running.store(false, Ordering::Release);
            });
    }

    fn stop_simple_spam(&self) {
        self.simple_spam_running.store(false, Ordering::Release);
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
        show_detached_window_sized(
            ctx,
            "grimlite_packet_logger",
            "Sniffer",
            [560.0, 420.0],
            Some([480.0, 320.0]),
            &mut open,
            |ui| {
                ui.spacing_mut().item_spacing = egui::vec2(5.0, 4.0);
                ui.spacing_mut().button_padding = egui::vec2(6.0, 2.0);
                ui.spacing_mut().interact_size.y = 20.0;

                // 1. Top toolbar: independent Client / Server checkboxes, Clear, Filter, Count
                egui::Panel::top("grimlite_pkt_logger_top")
                    .frame(egui::Frame::NONE)
                    .show_separator_line(false)
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            let mut log_client = self.bot.capture.is_logging_client();
                            if ui
                                .checkbox(&mut log_client, "Client")
                                .on_hover_text("Log packets sent by Client -> Server")
                                .changed()
                            {
                                self.bot.capture.set_logging_client(log_client);
                            }

                            let mut log_server = self.bot.capture.is_logging_server();
                            if ui
                                .checkbox(&mut log_server, "Server")
                                .on_hover_text("Log packets received from Server -> Client")
                                .changed()
                            {
                                self.bot.capture.set_logging_server(log_server);
                            }

                            if ui
                                .add_sized([46.0, 21.0], egui::Button::new("Clear"))
                                .clicked()
                            {
                                self.bot.capture.clear();
                                self.cached_packets.clear();
                                self.logger_selected = None;
                            }

                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    ui.label(
                                        RichText::new(format!(
                                            "{} pkts",
                                            self.cached_packets.len()
                                        ))
                                        .small()
                                        .weak(),
                                    );
                                    ui.add(
                                        TextEdit::singleline(&mut self.packet_filter)
                                            .hint_text("Filter packets...")
                                            .desired_width(f32::INFINITY),
                                    );
                                },
                            );
                        });
                        ui.add_space(4.0);
                    });

                // 2. Bottom: Simple Packet Spammer (mirrors Grimlite-Li `PacketLogger.cs`)
                egui::Panel::bottom("grimlite_pkt_logger_spammer")
                    .frame(egui::Frame::NONE)
                    .show_separator_line(false)
                    .show(ui, |ui| {
                        ui.add_space(5.0);
                        crate::theme::card(ui, "Packet spammer", |ui| {
                            ui.add(
                                TextEdit::singleline(&mut self.simple_spam_packet)
                                    .desired_width(f32::INFINITY)
                                    .code_editor()
                                    .hint_text("%xt%zm%...%"),
                            );

                            let spamming = self.simple_spam_running.load(Ordering::Acquire);
                            let has_pkt = !self.simple_spam_packet.trim().is_empty();

                            ui.horizontal(|ui| {
                                ui.add_enabled_ui(!spamming, |ui| {
                                    crate::theme::segmented_switch(
                                        ui,
                                        &mut self.simple_spam_to_client,
                                        "Server",
                                        "Client",
                                        102.0,
                                    )
                                    .on_hover_text(
                                        "Target destination: Server (Client -> Server) or Client (Server -> Client)",
                                    );
                                    if self.simple_spam_delay_ms < MIN_SPAMMER_DELAY_MS {
                                        self.simple_spam_delay_ms = MIN_SPAMMER_DELAY_MS;
                                    }
                                    crate::theme::num_input(
                                        ui,
                                        &mut self.simple_spam_delay_ms,
                                        MIN_SPAMMER_DELAY_MS..=60_000,
                                        56.0,
                                    );
                                    ui.label("ms");
                                    crate::theme::num_input(
                                        ui,
                                        &mut self.simple_spam_times,
                                        0..=100_000,
                                        48.0,
                                    )
                                    .on_hover_text("0 = spam continuously until stopped");
                                    ui.label("x");
                                });

                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        if ui
                                            .add_enabled(
                                                !spamming && has_pkt,
                                                egui::Button::new("Send Once")
                                                    .min_size(egui::vec2(72.0, 22.0)),
                                            )
                                            .clicked()
                                        {
                                            let pkt = self.simple_spam_packet.trim().to_string();
                                            if self.simple_spam_to_client {
                                                let _ = self.bot.capture.send_to_client(&pkt);
                                            } else {
                                                let _ = self.bot.capture.send_to_server(&pkt);
                                            }
                                        }

                                        if crate::theme::toggle_button(
                                            ui,
                                            spamming,
                                            spamming || has_pkt,
                                            "Spam",
                                            "Stop",
                                            Some([62.0, 22.0]),
                                        )
                                        .clicked()
                                        {
                                            if spamming {
                                                self.stop_simple_spam();
                                            } else {
                                                self.start_simple_spam();
                                            }
                                        }
                                    },
                                );
                            });
                        });
                    });

                // 3. Center: Scrollable Packet Log list filling exact remaining space
                egui::CentralPanel::default()
                    .frame(egui::Frame::NONE)
                    .show(ui, |ui| {
                        let list_h = (ui.available_height() - 8.0).max(60.0);
                        crate::theme::list_box(ui, Some(list_h), |ui| {
                            ScrollArea::vertical()
                                .id_salt("packet_logger_list")
                                .stick_to_bottom(true)
                                .auto_shrink([false, false])
                                .show(ui, |ui| {
                                    ui.spacing_mut().item_spacing.y = 1.0;
                                    let filter = self.packet_filter.trim().to_lowercase();
                                    let mut row_idx = 0usize;
                                    let mut send_to_spammer: Option<(String, bool)> = None;
                                    let mut load_simple: Option<(String, bool)> = None;

                                    if self.cached_packets.is_empty() {
                                        let hint = if self.bot.capture.is_logging() {
                                            "Listening for packets..."
                                        } else {
                                            "Enable \"Client\" and/or \"Server\" above to capture traffic."
                                        };
                                        ui.label(RichText::new(hint).small().weak());
                                    } else {
                                        for (i, (ts, dir, pkt)) in
                                            self.cached_packets.iter().enumerate()
                                        {
                                            if !filter.is_empty()
                                                && !pkt.to_lowercase().contains(&filter)
                                            {
                                                continue;
                                            }
                                            let selected = self.logger_selected == Some(i);
                                            let dir_color = if selected {
                                                Color32::WHITE
                                            } else {
                                                match dir.as_str() {
                                                    "CLIENT" => Color32::from_rgb(120, 190, 255),
                                                    "SERVER" => Color32::from_rgb(130, 225, 150),
                                                    _ => Color32::from_rgb(255, 110, 110),
                                                }
                                            };
                                            let ts_color = if selected {
                                                Color32::from_rgb(210, 230, 255)
                                            } else {
                                                Color32::from_rgb(130, 140, 160)
                                            };
                                            let resp = crate::theme::list_row(
                                                ui,
                                                row_idx,
                                                selected,
                                                0,
                                                |ui| {
                                                    ui.horizontal(|ui| {
                                                        ui.spacing_mut().item_spacing.x = 4.0;
                                                        ui.add(
                                                            egui::Label::new(
                                                                RichText::new(format!("[{ts}]"))
                                                                    .monospace()
                                                                    .size(
                                                                        crate::theme::LIST_ITEM_FONT_SIZE,
                                                                    )
                                                                    .color(ts_color),
                                                            )
                                                            .selectable(false),
                                                        );
                                                        ui.add(
                                                            egui::Label::new(
                                                                RichText::new(format!("[{dir}]"))
                                                                    .monospace()
                                                                    .strong()
                                                                    .size(
                                                                        crate::theme::LIST_ITEM_FONT_SIZE,
                                                                    )
                                                                    .color(dir_color),
                                                            )
                                                            .selectable(false),
                                                        );
                                                        let mut pkt_rt = RichText::new(pkt)
                                                            .monospace()
                                                            .size(crate::theme::LIST_ITEM_FONT_SIZE);
                                                        if selected {
                                                            pkt_rt = pkt_rt.color(Color32::WHITE);
                                                        }
                                                        ui.add(
                                                            egui::Label::new(pkt_rt)
                                                                .truncate()
                                                                .selectable(false),
                                                        );
                                                    });
                                                },
                                            )
                                            .interact(egui::Sense::click())
                                            .on_hover_cursor(egui::CursorIcon::PointingHand);
                                            row_idx += 1;

                                            if resp.clicked() || resp.secondary_clicked() {
                                                self.logger_selected = Some(i);
                                            }
                                            if resp.double_clicked() {
                                                self.logger_selected = Some(i);
                                                load_simple = Some((pkt.clone(), dir == "SERVER"));
                                            }
                                            resp.context_menu(|ui| {
                                                if ui.button("Copy packet").clicked() {
                                                    ui.ctx().copy_text(pkt.clone());
                                                    ui.close();
                                                }
                                                if ui.button("Load to Packet Spammer").clicked() {
                                                    load_simple =
                                                        Some((pkt.clone(), dir == "SERVER"));
                                                    ui.close();
                                                }
                                                if ui.button("Open in Spammer & Sender").clicked() {
                                                    send_to_spammer =
                                                        Some((pkt.clone(), dir == "SERVER"));
                                                    ui.close();
                                                }
                                            });
                                        }
                                    }

                                    if let Some((pkt, to_client)) = load_simple {
                                        self.simple_spam_packet = pkt;
                                        self.simple_spam_to_client = to_client;
                                    }
                                    if let Some((pkt, to_client)) = send_to_spammer {
                                        self.raw_packet_input = pkt;
                                        self.spammer_to_client = to_client;
                                        self.show_spammer = true;
                                    }
                                });
                        });
                    });
            },
        );
        if !open {
            self.stop_simple_spam();
        }
        self.show_logger = open;
    }

    fn show_spammer_window(&mut self, ctx: &Context) {
        if !self.show_spammer {
            return;
        }
        let mut open = self.show_spammer;
        crate::show_detached_window_sized(
            ctx,
            "grimlite_packet_spammer",
            "Spammer",
            [450.0, 325.0],
            Some([390.0, 275.0]),
            &mut open,
            |ui| {
                ui.spacing_mut().item_spacing = egui::vec2(5.0, 4.0);
                ui.spacing_mut().button_padding = egui::vec2(6.0, 2.0);
                ui.spacing_mut().interact_size.y = 20.0;

                // 1. Top: Packet Sender card
                egui::Panel::top("grimlite_spammer_top")
                    .frame(egui::Frame::NONE)
                    .show_separator_line(false)
                    .show(ui, |ui| {
                        crate::theme::card(ui, "Packet Sender", |ui| {
                            ui.add(
                                TextEdit::singleline(&mut self.raw_packet_input)
                                    .desired_width(f32::INFINITY)
                                    .code_editor()
                                    .hint_text("%xt%zm%...%"),
                            );
                            ui.horizontal(|ui| {
                                crate::theme::segmented_switch(
                                    ui,
                                    &mut self.spammer_to_client,
                                    "Server",
                                    "Client",
                                    112.0,
                                );
                                let pkt_trimmed = self.raw_packet_input.trim().to_string();
                                if crate::theme::primary_button(ui, "Send", [58.0, 21.0]).clicked()
                                    && !pkt_trimmed.is_empty()
                                {
                                    self.raw_send_status = if self.spammer_to_client {
                                        match self.bot.capture.send_to_client(&pkt_trimmed) {
                                            Ok(()) => "Injected to client".to_string(),
                                            Err(e) => format!("Error: {e}"),
                                        }
                                    } else {
                                        match self.bot.capture.send_to_server(&pkt_trimmed) {
                                            Ok(()) => "Sent to server".to_string(),
                                            Err(e) => format!("Error: {e}"),
                                        }
                                    };
                                }
                                if ui
                                    .add_sized([92.0, 21.0], egui::Button::new("+ Add to Queue"))
                                    .clicked()
                                    && !pkt_trimmed.is_empty()
                                {
                                    self.spammer_queue.push(QueuedPacket {
                                        packet: pkt_trimmed,
                                        to_client: self.spammer_to_client,
                                    });
                                    self.raw_send_status =
                                        format!("Queued ({})", self.spammer_queue.len());
                                }
                                if !self.raw_send_status.is_empty() {
                                    ui.with_layout(
                                        egui::Layout::right_to_left(egui::Align::Center),
                                        |ui| {
                                            ui.label(
                                                RichText::new(&self.raw_send_status).small().weak(),
                                            );
                                        },
                                    );
                                }
                            });
                        });
                        ui.add_space(5.0);
                    });

                // 2. Bottom: Spammer delay + Clear + Start/Stop controls (pinned — zero height drift!)
                egui::Panel::bottom("grimlite_spammer_bottom")
                    .frame(egui::Frame::NONE)
                    .show_separator_line(false)
                    .show(ui, |ui| {
                        ui.add_space(5.0);
                        ui.horizontal(|ui| {
                            ui.label("Delay");
                            if self.spammer_delay_ms < MIN_SPAMMER_DELAY_MS {
                                self.spammer_delay_ms = MIN_SPAMMER_DELAY_MS;
                            }
                            crate::theme::num_input(
                                ui,
                                &mut self.spammer_delay_ms,
                                MIN_SPAMMER_DELAY_MS..=10_000,
                                62.0,
                            );
                            ui.label("ms");

                            let running = self.bot.spammer.is_running();
                            let has_items = !self.spammer_queue.is_empty();

                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    if crate::theme::toggle_button(
                                        ui,
                                        running,
                                        running || has_items,
                                        "Start Spammer",
                                        "Spammer: ON",
                                        Some([104.0, 22.0]),
                                    )
                                    .clicked()
                                    {
                                        if running {
                                            self.bot.stop_spammer();
                                        } else {
                                            self.bot.start_spammer_queue(
                                                self.spammer_queue.clone(),
                                                self.spammer_delay_ms.max(MIN_SPAMMER_DELAY_MS),
                                            );
                                        }
                                    }
                                    if ui
                                        .add_sized([48.0, 22.0], egui::Button::new("Clear"))
                                        .clicked()
                                    {
                                        self.spammer_queue.clear();
                                        self.spammer_selected = None;
                                        if running {
                                            self.bot.stop_spammer();
                                        }
                                    }
                                },
                            );
                        });
                    });

                // 3. Center: Spammer Queue list filling exact remaining vertical space
                egui::CentralPanel::default()
                    .frame(egui::Frame::NONE)
                    .show(ui, |ui| {
                        ui.label(
                            RichText::new(format!("Spammer Queue ({})", self.spammer_queue.len()))
                                .small()
                                .strong()
                                .color(crate::theme::Palette::TEXT_MUTED),
                        );
                        ui.add_space(2.0);
                        let list_h = (ui.available_height() - 8.0).max(60.0);
                        let mut remove_idx: Option<usize> = None;

                        crate::theme::list_box(ui, Some(list_h), |ui| {
                            ScrollArea::vertical()
                                .id_salt("spammer_queue_list")
                                .auto_shrink([false, false])
                                .show(ui, |ui| {
                                    ui.spacing_mut().item_spacing.y = 1.0;
                                    if self.spammer_queue.is_empty() {
                                        ui.label(
                                            RichText::new(
                                                "Queue is empty — use \"+ Add to Queue\" above.",
                                            )
                                            .small()
                                            .weak(),
                                        );
                                    } else {
                                        for (idx, item) in self.spammer_queue.iter().enumerate() {
                                            let selected = self.spammer_selected == Some(idx);
                                            let badge =
                                                if item.to_client { "[CLI]" } else { "[SRV]" };
                                            let badge_color = if selected {
                                                Color32::WHITE
                                            } else if item.to_client {
                                                crate::theme::Palette::WARNING
                                            } else {
                                                crate::theme::Palette::BORDER_FOCUS
                                            };
                                            let mut pkt_clicked = false;
                                            crate::theme::list_row(ui, idx, selected, 0, |ui| {
                                                ui.horizontal(|ui| {
                                                    ui.label(
                                                        RichText::new(badge)
                                                            .monospace()
                                                            .strong()
                                                            .size(crate::theme::LIST_ITEM_FONT_SIZE)
                                                            .color(badge_color),
                                                    );
                                                    let mut pkt_rt = RichText::new(&item.packet)
                                                        .monospace()
                                                        .size(crate::theme::LIST_ITEM_FONT_SIZE);
                                                    if selected {
                                                        pkt_rt = pkt_rt.color(Color32::WHITE);
                                                    }
                                                    if ui
                                                        .add(
                                                            egui::Label::new(pkt_rt)
                                                                .truncate()
                                                                .sense(egui::Sense::click()),
                                                        )
                                                        .clicked()
                                                    {
                                                        pkt_clicked = true;
                                                    }
                                                    ui.with_layout(
                                                        egui::Layout::right_to_left(
                                                            egui::Align::Center,
                                                        ),
                                                        |ui| {
                                                            if ui
                                                                .add_sized(
                                                                    [20.0, 18.0],
                                                                    egui::Button::new("×"),
                                                                )
                                                                .clicked()
                                                            {
                                                                remove_idx = Some(idx);
                                                            }
                                                        },
                                                    );
                                                });
                                            });
                                            if pkt_clicked {
                                                self.spammer_selected = Some(idx);
                                            }
                                        }
                                    }
                                });
                        });

                        if let Some(idx) = remove_idx {
                            self.spammer_queue.remove(idx);
                            if self.spammer_selected == Some(idx) {
                                self.spammer_selected = None;
                            }
                        }
                    });
            },
        );
        self.show_spammer = open;
    }

    fn show_tamperer_window(&mut self, ctx: &Context) {
        let mut open = self.show_tamperer;
        show_detached_window_sized(
            ctx,
            "grimlite_packet_tamperer",
            "Tamperer / Interceptor",
            [490.0, 330.0],
            Some([420.0, 270.0]),
            &mut open,
            |ui| {
                ui.spacing_mut().item_spacing = egui::vec2(5.0, 4.0);
                ui.spacing_mut().button_padding = egui::vec2(6.0, 2.0);
                ui.spacing_mut().interact_size.y = 20.0;

                // 1. Top: Rule builder card
                egui::Panel::top("grimlite_tamperer_top")
                    .frame(egui::Frame::NONE)
                    .show_separator_line(false)
                    .show(ui, |ui| {
                        crate::theme::card(ui, "Add Rule", |ui| {
                            ui.horizontal(|ui| {
                                crate::theme::dropdown(
                                    ui,
                                    "tamper_dir",
                                    tamper_dir_label(self.tamper_dir),
                                    76.0,
                                    |ui| {
                                        ui.selectable_value(
                                            &mut self.tamper_dir,
                                            TamperDirection::Outbound,
                                            "CLIENT",
                                        );
                                        ui.selectable_value(
                                            &mut self.tamper_dir,
                                            TamperDirection::Inbound,
                                            "SERVER",
                                        );
                                        ui.selectable_value(
                                            &mut self.tamper_dir,
                                            TamperDirection::Both,
                                            "BOTH",
                                        );
                                    },
                                );
                                ui.checkbox(&mut self.tamper_is_block, "Block");
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        if crate::theme::primary_button(ui, "+ Add", [68.0, 21.0])
                                            .clicked()
                                            && !self.tamper_contains.trim().is_empty()
                                        {
                                            let action = if self.tamper_is_block {
                                                TamperAction::Block
                                            } else {
                                                TamperAction::Replace {
                                                    from: self.tamper_from.clone(),
                                                    to: self.tamper_to.clone(),
                                                }
                                            };
                                            self.bot.tamper.add_rule(
                                                self.tamper_dir,
                                                self.tamper_contains.trim().to_string(),
                                                action,
                                            );
                                        }
                                        ui.add(
                                            TextEdit::singleline(&mut self.tamper_contains)
                                                .hint_text("If contains...")
                                                .desired_width(f32::INFINITY),
                                        );
                                    },
                                );
                            });

                            if !self.tamper_is_block {
                                let half = ((ui.available_width() - 6.0) * 0.5).max(80.0);
                                ui.horizontal(|ui| {
                                    ui.add(
                                        TextEdit::singleline(&mut self.tamper_from)
                                            .hint_text("Replace text...")
                                            .desired_width(half),
                                    );
                                    ui.add(
                                        TextEdit::singleline(&mut self.tamper_to)
                                            .hint_text("With text...")
                                            .desired_width(ui.available_width().max(80.0)),
                                    );
                                });
                            }
                        });
                        ui.add_space(5.0);
                    });

                // 2. Center: Active rules list filling exact remaining vertical space
                egui::CentralPanel::default()
                    .frame(egui::Frame::NONE)
                    .show(ui, |ui| {
                        let rules = self.bot.tamper.rules();
                        ui.label(
                            RichText::new(format!("Active Rules ({})", rules.len()))
                                .small()
                                .strong()
                                .color(crate::theme::Palette::TEXT_MUTED),
                        );
                        ui.add_space(2.0);
                        let list_h = (ui.available_height() - 8.0).max(60.0);
                        crate::theme::list_box(ui, Some(list_h), |ui| {
                            ScrollArea::vertical()
                                .id_salt("tamper_rules_list")
                                .auto_shrink([false, false])
                                .show(ui, |ui| {
                                    ui.spacing_mut().item_spacing.y = 1.0;
                                    if rules.is_empty() {
                                        ui.label(
                                            RichText::new("No tamper rules configured.")
                                                .small()
                                                .weak(),
                                        );
                                    }
                                    for (idx, rule) in rules.into_iter().enumerate() {
                                        let dir_color = match rule.direction {
                                            TamperDirection::Outbound => {
                                                Color32::from_rgb(120, 190, 255)
                                            }
                                            TamperDirection::Inbound => {
                                                Color32::from_rgb(130, 225, 150)
                                            }
                                            TamperDirection::Both => crate::theme::Palette::WARNING,
                                        };
                                        crate::theme::list_row(ui, idx, false, 0, |ui| {
                                            ui.horizontal(|ui| {
                                                let mut enabled = rule.enabled;
                                                if ui.checkbox(&mut enabled, "").changed() {
                                                    self.bot.tamper.set_enabled(rule.id, enabled);
                                                }
                                                ui.label(
                                                    RichText::new(format!(
                                                        "[{}]",
                                                        tamper_dir_label(rule.direction)
                                                    ))
                                                    .monospace()
                                                    .strong()
                                                    .size(crate::theme::LIST_ITEM_FONT_SIZE)
                                                    .color(dir_color),
                                                );
                                                let desc = match &rule.action {
                                                    TamperAction::Block => {
                                                        format!("BLOCK `{}`", rule.contains)
                                                    }
                                                    TamperAction::Replace { from, to } => format!(
                                                        "REPLACE `{from}` -> `{to}` (match `{}`)",
                                                        rule.contains
                                                    ),
                                                };
                                                ui.add(
                                                    egui::Label::new(
                                                        RichText::new(desc).monospace().size(
                                                            crate::theme::LIST_ITEM_FONT_SIZE,
                                                        ),
                                                    )
                                                    .truncate()
                                                    .selectable(false),
                                                );
                                                ui.with_layout(
                                                    egui::Layout::right_to_left(
                                                        egui::Align::Center,
                                                    ),
                                                    |ui| {
                                                        if ui
                                                            .add_sized(
                                                                [20.0, 18.0],
                                                                egui::Button::new("×"),
                                                            )
                                                            .clicked()
                                                        {
                                                            self.bot.tamper.remove_rule(rule.id);
                                                        }
                                                    },
                                                );
                                            });
                                        });
                                    }
                                });
                        });
                    });
            },
        );
        self.show_tamperer = open;
    }
}

fn tamper_dir_label(dir: TamperDirection) -> &'static str {
    match dir {
        TamperDirection::Outbound => "CLIENT",
        TamperDirection::Inbound => "SERVER",
        TamperDirection::Both => "BOTH",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn packet_windows_do_not_grow_in_height() {
        let ctx = egui::Context::default();
        let bot = Bot::new();
        let mut pw = PacketsWindows::new(bot);
        pw.show_logger = true;
        pw.show_spammer = true;
        pw.show_tamperer = true;

        let mut logger_heights = Vec::new();
        let mut spammer_heights = Vec::new();
        let mut tamperer_heights = Vec::new();

        for _ in 0..12 {
            let mut input = egui::RawInput::default();
            input.screen_rect = Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1280.0, 800.0),
            ));
            let mut out = ctx.run_ui(input, |ui| {
                pw.show(ui.ctx());
            });
            out.textures_delta.clear();
            let mem = ctx.memory(|m| {
                (
                    m.area_rect(egui::Id::new("grimlite_packet_logger")),
                    m.area_rect(egui::Id::new("grimlite_packet_spammer")),
                    m.area_rect(egui::Id::new("grimlite_packet_tamperer")),
                )
            });
            if let Some(r) = mem.0 {
                logger_heights.push(r.height());
            }
            if let Some(r) = mem.1 {
                spammer_heights.push(r.height());
            }
            if let Some(r) = mem.2 {
                tamperer_heights.push(r.height());
            }
        }

        let check_stable = |name: &str, hs: &[f32]| {
            assert!(hs.len() >= 6, "{name} should have recorded heights");
            let base = hs[3];
            let last = *hs.last().unwrap();
            assert!(
                (last - base).abs() <= 1.0,
                "{name} height drifted from {base} to {last}: {hs:?}"
            );
        };
        check_stable("logger", &logger_heights);
        check_stable("spammer", &spammer_heights);
        check_stable("tamperer", &tamperer_heights);
    }

    #[test]
    fn packet_windows_do_not_grow_on_mobile_viewport() {
        let ctx = egui::Context::default();
        let bot = Bot::new();
        let mut pw = PacketsWindows::new(bot);
        pw.show_logger = true;
        pw.show_spammer = true;
        pw.show_tamperer = true;

        let mobile_bounds =
            egui::Rect::from_min_max(egui::pos2(58.0, 4.0), egui::pos2(796.0, 356.0));
        ctx.data_mut(|d| {
            d.insert_temp(
                egui::Id::new(crate::mobile::MOBILE_CONSTRAIN_RECT_KEY),
                mobile_bounds,
            );
        });

        let mut logger_heights = Vec::new();
        let mut spammer_heights = Vec::new();
        let mut tamperer_heights = Vec::new();

        for _ in 0..12 {
            let mut input = egui::RawInput::default();
            input.screen_rect = Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(800.0, 360.0),
            ));
            let mut out = ctx.run_ui(input, |ui| {
                pw.show(ui.ctx());
            });
            out.textures_delta.clear();
            let mem = ctx.memory(|m| {
                (
                    m.area_rect(egui::Id::new("grimlite_packet_logger")),
                    m.area_rect(egui::Id::new("grimlite_packet_spammer")),
                    m.area_rect(egui::Id::new("grimlite_packet_tamperer")),
                )
            });
            if let Some(r) = mem.0 {
                logger_heights.push(r.height());
            }
            if let Some(r) = mem.1 {
                spammer_heights.push(r.height());
            }
            if let Some(r) = mem.2 {
                tamperer_heights.push(r.height());
            }
        }

        let check_stable = |name: &str, hs: &[f32]| {
            assert!(hs.len() >= 6, "{name} should have recorded heights");
            let base = hs[3];
            let last = *hs.last().unwrap();
            assert!(
                (last - base).abs() <= 1.0,
                "{name} mobile height drifted from {base} to {last}: {hs:?}"
            );
        };
        check_stable("mobile_logger", &logger_heights);
        check_stable("mobile_spammer", &spammer_heights);
        check_stable("mobile_tamperer", &tamperer_heights);
    }
}
