//! Grimlite Ri cross-platform egui overlay & detached tool windows.

pub mod bot_managers;
pub mod command_editor;
pub mod maid;
pub mod system;
pub mod theme;
pub mod tools;

use crate::bot_managers::{BotManagerWindow, FileDialogFn};
use crate::tools::ToolsWindows;
use egui::{Color32, Context, Panel, RichText, ScrollArea, Slider, Ui};
use grimlite_bot::Bot;
use grimlite_bot::host::FlashArg;
use grimlite_bot::options::BotOptions;
use std::sync::Arc;

pub const TAB_BAR_HEIGHT: f32 = 26.0;
pub const TAB_RAIL_WIDTH: f32 = 58.0;
pub const NAV_BAR_HEIGHT: f32 = 34.0;
const APP_ICON_RGBA: &[u8] = include_bytes!("../assets/grimlite-icon-64.rgba");
const GITHUB_URL: &str = "https://github.com/Froztt13/Grimlite-Ri";

/// Layout orientation for the multi-account tab switcher (`TopBar` on desktop, `LeftRail` on mobile/Android).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TabLayout {
    TopBar,
    LeftRail,
}

/// Dispatches to [`show_tab_bar`] (desktop) or [`show_tab_rail_left`] (mobile/Android).
pub fn show_tabs(
    egui_ui: &mut Ui,
    layout: TabLayout,
    tabs: &[TabDescriptor],
    active_id: u64,
) -> TabBarAction {
    match layout {
        TabLayout::TopBar => show_tab_bar(egui_ui, tabs, active_id),
        TabLayout::LeftRail => show_tab_rail_left(egui_ui, tabs, active_id),
    }
}

/// Renders a thumb-friendly vertical multi-account tab rail on the left edge (mobile/Android layout).
pub fn show_tab_rail_left(egui_ui: &mut Ui, tabs: &[TabDescriptor], active_id: u64) -> TabBarAction {
    let mut action = TabBarAction::None;
    let can_close = tabs.len() > 1;

    Panel::left("grimlite_tab_rail")
        .exact_size(TAB_RAIL_WIDTH)
        .frame(
            egui::Frame::new()
                .fill(theme::Palette::BG_INPUT)
                .inner_margin(egui::Margin::symmetric(5, 6))
                .stroke(egui::Stroke::new(1.0, theme::Palette::BORDER_SUBTLE)),
        )
        .show(egui_ui, |ui| {
            ui.vertical_centered(|ui| {
                ui.spacing_mut().item_spacing.y = 6.0;

                // "+ New" account button at top of rail
                if ui
                    .add_sized(
                        [46.0, 30.0],
                        egui::Button::new(RichText::new("+").strong().size(16.0))
                            .fill(theme::Palette::ACCENT_PRIMARY)
                            .corner_radius(egui::CornerRadius::same(6)),
                    )
                    .on_hover_text("Open new account tab")
                    .clicked()
                {
                    action = TabBarAction::NewTab;
                }

                ui.separator();

                ScrollArea::vertical()
                    .id_salt("grimlite_tab_rail_scroll")
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.vertical_centered(|ui| {
                            ui.spacing_mut().item_spacing.y = 6.0;
                            for (idx, tab) in tabs.iter().enumerate() {
                                let is_active = tab.id == active_id;
                                let fill = if is_active {
                                    theme::Palette::ACCENT_SELECT
                                } else {
                                    theme::Palette::BG_SURFACE
                                };
                                let stroke = if is_active {
                                    egui::Stroke::new(1.5, theme::Palette::BORDER_FOCUS)
                                } else {
                                    egui::Stroke::new(1.0, theme::Palette::BORDER_SUBTLE)
                                };

                                let frame_resp = egui::Frame::new()
                                    .fill(fill)
                                    .stroke(stroke)
                                    .corner_radius(egui::CornerRadius::same(6))
                                    .inner_margin(egui::Margin::symmetric(3, 4))
                                    .show(ui, |ui| {
                                        ui.set_min_size(egui::vec2(40.0, 36.0));
                                        ui.vertical_centered(|ui| {
                                            ui.spacing_mut().item_spacing.y = 2.0;

                                            // Top row: status dot + optional close button on active tab
                                            ui.horizontal(|ui| {
                                                let dot_color = if tab.bot_running {
                                                    theme::Palette::WARNING
                                                } else if tab.logged_in {
                                                    theme::Palette::SUCCESS
                                                } else {
                                                    theme::Palette::TEXT_MUTED
                                                };
                                                let (rect, _) = ui.allocate_exact_size(
                                                    egui::vec2(7.0, 7.0),
                                                    egui::Sense::hover(),
                                                );
                                                ui.painter()
                                                    .circle_filled(rect.center(), 3.2, dot_color);

                                                if can_close && is_active {
                                                    ui.with_layout(
                                                        egui::Layout::right_to_left(
                                                            egui::Align::Center,
                                                        ),
                                                        |ui| {
                                                            if ui
                                                                .add(
                                                                    egui::Label::new(
                                                                        RichText::new("×")
                                                                            .size(11.0)
                                                                            .strong()
                                                                            .color(Color32::WHITE),
                                                                    )
                                                                    .sense(egui::Sense::click()),
                                                                )
                                                                .clicked()
                                                            {
                                                                action =
                                                                    TabBarAction::Close(tab.id);
                                                            }
                                                        },
                                                    );
                                                }
                                            });

                                            // Short label (username prefix or #N)
                                            let short = if tab.logged_in && !tab.label.is_empty() {
                                                tab.label.chars().take(5).collect::<String>()
                                            } else {
                                                format!("#{}", idx + 1)
                                            };
                                            let lbl = ui
                                                .add(
                                                    egui::Label::new(
                                                        RichText::new(short)
                                                            .size(10.5)
                                                            .strong()
                                                            .color(if is_active {
                                                                Color32::WHITE
                                                            } else {
                                                                theme::Palette::TEXT_PRIMARY
                                                            }),
                                                    )
                                                    .sense(egui::Sense::click()),
                                                )
                                                .on_hover_text(&tab.label);
                                            if lbl.clicked() && !is_active {
                                                action = TabBarAction::Switch(tab.id);
                                            }
                                        });
                                    });

                                if frame_resp
                                    .response
                                    .interact(egui::Sense::click())
                                    .clicked()
                                    && !is_active
                                    && action == TabBarAction::None
                                {
                                    action = TabBarAction::Switch(tab.id);
                                }
                            }
                        });
                    });
            });
        });

    action
}

/// Lightweight metadata for rendering a single account tab in the desktop multi-tab bar.
#[derive(Clone, Debug)]
pub struct TabDescriptor {
    pub id: u64,
    pub label: String,
    pub logged_in: bool,
    pub bot_running: bool,
}

/// User interaction returned by [`show_tab_bar`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TabBarAction {
    None,
    Switch(u64),
    NewTab,
    Close(u64),
}

/// Renders the compact browser-style account tab bar above the main Grimlite navbar.
pub fn show_tab_bar(egui_ui: &mut Ui, tabs: &[TabDescriptor], active_id: u64) -> TabBarAction {
    let mut action = TabBarAction::None;
    let can_close = tabs.len() > 1;

    Panel::top("grimlite_tab_bar")
        .exact_size(TAB_BAR_HEIGHT)
        .frame(
            egui::Frame::new()
                .fill(theme::Palette::BG_INPUT)
                .inner_margin(egui::Margin::symmetric(6, 2))
                .stroke(egui::Stroke::new(1.0, theme::Palette::BORDER_SUBTLE)),
        )
        .show(egui_ui, |ui| {
            ui.horizontal_centered(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;

                for tab in tabs {
                    let is_active = tab.id == active_id;
                    let fill = if is_active {
                        theme::Palette::BG_SURFACE
                    } else {
                        Color32::TRANSPARENT
                    };
                    let stroke = if is_active {
                        egui::Stroke::new(1.0, theme::Palette::BORDER_CONTROL)
                    } else {
                        egui::Stroke::NONE
                    };

                    egui::Frame::new()
                        .fill(fill)
                        .stroke(stroke)
                        .corner_radius(egui::CornerRadius::same(4))
                        .inner_margin(egui::Margin::symmetric(6, 1))
                        .show(ui, |ui| {
                            ui.horizontal_centered(|ui| {
                                ui.spacing_mut().item_spacing.x = 5.0;

                                // Status indicator dot
                                let dot_color = if tab.bot_running {
                                    theme::Palette::WARNING
                                } else if tab.logged_in {
                                    theme::Palette::SUCCESS
                                } else {
                                    theme::Palette::TEXT_MUTED
                                };
                                let (rect, _) = ui.allocate_exact_size(
                                    egui::vec2(7.0, 7.0),
                                    egui::Sense::hover(),
                                );
                                ui.painter()
                                    .circle_filled(rect.center(), 3.2, dot_color);

                                let text = if is_active {
                                    RichText::new(&tab.label)
                                        .size(11.5)
                                        .strong()
                                        .color(Color32::WHITE)
                                } else {
                                    RichText::new(&tab.label)
                                        .size(11.5)
                                        .color(theme::Palette::TEXT_MUTED)
                                };
                                if ui
                                    .add(egui::Label::new(text).sense(egui::Sense::click()))
                                    .on_hover_cursor(egui::CursorIcon::PointingHand)
                                    .clicked()
                                    && !is_active
                                {
                                    action = TabBarAction::Switch(tab.id);
                                }

                                if can_close {
                                    let close = ui
                                        .add(
                                            egui::Label::new(
                                                RichText::new("×")
                                                    .size(12.0)
                                                    .color(theme::Palette::TEXT_MUTED),
                                            )
                                            .sense(egui::Sense::click()),
                                        )
                                        .on_hover_cursor(egui::CursorIcon::PointingHand)
                                        .on_hover_text("Close account tab");
                                    if close.clicked() {
                                        action = TabBarAction::Close(tab.id);
                                    }
                                }
                            });
                        });
                }

                // "+ New" tab button
                if ui
                    .add_sized(
                        [22.0, 19.0],
                        egui::Button::new(RichText::new("+").strong().size(13.0)),
                    )
                    .on_hover_text("Open new account tab")
                    .clicked()
                {
                    action = TabBarAction::NewTab;
                }
            });
        });

    action
}

const PADS: &[&str] = &[
    "Center", "Spawn", "Left", "Right", "Top", "Bottom", "Up", "Down",
];

/// Shows a Grimlite tool window as an in-canvas `egui::Window` inside the main application window.
pub fn show_detached_window(
    ctx: &Context,
    id: &str,
    title: &str,
    default_size: [f32; 2],
    open: &mut bool,
    add_contents: impl FnMut(&mut Ui),
) {
    show_detached_window_sized(ctx, id, title, default_size, None, open, add_contents);
}

/// Same as [`show_detached_window`], with an optional minimum inner size `[min_w, min_h]`.
pub fn show_detached_window_sized(
    ctx: &Context,
    id: &str,
    title: &str,
    default_size: [f32; 2],
    min_size: Option<[f32; 2]>,
    open: &mut bool,
    mut add_contents: impl FnMut(&mut Ui),
) {
    if !*open {
        return;
    }
    let mut win = egui::Window::new(title)
        .id(egui::Id::new(id))
        .open(open)
        .default_size(default_size)
        .resizable(true)
        .collapsible(true);
    if let Some(min) = min_size {
        win = win.min_size(min);
    }
    win.show(ctx, |ui| add_contents(ui));
}

pub struct GrimliteUi {
    bot: Arc<Bot>,
    bot_manager: BotManagerWindow,
    tools: ToolsWindows,

    show_about: bool,
    show_bot_manager: bool,
    show_options: bool,
    show_logs: bool,

    options: BotOptions,
    theme_applied: bool,
    app_icon: Option<egui::TextureHandle>,
    resources: system::ResourceMonitor,
    pick_image: Option<FileDialogFn>,

    // Header Cell/Pad quick-jump state (Grimlite-Li style)
    jump_cell: String,
    jump_pad: String,
    last_map_name: String,
}

impl GrimliteUi {
    pub fn new(
        bot: Arc<Bot>,
        pick_gbot: Option<FileDialogFn>,
        save_gbot: Option<FileDialogFn>,
    ) -> Self {
        let options = bot.options.get();
        let bot_manager = BotManagerWindow::new(bot.clone(), pick_gbot, save_gbot);
        let tools = ToolsWindows::new(bot.clone());
        Self {
            bot,
            bot_manager,
            tools,
            show_about: false,
            show_bot_manager: false,
            show_options: false,
            show_logs: false,
            options,
            theme_applied: false,
            app_icon: None,
            resources: system::ResourceMonitor::default(),
            pick_image: None,
            jump_cell: "Enter".to_string(),
            jump_pad: "Spawn".to_string(),
            last_map_name: String::new(),
        }
    }

    pub fn with_maid_dialogs(
        mut self,
        pick_json: Option<FileDialogFn>,
        save_json: Option<FileDialogFn>,
    ) -> Self {
        self.tools.set_maid_dialogs(pick_json, save_json);
        self
    }

    pub fn with_image_dialog(mut self, pick_image: Option<FileDialogFn>) -> Self {
        self.pick_image = pick_image;
        self
    }

    pub fn show(&mut self, ctx: &Context, egui_ui: &mut Ui) {
        if !self.theme_applied {
            theme::apply(ctx);
            self.theme_applied = true;
        }
        Panel::top("grimlite_nav_bar")
            .exact_size(NAV_BAR_HEIGHT)
            .show(egui_ui, |ui| {
                ui.horizontal_centered(|ui| {
                    let brand = ui
                        .add(
                            egui::Label::new(
                                RichText::new("Grimlite Ri")
                                    .strong()
                                    .color(Color32::from_rgb(230, 57, 70)),
                            )
                            .sense(egui::Sense::click()),
                        )
                        .on_hover_cursor(egui::CursorIcon::PointingHand)
                        .on_hover_text("About Grimlite Ri");
                    if brand.clicked() {
                        self.show_about = !self.show_about;
                    }
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

                    ui.scope(|ui| {
                        let inactive = &mut ui.visuals_mut().widgets.inactive;
                        inactive.weak_bg_fill = Color32::TRANSPARENT;
                        inactive.bg_fill = Color32::TRANSPARENT;
                        inactive.bg_stroke = egui::Stroke::NONE;
                        ui.menu_button("Tools", |ui| {
                            self.tools.menu_items(ui);
                        });
                    });

                    if ui.selectable_label(self.show_logs, "Logs").clicked() {
                        self.show_logs = !self.show_logs;
                    }

                    ui.separator();

                    ui.scope(|ui| {
                        ui.spacing_mut().item_spacing.x = 2.0;
                        let mut maid_on = self.bot.maid.is_running();
                        if ui
                            .checkbox(&mut maid_on, "")
                            .on_hover_text("Start / Stop Maid")
                            .changed()
                        {
                            self.bot.maid.toggle(self.bot.clone());
                        }
                        if ui
                            .selectable_label(self.tools.maid.open, "Maid")
                            .on_hover_text("Open Maid Remake window")
                            .clicked()
                        {
                            self.tools.maid.open = !self.tools.maid.open;
                        }
                    });

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let snap = self.bot.player_snapshot();
                        if !snap.logged_in {
                            return;
                        }

                        ui.spacing_mut().item_spacing.x = 6.0;

                        // Sync cell list automatically whenever player joins a new map
                        if !snap.map.is_empty() && snap.map != self.last_map_name {
                            self.last_map_name = snap.map.clone();
                            if !snap.cell.is_empty() {
                                self.jump_cell = snap.cell.clone();
                            }
                            if !snap.pad.is_empty() {
                                self.jump_pad = snap.pad.clone();
                            }
                            self.bot.refresh_map_cells();
                        }

                        if ui.button("Bank").clicked() {
                            self.bot.spawn_flash_action("openBank", vec![]);
                        }

                        ui.separator();

                        // Tightly grouped [Pad] [Cell] [x] jump controls
                        ui.scope(|ui| {
                            ui.spacing_mut().item_spacing.x = 2.0;

                            // Refresh cells & jump to current cell/pad (`x` button in Grimlite-Li)
                            if ui
                                .button("x")
                                .on_hover_text("Refresh cells & re-sync current cell/pad")
                                .clicked()
                            {
                                self.bot.refresh_map_cells();
                                if !snap.cell.is_empty() {
                                    self.jump_cell = snap.cell.clone();
                                }
                                if !snap.pad.is_empty() {
                                    self.jump_pad = snap.pad.clone();
                                }
                                self.bot.spawn_flash_action(
                                    "moveToCell",
                                    vec![
                                        FlashArg::from(self.jump_cell.as_str()),
                                        FlashArg::from(self.jump_pad.as_str()),
                                    ],
                                );
                            }

                            // Cell dropdown (taller popup height to show more cells at once)
                            let cells = self.bot.state.cell_list();
                            let mut cell_changed = false;
                            let cell_resp = egui::ComboBox::from_id_salt("header_cell_cb")
                                .selected_text(self.jump_cell.clone())
                                .width(82.0)
                                .height(340.0)
                                .show_ui(ui, |ui| {
                                    for c in &cells {
                                        if ui
                                            .selectable_value(&mut self.jump_cell, c.clone(), c)
                                            .changed()
                                        {
                                            cell_changed = true;
                                        }
                                    }
                                });
                            if cell_resp.response.clicked() {
                                self.bot.refresh_map_cells();
                            }
                            if cell_changed {
                                self.bot.spawn_flash_action(
                                    "moveToCell",
                                    vec![
                                        FlashArg::from(self.jump_cell.as_str()),
                                        FlashArg::from(self.jump_pad.as_str()),
                                    ],
                                );
                            }

                            // Pad dropdown
                            let mut pad_changed = false;
                            theme::dropdown(ui, "header_pad_cb", self.jump_pad.clone(), 74.0, |ui| {
                                for &p in PADS {
                                    if ui
                                        .selectable_value(&mut self.jump_pad, p.to_string(), p)
                                        .changed()
                                    {
                                        pad_changed = true;
                                    }
                                }
                            });
                            if pad_changed {
                                let target_cell = if self.jump_cell.is_empty() {
                                    snap.cell.as_str()
                                } else {
                                    self.jump_cell.as_str()
                                };
                                self.bot.spawn_flash_action(
                                    "moveToCell",
                                    vec![
                                        FlashArg::from(target_cell),
                                        FlashArg::from(self.jump_pad.as_str()),
                                    ],
                                );
                            }
                        });

                        ui.separator();

                        // Auto Attack toggle button
                        let aa_on = self.bot.auto_attack.is_running();
                        if theme::toggle_button(
                            ui,
                            aa_on,
                            true,
                            "Auto Attack",
                            "Auto Attack: ON",
                            None,
                        )
                        .clicked()
                        {
                            self.bot.auto_attack.toggle(self.bot.clone());
                        }

                        // Start Bot toggle button
                        let bot_running = self.bot.runner.is_running();
                        let has_cmds = !self.bot.runner.config().commands.is_empty();
                        if theme::toggle_button(
                            ui,
                            bot_running,
                            bot_running || has_cmds,
                            "Start Bot",
                            "Bot: ON",
                            None,
                        )
                        .clicked()
                        {
                            if bot_running {
                                self.bot.stop_bot();
                            } else {
                                self.bot.start_bot();
                            }
                        }
                    });
                });
            });

        let hotkey_effects = self.tools.show(ctx);
        if hotkey_effects.toggle_bot_manager {
            self.show_bot_manager = !self.show_bot_manager;
        }

        if self.show_about {
            self.show_about_window(ctx);
        }
        if self.show_bot_manager {
            let mut open = self.show_bot_manager;
            self.bot_manager.show(ctx, &mut open);
            self.show_bot_manager = open;
        }
        if self.show_options {
            self.show_options_window(ctx);
        }
        if self.show_logs {
            self.show_logs_window(ctx);
        }
    }

    fn show_about_window(&mut self, ctx: &Context) {
        let icon_tex = self
            .app_icon
            .get_or_insert_with(|| {
                let image = egui::ColorImage::from_rgba_unmultiplied([64, 64], APP_ICON_RGBA);
                ctx.load_texture("grimlite_app_icon", image, egui::TextureOptions::LINEAR)
            })
            .clone();

        // Poll process CPU & RAM at 1 Hz only while the About window is open
        let res = self
            .resources
            .sample(std::time::Duration::from_secs(1))
            .clone();
        ctx.request_repaint_after(std::time::Duration::from_secs(1));

        let mut open = self.show_about;
        show_detached_window_sized(
            ctx,
            "grimlite_about",
            "About Grimlite Ri",
            [400.0, 445.0],
            Some([360.0, 420.0]),
            &mut open,
            |ui| {
                ui.vertical_centered(|ui| {
                    ui.add_space(4.0);
                    ui.image((icon_tex.id(), egui::vec2(52.0, 52.0)));
                    ui.add_space(4.0);
                    ui.label(
                        RichText::new("Grimlite Ri")
                            .size(20.0)
                            .strong()
                            .color(Color32::from_rgb(230, 57, 70)),
                    );
                    ui.label(
                        RichText::new(format!("v{}", env!("CARGO_PKG_VERSION")))
                            .small()
                            .weak(),
                    );
                });

                ui.add_space(6.0);
                ui.label(
                    "Multiplatform revamp of Grimlite-Li for AdventureQuest Worlds, \
                     built on Ruffle (AQW) and egui while preserving Grimlite-Li bot behavior.",
                );
                ui.add_space(6.0);
                ui.separator();
                ui.add_space(4.0);

                egui::Grid::new("grimlite_about_grid")
                    .num_columns(2)
                    .spacing([14.0, 4.0])
                    .show(ui, |ui| {
                        ui.label(RichText::new("Bot Engine").weak());
                        ui.label("Grimlite-Li behavior & .gbot format");
                        ui.end_row();

                        ui.label(RichText::new("Flash Engine").weak());
                        ui.label("Ruffle (AQW / Mugetsu patches)");
                        ui.end_row();

                        ui.label(RichText::new("UI & Platform").weak());
                        ui.label("egui + wgpu (Multiplatform)");
                        ui.end_row();

                        ui.label(RichText::new("Packet Capture").weak());
                        ui.label("In-process Socket Interception");
                        ui.end_row();

                        ui.label(RichText::new("Modes").weak());
                        ui.label("Desktop (GPU) & Headless (2 FPS)");
                        ui.end_row();

                        ui.label(RichText::new("GitHub").weak());
                        ui.hyperlink_to("github.com/Froztt13/Grimlite-Ri", GITHUB_URL);
                        ui.end_row();
                    });

                ui.add_space(8.0);

                theme::card(ui, "Resource Monitor", |ui| {
                    ui.spacing_mut().item_spacing.y = 5.0;
                    let cpu_frac = (res.cpu_percent / 100.0).clamp(0.0, 1.0);
                    theme::stat_bar(
                        ui,
                        "CPU",
                        &format!("{:.1}%", res.cpu_percent),
                        cpu_frac,
                        Color32::from_rgb(52, 152, 219),
                    );
                    let ram_scale = res.peak_ram_mb.max(512.0);
                    let ram_frac = (res.ram_mb / ram_scale).clamp(0.0, 1.0);
                    theme::stat_bar(
                        ui,
                        "RAM",
                        &format!("{:.1} MB (Peak {:.1} MB)", res.ram_mb, res.peak_ram_mb),
                        ram_frac,
                        Color32::from_rgb(46, 170, 115),
                    );
                    ui.horizontal(|ui| {
                        let h = res.uptime_secs / 3600;
                        let m = (res.uptime_secs % 3600) / 60;
                        let s = res.uptime_secs % 60;
                        ui.label(
                            RichText::new(format!("Uptime  {h:02}:{m:02}:{s:02}"))
                                .small()
                                .weak(),
                        );
                    });
                });
            },
        );
        self.show_about = open;
    }

    fn show_options_window(&mut self, ctx: &Context) {
        let mut open = self.show_options;
        let mut changed = false;
        show_detached_window_sized(
            ctx,
            "grimlite_options",
            "Game Options",
            [315.0, 325.0],
            Some([295.0, 305.0]),
            &mut open,
            |ui| {
                ui.spacing_mut().item_spacing = egui::vec2(6.0, 4.0);
                ui.spacing_mut().button_padding = egui::vec2(5.0, 2.0);
                ui.spacing_mut().interact_size.y = 20.0;

                theme::card(ui, "Gameplay", |ui| {
                    ui.columns(2, |cols| {
                        cols[0].spacing_mut().item_spacing.y = 4.0;
                        cols[1].spacing_mut().item_spacing.y = 4.0;
                        changed |= cols[0]
                            .checkbox(&mut self.options.infinite_range, "Infinite Range")
                            .changed();
                        changed |= cols[0]
                            .checkbox(&mut self.options.enemy_magnet, "Enemy Magnet")
                            .changed();
                        changed |= cols[0]
                            .checkbox(&mut self.options.skip_cutscenes, "Skip Cutscenes")
                            .changed();
                        changed |= cols[0]
                            .checkbox(&mut self.options.anti_counter, "Anti Counter")
                            .changed();

                        changed |= cols[1]
                            .checkbox(&mut self.options.provoke_monsters, "Provoke Monsters")
                            .changed();
                        changed |= cols[1]
                            .checkbox(&mut self.options.lag_killer, "Lag Killer")
                            .changed();
                        changed |= cols[1]
                            .checkbox(&mut self.options.hide_players, "Hide Players")
                            .changed();
                    });
                });

                ui.add_space(5.0);

                theme::card(ui, "Speed & FPS", |ui| {
                    egui::Grid::new("game_opts_sliders_grid")
                        .num_columns(2)
                        .spacing([8.0, 4.0])
                        .show(ui, |ui| {
                            ui.label("Walk Speed");
                            ui.horizontal(|ui| {
                                if ui
                                    .add(Slider::new(&mut self.options.walk_speed, 4..=40))
                                    .changed()
                                {
                                    self.options.walk_speed_enabled = self.options.walk_speed != 8;
                                    changed = true;
                                }
                            });
                            ui.end_row();

                            ui.label("Stage FPS");
                            changed |= ui
                                .add(Slider::new(&mut self.options.fps, 10..=60))
                                .changed();
                            ui.end_row();
                        });
                });

                ui.add_space(5.0);

                theme::card(ui, "Lag Killer Wallpaper", |ui| {
                    let has_wp = self.options.lag_killer_image.is_some();
                    let display = if has_wp {
                        "Cached (960×550)"
                    } else {
                        "None (Black)"
                    };
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new(display)
                                .small()
                                .color(if has_wp {
                                    theme::Palette::SUCCESS
                                } else {
                                    theme::Palette::TEXT_MUTED
                                }),
                        );
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if has_wp && ui.button("Clear").clicked() {
                                grimlite_bot::options::clear_lag_killer_wallpaper_cache();
                                self.options.lag_killer_image = None;
                                changed = true;
                            }
                            if ui
                                .button("Choose...")
                                .on_hover_text(
                                    "Pick PNG/JPG — auto-cropped to 960×550 and saved to cache",
                                )
                                .clicked()
                                && let Some(ref picker) = self.pick_image
                                && let Some(path) = picker()
                            {
                                match grimlite_bot::options::prepare_lag_killer_wallpaper(&path) {
                                    Ok(cached) => {
                                        self.options.lag_killer_image =
                                            Some(cached.to_string_lossy().to_string());
                                        self.options.lag_killer_image_rev =
                                            self.options.lag_killer_image_rev.wrapping_add(1);
                                        changed = true;
                                    }
                                    Err(e) => {
                                        self.bot
                                            .logs
                                            .push_debug(format!("[LagKiller] {e}"));
                                    }
                                }
                            }
                        });
                    });
                });
            },
        );
        if changed {
            self.bot.apply_options(self.options.clone());
        }
        self.show_options = open;
    }

    fn show_logs_window(&mut self, ctx: &Context) {
        let mut open = self.show_logs;
        show_detached_window(
            ctx,
            "grimlite_debug_logs",
            "Debug Logs",
            [520.0, 300.0],
            &mut open,
            |ui| {
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
            },
        );
        self.show_logs = open;
    }
}
