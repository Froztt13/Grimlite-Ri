//! Mobile left-side icon toolbar, collapsible rail handle, and touch-friendly flyout popovers.
//!
//! Replaces the desktop top text navbar + separate tab strip on mobile/Android with a single
//! thumb-friendly vertical icon dock on the left edge (`MOBILE_RAIL_WIDTH`), giving the 16:9 AQW
//! canvas 100% of the vertical screen height while keeping Accounts, Bot Manager, Maid, Quick
//! Controls (Start/Stop, Cell/Pad Jump, Bank), 1-Tap Auto Attack, Tools, Options, Logs, and About
//! accessible in one tap.

use crate::theme::{self, Palette};
use crate::{GrimliteUi, PADS, TabBarAction, TabDescriptor};
use egui::{
    Align, Align2, Color32, Context, CornerRadius, FontId, Frame, Id, Layout, Margin, Order, Panel,
    Pos2, Rect, Response, RichText, ScrollArea, Sense, Shadow, Shape, Stroke, StrokeKind, Ui, pos2,
    vec2,
};
use grimlite_bot::host::FlashArg;

/// Width of the mobile left icon toolbar in logical points.
pub const MOBILE_RAIL_WIDTH: f32 = 54.0;

/// Height of the floating Flash text-input preview bar shown above the soft keyboard.
pub const MOBILE_KB_BAR_HEIGHT: f32 = 36.0;

/// Key in `egui::Context::data` storing the available canvas rect to the right of the mobile rail.
pub(crate) const MOBILE_CONSTRAIN_RECT_KEY: &str = "grimlite_mobile_constrain_rect";

const ICON_BTN_SIZE: egui::Vec2 = vec2(38.0, 31.0);
const COLLAPSE_BTN_SIZE: egui::Vec2 = vec2(38.0, 22.0);

/// Popover flyout anchored immediately to the right of the mobile icon rail.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MobileFlyout {
    Accounts,
    QuickActions,
    Tools,
    Packets,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MobileIcon {
    Accounts,
    Bot,
    Maid,
    Quick,
    Swords,
    Tools,
    Packets,
    Sliders,
    Terminal,
    CollapseLeft,
}

#[derive(Clone, Copy)]
enum ButtonTone {
    /// Normal surface button; highlighted in cyan when its window/flyout is `open`.
    Standard { open: bool },
    /// Direct feature toggle (`Auto Attack`); glows emerald green when `active`.
    Toggle { active: bool },
}

impl GrimliteUi {
    /// Renders the mobile left-side icon toolbar, active flyout popover, and open tool windows.
    pub fn show_mobile(
        &mut self,
        ctx: &Context,
        egui_ui: &mut Ui,
        tabs: &[TabDescriptor],
        active_id: u64,
        rail_collapsed: &mut bool,
    ) -> TabBarAction {
        if !self.theme_applied {
            theme::apply(ctx);
            self.theme_applied = true;
        }

        // Keep cell list synced whenever the player enters a new map even while popovers are closed.
        let snap = self.bot.player_snapshot();
        if snap.logged_in && self.tools.accounts.open {
            self.tools.accounts.open = false;
        }
        if snap.logged_in && !snap.map.is_empty() && snap.map != self.last_map_name {
            self.last_map_name = snap.map.clone();
            if !snap.cell.is_empty() {
                self.jump_cell = snap.cell.clone();
            }
            if !snap.pad.is_empty() {
                self.jump_pad = snap.pad.clone();
            }
            self.bot.refresh_map_cells();
        }

        let screen = ctx.content_rect();
        let left_offset = if *rail_collapsed {
            0.0
        } else {
            MOBILE_RAIL_WIDTH
        };
        let constrain_rect = Rect::from_min_max(
            pos2((screen.left() + left_offset + 4.0).min(screen.right()), screen.top() + 4.0),
            pos2(screen.right() - 4.0, (screen.bottom() - 4.0).max(screen.top() + 60.0)),
        );
        ctx.data_mut(|d| {
            d.insert_temp(Id::new(MOBILE_CONSTRAIN_RECT_KEY), constrain_rect);
        });

        let mut tab_action = TabBarAction::None;

        if *rail_collapsed {
            self.mobile_flyout = None;
            let any_active = self.bot.runner.is_running()
                || self.bot.auto_attack.is_running()
                || self.bot.maid.is_running()
                || tabs.iter().any(|t| t.bot_running);
            show_collapsed_handle(ctx, any_active, rail_collapsed);
        } else {
            self.render_left_icon_rail(ctx, egui_ui, tabs, &snap, rail_collapsed);
            if let Some(flyout) = self.mobile_flyout {
                tab_action = self.render_mobile_flyout(ctx, flyout, tabs, active_id, &snap);
            }
        }

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

        tab_action
    }

    fn toggle_flyout(&mut self, target: MobileFlyout) {
        if self.mobile_flyout == Some(target) {
            self.mobile_flyout = None;
        } else {
            self.mobile_flyout = Some(target);
        }
    }

    fn render_left_icon_rail(
        &mut self,
        ctx: &Context,
        egui_ui: &mut Ui,
        tabs: &[TabDescriptor],
        snap: &grimlite_bot::state::PlayerSnapshot,
        rail_collapsed: &mut bool,
    ) {
        let icon_tex_id = self.ensure_app_icon(ctx).id();

        Panel::left("grimlite_mobile_rail")
            .exact_size(MOBILE_RAIL_WIDTH)
            .resizable(false)
            .frame(
                Frame::new()
                    .fill(Palette::BG_INPUT)
                    .inner_margin(Margin {
                        left: 8,
                        right: 6,
                        top: 8,
                        bottom: 8,
                    })
                    .stroke(Stroke::new(1.0, Palette::BORDER_SUBTLE)),
            )
            .show(egui_ui, |ui| {
                let avail_h = ui.available_height();
                if avail_h >= 400.0 {
                    ui.vertical_centered(|ui| {
                        self.draw_rail_top_items(ui, tabs, snap);
                    });
                    ui.with_layout(Layout::bottom_up(Align::Center), |ui| {
                        ui.spacing_mut().item_spacing.y = 4.0;
                        self.draw_rail_bottom_items(ui, icon_tex_id, rail_collapsed, true);
                    });
                } else {
                    ScrollArea::vertical()
                        .id_salt("grimlite_mobile_rail_scroll")
                        .scroll_bar_visibility(
                            egui::scroll_area::ScrollBarVisibility::AlwaysHidden,
                        )
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            ui.vertical_centered(|ui| {
                                self.draw_rail_top_items(ui, tabs, snap);
                                ui.add_space(4.0);
                                self.draw_rail_bottom_items(
                                    ui,
                                    icon_tex_id,
                                    rail_collapsed,
                                    false,
                                );
                            });
                        });
                }
            });
    }

    fn draw_rail_top_items(
        &mut self,
        ui: &mut Ui,
        tabs: &[TabDescriptor],
        snap: &grimlite_bot::state::PlayerSnapshot,
    ) {
        let bot_running = self.bot.runner.is_running();
        let maid_running = self.bot.maid.is_running();
        let aa_running = self.bot.auto_attack.is_running();
        let any_tab_busy = tabs.iter().any(|t| t.bot_running);
        let any_tab_logged = snap.logged_in || tabs.iter().any(|t| t.logged_in);
        let opts_active = self.options.lag_killer
            || self.options.hide_players
            || self.options.disable_animations;
        let tools_open = self.mobile_flyout == Some(MobileFlyout::Tools)
            || self.tools.travels.open
            || self.tools.loaders.open
            || self.tools.hotkeys.open
            || self.tools.cosmetics.open;
        let packets_open = self.mobile_flyout == Some(MobileFlyout::Packets)
            || self.tools.packets.show_logger
            || self.tools.packets.show_spammer
            || self.tools.packets.show_tamperer;

        ui.spacing_mut().item_spacing.y = 4.0;

        // 1. Accounts (Multi-account switcher + New Tab)
        let acc_dot = if any_tab_busy {
            Some(Palette::WARNING)
        } else if any_tab_logged {
            Some(Palette::SUCCESS)
        } else {
            None
        };
        let badge = (tabs.len() > 1).then_some(tabs.len());
        if rail_icon_button(
            ui,
            MobileIcon::Accounts,
            ButtonTone::Standard {
                open: self.mobile_flyout == Some(MobileFlyout::Accounts)
                    || self.tools.accounts.open,
            },
            acc_dot,
            badge,
            "Accounts & Tabs",
        )
        .clicked()
        {
            self.toggle_flyout(MobileFlyout::Accounts);
        }

        // 2. Quick Controls (Start/Stop Bot & Maid, Cell/Pad Jump, Bank)
        let quick_dot = (bot_running || maid_running || aa_running).then_some(Palette::SUCCESS);
        if rail_icon_button(
            ui,
            MobileIcon::Quick,
            ButtonTone::Standard {
                open: self.mobile_flyout == Some(MobileFlyout::QuickActions),
            },
            quick_dot,
            None,
            "Quick Controls, Cell Jump & Bank",
        )
        .clicked()
        {
            self.toggle_flyout(MobileFlyout::QuickActions);
        }

        // 3. 1-Tap Auto Attack Toggle
        if rail_icon_button(
            ui,
            MobileIcon::Swords,
            ButtonTone::Toggle { active: aa_running },
            None,
            None,
            if aa_running {
                "Auto Attack: ON (Tap to stop)"
            } else {
                "Auto Attack: OFF (Tap to start)"
            },
        )
        .clicked()
        {
            self.bot.auto_attack.toggle(self.bot.clone());
        }

        rail_divider(ui);

        // Main menu order (mirrors desktop navbar): Bot, Tools, Packets, Options, Logs, Maid

        // 4. Bot Manager
        if rail_icon_button(
            ui,
            MobileIcon::Bot,
            ButtonTone::Standard {
                open: self.show_bot_manager,
            },
            bot_running.then_some(Palette::SUCCESS),
            None,
            "Bot Manager",
        )
        .clicked()
        {
            self.mobile_flyout = None;
            self.show_bot_manager = !self.show_bot_manager;
        }

        // 5. Tools Menu (Fast Travel, Loaders, Hotkeys, Cosmetics)
        if rail_icon_button(
            ui,
            MobileIcon::Tools,
            ButtonTone::Standard { open: tools_open },
            None,
            None,
            "Tools",
        )
        .clicked()
        {
            self.toggle_flyout(MobileFlyout::Tools);
        }

        // 6. Packets Menu (Packet Logger, Spammer & Sender, Tamperer)
        if rail_icon_button(
            ui,
            MobileIcon::Packets,
            ButtonTone::Standard { open: packets_open },
            None,
            None,
            "Packets",
        )
        .clicked()
        {
            self.toggle_flyout(MobileFlyout::Packets);
        }

        // 7. Game Options (FPS, Walk Speed, Lag Killer, Hide Players, Disable FX)
        if rail_icon_button(
            ui,
            MobileIcon::Sliders,
            ButtonTone::Standard {
                open: self.show_options,
            },
            opts_active.then_some(Palette::BORDER_FOCUS),
            None,
            "Game Options",
        )
        .clicked()
        {
            self.mobile_flyout = None;
            self.show_options = !self.show_options;
        }

        // 8. Debug Logs
        if rail_icon_button(
            ui,
            MobileIcon::Terminal,
            ButtonTone::Standard {
                open: self.show_logs,
            },
            None,
            None,
            "Debug Logs",
        )
        .clicked()
        {
            self.mobile_flyout = None;
            self.show_logs = !self.show_logs;
        }

        // 9. Maid Remake
        if rail_icon_button(
            ui,
            MobileIcon::Maid,
            ButtonTone::Standard {
                open: self.tools.maid.open,
            },
            maid_running.then_some(Palette::SUCCESS),
            None,
            "Maid Remake",
        )
        .clicked()
        {
            self.mobile_flyout = None;
            self.tools.maid.open = !self.tools.maid.open;
        }
    }

    fn draw_rail_bottom_items(
        &mut self,
        ui: &mut Ui,
        icon_tex_id: egui::TextureId,
        rail_collapsed: &mut bool,
        reverse: bool,
    ) {
        if reverse {
            if rail_collapse_button(ui).clicked() {
                *rail_collapsed = true;
                self.mobile_flyout = None;
            }
            if rail_brand_button(ui, icon_tex_id, self.show_about).clicked() {
                self.mobile_flyout = None;
                self.show_about = !self.show_about;
            }
        } else {
            if rail_brand_button(ui, icon_tex_id, self.show_about).clicked() {
                self.mobile_flyout = None;
                self.show_about = !self.show_about;
            }
            if rail_collapse_button(ui).clicked() {
                *rail_collapsed = true;
                self.mobile_flyout = None;
            }
        }
    }

    fn render_mobile_flyout(
        &mut self,
        ctx: &Context,
        flyout: MobileFlyout,
        tabs: &[TabDescriptor],
        active_id: u64,
        snap: &grimlite_bot::state::PlayerSnapshot,
    ) -> TabBarAction {
        let mut tab_action = TabBarAction::None;
        let anchor_y: f32 = match flyout {
            MobileFlyout::Accounts => 8.0,
            MobileFlyout::QuickActions => 44.0,
            MobileFlyout::Tools => 152.0,
            MobileFlyout::Packets => 186.0,
        };
        let screen = ctx.content_rect();
        let max_y = (screen.bottom() - 220.0).max(8.0);
        let pos = pos2(MOBILE_RAIL_WIDTH + 6.0, anchor_y.min(max_y));

        let win_id = match flyout {
            MobileFlyout::Accounts => "grimlite_flyout_accounts",
            MobileFlyout::QuickActions => "grimlite_flyout_quick",
            MobileFlyout::Tools => "grimlite_flyout_tools",
            MobileFlyout::Packets => "grimlite_flyout_packets",
        };

        let frame = Frame::new()
            .fill(Palette::BG_BASE)
            .stroke(Stroke::new(1.0, Palette::BORDER_CONTROL))
            .corner_radius(CornerRadius::same(8))
            .inner_margin(Margin::symmetric(10, 8))
            .shadow(Shadow {
                offset: [0, 4],
                blur: 14,
                spread: 0,
                color: Color32::from_black_alpha(150),
            });

        let mut close_flyout = false;
        let mut combo_open_now = false;
        let inner = egui::Window::new(win_id)
            .id(Id::new(win_id))
            .title_bar(false)
            .resizable(false)
            .movable(false)
            .collapsible(false)
            .order(Order::Foreground)
            .fixed_pos(pos)
            .frame(frame)
            .show(ctx, |ui| match flyout {
                MobileFlyout::Accounts => {
                    ui.set_width(214.0);
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new("Accounts")
                                .size(12.5)
                                .strong()
                                .color(Color32::WHITE),
                        );
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            ui.label(
                                RichText::new(format!("{} open", tabs.len()))
                                    .size(11.0)
                                    .color(Palette::TEXT_MUTED),
                            );
                        });
                    });
                    ui.add_space(4.0);

                    let can_close = tabs.len() > 1;
                    ScrollArea::vertical()
                        .id_salt("grimlite_mobile_accounts_scroll")
                        .max_height(210.0)
                        .auto_shrink([false, true])
                        .show(ui, |ui| {
                            ui.spacing_mut().item_spacing.y = 4.0;
                            for (idx, tab) in tabs.iter().enumerate() {
                                let is_active = tab.id == active_id;
                                let fill = if is_active {
                                    Palette::ACCENT_SELECT
                                } else {
                                    Palette::BG_SURFACE
                                };
                                let stroke = if is_active {
                                    Stroke::new(1.2, Palette::BORDER_FOCUS)
                                } else {
                                    Stroke::new(1.0, Palette::BORDER_SUBTLE)
                                };
                                let dot_color = if tab.bot_running {
                                    Palette::WARNING
                                } else if tab.logged_in {
                                    Palette::SUCCESS
                                } else {
                                    Palette::TEXT_MUTED
                                };

                                let mut close_this = false;
                                let row_resp = Frame::new()
                                    .fill(fill)
                                    .stroke(stroke)
                                    .corner_radius(CornerRadius::same(6))
                                    .inner_margin(Margin::symmetric(8, 6))
                                    .show(ui, |ui| {
                                        ui.set_min_width(ui.available_width());
                                        ui.horizontal(|ui| {
                                            let (dot_rect, _) = ui.allocate_exact_size(
                                                vec2(8.0, 8.0),
                                                Sense::hover(),
                                            );
                                            ui.painter().circle_filled(
                                                dot_rect.center(),
                                                3.6,
                                                dot_color,
                                            );

                                            let title = if tab.label.is_empty() {
                                                format!("Account #{}", idx + 1)
                                            } else {
                                                tab.label.clone()
                                            };
                                            if ui
                                                .add(
                                                    egui::Label::new(
                                                        RichText::new(title)
                                                            .size(12.0)
                                                            .strong()
                                                            .color(if is_active {
                                                                Color32::WHITE
                                                            } else {
                                                                Palette::TEXT_PRIMARY
                                                            }),
                                                    )
                                                    .sense(Sense::click()),
                                                )
                                                .clicked()
                                                && !is_active
                                            {
                                                tab_action = TabBarAction::Switch(tab.id);
                                                close_flyout = true;
                                            }

                                            ui.with_layout(
                                                Layout::right_to_left(Align::Center),
                                                |ui| {
                                                    if can_close {
                                                        if ui
                                                            .add_sized(
                                                                [22.0, 20.0],
                                                                egui::Button::new(
                                                                    RichText::new("×")
                                                                        .size(13.0)
                                                                        .strong(),
                                                                ),
                                                            )
                                                            .on_hover_text("Close account tab")
                                                            .clicked()
                                                        {
                                                            close_this = true;
                                                        }
                                                    } else if is_active {
                                                        ui.label(
                                                            RichText::new("Active")
                                                                .size(10.5)
                                                                .color(Palette::BORDER_FOCUS),
                                                        );
                                                    }
                                                },
                                            );
                                        });
                                    });

                                if close_this {
                                    tab_action = TabBarAction::Close(tab.id);
                                    close_flyout = true;
                                } else if row_resp.response.interact(Sense::click()).clicked()
                                    && !is_active
                                    && tab_action == TabBarAction::None
                                {
                                    tab_action = TabBarAction::Switch(tab.id);
                                    close_flyout = true;
                                }
                            }
                        });

                    ui.add_space(6.0);
                    if !snap.logged_in {
                        if flyout_menu_row(ui, "Account Manager", self.tools.accounts.open)
                            .clicked()
                        {
                            self.tools.accounts.open = !self.tools.accounts.open;
                            close_flyout = true;
                        }
                        ui.add_space(4.0);
                    }
                    if theme::primary_button(ui, "+ New Account Tab", [214.0, 28.0]).clicked() {
                        tab_action = TabBarAction::NewTab;
                        close_flyout = true;
                    }
                }

                MobileFlyout::QuickActions => {
                    ui.set_width(248.0);
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new("Quick Controls")
                                .size(12.5)
                                .strong()
                                .color(Color32::WHITE),
                        );
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            let status_txt = if snap.logged_in && !snap.map.is_empty() {
                                snap.map.as_str()
                            } else if snap.logged_in {
                                "In Game"
                            } else {
                                "Login Screen"
                            };
                            ui.label(
                                RichText::new(status_txt)
                                    .size(11.0)
                                    .color(if snap.logged_in {
                                        Palette::SUCCESS
                                    } else {
                                        Palette::TEXT_MUTED
                                    }),
                            );
                        });
                    });
                    ui.add_space(5.0);

                    let bot_running = self.bot.runner.is_running();
                    let has_cmds = !self.bot.runner.config().commands.is_empty();
                    let aa_on = self.bot.auto_attack.is_running();
                    let maid_on = self.bot.maid.is_running();

                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 6.0;
                        let bot_off_label = if has_cmds { "Start Bot" } else { "Setup Bot…" };
                        if theme::toggle_button(
                            ui,
                            bot_running,
                            true,
                            bot_off_label,
                            "Bot: ON",
                            Some([121.0, 28.0]),
                        )
                        .clicked()
                        {
                            if bot_running {
                                self.bot.stop_bot();
                            } else if has_cmds {
                                self.bot.start_bot();
                            } else {
                                self.show_bot_manager = true;
                                close_flyout = true;
                            }
                        }

                        if theme::toggle_button(
                            ui,
                            aa_on,
                            true,
                            "Auto Attack",
                            "Auto: ON",
                            Some([121.0, 28.0]),
                        )
                        .clicked()
                        {
                            self.bot.auto_attack.toggle(self.bot.clone());
                        }
                    });

                    ui.add_space(5.0);
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 6.0;
                        if theme::toggle_button(
                            ui,
                            maid_on,
                            true,
                            "Start Maid",
                            "Maid: ON",
                            Some([121.0, 28.0]),
                        )
                        .clicked()
                        {
                            self.bot.maid.toggle(self.bot.clone());
                        }

                        if ui
                            .add_enabled(
                                snap.logged_in,
                                egui::Button::new("Open Bank").min_size(vec2(121.0, 28.0)),
                            )
                            .clicked()
                        {
                            self.bot.spawn_flash_action("openBank", vec![]);
                            close_flyout = true;
                        }
                    });

                    ui.add_space(6.0);
                    theme::card(ui, "Cell & Pad Jump", |ui| {
                        ui.add_enabled_ui(snap.logged_in, |ui| {
                            ui.horizontal(|ui| {
                                ui.spacing_mut().item_spacing.x = 4.0;

                                let cells = self.bot.state.cell_list();
                                let mut cell_changed = false;
                                let cell_resp = theme::dropdown(
                                    ui,
                                    "mobile_quick_cell_cb",
                                    self.jump_cell.clone(),
                                    104.0,
                                    |ui| {
                                        for c in &cells {
                                            if ui
                                                .selectable_value(
                                                    &mut self.jump_cell,
                                                    c.clone(),
                                                    c,
                                                )
                                                .changed()
                                            {
                                                cell_changed = true;
                                            }
                                        }
                                    },
                                );
                                if cell_resp.inner.is_some() {
                                    combo_open_now = true;
                                }
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

                                let mut pad_changed = false;
                                let pad_resp = theme::dropdown(
                                    ui,
                                    "mobile_quick_pad_cb",
                                    self.jump_pad.clone(),
                                    86.0,
                                    |ui| {
                                        for &p in PADS {
                                            if ui
                                                .selectable_value(
                                                    &mut self.jump_pad,
                                                    p.to_string(),
                                                    p,
                                                )
                                                .changed()
                                            {
                                                pad_changed = true;
                                            }
                                        }
                                    },
                                );
                                if pad_resp.inner.is_some() {
                                    combo_open_now = true;
                                }
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

                                if theme::primary_button(ui, "Go", [34.0, 22.0])
                                    .on_hover_text("Refresh cells & jump")
                                    .clicked()
                                {
                                    self.bot.refresh_map_cells();
                                    let target_cell = if self.jump_cell.is_empty() {
                                        snap.cell.clone()
                                    } else {
                                        self.jump_cell.clone()
                                    };
                                    self.bot.spawn_flash_action(
                                        "moveToCell",
                                        vec![
                                            FlashArg::from(target_cell.as_str()),
                                            FlashArg::from(self.jump_pad.as_str()),
                                        ],
                                    );
                                }
                            });
                        });
                    });
                }

                MobileFlyout::Tools => {
                    ui.set_width(196.0);
                    ui.label(
                        RichText::new("Tools")
                            .size(12.5)
                            .strong()
                            .color(Color32::WHITE),
                    );
                    ui.add_space(4.0);
                    ui.spacing_mut().item_spacing.y = 3.0;

                    if flyout_menu_row(ui, "Fast Travel", self.tools.travels.open).clicked() {
                        self.tools.travels.open = !self.tools.travels.open;
                        close_flyout = true;
                    }
                    if flyout_menu_row(ui, "Loader / Grabber", self.tools.loaders.open).clicked() {
                        self.tools.loaders.open = !self.tools.loaders.open;
                        close_flyout = true;
                    }
                    if flyout_menu_row(ui, "Hotkeys", self.tools.hotkeys.open).clicked() {
                        self.tools.hotkeys.open = !self.tools.hotkeys.open;
                        close_flyout = true;
                    }
                    if flyout_menu_row(ui, "Cosmetics", self.tools.cosmetics.open).clicked() {
                        self.tools.cosmetics.open = !self.tools.cosmetics.open;
                        close_flyout = true;
                    }
                }

                MobileFlyout::Packets => {
                    ui.set_width(210.0);
                    ui.label(
                        RichText::new("Packets")
                            .size(12.5)
                            .strong()
                            .color(Color32::WHITE),
                    );
                    ui.add_space(4.0);
                    ui.spacing_mut().item_spacing.y = 3.0;

                    if flyout_menu_row(ui, "Packet Logger", self.tools.packets.show_logger)
                        .clicked()
                    {
                        self.tools.packets.show_logger = !self.tools.packets.show_logger;
                        close_flyout = true;
                    }
                    if flyout_menu_row(
                        ui,
                        "Packet Spammer & Sender",
                        self.tools.packets.show_spammer,
                    )
                    .clicked()
                    {
                        self.tools.packets.show_spammer = !self.tools.packets.show_spammer;
                        close_flyout = true;
                    }
                    if flyout_menu_row(
                        ui,
                        "Packet Tamperer / Interceptor",
                        self.tools.packets.show_tamperer,
                    )
                    .clicked()
                    {
                        self.tools.packets.show_tamperer = !self.tools.packets.show_tamperer;
                        close_flyout = true;
                    }
                }
            });

        // Dismiss the flyout cleanly when tapping outside both the flyout card and the left rail,
        // unless an inner dropdown (`ComboBox` popup) was open on the previous or current frame.
        let combo_guard = self.mobile_combo_open || combo_open_now;
        self.mobile_combo_open = combo_open_now;
        if let Some(inner_resp) = inner {
            let flyout_rect = inner_resp.response.rect;
            let tapped_outside = !combo_guard
                && ctx.input(|i| {
                    i.pointer.any_pressed()
                        && i.pointer.interact_pos().is_some_and(|p| {
                            p.x > MOBILE_RAIL_WIDTH && !flyout_rect.contains(p)
                        })
                });
            if tapped_outside {
                close_flyout = true;
            }
        }

        if close_flyout {
            self.mobile_flyout = None;
            self.mobile_combo_open = false;
        }

        tab_action
    }
}

fn flyout_menu_row(ui: &mut Ui, label: &str, active: bool) -> Response {
    let desired = vec2(ui.available_width(), 28.0);
    let (rect, response) = ui.allocate_exact_size(desired, Sense::click());
    let hovered = response.hovered() || response.is_pointer_button_down_on();

    let fill = if active {
        Palette::ACCENT_SELECT
    } else if hovered {
        Palette::BG_BUTTON_HOVER
    } else {
        Palette::BG_SURFACE
    };
    let stroke = if active {
        Stroke::new(1.0, Palette::BORDER_FOCUS)
    } else if hovered {
        Stroke::new(1.0, Palette::BORDER_HOVER)
    } else {
        Stroke::new(1.0, Palette::BORDER_SUBTLE)
    };

    let painter = ui.painter();
    painter.rect(rect, CornerRadius::same(5), fill, stroke, StrokeKind::Inside);

    if active {
        painter.circle_filled(
            pos2(rect.right() - 11.0, rect.center().y),
            3.5,
            Palette::BORDER_FOCUS,
        );
    }

    painter.text(
        pos2(rect.left() + 10.0, rect.center().y),
        Align2::LEFT_CENTER,
        label,
        FontId::proportional(12.0),
        if active || hovered {
            Color32::WHITE
        } else {
            Palette::TEXT_PRIMARY
        },
    );

    response
}

fn rail_divider(ui: &mut Ui) {
    let (rect, _) = ui.allocate_exact_size(vec2(28.0, 5.0), Sense::hover());
    let y = rect.center().y;
    ui.painter().line_segment(
        [pos2(rect.left(), y), pos2(rect.right(), y)],
        Stroke::new(1.0, Palette::BORDER_SUBTLE),
    );
}

fn rail_icon_button(
    ui: &mut Ui,
    icon: MobileIcon,
    tone: ButtonTone,
    led: Option<Color32>,
    badge_count: Option<usize>,
    tooltip: &str,
) -> Response {
    let (rect, response) = ui.allocate_exact_size(ICON_BTN_SIZE, Sense::click());
    let pressed = response.is_pointer_button_down_on() || response.hovered();

    let (fill, stroke, ink) = match tone {
        ButtonTone::Toggle { active: true } => (
            Palette::SUCCESS_BG,
            Stroke::new(1.4, Palette::SUCCESS),
            Palette::SUCCESS,
        ),
        ButtonTone::Standard { open: true } => (
            Palette::ACCENT_SELECT,
            Stroke::new(1.4, Palette::BORDER_FOCUS),
            Color32::WHITE,
        ),
        ButtonTone::Toggle { active: false } | ButtonTone::Standard { open: false } => {
            if pressed {
                (
                    Palette::BG_BUTTON_HOVER,
                    Stroke::new(1.0, Palette::BORDER_HOVER),
                    Color32::WHITE,
                )
            } else {
                (
                    Palette::BG_SURFACE,
                    Stroke::new(1.0, Palette::BORDER_SUBTLE),
                    Palette::TEXT_PRIMARY,
                )
            }
        }
    };

    let painter = ui.painter();
    painter.rect(rect, CornerRadius::same(7), fill, stroke, StrokeKind::Inside);
    paint_mobile_icon(painter, icon, rect.center(), ink);

    if let Some(count) = badge_count {
        let text = if count > 9 {
            "9+".to_string()
        } else {
            count.to_string()
        };
        let galley =
            painter.layout_no_wrap(text, FontId::proportional(9.5), Color32::WHITE);
        let bsize = vec2((galley.size().x + 6.0).max(14.0), 13.0);
        let brect = Rect::from_center_size(pos2(rect.right() - 5.0, rect.top() + 5.0), bsize);
        painter.rect_filled(brect, CornerRadius::same(7), Palette::ACCENT_PRIMARY);
        painter.rect_stroke(
            brect,
            CornerRadius::same(7),
            Stroke::new(1.0, Palette::BORDER_FOCUS),
            StrokeKind::Inside,
        );
        painter.galley(brect.center() - galley.size() / 2.0, galley, Color32::WHITE);
    } else if let Some(dot_color) = led {
        let at = pos2(rect.right() - 5.5, rect.top() + 5.5);
        painter.circle_filled(at, 4.8, fill);
        painter.circle_filled(at, 3.3, dot_color);
    }

    if badge_count.is_some()
        && let Some(dot_color) = led
    {
        let at = pos2(rect.left() + 5.5, rect.top() + 5.5);
        painter.circle_filled(at, 4.5, fill);
        painter.circle_filled(at, 3.0, dot_color);
    }

    response.on_hover_text(tooltip)
}

fn rail_brand_button(ui: &mut Ui, tex_id: egui::TextureId, open: bool) -> Response {
    let (rect, response) = ui.allocate_exact_size(ICON_BTN_SIZE, Sense::click());
    let pressed = response.is_pointer_button_down_on() || response.hovered();
    let (fill, stroke) = if open {
        (
            Palette::ACCENT_SELECT,
            Stroke::new(1.4, Palette::BORDER_FOCUS),
        )
    } else if pressed {
        (
            Palette::BG_BUTTON_HOVER,
            Stroke::new(1.0, Palette::BORDER_HOVER),
        )
    } else {
        (
            Palette::BG_SURFACE,
            Stroke::new(1.0, Palette::BORDER_SUBTLE),
        )
    };
    let painter = ui.painter();
    painter.rect(rect, CornerRadius::same(7), fill, stroke, StrokeKind::Inside);
    let img_rect = Rect::from_center_size(rect.center(), vec2(20.0, 20.0));
    painter.image(
        tex_id,
        img_rect,
        Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
        Color32::WHITE,
    );
    response.on_hover_text("About Grimlite Ri & Resource Monitor")
}

fn rail_collapse_button(ui: &mut Ui) -> Response {
    let (rect, response) = ui.allocate_exact_size(COLLAPSE_BTN_SIZE, Sense::click());
    let pressed = response.is_pointer_button_down_on() || response.hovered();
    let fill = if pressed {
        Palette::BG_BUTTON_HOVER
    } else {
        Color32::TRANSPARENT
    };
    let stroke = if pressed {
        Stroke::new(1.0, Palette::BORDER_HOVER)
    } else {
        Stroke::new(1.0, Palette::BORDER_SUBTLE)
    };
    let ink = if pressed {
        Color32::WHITE
    } else {
        Palette::TEXT_MUTED
    };
    let painter = ui.painter();
    painter.rect(rect, CornerRadius::same(5), fill, stroke, StrokeKind::Inside);
    paint_mobile_icon(painter, MobileIcon::CollapseLeft, rect.center(), ink);
    response.on_hover_text("Hide sidebar")
}

fn show_collapsed_handle(ctx: &Context, any_active: bool, rail_collapsed: &mut bool) {
    let screen = ctx.content_rect();
    let size = vec2(22.0, 56.0);
    let pos = pos2(screen.left(), screen.center().y - size.y / 2.0);
    egui::Area::new(Id::new("grimlite_mobile_rail_handle"))
        .order(Order::Foreground)
        .fixed_pos(pos)
        .show(ctx, |ui| {
            let (rect, response) = ui.allocate_exact_size(size, Sense::click());
            let pressed = response.hovered() || response.is_pointer_button_down_on();
            let fill = if pressed {
                Palette::ACCENT_PRIMARY
            } else {
                Palette::BG_BASE.gamma_multiply(0.88)
            };
            let corners = CornerRadius {
                nw: 0,
                sw: 0,
                ne: 9,
                se: 9,
            };
            let painter = ui.painter();
            painter.rect(
                rect,
                corners,
                fill,
                Stroke::new(1.0, Palette::BORDER_CONTROL),
                StrokeKind::Inside,
            );
            let c = rect.center();
            let s = Stroke::new(1.8, Color32::WHITE);
            painter.add(Shape::line(
                vec![
                    pos2(c.x - 2.2, c.y - 5.0),
                    pos2(c.x + 2.6, c.y),
                    pos2(c.x - 2.2, c.y + 5.0),
                ],
                s,
            ));
            if any_active {
                painter.circle_filled(pos2(c.x, rect.top() + 8.0), 3.2, Palette::SUCCESS);
            }
            if response.on_hover_text("Show Grimlite toolbar").clicked() {
                *rail_collapsed = false;
            }
        });
}

/// Crisp custom anti-aliased vector icons drawn on a `16x16` logical grid centered at `c`.
fn paint_mobile_icon(painter: &egui::Painter, icon: MobileIcon, c: Pos2, color: Color32) {
    let s = Stroke::new(1.65, color);
    match icon {
        MobileIcon::Accounts => {
            // Primary user head + shoulders
            painter.circle_stroke(pos2(c.x - 1.6, c.y - 2.8), 2.5, s);
            painter.add(Shape::line(
                vec![
                    pos2(c.x - 6.0, c.y + 5.2),
                    pos2(c.x - 5.4, c.y + 2.2),
                    pos2(c.x - 3.6, c.y + 1.2),
                    pos2(c.x + 0.4, c.y + 1.2),
                    pos2(c.x + 2.2, c.y + 2.2),
                    pos2(c.x + 2.8, c.y + 5.2),
                ],
                s,
            ));
            // Secondary stacked user silhouette on the right
            painter.circle_stroke(pos2(c.x + 3.6, c.y - 2.4), 1.9, s);
            painter.add(Shape::line(
                vec![
                    pos2(c.x + 3.4, c.y + 1.4),
                    pos2(c.x + 5.4, c.y + 2.2),
                    pos2(c.x + 6.2, c.y + 5.0),
                ],
                s,
            ));
        }

        MobileIcon::Bot => {
            // Antenna stem + tip
            painter.line_segment([pos2(c.x, c.y - 6.4), pos2(c.x, c.y - 4.0)], s);
            painter.circle_filled(pos2(c.x, c.y - 6.7), 1.15, color);
            // Robot head frame
            let head = Rect::from_min_max(pos2(c.x - 5.6, c.y - 4.0), pos2(c.x + 5.6, c.y + 5.0));
            painter.rect_stroke(head, CornerRadius::same(2), s, StrokeKind::Inside);
            // Side ears
            painter.line_segment([pos2(c.x - 6.9, c.y - 0.8), pos2(c.x - 6.9, c.y + 1.8)], s);
            painter.line_segment([pos2(c.x + 6.9, c.y - 0.8), pos2(c.x + 6.9, c.y + 1.8)], s);
            // Eyes + mouth
            painter.circle_filled(pos2(c.x - 2.3, c.y - 0.3), 1.15, color);
            painter.circle_filled(pos2(c.x + 2.3, c.y - 0.3), 1.15, color);
            painter.line_segment([pos2(c.x - 2.2, c.y + 2.6), pos2(c.x + 2.2, c.y + 2.6)], s);
        }

        MobileIcon::Maid => {
            // Guardian shield outline + inner 4-point crest
            let pts = vec![
                pos2(c.x, c.y - 6.6),
                pos2(c.x + 5.6, c.y - 4.2),
                pos2(c.x + 5.0, c.y + 1.0),
                pos2(c.x, c.y + 6.6),
                pos2(c.x - 5.0, c.y + 1.0),
                pos2(c.x - 5.6, c.y - 4.2),
            ];
            painter.add(Shape::closed_line(pts, s));
            painter.line_segment([pos2(c.x, c.y - 2.6), pos2(c.x, c.y + 2.6)], s);
            painter.line_segment([pos2(c.x - 2.6, c.y), pos2(c.x + 2.6, c.y)], s);
        }

        MobileIcon::Quick => {
            // Crisp lightning bolt (`zap`)
            let pts = vec![
                pos2(c.x + 0.9, c.y - 6.6),
                pos2(c.x - 5.0, c.y + 0.6),
                pos2(c.x - 0.3, c.y + 0.6),
                pos2(c.x - 0.9, c.y + 6.6),
                pos2(c.x + 5.0, c.y - 0.6),
                pos2(c.x + 0.3, c.y - 0.6),
            ];
            painter.add(Shape::closed_line(pts, s));
        }

        MobileIcon::Swords => {
            // Crossed swords (`swords`)
            painter.line_segment([pos2(c.x - 5.4, c.y + 5.4), pos2(c.x + 5.6, c.y - 5.6)], s);
            painter.line_segment([pos2(c.x - 5.0, c.y + 1.6), pos2(c.x - 1.6, c.y + 5.0)], s);

            painter.line_segment([pos2(c.x + 5.4, c.y + 5.4), pos2(c.x - 5.6, c.y - 5.6)], s);
            painter.line_segment([pos2(c.x + 1.6, c.y + 5.0), pos2(c.x + 5.0, c.y + 1.6)], s);
        }

        MobileIcon::Tools => {
            // 2x2 modular grid (`layout-grid`)
            let r = CornerRadius::same(1);
            for (dx, dy) in [(-3.3, -3.3), (3.3, -3.3), (-3.3, 3.3), (3.3, 3.3)] {
                let box_rect = Rect::from_center_size(pos2(c.x + dx, c.y + dy), vec2(4.8, 4.8));
                painter.rect_stroke(box_rect, r, s, StrokeKind::Inside);
            }
        }

        MobileIcon::Packets => {
            // Bidirectional network packet arrows (`arrow-left-right`)
            // Top arrow pointing right (->)
            painter.line_segment([pos2(c.x - 5.6, c.y - 2.8), pos2(c.x + 5.2, c.y - 2.8)], s);
            painter.add(Shape::line(
                vec![
                    pos2(c.x + 2.4, c.y - 5.6),
                    pos2(c.x + 5.4, c.y - 2.8),
                    pos2(c.x + 2.4, c.y),
                ],
                s,
            ));
            // Bottom arrow pointing left (<-)
            painter.line_segment([pos2(c.x + 5.6, c.y + 2.8), pos2(c.x - 5.2, c.y + 2.8)], s);
            painter.add(Shape::line(
                vec![
                    pos2(c.x - 2.4, c.y),
                    pos2(c.x - 5.4, c.y + 2.8),
                    pos2(c.x - 2.4, c.y + 5.6),
                ],
                s,
            ));
        }

        MobileIcon::Sliders => {
            // 3 horizontal tuning tracks with staggered circular knobs (`sliders-horizontal`)
            for (dy, kx) in [(-4.4, 2.2), (0.0, -2.3), (4.4, 1.4)] {
                let y = c.y + dy;
                painter.line_segment([pos2(c.x - 6.0, y), pos2(c.x + 6.0, y)], s);
                painter.circle_filled(pos2(c.x + kx, y), 2.1, Palette::BG_SURFACE);
                painter.circle_stroke(pos2(c.x + kx, y), 1.8, s);
            }
        }

        MobileIcon::Terminal => {
            // Terminal window (`>_`)
            let box_rect = Rect::from_min_max(pos2(c.x - 6.2, c.y - 5.0), pos2(c.x + 6.2, c.y + 5.0));
            painter.rect_stroke(box_rect, CornerRadius::same(2), s, StrokeKind::Inside);
            painter.add(Shape::line(
                vec![
                    pos2(c.x - 3.6, c.y - 2.0),
                    pos2(c.x - 1.0, c.y),
                    pos2(c.x - 3.6, c.y + 2.0),
                ],
                s,
            ));
            painter.line_segment([pos2(c.x + 0.5, c.y + 2.2), pos2(c.x + 3.8, c.y + 2.2)], s);
        }

        MobileIcon::CollapseLeft => {
            // Double left chevron (`«`)
            for dx in [-2.2, 2.4] {
                painter.add(Shape::line(
                    vec![
                        pos2(c.x + dx + 1.6, c.y - 3.6),
                        pos2(c.x + dx - 1.8, c.y),
                        pos2(c.x + dx + 1.6, c.y + 3.6),
                    ],
                    s,
                ));
            }
        }
    }
}
