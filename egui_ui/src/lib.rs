//! Grimlite Ri cross-platform egui overlay & detached tool windows.

pub mod bot_managers;
pub mod command_editor;
pub mod theme;
pub mod tools;

use crate::bot_managers::{BotManagerWindow, FileDialogFn};
use crate::tools::ToolsWindows;
use egui::{
    CentralPanel, Color32, Context, Panel, RichText, ScrollArea, Slider, Ui, ViewportBuilder,
    ViewportClass, ViewportId,
};
use grimlite_bot::Bot;
use grimlite_bot::host::FlashArg;
use grimlite_bot::options::BotOptions;
use std::sync::Arc;

pub const NAV_BAR_HEIGHT: f32 = 34.0;

const PADS: &[&str] = &[
    "Center", "Spawn", "Left", "Right", "Top", "Bottom", "Up", "Down",
];

/// Shows a tool window as a native OS viewport on desktop (macOS/Windows/Linux) so it can be
/// dragged freely outside the main Grimlite window. Automatically falls back to an embedded
/// in-canvas window on Android or backends without multi-viewport support.
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
    let viewport_id = ViewportId::from_hash_of(id);
    let mut builder = ViewportBuilder::default()
        .with_title(title)
        .with_inner_size(default_size)
        .with_resizable(true)
        .with_always_on_top();
    if let Some(min) = min_size {
        builder = builder.with_min_inner_size(min);
    }
    ctx.show_viewport_immediate(viewport_id, builder, |ui, class| {
        if class == ViewportClass::EmbeddedWindow {
            add_contents(ui);
        } else {
            CentralPanel::default().show(ui, |ui| add_contents(ui));
        }

        // Forward non-repeat key presses from focused child viewports so global hotkeys
        // (e.g. pressing F5 twice to open then close a window) work seamlessly across windows.
        if !ui.ctx().egui_wants_keyboard_input() {
            let pressed: Vec<egui::Key> = ui.input(|i| {
                i.events
                    .iter()
                    .filter_map(|e| match e {
                        egui::Event::Key {
                            key,
                            pressed: true,
                            repeat: false,
                            ..
                        } => Some(*key),
                        _ => None,
                    })
                    .collect()
            });
            if !pressed.is_empty() {
                ui.ctx().data_mut(|d| {
                    let buf = d.get_temp_mut_or_default::<Vec<egui::Key>>(
                        egui::Id::new("grimlite_child_pressed_keys"),
                    );
                    buf.extend(pressed);
                });
                ui.ctx().request_repaint();
            }
        }

        if ui.input(|i| i.viewport().close_requested()) {
            *open = false;
        }
    });
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
            jump_cell: "Enter".to_string(),
            jump_pad: "Spawn".to_string(),
            last_map_name: String::new(),
        }
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

                    ui.menu_button("Tools", |ui| {
                        self.tools.menu_items(ui);
                    });

                    if ui.selectable_label(self.show_logs, "Logs").clicked() {
                        self.show_logs = !self.show_logs;
                    }

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

                        // Cell dropdown
                        let cells = self.bot.state.cell_list();
                        let mut cell_changed = false;
                        let cell_resp = theme::dropdown(
                            ui,
                            "header_cell_cb",
                            self.jump_cell.clone(),
                            82.0,
                            |ui| {
                                for c in &cells {
                                    if ui
                                        .selectable_value(&mut self.jump_cell, c.clone(), c)
                                        .changed()
                                    {
                                        cell_changed = true;
                                    }
                                }
                            },
                        );
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
        let mut open = self.show_about;
        show_detached_window(
            ctx,
            "grimlite_about",
            "About Grimlite Ri",
            [400.0, 260.0],
            &mut open,
            |ui| {
                ui.vertical_centered(|ui| {
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
            [295.0, 215.0],
            Some([275.0, 200.0]),
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
