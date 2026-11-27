//! Compact Hotkeys tool window: configurable keyboard shortcuts for bot & game actions.

use crate::show_detached_window_sized;
use crate::theme;
use egui::{Context, Key, RichText, ScrollArea};
use grimlite_bot::Bot;
use std::sync::Arc;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum HotkeyAction {
    ToggleBot,
    ToggleAutoAttack,
    OpenBank,
    ToggleLagKiller,
    ToggleInfiniteRange,
    ToggleEnemyMagnet,
    ToggleProvoke,
    Rest,
    CancelTarget,
    ShowBotManager,
    ShowTravels,
    ShowLoaders,
    ShowCosmetics,
}

impl HotkeyAction {
    const ALL: &[Self] = &[
        Self::ToggleBot,
        Self::ToggleAutoAttack,
        Self::OpenBank,
        Self::ToggleLagKiller,
        Self::ToggleInfiniteRange,
        Self::ToggleEnemyMagnet,
        Self::ToggleProvoke,
        Self::Rest,
        Self::CancelTarget,
        Self::ShowBotManager,
        Self::ShowTravels,
        Self::ShowLoaders,
        Self::ShowCosmetics,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::ToggleBot => "Start / Stop Bot",
            Self::ToggleAutoAttack => "Toggle Auto Attack",
            Self::OpenBank => "Open Bank",
            Self::ToggleLagKiller => "Toggle Lag Killer",
            Self::ToggleInfiniteRange => "Toggle Infinite Range",
            Self::ToggleEnemyMagnet => "Toggle Enemy Magnet",
            Self::ToggleProvoke => "Toggle Provoke Monsters",
            Self::Rest => "Rest",
            Self::CancelTarget => "Cancel Target",
            Self::ShowBotManager => "Toggle Bot Manager",
            Self::ShowTravels => "Toggle Fast Travel",
            Self::ShowLoaders => "Toggle Loader / Grabber",
            Self::ShowCosmetics => "Toggle Cosmetics",
        }
    }
}

const AVAILABLE_KEYS: &[Key] = &[
    Key::F1,
    Key::F2,
    Key::F3,
    Key::F4,
    Key::F5,
    Key::F6,
    Key::F7,
    Key::F8,
    Key::F9,
    Key::F10,
    Key::F11,
    Key::F12,
    Key::Num1,
    Key::Num2,
    Key::Num3,
    Key::Num4,
    Key::Num5,
    Key::Num6,
    Key::Num7,
    Key::Num8,
    Key::Num9,
    Key::Num0,
];

#[derive(Clone)]
struct HotkeyBinding {
    key: Key,
    action: HotkeyAction,
    enabled: bool,
}

/// UI-level window toggles requested by triggered hotkeys.
#[derive(Default)]
pub struct HotkeyEffects {
    pub toggle_bot_manager: bool,
    pub toggle_travels: bool,
    pub toggle_loaders: bool,
    pub toggle_cosmetics: bool,
}

pub struct HotkeysWindow {
    bot: Arc<Bot>,
    pub open: bool,
    enabled: bool,
    bindings: Vec<HotkeyBinding>,
    draft_key: Key,
    draft_action: HotkeyAction,
}

impl HotkeysWindow {
    pub fn new(bot: Arc<Bot>) -> Self {
        Self {
            bot,
            open: false,
            enabled: true,
            bindings: vec![
                HotkeyBinding {
                    key: Key::F1,
                    action: HotkeyAction::ToggleBot,
                    enabled: true,
                },
                HotkeyBinding {
                    key: Key::F2,
                    action: HotkeyAction::ToggleAutoAttack,
                    enabled: true,
                },
                HotkeyBinding {
                    key: Key::F3,
                    action: HotkeyAction::OpenBank,
                    enabled: true,
                },
                HotkeyBinding {
                    key: Key::F4,
                    action: HotkeyAction::ToggleLagKiller,
                    enabled: true,
                },
            ],
            draft_key: Key::F5,
            draft_action: HotkeyAction::ShowBotManager,
        }
    }

    /// Checks keyboard input across both the main game window and all detached child windows
    /// (skipped while typing in a text field) and executes matching hotkey actions.
    pub fn poll(&self, ctx: &Context) -> HotkeyEffects {
        let mut effects = HotkeyEffects::default();

        // Always drain child-viewport key buffer so stale presses don't linger
        let child_keys: Vec<Key> = ctx.data_mut(|d| {
            let buf = d.get_temp_mut_or_default::<Vec<Key>>(egui::Id::new(
                "grimlite_child_pressed_keys",
            ));
            std::mem::take(buf)
        });

        if !self.enabled {
            return effects;
        }

        let root_wants_kb = ctx.egui_wants_keyboard_input();

        for b in &self.bindings {
            if !b.enabled {
                continue;
            }
            let pressed_root = !root_wants_kb && ctx.input(|i| i.key_pressed(b.key));
            let pressed_child = child_keys.contains(&b.key);
            if pressed_root || pressed_child {
                match b.action {
                    HotkeyAction::ToggleBot => {
                        if self.bot.runner.is_running() {
                            self.bot.stop_bot();
                        } else {
                            self.bot.start_bot();
                        }
                    }
                    HotkeyAction::ToggleAutoAttack => {
                        self.bot.auto_attack.toggle(self.bot.clone());
                    }
                    HotkeyAction::OpenBank => {
                        self.bot.spawn_flash_action("openBank", vec![]);
                    }
                    HotkeyAction::ToggleLagKiller => {
                        let mut opts = self.bot.options.get();
                        opts.lag_killer = !opts.lag_killer;
                        self.bot.apply_options(opts);
                    }
                    HotkeyAction::ToggleInfiniteRange => {
                        let mut opts = self.bot.options.get();
                        opts.infinite_range = !opts.infinite_range;
                        self.bot.apply_options(opts);
                    }
                    HotkeyAction::ToggleEnemyMagnet => {
                        let mut opts = self.bot.options.get();
                        opts.enemy_magnet = !opts.enemy_magnet;
                        self.bot.apply_options(opts);
                    }
                    HotkeyAction::ToggleProvoke => {
                        let mut opts = self.bot.options.get();
                        opts.provoke_monsters = !opts.provoke_monsters;
                        self.bot.apply_options(opts);
                    }
                    HotkeyAction::Rest => {
                        self.bot.spawn_flash_action("rest", vec![]);
                    }
                    HotkeyAction::CancelTarget => {
                        self.bot.spawn_flash_action("cancelTarget", vec![]);
                    }
                    HotkeyAction::ShowBotManager => effects.toggle_bot_manager = true,
                    HotkeyAction::ShowTravels => effects.toggle_travels = true,
                    HotkeyAction::ShowLoaders => effects.toggle_loaders = true,
                    HotkeyAction::ShowCosmetics => effects.toggle_cosmetics = true,
                }
            }
        }
        effects
    }

    pub fn show(&mut self, ctx: &Context) {
        if !self.open {
            return;
        }
        let mut open = self.open;
        show_detached_window_sized(
            ctx,
            "grimlite_hotkeys",
            "Hotkeys",
            [335.0, 245.0],
            Some([310.0, 210.0]),
            &mut open,
            |ui| {
                ui.spacing_mut().item_spacing = egui::vec2(5.0, 4.0);
                ui.spacing_mut().button_padding = egui::vec2(5.0, 2.0);
                ui.spacing_mut().interact_size.y = 20.0;

                // Header bar
                ui.horizontal(|ui| {
                    ui.checkbox(&mut self.enabled, "Enable Hotkeys");
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let active = self.bindings.iter().filter(|b| b.enabled).count();
                        ui.label(
                            RichText::new(format!("{active}/{} active", self.bindings.len()))
                                .small()
                                .weak(),
                        );
                    });
                });

                // Compact add row
                ui.horizontal(|ui| {
                    let key_w = 58.0;
                    let add_w = 50.0;
                    let act_w = (ui.available_width() - key_w - add_w - 10.0).max(110.0);
                    theme::dropdown(ui, "hotkey_draft_key", self.draft_key.name(), key_w, |ui| {
                        for &k in AVAILABLE_KEYS {
                            ui.selectable_value(&mut self.draft_key, k, k.name());
                        }
                    });
                    theme::dropdown(
                        ui,
                        "hotkey_draft_action",
                        self.draft_action.label(),
                        act_w,
                        |ui| {
                            for &act in HotkeyAction::ALL {
                                ui.selectable_value(&mut self.draft_action, act, act.label());
                            }
                        },
                    );
                    if theme::primary_button(ui, "+ Add", [add_w, 21.0]).clicked() {
                        self.bindings.retain(|b| b.key != self.draft_key);
                        self.bindings.push(HotkeyBinding {
                            key: self.draft_key,
                            action: self.draft_action,
                            enabled: true,
                        });
                    }
                });

                ui.add_space(2.0);

                // Zebra-striped bindings list
                let mut remove_idx: Option<usize> = None;
                theme::list_box(ui, None, |ui| {
                    ScrollArea::vertical()
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            ui.spacing_mut().item_spacing.y = 1.0;
                            if self.bindings.is_empty() {
                                ui.label(RichText::new("No hotkeys configured.").small().weak());
                            }
                            for (idx, b) in self.bindings.iter_mut().enumerate() {
                                theme::list_row(ui, idx, false, 0, |ui| {
                                    ui.horizontal(|ui| {
                                        ui.checkbox(&mut b.enabled, "");
                                        ui.label(
                                            RichText::new(format!("[{}]", b.key.name()))
                                                .monospace()
                                                .strong()
                                                .size(theme::LIST_ITEM_FONT_SIZE)
                                                .color(theme::Palette::BORDER_FOCUS),
                                        );
                                        ui.label(
                                            RichText::new(b.action.label())
                                                .size(theme::LIST_ITEM_FONT_SIZE),
                                        );
                                        ui.with_layout(
                                            egui::Layout::right_to_left(egui::Align::Center),
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
                            }
                        });
                });
                if let Some(idx) = remove_idx {
                    self.bindings.remove(idx);
                }
            },
        );
        self.open = open;
    }
}
