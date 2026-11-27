//! Compact, modern Maid Remake tool window (`egui`).
//! Mirrors all Grimlite-Li Maid Remake features in a clean 3-tab detached window.

use crate::bot_managers::FileDialogFn;
use crate::show_detached_window_sized;
use crate::theme;
use egui::{Color32, Context, Key, RichText, ScrollArea, TextEdit, Ui};
use grimlite_bot::Bot;
use grimlite_bot::maid::{MaidConfig, MaidPreset, UltraBossMode};
use std::sync::{Arc, Mutex};

#[derive(Clone, Copy, PartialEq, Eq)]
enum MaidTab {
    Main,
    Combat,
    Maps,
}

pub struct MaidWindow {
    bot: Arc<Bot>,
    pub open: bool,
    tab: MaidTab,
    players_cache: Arc<Mutex<Vec<String>>>,
    status_msg: Option<(String, bool)>,
    locked_map_draft: String,
    whitelist_map_draft: String,
    pick_json: Option<FileDialogFn>,
    save_json: Option<FileDialogFn>,
}

impl MaidWindow {
    pub fn new(
        bot: Arc<Bot>,
        pick_json: Option<FileDialogFn>,
        save_json: Option<FileDialogFn>,
    ) -> Self {
        Self {
            bot,
            open: false,
            tab: MaidTab::Main,
            players_cache: Arc::new(Mutex::new(Vec::new())),
            status_msg: None,
            locked_map_draft: String::new(),
            whitelist_map_draft: String::new(),
            pick_json,
            save_json,
        }
    }

    pub fn set_dialogs(
        &mut self,
        pick_json: Option<FileDialogFn>,
        save_json: Option<FileDialogFn>,
    ) {
        self.pick_json = pick_json;
        self.save_json = save_json;
    }

    /// Polls Maid's quick hotkeys (`R` = Unfollow/LockCell, `T` = Stop Attack) when Maid is running.
    pub fn poll_quick_hotkeys(&self, ctx: &Context, child_keys: &[Key]) {
        if !self.bot.maid.is_running() {
            return;
        }
        let cfg = self.bot.maid.config();
        if !cfg.global_hotkey {
            return;
        }
        let root_wants_kb = ctx.egui_wants_keyboard_input();
        let pressed_r =
            (!root_wants_kb && ctx.input(|i| i.key_pressed(Key::R))) || child_keys.contains(&Key::R);
        let pressed_t =
            (!root_wants_kb && ctx.input(|i| i.key_pressed(Key::T))) || child_keys.contains(&Key::T);

        if pressed_r {
            self.bot.maid.toggle_unfollow();
        }
        if pressed_t {
            self.bot.maid.toggle_stop_attack(&self.bot);
        }
    }

    fn refresh_players(&self) {
        let bot = self.bot.clone();
        let cache = self.players_cache.clone();
        let _ = std::thread::Builder::new()
            .name("maid-players".into())
            .spawn(move || {
                if let Some(flash) = bot.flash()
                    && let Ok(raw) = flash.call_str("PlayersInMap", &[])
                    && let Ok(mut list) = serde_json::from_str::<Vec<String>>(&raw)
                {
                    list.sort_by_key(|s| s.to_ascii_lowercase());
                    list.dedup();
                    if let Ok(mut guard) = cache.lock() {
                        *guard = list;
                    }
                }
            });
    }

    pub fn show(&mut self, ctx: &Context) {
        if !self.open {
            return;
        }

        let mut open = self.open;
        let rt = self.bot.maid.snapshot();
        if rt.stop_attack {
            ctx.request_repaint_after(std::time::Duration::from_millis(500));
        }

        show_detached_window_sized(
            ctx,
            "grimlite_maid",
            "Maid Remake",
            [380.0, 425.0],
            Some([355.0, 400.0]),
            &mut open,
            |ui| {
                ui.spacing_mut().item_spacing = egui::vec2(6.0, 5.0);
                ui.spacing_mut().button_padding = egui::vec2(7.0, 3.0);
                ui.spacing_mut().interact_size.y = 20.0;

                // ---- Top Bar: Primary Start/Stop + Status + Load/Save ----
                ui.horizontal(|ui| {
                    if theme::toggle_button(
                        ui,
                        rt.running,
                        true,
                        "Start Maid",
                        "Maid: ON",
                        Some([96.0, 24.0]),
                    )
                    .clicked()
                    {
                        self.bot.maid.toggle(self.bot.clone());
                    }

                    if rt.on_pause {
                        ui.label(
                            RichText::new("Paused")
                                .small()
                                .strong()
                                .color(Color32::from_rgb(245, 185, 66)),
                        );
                    } else if let Some(secs) = rt.stop_attack_elapsed_secs {
                        ui.label(
                            RichText::new(format!("Counter {:02}:{:02}", secs / 60, secs % 60))
                                .small()
                                .strong()
                                .color(Color32::from_rgb(235, 90, 90)),
                        );
                    } else if rt.taunt_count > 0 {
                        ui.label(
                            RichText::new(format!("Taunt #{}", rt.taunt_count))
                                .small()
                                .color(theme::Palette::BORDER_FOCUS),
                        );
                    }

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui
                            .add_sized([48.0, 22.0], egui::Button::new("Save"))
                            .on_hover_text("Save MaidConfig (.json)")
                            .clicked()
                        {
                            self.handle_save_config();
                        }
                        if ui
                            .add_sized([48.0, 22.0], egui::Button::new("Load"))
                            .on_hover_text("Load MaidConfig (.json)")
                            .clicked()
                        {
                            self.handle_load_config();
                        }
                    });
                });

                if !rt.loaded_config_name.is_empty() || self.status_msg.is_some() {
                    ui.horizontal(|ui| {
                        if !rt.loaded_config_name.is_empty() {
                            ui.label(
                                RichText::new(format!("Config: {}", rt.loaded_config_name))
                                    .small()
                                    .weak(),
                            );
                        }
                        if let Some((ref msg, is_err)) = self.status_msg {
                            ui.label(
                                RichText::new(msg).small().color(if is_err {
                                    Color32::from_rgb(235, 90, 90)
                                } else {
                                    Color32::from_rgb(110, 210, 140)
                                }),
                            );
                        }
                    });
                }

                ui.separator();

                // ---- Compact 3-Tab Switcher ----
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut self.tab, MaidTab::Main, "Main");
                    ui.selectable_value(&mut self.tab, MaidTab::Combat, "Combat & Specials");
                    ui.selectable_value(&mut self.tab, MaidTab::Maps, "Maps & Relog");
                });

                // ---- Tab Body ----
                let mut cfg = self.bot.maid.config();
                let mut dirty = false;

                ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| match self.tab {
                        MaidTab::Main => dirty |= self.show_main_tab(ui, &mut cfg),
                        MaidTab::Combat => dirty |= self.show_combat_tab(ui, &mut cfg),
                        MaidTab::Maps => dirty |= self.show_maps_tab(ui, &mut cfg),
                    });

                if dirty {
                    self.bot.maid.set_config(cfg);
                }
            },
        );
        self.open = open;
    }

    fn show_main_tab(&mut self, ui: &mut Ui, cfg: &mut MaidConfig) -> bool {
        let mut dirty = false;

        theme::card(ui, "Target & Following", |ui| {
            ui.horizontal(|ui| {
                ui.label("Target");
                let trailing_w = 56.0 + 26.0 + 32.0 + 18.0;
                let input_w = (ui.available_width() - trailing_w).max(80.0);
                if ui
                    .add_sized(
                        [input_w, 22.0],
                        TextEdit::singleline(&mut cfg.target).hint_text("Player username..."),
                    )
                    .changed()
                {
                    dirty = true;
                }

                let players = self
                    .players_cache
                    .lock()
                    .map(|g| g.clone())
                    .unwrap_or_default();
                let resp = theme::dropdown(ui, "maid_target_picker", "Map", 54.0, |ui| {
                    if players.is_empty() {
                        ui.label(RichText::new("Click ↻ to load").small().weak());
                    }
                    for p in &players {
                        if ui
                            .selectable_label(cfg.target.eq_ignore_ascii_case(p), p)
                            .clicked()
                        {
                            cfg.target = p.clone();
                            dirty = true;
                        }
                    }
                });
                if resp.response.clicked() {
                    self.refresh_players();
                }

                if ui
                    .add_sized([24.0, 22.0], egui::Button::new("↻"))
                    .on_hover_text("Refresh players in current map")
                    .clicked()
                {
                    self.refresh_players();
                }
                if ui
                    .add_sized([30.0, 22.0], egui::Button::new("Me"))
                    .on_hover_text("Set target to your own username")
                    .clicked()
                {
                    let me = self.bot.player_snapshot().username;
                    if !me.is_empty() {
                        cfg.target = me;
                        dirty = true;
                    }
                }
            });

            ui.add_space(2.0);
            ui.horizontal_wrapped(|ui| {
                dirty |= ui
                    .checkbox(&mut cfg.copy_walk, "Copy Walk")
                    .on_hover_text("Mirror target player's X/Y movement")
                    .changed();
                ui.add_space(10.0);
                dirty |= ui
                    .checkbox(&mut cfg.anti_counter, "Anti Counter")
                    .on_hover_text("Auto pause attack on counter attack")
                    .changed();
            });
        });

        ui.add_space(4.0);

        theme::card(ui, "Class Preset & Skills", |ui| {
            let field_w = (ui.available_width() - 78.0).max(150.0);
            egui::Grid::new("maid_skills_grid")
                .num_columns(2)
                .min_col_width(68.0)
                .spacing([8.0, 6.0])
                .show(ui, |ui| {
                    ui.label("Preset");
                    let mut selected_preset = cfg.class_preset;
                    theme::dropdown(
                        ui,
                        "maid_class_preset",
                        selected_preset.label(),
                        field_w,
                        |ui| {
                            for &p in MaidPreset::ALL {
                                if ui
                                    .selectable_value(&mut selected_preset, p, p.label())
                                    .changed()
                                {
                                    p.apply_to(cfg);
                                    dirty = true;
                                }
                            }
                        },
                    );
                    ui.end_row();

                    ui.label("Ultra Boss");
                    let mut ultra = cfg.ultra_boss_mode();
                    theme::dropdown(ui, "maid_ultra_boss", ultra.display_name(), field_w, |ui| {
                        for &mode in UltraBossMode::DISPLAY_ORDER {
                            if ui
                                .selectable_value(&mut ultra, mode, mode.display_name())
                                .changed()
                            {
                                cfg.set_ultra_boss_mode(mode);
                                dirty = true;
                            }
                        }
                    });
                    ui.end_row();

                    ui.label("Skills");
                    if ui
                        .add_sized(
                            [field_w, 22.0],
                            TextEdit::singleline(&mut cfg.skill_list).hint_text("1,2,3,4"),
                        )
                        .changed()
                    {
                        dirty = true;
                    }
                    ui.end_row();

                    ui.label("Delay");
                    ui.horizontal(|ui| {
                        dirty |= ui
                            .add_sized(
                                [76.0, 22.0],
                                egui::DragValue::new(&mut cfg.skill_delay)
                                    .range(80..=5000)
                                    .speed(10)
                                    .suffix(" ms"),
                            )
                            .changed();
                        ui.add_space(8.0);
                        dirty |= ui
                            .checkbox(&mut cfg.wait_skill, "Wait Skill")
                            .on_hover_text("Wait until each skill in the rotation is off cooldown")
                            .changed();
                    });
                    ui.end_row();
                });
        });

        dirty
    }

    fn show_combat_tab(&mut self, ui: &mut Ui, cfg: &mut MaidConfig) -> bool {
        let mut dirty = false;

        theme::card(ui, "Safe Skill (Auto Heal)", |ui| {
            ui.horizontal(|ui| {
                dirty |= ui.checkbox(&mut cfg.safe_skill, "Enable").changed();
                ui.label("HP <=");
                dirty |= ui
                    .add_sized(
                        [52.0, 20.0],
                        egui::DragValue::new(&mut cfg.safe_skill_hp)
                            .range(1..=100)
                            .suffix("%"),
                    )
                    .changed();
                ui.label("Skills");
                let w = ui.available_width().max(60.0);
                dirty |= ui
                    .add_sized(
                        [w, 20.0],
                        TextEdit::singleline(&mut cfg.safe_skill_list).hint_text("e.g. 2,3"),
                    )
                    .changed();
            });
        });

        ui.add_space(4.0);

        theme::card(ui, "Buff When Attack Stopped", |ui| {
            ui.horizontal(|ui| {
                dirty |= ui.checkbox(&mut cfg.buff_stop_attack, "Enable").changed();
                ui.label("Skills");
                let w = ui.available_width().max(80.0);
                dirty |= ui
                    .add_sized(
                        [w, 20.0],
                        TextEdit::singleline(&mut cfg.buff_stop_attack_list)
                            .hint_text("e.g. 1,2,3 or 5"),
                    )
                    .changed();
            });
        });

        ui.add_space(4.0);

        theme::card(ui, "Attack Priority", |ui| {
            ui.horizontal(|ui| {
                dirty |= ui.checkbox(&mut cfg.attack_priority, "Enable").changed();
                let w = ui.available_width().max(100.0);
                dirty |= ui
                    .add_sized(
                        [w, 20.0],
                        TextEdit::singleline(&mut cfg.attack_priority_monster)
                            .hint_text("Monster1, id.2, Monster2"),
                    )
                    .changed();
            });
        });

        ui.add_space(4.0);

        theme::card(ui, "Special Anims & Taunt Cycle", |ui| {
            ui.horizontal(|ui| {
                dirty |= ui.checkbox(&mut cfg.special_anims, "Enable").changed();
                ui.label("Force Skill #");
                dirty |= ui
                    .add_sized([48.0, 20.0], egui::DragValue::new(&mut cfg.special_act).range(0..=5))
                    .changed();
            });
            dirty |= ui
                .add_sized(
                    [ui.available_width(), 22.0],
                    TextEdit::singleline(&mut cfg.special_msg)
                        .hint_text("Anim msg CSV or tc;cycle;monster;sec;order"),
                )
                .changed();
            ui.label(
                RichText::new("Tip: `tc;2;Ultra Gramiel;0;1` for timed taunt cycles.")
                    .small()
                    .weak(),
            );
        });

        dirty
    }

    fn show_maps_tab(&mut self, ui: &mut Ui, cfg: &mut MaidConfig) -> bool {
        let mut dirty = false;

        // ---- Locked Map Handler (item list) ----
        theme::card(ui, "Locked Map Handler", |ui| {
            let mut items = lines_to_vec(&cfg.locked_zone_handler_maps);
            let mut changed = false;

            ui.horizontal(|ui| {
                dirty |= ui
                    .checkbox(&mut cfg.locked_zone_handler, "Enable")
                    .changed();
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("Clear").clicked() && !items.is_empty() {
                        items.clear();
                        changed = true;
                    }
                    if ui
                        .button("Grab")
                        .on_hover_text("Add current map;cell;pad")
                        .clicked()
                    {
                        let snap = self.bot.player_snapshot();
                        if !snap.map.is_empty() {
                            let entry = format!("{};{};{}", snap.map, snap.cell, snap.pad);
                            if !items.iter().any(|x| x.eq_ignore_ascii_case(&entry)) {
                                items.push(entry);
                                changed = true;
                            }
                        }
                    }
                });
            });

            ui.horizontal(|ui| {
                let add_w = 48.0;
                let input_w = (ui.available_width() - add_w - 6.0).max(100.0);
                let resp = ui.add_sized(
                    [input_w, 20.0],
                    TextEdit::singleline(&mut self.locked_map_draft)
                        .hint_text("whitemap-100000;Enter;Spawn"),
                );
                let enter = resp.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter));
                if (theme::primary_button(ui, "+ Add", [add_w, 20.0]).clicked() || enter)
                    && !self.locked_map_draft.trim().is_empty()
                {
                    let val = self.locked_map_draft.trim().to_string();
                    if !items.iter().any(|x| x.eq_ignore_ascii_case(&val)) {
                        items.push(val);
                        changed = true;
                    }
                    self.locked_map_draft.clear();
                }
            });

            let mut remove_idx: Option<usize> = None;
            theme::list_box(ui, Some(74.0), |ui| {
                ScrollArea::vertical()
                    .id_salt("maid_locked_maps_list")
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.spacing_mut().item_spacing.y = 1.0;
                        if items.is_empty() {
                            ui.label(RichText::new("No alternative maps.").small().weak());
                        }
                        for (idx, item) in items.iter().enumerate() {
                            theme::list_row(ui, idx, false, 0, |ui| {
                                ui.horizontal(|ui| {
                                    ui.label(
                                        RichText::new(item)
                                            .monospace()
                                            .size(theme::LIST_ITEM_FONT_SIZE),
                                    );
                                    ui.with_layout(
                                        egui::Layout::right_to_left(egui::Align::Center),
                                        |ui| {
                                            if ui
                                                .add_sized([18.0, 16.0], egui::Button::new("×"))
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
                items.remove(idx);
                changed = true;
            }
            if changed {
                cfg.locked_zone_handler_maps = items.join("\n");
                dirty = true;
            }
        });

        ui.add_space(4.0);

        // ---- Whitelist Map (item list) ----
        theme::card(ui, "Whitelist Map", |ui| {
            let mut items = lines_to_vec(&cfg.whitelist_map_maps);
            let mut changed = false;

            ui.horizontal(|ui| {
                dirty |= ui.checkbox(&mut cfg.whitelist_map, "Enable").changed();
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("Clear").clicked() && !items.is_empty() {
                        items.clear();
                        changed = true;
                    }
                    if ui
                        .button("Grab")
                        .on_hover_text("Add current map;cell")
                        .clicked()
                    {
                        let snap = self.bot.player_snapshot();
                        if !snap.map.is_empty() {
                            let entry = format!("{};{}", snap.map, snap.cell);
                            if !items.iter().any(|x| x.eq_ignore_ascii_case(&entry)) {
                                items.push(entry);
                                changed = true;
                            }
                        }
                    }
                });
            });

            ui.horizontal(|ui| {
                let add_w = 48.0;
                let input_w = (ui.available_width() - add_w - 6.0).max(100.0);
                let resp = ui.add_sized(
                    [input_w, 20.0],
                    TextEdit::singleline(&mut self.whitelist_map_draft)
                        .hint_text("map;cell  or  map;all"),
                );
                let enter = resp.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter));
                if (theme::primary_button(ui, "+ Add", [add_w, 20.0]).clicked() || enter)
                    && !self.whitelist_map_draft.trim().is_empty()
                {
                    let val = self.whitelist_map_draft.trim().to_string();
                    if !items.iter().any(|x| x.eq_ignore_ascii_case(&val)) {
                        items.push(val);
                        changed = true;
                    }
                    self.whitelist_map_draft.clear();
                }
            });

            let mut remove_idx: Option<usize> = None;
            theme::list_box(ui, Some(74.0), |ui| {
                ScrollArea::vertical()
                    .id_salt("maid_whitelist_maps_list")
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.spacing_mut().item_spacing.y = 1.0;
                        if items.is_empty() {
                            ui.label(RichText::new("No whitelisted maps.").small().weak());
                        }
                        for (idx, item) in items.iter().enumerate() {
                            theme::list_row(ui, idx, false, 0, |ui| {
                                ui.horizontal(|ui| {
                                    ui.label(
                                        RichText::new(item)
                                            .monospace()
                                            .size(theme::LIST_ITEM_FONT_SIZE),
                                    );
                                    ui.with_layout(
                                        egui::Layout::right_to_left(egui::Align::Center),
                                        |ui| {
                                            if ui
                                                .add_sized([18.0, 16.0], egui::Button::new("×"))
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
                items.remove(idx);
                changed = true;
            }
            if changed {
                cfg.whitelist_map_maps = items.join("\n");
                dirty = true;
            }
        });

        ui.add_space(4.0);

        theme::card(ui, "Relogin", |ui| {
            ui.horizontal(|ui| {
                ui.label("Relog Delay");
                dirty |= ui
                    .add_sized(
                        [82.0, 20.0],
                        egui::DragValue::new(&mut cfg.relog_delay)
                            .range(1000..=30000)
                            .speed(100)
                            .suffix(" ms"),
                    )
                    .changed();
            });
            ui.horizontal(|ui| {
                dirty |= ui
                    .checkbox(&mut cfg.stop_failed_goto, "Stop if Goto Fails")
                    .on_hover_text("Stop Maid after consecutive failed goto attempts")
                    .changed();
                ui.label("Max Fails");
                dirty |= ui
                    .add_sized(
                        [52.0, 20.0],
                        egui::DragValue::new(&mut cfg.stop_failed_goto_limit)
                            .range(1..=50)
                            .suffix("x"),
                    )
                    .changed();
            });
        });

        dirty
    }

    fn handle_load_config(&mut self) {
        let Some(ref picker) = self.pick_json else {
            self.status_msg = Some(("No file dialog available".into(), true));
            return;
        };
        if let Some(path) = picker() {
            match self.bot.maid.load_json_file(&path) {
                Ok(name) => {
                    self.bot.logs.push_debug(format!("[Maid] Loaded `{name}`"));
                    self.status_msg = Some((format!("Loaded {name}"), false));
                }
                Err(e) => {
                    self.bot.logs.push_debug(format!("[Maid] Load failed: {e}"));
                    self.status_msg = Some((e, true));
                }
            }
        }
    }

    fn handle_save_config(&mut self) {
        let Some(ref saver) = self.save_json else {
            self.status_msg = Some(("No file dialog available".into(), true));
            return;
        };
        if let Some(path) = saver() {
            match self.bot.maid.save_json_file(&path) {
                Ok(name) => {
                    self.bot.logs.push_debug(format!("[Maid] Saved `{name}`"));
                    self.status_msg = Some((format!("Saved {name}"), false));
                }
                Err(e) => {
                    self.bot.logs.push_debug(format!("[Maid] Save failed: {e}"));
                    self.status_msg = Some((e, true));
                }
            }
        }
    }
}

fn lines_to_vec(raw: &str) -> Vec<String> {
    raw.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(ToString::to_string)
        .collect()
}
