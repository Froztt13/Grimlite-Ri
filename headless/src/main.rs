//! Grimlite Ri Headless Mode — Tabbed multi-account Ruffle `NullRenderer` (0% GPU, 2 FPS)
//! with dashboard navigation (Main, Inventory, Quests), item-list logs, and next-packet filtering.

mod null_engine;

use crate::null_engine::HeadlessEngine;
use eframe::egui;
use egui::{Color32, ComboBox, RichText, ScrollArea, TextEdit};
use grimlite_bot::Bot;
use grimlite_bot::capture::InterceptedPacket;
use grimlite_bot::host::FlashArg;
use grimlite_bot::state::{PlayerSnapshot, SimpleItem};
use grimlite_egui::bot_managers::BotManagerWindow;
use std::collections::{BTreeMap, VecDeque};
use std::sync::Arc;
use std::time::Instant;

const SERVERS: &[&str] = &[
    "Twilly",
    "Twig",
    "Artix",
    "Sepulchure",
    "Gravelyn",
    "Safiria",
    "Sir Ver",
    "Swordhaven (EU)",
    "Galanoth",
    "Alteon",
    "Yorumi",
    "Yokai (SEA)",
    "Espada",
];

const PACKET_LOG_CAPACITY: usize = 500;
const APP_ICON_RGBA: &[u8] = include_bytes!("../../app/assets/grimlite-icon-64.rgba");

fn main() -> eframe::Result {
    let icon = egui::IconData {
        rgba: APP_ICON_RGBA.to_vec(),
        width: 64,
        height: 64,
    };
    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Grimlite Ri — Headless")
            .with_inner_size([1080.0, 680.0])
            .with_icon(Arc::new(icon)),
        ..Default::default()
    };

    eframe::run_native(
        "Grimlite Ri Headless",
        native_options,
        Box::new(|cc| {
            grimlite_egui::theme::apply(&cc.egui_ctx);
            Ok(Box::new(HeadlessApp::new()))
        }),
    )
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum DashboardMenu {
    Main,
    Inventory,
    Quests,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum LogTab {
    Debug,
    Packets,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum InventoryTab {
    Inventory,
    Temp,
    Bank,
    Drops,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ItemTypeFilter {
    All,
    Weapon,
    Class,
    Armor,
    Helm,
    Cape,
    Pet,
    Etc,
    Misc,
    Pots,
}

impl ItemTypeFilter {
    const ALL: &[Self] = &[
        Self::All,
        Self::Weapon,
        Self::Class,
        Self::Armor,
        Self::Helm,
        Self::Cape,
        Self::Pet,
        Self::Etc,
        Self::Misc,
        Self::Pots,
    ];

    fn label(self) -> &'static str {
        match self {
            Self::All => "All",
            Self::Weapon => "Weapon",
            Self::Class => "Class",
            Self::Armor => "Armor",
            Self::Helm => "Helm",
            Self::Cape => "Cape",
            Self::Pet => "Pet",
            Self::Etc => "Etc",
            Self::Misc => "Misc",
            Self::Pots => "Pots",
        }
    }

    fn classify(category: &str) -> Self {
        let c = category.trim().to_lowercase();
        match c.as_str() {
            "sword" | "axe" | "dagger" | "gun" | "handgun" | "rifle" | "bow" | "mace"
            | "gauntlet" | "polearm" | "staff" | "wand" | "whip" | "weapon" => Self::Weapon,
            "class" => Self::Class,
            "armor" => Self::Armor,
            "helm" => Self::Helm,
            "cape" => Self::Cape,
            "pet" => Self::Pet,
            "necklace" | "amulet" | "ground" | "ground item" | "floor item" => Self::Etc,
            "potion" | "elixir" | "tonic" | "scroll" | "consumable" => Self::Pots,
            _ => {
                if c.contains("ground") || c.contains("amulet") || c.contains("necklace") {
                    Self::Etc
                } else if c.contains("pot") || c.contains("elixir") || c.contains("tonic") {
                    Self::Pots
                } else {
                    Self::Misc
                }
            }
        }
    }

    fn matches(self, category: &str) -> bool {
        self == Self::All || Self::classify(category) == self
    }

    fn is_equippable(category: &str) -> bool {
        matches!(
            Self::classify(category),
            Self::Weapon | Self::Class | Self::Armor | Self::Helm | Self::Cape | Self::Pet
        )
    }
}

struct Session {
    user: String,
    server: String,
    started_at: Instant,
    bot: Arc<Bot>,
    engine: Option<HeadlessEngine>,
    bot_manager: BotManagerWindow,
    show_bot_manager: bool,
    // Next-packet filtered log (already-written packets are never retroactively filtered)
    last_pkt_ver: u64,
    last_pkt_seq: u64,
    packet_log: VecDeque<InterceptedPacket>,
}

impl Session {
    fn new(user: String, password: String, server: String) -> Self {
        let bot = Bot::new();
        let bot_manager = BotManagerWindow::new(
            bot.clone(),
            Some(Box::new(|| {
                rfd::FileDialog::new()
                    .add_filter("Grimoire Bot", &["gbot", "json"])
                    .pick_file()
            })),
            Some(Box::new(|| {
                rfd::FileDialog::new()
                    .add_filter("Grimoire Bot", &["gbot"])
                    .set_file_name("bot.gbot")
                    .save_file()
            })),
        );
        let engine = Some(HeadlessEngine::spawn(
            bot.clone(),
            user.clone(),
            password,
            server.clone(),
        ));
        Self {
            user,
            server,
            started_at: Instant::now(),
            bot,
            engine,
            bot_manager,
            show_bot_manager: false,
            last_pkt_ver: 0,
            last_pkt_seq: 0,
            packet_log: VecDeque::with_capacity(256),
        }
    }

    fn is_running(&self) -> bool {
        self.engine
            .as_ref()
            .is_some_and(HeadlessEngine::is_running)
    }

    fn stop(&mut self) {
        self.bot.stop_bot();
        if let Some(eng) = self.engine.take() {
            eng.stop();
        }
    }

    /// Appends only newly arrived packets that match the current next-packet filters.
    /// Packets already written to `packet_log` are kept untouched.
    fn poll_next_packets(&mut self, allow_server: bool, allow_client: bool, filter_text: &str) {
        let ver = self.bot.capture.version();
        if ver == self.last_pkt_ver {
            return;
        }
        self.last_pkt_ver = ver;

        let (first_seq, pkts) = self.bot.capture.snapshot();
        let end_seq = first_seq + pkts.len() as u64;
        if end_seq <= self.last_pkt_seq {
            // Capture buffer was cleared externally; reset sequence cursor
            if end_seq < self.last_pkt_seq {
                self.last_pkt_seq = end_seq;
            }
            return;
        }

        let start_idx = self.last_pkt_seq.saturating_sub(first_seq) as usize;
        let q = filter_text.trim().to_lowercase();
        for pkt in pkts.into_iter().skip(start_idx) {
            let dir_ok = match pkt.outbound {
                Some(false) => allow_server,
                Some(true) => allow_client,
                None => allow_server || allow_client,
            };
            if !dir_ok {
                continue;
            }
            if !q.is_empty() && !pkt.packet.to_lowercase().contains(&q) {
                continue;
            }
            if self.packet_log.len() >= PACKET_LOG_CAPACITY {
                self.packet_log.pop_front();
            }
            self.packet_log.push_back(pkt);
        }
        self.last_pkt_seq = end_seq;
    }
}

/// Auth is considered complete once the game reports logged-in state and
/// player telemetry (username + max HP) has loaded.
fn is_auth_ready(snap: &PlayerSnapshot) -> bool {
    snap.logged_in && !snap.username.is_empty() && snap.max_hp > 0
}

struct HeadlessApp {
    // Multi-account tabs
    sessions: BTreeMap<String, Session>,
    active_session: Option<String>,
    next_id: usize,

    // Auth modal ("New Tab — Launch Headless Account")
    show_new_tab_dialog: bool,
    username: String,
    password: String,
    show_password: bool,
    server: String,
    error_msg: Option<String>,

    // Close-tab confirmation modal
    confirm_close_session: Option<String>,

    // Dashboard navigation
    menu: DashboardMenu,
    log_tab: LogTab,
    inv_tab: InventoryTab,
    item_type_filter: ItemTypeFilter,

    // Single filter text field for Logs (Debug & next-packet Live Packets)
    log_filter: String,
    auto_scroll: bool,
    show_server_packets: bool,
    show_client_packets: bool,

    // Inventory & Quests filters
    inv_search: String,
    quest_search: String,
    quest_active_only: bool,
}

impl HeadlessApp {
    fn new() -> Self {
        Self {
            sessions: BTreeMap::new(),
            active_session: None,
            next_id: 1,
            show_new_tab_dialog: true,
            username: String::new(),
            password: String::new(),
            show_password: false,
            server: "Twilly".to_string(),
            error_msg: None,
            confirm_close_session: None,
            menu: DashboardMenu::Main,
            log_tab: LogTab::Debug,
            inv_tab: InventoryTab::Inventory,
            item_type_filter: ItemTypeFilter::All,
            log_filter: String::new(),
            auto_scroll: true,
            show_server_packets: true,
            show_client_packets: true,
            inv_search: String::new(),
            quest_search: String::new(),
            quest_active_only: false,
        }
    }

    fn launch_account(&mut self) -> bool {
        self.error_msg = None;
        let user = self.username.trim().to_string();
        if user.is_empty() {
            self.error_msg = Some("Username is required.".to_string());
            return false;
        }
        if self.password.is_empty() {
            self.error_msg = Some("Password is required.".to_string());
            return false;
        }
        let server = if self.server.trim().is_empty() {
            "Twilly".to_string()
        } else {
            self.server.trim().to_string()
        };

        let id = format!("acc-{}", self.next_id);
        self.next_id += 1;
        let sess = Session::new(user, self.password.clone(), server);
        self.sessions.insert(id.clone(), sess);
        self.active_session = Some(id);
        self.password.clear();
        true
    }
}

impl Drop for HeadlessApp {
    fn drop(&mut self) {
        for sess in self.sessions.values_mut() {
            sess.stop();
        }
    }
}

impl eframe::App for HeadlessApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        ctx.request_repaint_after(std::time::Duration::from_millis(200));

        // Poll next packets for all sessions using the active next-packet filter settings
        for sess in self.sessions.values_mut() {
            sess.poll_next_packets(
                self.show_server_packets,
                self.show_client_packets,
                &self.log_filter,
            );
        }

        // Top bar: + New Tab, account tabs, and active session controls
        egui::Panel::top("headless_top").show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 8.0;

                if ui.button("+ New Tab").clicked() {
                    self.error_msg = None;
                    self.show_new_tab_dialog = true;
                }
                ui.separator();

                if self.sessions.is_empty() {
                    ui.label(RichText::new("No accounts launched yet.").weak());
                } else {
                    let mut select_tab: Option<String> = None;
                    let mut close_tab: Option<String> = None;

                    for (id, sess) in &self.sessions {
                        let selected = self.active_session.as_deref() == Some(id.as_str());
                        let fill = if selected {
                            Color32::from_rgb(12, 74, 110)
                        } else {
                            Color32::from_rgb(30, 34, 46)
                        };
                        let stroke = if selected {
                            egui::Stroke::new(1.0, Color32::from_rgb(56, 189, 248))
                        } else {
                            egui::Stroke::new(1.0, Color32::from_rgb(55, 65, 81))
                        };

                        let running = sess.is_running();
                        let snap = sess.bot.player_snapshot();
                        let ready = running && is_auth_ready(&snap);

                        egui::Frame::new()
                            .fill(fill)
                            .stroke(stroke)
                            .corner_radius(4.0)
                            .inner_margin(egui::Margin::symmetric(7, 2))
                            .show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    ui.spacing_mut().item_spacing.x = 6.0;
                                    let (dot_rect, _) = ui.allocate_exact_size(
                                        egui::vec2(8.0, 14.0),
                                        egui::Sense::hover(),
                                    );
                                    let dot_color = if ready {
                                        Color32::from_rgb(34, 197, 94)
                                    } else if running {
                                        Color32::from_rgb(240, 190, 70)
                                    } else {
                                        Color32::from_rgb(107, 114, 128)
                                    };
                                    ui.painter().circle_filled(dot_rect.center(), 3.5, dot_color);

                                    let label_text = format!("{} @ {}", sess.user, sess.server);
                                    let label_resp = ui.add(
                                        egui::Label::new(RichText::new(label_text).color(
                                            if selected {
                                                Color32::WHITE
                                            } else {
                                                Color32::from_rgb(209, 213, 219)
                                            },
                                        ))
                                        .sense(egui::Sense::click()),
                                    );
                                    if label_resp.clicked() {
                                        select_tab = Some(id.clone());
                                    }

                                    if ui
                                        .add(
                                            egui::Button::new(
                                                RichText::new("×")
                                                    .size(13.0)
                                                    .color(Color32::from_rgb(209, 213, 219)),
                                            )
                                            .frame(false),
                                        )
                                        .on_hover_text("Close tab")
                                        .clicked()
                                    {
                                        close_tab = Some(id.clone());
                                    }
                                });
                            });
                    }

                    if let Some(id) = select_tab {
                        self.active_session = Some(id);
                    }
                    if let Some(id) = close_tab {
                        self.confirm_close_session = Some(id);
                    }
                }

                // Right-side controls for active session
                if let Some(active_id) = self.active_session.clone()
                    && let Some(sess) = self.sessions.get_mut(&active_id)
                {
                    let running = sess.is_running();
                    let snap = sess.bot.player_snapshot();
                    let ready = running && is_auth_ready(&snap);

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui
                            .add_enabled(
                                running,
                                egui::Button::new(
                                    RichText::new("Stop Account")
                                        .color(Color32::from_rgb(252, 165, 165)),
                                ),
                            )
                            .on_hover_text("Disconnect and stop the headless engine for this tab")
                            .clicked()
                        {
                            sess.stop();
                        }

                        if ready {
                            ui.separator();
                            if ui
                                .selectable_label(sess.show_bot_manager, "Bot Editor")
                                .clicked()
                            {
                                sess.show_bot_manager = !sess.show_bot_manager;
                            }
                        }
                    });
                }
            });
        });

        // Dashboard body for active session
        let active_id = self.active_session.clone();
        if let Some(id) = active_id
            && let Some(sess) = self.sessions.get_mut(&id)
        {
            let running = sess.is_running();
            let snap = sess.bot.player_snapshot();
            let ready = running && is_auth_ready(&snap);

            // Dashboard navigation rail (Main, Inventory, Quests)
            egui::Panel::left("dashboard_nav")
                .default_size(132.0)
                .resizable(false)
                .show(ui, |ui| {
                    ui.add_space(6.0);
                    ui.label(
                        RichText::new("DASHBOARD")
                            .size(11.0)
                            .strong()
                            .color(Color32::from_rgb(130, 140, 160)),
                    );
                    ui.add_space(6.0);

                    nav_button(ui, &mut self.menu, DashboardMenu::Main, "Main");
                    ui.add_space(3.0);
                    nav_button(ui, &mut self.menu, DashboardMenu::Inventory, "Inventory");
                    ui.add_space(3.0);
                    nav_button(ui, &mut self.menu, DashboardMenu::Quests, "Quests");
                });

            match self.menu {
                DashboardMenu::Main => {
                    // Left column inside Main: Player Telemetry + Bot Controls (gated by auth)
                    egui::Panel::left("main_telemetry_panel")
                        .default_size(285.0)
                        .show(ui, |ui| {
                            render_telemetry_card(ui, &id, sess, &snap, running, ready);

                            if ready {
                                ui.add_space(6.0);
                                ui.separator();
                                ui.heading("Bot Controls");
                                ui.horizontal(|ui| {
                                    if ui.button("Load .gbot...").clicked()
                                        && let Some(path) = rfd::FileDialog::new()
                                            .add_filter("Grimoire Bot", &["gbot", "json"])
                                            .pick_file()
                                        && let Err(e) = sess.bot.load_gbot_file(&path)
                                    {
                                        sess.bot.logs.push_debug(format!("[Bot] Load error: {e}"));
                                    }
                                    let bot_running = sess.bot.runner.is_running();
                                    let cfg = sess.bot.runner.config();
                                    if !bot_running {
                                        if ui
                                            .add_enabled(
                                                !cfg.commands.is_empty(),
                                                egui::Button::new("▶ Start"),
                                            )
                                            .clicked()
                                        {
                                            sess.bot.start_bot();
                                        }
                                    } else if ui.button("⏹ Stop").clicked() {
                                        sess.bot.stop_bot();
                                    }
                                });

                                let loaded = sess.bot.runner.loaded_name();
                                let cfg = sess.bot.runner.config();
                                if !loaded.is_empty() {
                                    ui.add_space(3.0);
                                    ui.label(
                                        RichText::new(format!(
                                            "{loaded} — {} cmds | Step #{}",
                                            cfg.commands.len(),
                                            sess.bot.runner.current_index()
                                        ))
                                        .small(),
                                    );
                                }
                            } else if running {
                                ui.add_space(6.0);
                                ui.separator();
                                ui.label(
                                    RichText::new("Waiting for login & player telemetry to load...")
                                        .weak()
                                        .italics(),
                                );
                            }
                        });

                    // Center inside Main: Resizable Commands Panel (top) + Logs Panel (bottom)
                    egui::CentralPanel::default().show(ui, |ui| {
                        let cfg = sess.bot.runner.config();
                        let bot_running = sess.bot.runner.is_running();
                        let active_idx = sess.bot.runner.current_index();

                        egui::Panel::top(format!("main_commands_panel_{id}"))
                            .resizable(true)
                            .default_size(195.0)
                            .min_size(64.0)
                            .show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    ui.label(
                                        RichText::new(format!("Commands ({})", cfg.commands.len()))
                                            .strong(),
                                    );
                                    if bot_running {
                                        ui.label(
                                            RichText::new(format!("[Running step #{active_idx}]"))
                                                .small()
                                                .color(Color32::from_rgb(120, 225, 150)),
                                        );
                                    }
                                });
                                ui.add_space(2.0);

                                if cfg.commands.is_empty() {
                                    let hint = if ready {
                                        "No .gbot loaded. Click 'Load .gbot...' in Bot Controls."
                                    } else {
                                        "Waiting for authentication to finish before loading a bot."
                                    };
                                    ui.label(RichText::new(hint).weak());
                                } else {
                                    ScrollArea::vertical()
                                        .id_salt(format!("cmd_list_{id}"))
                                        .auto_shrink([false, false])
                                        .show(ui, |ui| {
                                            for (i, cmd) in cfg.commands.iter().enumerate() {
                                                let is_cur = bot_running && i == active_idx;
                                                let bg = if is_cur {
                                                    Color32::from_rgb(20, 62, 42)
                                                } else if i % 2 == 1 {
                                                    Color32::from_rgba_unmultiplied(
                                                        255, 255, 255, 6,
                                                    )
                                                } else {
                                                    Color32::TRANSPARENT
                                                };
                                                egui::Frame::new()
                                                    .fill(bg)
                                                    .corner_radius(3.0)
                                                    .inner_margin(egui::Margin::symmetric(6, 2))
                                                    .show(ui, |ui| {
                                                        ui.set_min_width(ui.available_width());
                                                        let prefix = if is_cur { "> " } else { "  " };
                                                        let text = format!("{prefix}[{i:02}] {cmd}");
                                                        let rt = if is_cur {
                                                            RichText::new(text)
                                                                .monospace()
                                                                .strong()
                                                                .color(Color32::from_rgb(
                                                                    120, 225, 150,
                                                                ))
                                                        } else {
                                                            RichText::new(text).monospace()
                                                        };
                                                        ui.label(rt);
                                                    });
                                            }
                                        });
                                }
                            });

                        egui::CentralPanel::default().show(ui, |ui| {
                            // Logs section (single filter text field + item list)
                            ui.horizontal_wrapped(|ui| {
                                ui.selectable_value(&mut self.log_tab, LogTab::Debug, "Debug Logs");
                                ui.selectable_value(
                                    &mut self.log_tab,
                                    LogTab::Packets,
                                    format!("Live Packets ({})", sess.packet_log.len()),
                                );
                                ui.separator();

                                if ui.button("Clear").clicked() {
                                    match self.log_tab {
                                        LogTab::Debug => sess.bot.logs.clear_debug(),
                                        LogTab::Packets => sess.packet_log.clear(),
                                    }
                                }
                                if self.log_tab == LogTab::Packets {
                                    ui.checkbox(&mut self.show_server_packets, "Server (IN)");
                                    ui.checkbox(&mut self.show_client_packets, "Client (OUT)");
                                }
                                ui.checkbox(&mut self.auto_scroll, "Auto-scroll");
                                ui.add(
                                    TextEdit::singleline(&mut self.log_filter)
                                        .hint_text(if self.log_tab == LogTab::Packets {
                                            "Next-packet filter..."
                                        } else {
                                            "Filter logs..."
                                        })
                                        .desired_width(180.0),
                                );
                            });
                            ui.separator();

                            match self.log_tab {
                                LogTab::Debug => {
                                    let q = self.log_filter.trim().to_lowercase();
                                    let lines: Vec<String> = sess
                                        .bot
                                        .logs
                                        .debug_lines()
                                        .into_iter()
                                        .filter(|l| q.is_empty() || l.to_lowercase().contains(&q))
                                        .collect();
                                    ScrollArea::vertical()
                                        .id_salt(format!("debug_logs_{id}"))
                                        .auto_shrink([false, false])
                                        .stick_to_bottom(self.auto_scroll)
                                        .show(ui, |ui| {
                                            if lines.is_empty() {
                                                ui.label(RichText::new("No logs yet.").weak());
                                            } else {
                                                for (idx, line) in lines.iter().enumerate() {
                                                    render_debug_list_item(ui, idx, line);
                                                }
                                            }
                                        });
                                }
                                LogTab::Packets => {
                                    // Already-written packets are displayed as-is (filters only apply to next packets)
                                    ScrollArea::vertical()
                                        .id_salt(format!("packets_{id}"))
                                        .auto_shrink([false, false])
                                        .stick_to_bottom(self.auto_scroll)
                                        .show(ui, |ui| {
                                            if sess.packet_log.is_empty() {
                                                ui.label(
                                                    RichText::new("No packets captured yet.")
                                                        .weak(),
                                                );
                                            } else {
                                                for (idx, pkt) in sess.packet_log.iter().enumerate()
                                                {
                                                    render_packet_list_item(ui, idx, pkt);
                                                }
                                            }
                                        });
                                }
                            }
                        });
                    });
                }

                DashboardMenu::Inventory => {
                    egui::CentralPanel::default().show(ui, |ui| {
                        let inv_items = sess.bot.state.inventory_list();
                        let temp_items = sess.bot.state.temp_inventory_list();
                        let bank_items = sess.bot.state.bank_list();
                        let drops = sess.bot.state.drop_list();

                        ui.horizontal_wrapped(|ui| {
                            ui.selectable_value(
                                &mut self.inv_tab,
                                InventoryTab::Inventory,
                                format!("Inventory ({})", inv_items.len()),
                            );
                            ui.selectable_value(
                                &mut self.inv_tab,
                                InventoryTab::Temp,
                                format!("Temp Inventory ({})", temp_items.len()),
                            );
                            ui.selectable_value(
                                &mut self.inv_tab,
                                InventoryTab::Bank,
                                format!("Bank ({})", bank_items.len()),
                            );
                            ui.selectable_value(
                                &mut self.inv_tab,
                                InventoryTab::Drops,
                                format!("Drops ({})", drops.len()),
                            );
                            ui.separator();
                            ui.add(
                                TextEdit::singleline(&mut self.inv_search)
                                    .hint_text("Search items...")
                                    .desired_width(180.0),
                            );
                            if self.inv_tab == InventoryTab::Bank && ready {
                                if ui.button("Load Bank").clicked() {
                                    let room_id = sess.bot.state.room_id();
                                    let _ = sess
                                        .bot
                                        .capture
                                        .send_to_server(&format!("%xt%zm%loadBank%{room_id}%All%"));
                                }
                            } else if self.inv_tab == InventoryTab::Drops {
                                let room_id = sess.bot.state.room_id();
                                if ui
                                    .add_enabled(
                                        !drops.is_empty(),
                                        egui::Button::new("Pickup All"),
                                    )
                                    .clicked()
                                {
                                    for d in &drops {
                                        let _ = sess.bot.capture.send_to_server(&format!(
                                            "%xt%zm%getDrop%{room_id}%{}%",
                                            d.id
                                        ));
                                    }
                                }
                            }
                        });

                        if self.inv_tab == InventoryTab::Drops {
                            // Item type grouping sub-tabs on Drops
                            ui.add_space(2.0);
                            ui.horizontal_wrapped(|ui| {
                                for &group in ItemTypeFilter::ALL {
                                    let count = if group == ItemTypeFilter::All {
                                        drops.len()
                                    } else {
                                        drops
                                            .iter()
                                            .filter(|d| group.matches(&d.category))
                                            .count()
                                    };
                                    ui.selectable_value(
                                        &mut self.item_type_filter,
                                        group,
                                        format!("{} ({count})", group.label()),
                                    );
                                }
                            });
                            ui.separator();

                            let room_id = sess.bot.state.room_id();
                            let q = self.inv_search.trim().to_lowercase();
                            let filtered_drops: Vec<_> = drops
                                .iter()
                                .filter(|d| {
                                    self.item_type_filter.matches(&d.category)
                                        && (q.is_empty()
                                            || d.name.to_lowercase().contains(&q)
                                            || d.category.to_lowercase().contains(&q))
                                })
                                .collect();
                            ScrollArea::vertical()
                                .id_salt(format!(
                                    "drops_list_{id}_{}",
                                    self.item_type_filter as u8
                                ))
                                .auto_shrink([false, false])
                                .show(ui, |ui| {
                                    if filtered_drops.is_empty() {
                                        ui.label(RichText::new("No drops available.").weak());
                                    } else {
                                        for (idx, d) in filtered_drops.iter().enumerate() {
                                            egui::Frame::new()
                                                .fill(row_bg(idx))
                                                .corner_radius(3.0)
                                                .inner_margin(egui::Margin::symmetric(8, 4))
                                                .show(ui, |ui| {
                                                    ui.horizontal(|ui| {
                                                        ui.label(RichText::new(&d.name).strong());
                                                        if d.qty > 1 {
                                                            ui.label(
                                                                RichText::new(format!("x{}", d.qty))
                                                                    .monospace()
                                                                    .color(Color32::from_rgb(
                                                                        120, 210, 255,
                                                                    )),
                                                            );
                                                        }
                                                        if d.is_ac {
                                                            ui.label(
                                                                RichText::new("AC")
                                                                    .small()
                                                                    .strong()
                                                                    .color(Color32::from_rgb(
                                                                        255, 210, 90,
                                                                    )),
                                                            );
                                                        }
                                                        ui.with_layout(
                                                            egui::Layout::right_to_left(
                                                                egui::Align::Center,
                                                            ),
                                                            |ui| {
                                                                if ui.button("Pickup").clicked() {
                                                                    let _ = sess
                                                                        .bot
                                                                        .capture
                                                                        .send_to_server(&format!(
                                                                            "%xt%zm%getDrop%{room_id}%{}%",
                                                                            d.id
                                                                        ));
                                                                }
                                                                if !d.category.is_empty() {
                                                                    ui.label(
                                                                        RichText::new(&d.category)
                                                                            .small()
                                                                            .weak(),
                                                                    );
                                                                }
                                                            },
                                                        );
                                                    });
                                                });
                                        }
                                    }
                                });
                        } else {
                            let items: &[SimpleItem] = match self.inv_tab {
                                InventoryTab::Inventory => &inv_items,
                                InventoryTab::Temp => &temp_items,
                                InventoryTab::Bank => &bank_items,
                                InventoryTab::Drops => &[],
                            };

                            // Show item type grouping sub-tabs for Inventory & Bank (not Temp Inventory)
                            let use_type_filter = self.inv_tab != InventoryTab::Temp;
                            if use_type_filter {
                                ui.add_space(2.0);
                                ui.horizontal_wrapped(|ui| {
                                    for &group in ItemTypeFilter::ALL {
                                        let count = if group == ItemTypeFilter::All {
                                            items.len()
                                        } else {
                                            items
                                                .iter()
                                                .filter(|it| group.matches(&it.category))
                                                .count()
                                        };
                                        ui.selectable_value(
                                            &mut self.item_type_filter,
                                            group,
                                            format!("{} ({count})", group.label()),
                                        );
                                    }
                                });
                            }
                            ui.separator();

                            let q = self.inv_search.trim().to_lowercase();
                            let mut filtered: Vec<&SimpleItem> = items
                                .iter()
                                .filter(|it| {
                                    (!use_type_filter || self.item_type_filter.matches(&it.category))
                                        && (q.is_empty()
                                            || it.name.to_lowercase().contains(&q)
                                            || it.category.to_lowercase().contains(&q))
                                })
                                .collect();
                            // Sort equipped items to the very top, then alphabetically
                            filtered.sort_by(|a, b| {
                                b.equipped
                                    .cmp(&a.equipped)
                                    .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
                            });

                            ScrollArea::vertical()
                                .id_salt(format!(
                                    "inv_list_{id}_{}_{}",
                                    self.inv_tab as u8,
                                    if use_type_filter {
                                        self.item_type_filter as u8
                                    } else {
                                        255
                                    }
                                ))
                                .auto_shrink([false, false])
                                .show(ui, |ui| {
                                    if filtered.is_empty() {
                                        let empty_msg = match self.inv_tab {
                                            InventoryTab::Bank if bank_items.is_empty() => {
                                                "Bank not loaded yet. Click 'Load Bank' to fetch bank items."
                                            }
                                            _ => "No items found.",
                                        };
                                        ui.label(RichText::new(empty_msg).weak());
                                    } else {
                                        for (idx, it) in filtered.iter().enumerate() {
                                            let bg = if it.equipped {
                                                Color32::from_rgb(18, 52, 38)
                                            } else {
                                                row_bg(idx)
                                            };
                                            egui::Frame::new()
                                                .fill(bg)
                                                .corner_radius(3.0)
                                                .inner_margin(egui::Margin::symmetric(8, 4))
                                                .show(ui, |ui| {
                                                    ui.horizontal(|ui| {
                                                        ui.label(RichText::new(&it.name).strong());
                                                        if it.qty > 1 {
                                                            ui.label(
                                                                RichText::new(format!("x{}", it.qty))
                                                                    .monospace()
                                                                    .color(Color32::from_rgb(
                                                                        120, 210, 255,
                                                                    )),
                                                            );
                                                        }
                                                        if it.is_ac {
                                                            ui.label(
                                                                RichText::new("AC")
                                                                    .small()
                                                                    .strong()
                                                                    .color(Color32::from_rgb(
                                                                        255, 210, 90,
                                                                    )),
                                                            );
                                                        }
                                                        if it.equipped {
                                                            ui.label(
                                                                RichText::new("Equipped")
                                                                    .small()
                                                                    .strong()
                                                                    .color(Color32::from_rgb(
                                                                        120, 225, 150,
                                                                    )),
                                                            );
                                                        }
                                                        ui.with_layout(
                                                            egui::Layout::right_to_left(
                                                                egui::Align::Center,
                                                            ),
                                                            |ui| {
                                                                if self.inv_tab
                                                                    == InventoryTab::Inventory
                                                                    && ready
                                                                    && !it.equipped
                                                                    && ItemTypeFilter::is_equippable(
                                                                        &it.category,
                                                                    )
                                                                    && ui.button("Equip").clicked()
                                                                {
                                                                    let bot = sess.bot.clone();
                                                                    let item_name = it.name.clone();
                                                                    std::thread::spawn(move || {
                                                                        if let Some(flash) =
                                                                            bot.flash()
                                                                        {
                                                                            let _ = flash.call_bool(
                                                                                "equipItem",
                                                                                &[FlashArg::from(
                                                                                    item_name
                                                                                        .as_str(),
                                                                                )],
                                                                            );
                                                                        }
                                                                    });
                                                                }
                                                                if !it.category.is_empty() {
                                                                    ui.label(
                                                                        RichText::new(&it.category)
                                                                            .small()
                                                                            .weak(),
                                                                    );
                                                                }
                                                                ui.label(
                                                                    RichText::new(format!(
                                                                        "ID {}",
                                                                        it.id
                                                                    ))
                                                                    .small()
                                                                    .monospace()
                                                                    .weak(),
                                                                );
                                                            },
                                                        );
                                                    });
                                                });
                                        }
                                    }
                                });
                        }
                    });
                }

                DashboardMenu::Quests => {
                    egui::CentralPanel::default().show(ui, |ui| {
                        let quests = sess.bot.state.quest_list();
                        let active_count = quests.iter().filter(|q| q.accepted).count();

                        ui.horizontal(|ui| {
                            ui.label(
                                RichText::new(format!(
                                    "Quests ({active_count} active / {} loaded)",
                                    quests.len()
                                ))
                                .strong(),
                            );
                            ui.separator();
                            ui.checkbox(&mut self.quest_active_only, "Active only");
                            ui.add(
                                TextEdit::singleline(&mut self.quest_search)
                                    .hint_text("Search quests...")
                                    .desired_width(200.0),
                            );
                        });
                        ui.separator();

                        let q_search = self.quest_search.trim().to_lowercase();
                        let filtered: Vec<_> = quests
                            .iter()
                            .filter(|q| {
                                (!self.quest_active_only || q.accepted)
                                    && (q_search.is_empty()
                                        || q.name.to_lowercase().contains(&q_search)
                                        || q.id.to_string().contains(&q_search))
                            })
                            .collect();

                        ScrollArea::vertical()
                            .id_salt(format!("quests_list_{id}"))
                            .auto_shrink([false, false])
                            .show(ui, |ui| {
                                if filtered.is_empty() {
                                    ui.label(
                                        RichText::new(
                                            "No quests loaded yet. Quests appear automatically as they are loaded or accepted in-game.",
                                        )
                                        .weak(),
                                    );
                                } else {
                                    for quest in filtered {
                                        let all_done = !quest.reqs.is_empty()
                                            && quest.reqs.iter().all(|r| {
                                                sess.bot
                                                    .state
                                                    .item_qty(r.item_id, &r.name, r.is_temp)
                                                    >= r.required
                                            });
                                        egui::Frame::group(ui.style()).show(ui, |ui| {
                                            ui.set_min_width(ui.available_width());
                                            ui.horizontal(|ui| {
                                                ui.label(
                                                    RichText::new(format!(
                                                        "[{}] {}",
                                                        quest.id, quest.name
                                                    ))
                                                    .strong(),
                                                );
                                                ui.with_layout(
                                                    egui::Layout::right_to_left(
                                                        egui::Align::Center,
                                                    ),
                                                    |ui| {
                                                        if quest.accepted && all_done {
                                                            ui.label(
                                                                RichText::new("Ready to Turn In")
                                                                    .small()
                                                                    .strong()
                                                                    .color(Color32::from_rgb(
                                                                        120, 225, 150,
                                                                    )),
                                                            );
                                                        } else if quest.accepted {
                                                            ui.label(
                                                                RichText::new("In Progress")
                                                                    .small()
                                                                    .color(Color32::from_rgb(
                                                                        240, 190, 70,
                                                                    )),
                                                            );
                                                        } else {
                                                            ui.label(
                                                                RichText::new("Loaded")
                                                                    .small()
                                                                    .weak(),
                                                            );
                                                        }
                                                    },
                                                );
                                            });

                                            if !quest.reqs.is_empty() {
                                                ui.add_space(4.0);
                                                for req in &quest.reqs {
                                                    let have = sess.bot.state.item_qty(
                                                        req.item_id,
                                                        &req.name,
                                                        req.is_temp,
                                                    );
                                                    let frac = if req.required > 0 {
                                                        (have as f32 / req.required as f32)
                                                            .clamp(0.0, 1.0)
                                                    } else {
                                                        1.0
                                                    };
                                                    let bar_color = if have >= req.required {
                                                        Color32::from_rgb(34, 150, 80)
                                                    } else {
                                                        Color32::from_rgb(45, 115, 215)
                                                    };
                                                    grimlite_egui::theme::stat_bar(
                                                        ui,
                                                        &req.name,
                                                        &format!("{have} / {}", req.required),
                                                        frac,
                                                        bar_color,
                                                    );
                                                    ui.add_space(2.0);
                                                }
                                            }
                                        });
                                        ui.add_space(4.0);
                                    }
                                }
                            });
                    });
                }
            }

            if ready && sess.show_bot_manager {
                let mut open = sess.show_bot_manager;
                sess.bot_manager.show(&ctx, &mut open);
                sess.show_bot_manager = open;
            }
        } else {
            egui::CentralPanel::default().show(ui, |ui| {
                ui.centered_and_justified(|ui| {
                    ui.label(
                        RichText::new(
                            "No active account tab. Click '+ New Tab' to launch an account.",
                        )
                        .weak(),
                    );
                });
            });
        }

        // Auth Modal Window ("New Tab — Launch Headless Account")
        if self.show_new_tab_dialog {
            let mut open = self.show_new_tab_dialog;
            let mut close_now = false;
            egui::Window::new("New Tab — Launch Headless Account")
                .open(&mut open)
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .fixed_size([380.0, 0.0])
                .show(&ctx, |ui| {
                    if let Some(err) = &self.error_msg {
                        ui.colored_label(Color32::from_rgb(248, 113, 113), err);
                        ui.add_space(4.0);
                    }

                    ui.label("Username");
                    ui.add(
                        TextEdit::singleline(&mut self.username)
                            .hint_text("AQW account username")
                            .desired_width(ui.available_width()),
                    );
                    ui.add_space(6.0);

                    ui.label("Password");
                    ui.horizontal(|ui| {
                        ui.add(
                            TextEdit::singleline(&mut self.password)
                                .password(!self.show_password)
                                .hint_text("Account password")
                                .desired_width(ui.available_width() - 56.0),
                        );
                        if ui
                            .button(if self.show_password { "Hide" } else { "Show" })
                            .clicked()
                        {
                            self.show_password = !self.show_password;
                        }
                    });
                    ui.add_space(6.0);

                    ui.label("Server");
                    ComboBox::from_id_salt("headless_server_select")
                        .selected_text(&self.server)
                        .width(ui.available_width())
                        .show_ui(ui, |ui| {
                            for &srv in SERVERS {
                                ui.selectable_value(&mut self.server, srv.to_string(), srv);
                            }
                        });

                    ui.add_space(10.0);
                    ui.horizontal(|ui| {
                        let enter_pressed = ui.input(|i| i.key_pressed(egui::Key::Enter));
                        if (ui.button("Launch Account").clicked() || enter_pressed)
                            && self.launch_account()
                        {
                            close_now = true;
                        }
                        if ui.button("Cancel").clicked() {
                            close_now = true;
                        }
                    });
                });
            self.show_new_tab_dialog = open && !close_now;
        }

        // Close Tab Confirmation Modal
        if let Some(target_id) = self.confirm_close_session.clone() {
            if let Some(info) = self
                .sessions
                .get(&target_id)
                .map(|s| (s.user.clone(), s.server.clone(), s.is_running()))
            {
                let (user, server, is_running) = info;
                let mut open = true;
                let mut close_dialog = false;
                egui::Window::new("Close Account Tab?")
                    .open(&mut open)
                    .collapsible(false)
                    .resizable(false)
                    .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                    .fixed_size([360.0, 0.0])
                    .show(&ctx, |ui| {
                        ui.label(format!("Close tab for {user} @ {server}?"));
                        if is_running {
                            ui.add_space(4.0);
                            ui.colored_label(
                                Color32::from_rgb(252, 211, 77),
                                "This will stop the running headless engine and disconnect the account.",
                            );
                        }
                        ui.add_space(10.0);
                        ui.horizontal(|ui| {
                            if ui
                                .button(
                                    RichText::new("Yes, Close Tab")
                                        .color(Color32::from_rgb(248, 113, 113)),
                                )
                                .clicked()
                            {
                                if let Some(mut sess) = self.sessions.remove(&target_id) {
                                    sess.stop();
                                }
                                if self.active_session.as_deref() == Some(target_id.as_str()) {
                                    self.active_session = self.sessions.keys().next().cloned();
                                }
                                close_dialog = true;
                            }
                            if ui.button("Cancel").clicked() {
                                close_dialog = true;
                            }
                        });
                    });
                if !open || close_dialog {
                    self.confirm_close_session = None;
                }
            } else {
                self.confirm_close_session = None;
            }
        }
    }
}

fn nav_button(ui: &mut egui::Ui, current: &mut DashboardMenu, target: DashboardMenu, label: &str) {
    let selected = *current == target;
    let fill = if selected {
        Color32::from_rgb(12, 74, 110)
    } else {
        Color32::TRANSPARENT
    };
    let text_color = if selected {
        Color32::WHITE
    } else {
        Color32::from_rgb(200, 206, 216)
    };
    let btn = egui::Button::new(RichText::new(label).color(text_color).strong())
        .fill(fill)
        .min_size(egui::vec2(ui.available_width(), 28.0));
    if ui.add(btn).clicked() {
        *current = target;
    }
}

fn row_bg(idx: usize) -> Color32 {
    if idx % 2 == 1 {
        Color32::from_rgba_unmultiplied(255, 255, 255, 7)
    } else {
        Color32::TRANSPARENT
    }
}

fn render_debug_list_item(ui: &mut egui::Ui, idx: usize, line: &str) {
    let font = egui::FontId::monospace(12.0);
    let mut job = egui::text::LayoutJob::default();
    let append = |job: &mut egui::text::LayoutJob, text: &str, color: Color32| {
        job.append(
            text,
            0.0,
            egui::TextFormat {
                font_id: font.clone(),
                color,
                ..Default::default()
            },
        );
    };

    if line.starts_with('[')
        && let Some(end_ts) = line.find(']')
    {
        append(&mut job, &line[..=end_ts], Color32::from_rgb(130, 140, 160));
        let rest = &line[end_ts + 1..];
        let msg_color = if rest.contains("Error") || rest.contains("Failed") {
            Color32::from_rgb(248, 113, 113)
        } else if rest.contains("Warning") {
            Color32::from_rgb(252, 211, 77)
        } else {
            Color32::from_rgb(225, 229, 236)
        };
        append(&mut job, rest, msg_color);
    } else {
        append(&mut job, line, Color32::from_rgb(225, 229, 236));
    }

    egui::Frame::new()
        .fill(row_bg(idx))
        .corner_radius(3.0)
        .inner_margin(egui::Margin::symmetric(6, 2))
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.label(job);
        });
}

fn render_packet_list_item(ui: &mut egui::Ui, idx: usize, pkt: &InterceptedPacket) {
    let font = egui::FontId::monospace(12.0);
    let mut job = egui::text::LayoutJob::default();
    let append = |job: &mut egui::text::LayoutJob, text: &str, color: Color32| {
        job.append(
            text,
            0.0,
            egui::TextFormat {
                font_id: font.clone(),
                color,
                ..Default::default()
            },
        );
    };

    let dir_color = match pkt.outbound {
        Some(true) => Color32::from_rgb(120, 190, 255),
        Some(false) => Color32::from_rgb(130, 225, 150),
        None => Color32::from_rgb(255, 110, 110),
    };
    append(
        &mut job,
        &format!("[{}] ", pkt.timestamp),
        Color32::from_rgb(130, 140, 160),
    );
    append(&mut job, &format!("[{}] ", pkt.direction()), dir_color);
    append(&mut job, &pkt.packet, Color32::from_rgb(225, 229, 236));

    egui::Frame::new()
        .fill(row_bg(idx))
        .corner_radius(3.0)
        .inner_margin(egui::Margin::symmetric(6, 2))
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.label(job);
        });
}

fn render_telemetry_card(
    ui: &mut egui::Ui,
    id: &str,
    sess: &Session,
    snap: &PlayerSnapshot,
    running: bool,
    ready: bool,
) {
    grimlite_egui::theme::card(ui, "Player Telemetry", |ui| {
        let mut display_snap = snap.clone();
        if display_snap.username.is_empty() {
            display_snap.username = sess.user.clone();
        }
        grimlite_egui::theme::player_telemetry(ui, &display_snap);

        ui.add_space(6.0);
        egui::Grid::new(format!("telemetry_grid_{id}"))
            .num_columns(2)
            .spacing([12.0, 3.0])
            .show(ui, |ui| {
                ui.label(RichText::new("Status").weak());
                if ready {
                    ui.label(
                        RichText::new("Online (2 FPS)").color(Color32::from_rgb(120, 225, 150)),
                    );
                } else if running {
                    ui.label(
                        RichText::new("Authenticating...").color(Color32::from_rgb(240, 190, 70)),
                    );
                } else {
                    ui.label(RichText::new("Stopped").color(Color32::GRAY));
                }
                ui.end_row();

                ui.label(RichText::new("Server").weak());
                ui.label(RichText::new(&sess.server).monospace());
                ui.end_row();

                ui.label(RichText::new("Uptime").weak());
                let secs = sess.started_at.elapsed().as_secs();
                ui.label(RichText::new(format!("{}m {}s", secs / 60, secs % 60)).monospace());
                ui.end_row();

                ui.label(RichText::new("Map").weak());
                let map_str = if snap.map.is_empty() {
                    "-".to_string()
                } else {
                    format!("{}-{}", snap.map, sess.bot.state.room_id())
                };
                ui.label(RichText::new(map_str).monospace());
                ui.end_row();

                ui.label(RichText::new("Cell / Pad").weak());
                ui.label(RichText::new(format!("{} / {}", snap.cell, snap.pad)).monospace());
                ui.end_row();

                ui.label(RichText::new("Gold").weak());
                ui.label(
                    RichText::new(format!("{}", snap.gold))
                        .monospace()
                        .color(Color32::from_rgb(255, 210, 90)),
                );
                ui.end_row();
            });
    });
}
