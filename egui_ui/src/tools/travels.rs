//! Fast Travel tool window (compact UX):
//! - **Presets**: User-managed custom travel presets (persisted to JSON, with Tercess examples)
//! - **Bypasser**: Story Quest Map Unlocker (`updateQuest` packet) + Bypass All Storylines (`Travel.cs`)
//! - **Awe Shop**: Direct Awe Enhancement Shop loader (`Lucky`, `Thief`, `Hybrid`, `Healer`, `Fighter`, `Wizard`)

use crate::show_detached_window_sized;
use crate::theme;
use egui::{Color32, Context, RichText, ScrollArea, TextEdit, Ui};
use grimlite_bot::Bot;
use grimlite_bot::host::FlashArg;
use serde_json::{Value, json};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TravelPreset {
    pub name: String,
    pub map: String,
    pub cell: String,
    pub pad: String,
}

impl TravelPreset {
    fn defaults() -> Vec<Self> {
        vec![
            Self {
                name: "Tercess (Nulgath)".into(),
                map: "tercessuinotlim".into(),
                cell: "Boss2".into(),
                pad: "Right".into(),
            },
            Self {
                name: "Tercess (Swindle)".into(),
                map: "tercessuinotlim".into(),
                cell: "Swindle".into(),
                pad: "Left".into(),
            },
        ]
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum TravelTab {
    Presets,
    Bypasser,
    AweShop,
}

impl TravelTab {
    const ALL: &[Self] = &[Self::Presets, Self::Bypasser, Self::AweShop];

    fn label(self) -> &'static str {
        match self {
            Self::Presets => "Presets",
            Self::Bypasser => "Map Bypasser",
            Self::AweShop => "Awe Shop",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum AweEnhancement {
    Lucky,
    Thief,
    Hybrid,
    Healer,
    Fighter,
    Wizard,
}

impl AweEnhancement {
    const ALL: &[Self] = &[
        Self::Lucky,
        Self::Thief,
        Self::Hybrid,
        Self::Healer,
        Self::Fighter,
        Self::Wizard,
    ];

    fn label(self) -> &'static str {
        match self {
            Self::Lucky => "Lucky",
            Self::Thief => "Thief",
            Self::Hybrid => "Hybrid",
            Self::Healer => "Healer",
            Self::Fighter => "Fighter",
            Self::Wizard => "Wizard",
        }
    }

    /// Exact AQW Awe Enhancement shop IDs from Grimlite-Li `Travel.cs`.
    fn shop_id(self) -> i32 {
        match self {
            Self::Hybrid => 633,
            Self::Fighter => 635,
            Self::Wizard => 636,
            Self::Thief => 637,
            Self::Healer => 638,
            Self::Lucky => 639,
        }
    }
}

pub struct TravelsWindow {
    bot: Arc<Bot>,
    pub open: bool,
    tab: TravelTab,

    // Custom presets
    presets: Vec<TravelPreset>,
    private_room: bool,
    private_num: i32,
    editor_open: bool,
    editing_idx: Option<usize>,
    draft_name: String,
    draft_map: String,
    draft_cell: String,
    draft_pad: String,

    // Map Bypasser (`Travel.cs`)
    story_quest_id: i32,
    generated_packet: Arc<Mutex<String>>,
    bypass_busy: Arc<AtomicBool>,

    // Awe Enhancement Shop (`Travel.cs`)
    awe_kind: AweEnhancement,

    // Shared status message (updated from UI or background worker)
    status: Arc<Mutex<String>>,
}

impl TravelsWindow {
    pub fn new(bot: Arc<Bot>) -> Self {
        let presets = load_presets().unwrap_or_else(TravelPreset::defaults);
        Self {
            bot,
            open: false,
            tab: TravelTab::Presets,
            presets,
            private_room: false,
            private_num: 100000,
            editor_open: false,
            editing_idx: None,
            draft_name: String::new(),
            draft_map: String::new(),
            draft_cell: "Enter".into(),
            draft_pad: "Spawn".into(),
            story_quest_id: 0,
            generated_packet: Arc::new(Mutex::new(String::new())),
            bypass_busy: Arc::new(AtomicBool::new(false)),
            awe_kind: AweEnhancement::Lucky,
            status: Arc::new(Mutex::new(String::new())),
        }
    }

    fn set_status(&self, msg: impl Into<String>) {
        if let Ok(mut s) = self.status.lock() {
            *s = msg.into();
        }
    }

    fn travel_to(&self, preset: &TravelPreset) {
        let map_raw = preset.map.trim().to_string();
        if map_raw.is_empty() {
            self.set_status("Preset map is empty");
            return;
        }
        let cell = if preset.cell.trim().is_empty() {
            "Enter".to_string()
        } else {
            preset.cell.trim().to_string()
        };
        let pad = if preset.pad.trim().is_empty() {
            "Spawn".to_string()
        } else {
            preset.pad.trim().to_string()
        };
        let target_map = if self.private_room && !map_raw.contains('-') {
            format!("{map_raw}-{}", self.private_num.clamp(1, 999_999))
        } else {
            map_raw.clone()
        };

        let base_map = map_raw.split('-').next().unwrap_or(&map_raw);
        let cur_map = self.bot.player_snapshot().map;
        let needs_tercess_jump = base_map.eq_ignore_ascii_case("tercessuinotlim")
            && !cur_map.eq_ignore_ascii_case("tercessuinotlim");

        self.set_status(format!("Traveling → {target_map} ({cell}, {pad})"));

        let bot = Arc::clone(&self.bot);
        let _ = std::thread::Builder::new()
            .name("grimlite-travel".into())
            .spawn(move || {
                let Some(flash) = bot.flash() else {
                    return;
                };
                // Mirror `Travel.cs::CreateJoinCommand`: Tercess requires moving to cell `m22, Left` first.
                if needs_tercess_jump {
                    let _ = flash.call_bool(
                        "moveToCell",
                        &[FlashArg::from("m22"), FlashArg::from("Left")],
                    );
                    std::thread::sleep(Duration::from_millis(250));
                }
                let _ = flash.call_bool(
                    "joinMap",
                    &[
                        FlashArg::from(target_map.as_str()),
                        FlashArg::from(cell.as_str()),
                        FlashArg::from(pad.as_str()),
                    ],
                );
            });
    }

    fn open_new_editor(&mut self) {
        self.editor_open = true;
        self.editing_idx = None;
        self.draft_name.clear();
        self.draft_map.clear();
        self.draft_cell = "Enter".into();
        self.draft_pad = "Spawn".into();
    }

    fn open_edit_editor(&mut self, idx: usize) {
        if let Some(p) = self.presets.get(idx) {
            self.editor_open = true;
            self.editing_idx = Some(idx);
            self.draft_name = p.name.clone();
            self.draft_map = p.map.clone();
            self.draft_cell = p.cell.clone();
            self.draft_pad = p.pad.clone();
        }
    }

    fn save_draft_preset(&mut self) {
        let map = self.draft_map.trim().to_string();
        if map.is_empty() {
            self.set_status("Map name is required");
            return;
        }
        let cell = {
            let c = self.draft_cell.trim();
            if c.is_empty() { "Enter" } else { c }.to_string()
        };
        let pad = {
            let p = self.draft_pad.trim();
            if p.is_empty() { "Spawn" } else { p }.to_string()
        };
        let name = {
            let n = self.draft_name.trim();
            if n.is_empty() {
                format!("{map} ({cell})")
            } else {
                n.to_string()
            }
        };
        let preset = TravelPreset {
            name: name.clone(),
            map,
            cell,
            pad,
        };
        if let Some(idx) = self.editing_idx
            && idx < self.presets.len()
        {
            self.presets[idx] = preset;
            self.set_status(format!("Updated preset: {name}"));
        } else {
            self.presets.push(preset);
            self.set_status(format!("Added preset: {name}"));
        }
        save_presets(&self.presets);
        self.editor_open = false;
        self.editing_idx = None;
    }

    fn trigger_unlock_map(&self) {
        let quest_id = self.story_quest_id;
        if quest_id <= 0 {
            self.set_status("Enter a valid Story Quest ID");
            return;
        }
        if self.bypass_busy.swap(true, Ordering::AcqRel) {
            return;
        }
        if let Ok(mut pkt) = self.generated_packet.lock() {
            pkt.clear();
        }
        self.set_status(format!("Loading quest {quest_id}..."));

        let bot = Arc::clone(&self.bot);
        let out_pkt = Arc::clone(&self.generated_packet);
        let busy = Arc::clone(&self.bypass_busy);
        let status = Arc::clone(&self.status);

        let _ = std::thread::Builder::new()
            .name("grimlite-map-unlock".into())
            .spawn(move || {
                let set_msg = |m: String| {
                    if let Ok(mut s) = status.lock() {
                        *s = m;
                    }
                };
                let Some(flash) = bot.flash() else {
                    set_msg("Game is not loaded".into());
                    busy.store(false, Ordering::Release);
                    return;
                };

                let path = format!("world.questTree.{quest_id}");
                let fetch_quest = || -> Option<Value> {
                    let raw = flash
                        .call_str("getGameObject", &[FlashArg::from(path.as_str())])
                        .ok()?;
                    let trimmed = raw.trim();
                    if trimmed.is_empty() || trimmed == "null" || trimmed == "undefined" {
                        return None;
                    }
                    let val: Value = serde_json::from_str(trimmed).ok()?;
                    val.is_object().then_some(val)
                };

                let mut quest_obj = fetch_quest();
                if quest_obj.is_none() {
                    let _ = flash.call_bool("loadQuest", &[FlashArg::from(quest_id)]);
                    for _ in 0..8 {
                        std::thread::sleep(Duration::from_millis(500));
                        quest_obj = fetch_quest();
                        if quest_obj.is_some() {
                            break;
                        }
                    }
                }

                if let Some(q) = quest_obj {
                    let i_value = json_i32(&q, "iValue");
                    let i_slot = json_i32(&q, "iSlot");
                    let packet = format!(
                        r#"{{"t":"xt","b":{{"r":-1,"o":{{"cmd":"updateQuest","iValue":{i_value},"iIndex":{i_slot}}}}}}}"#
                    );
                    let res = bot.capture.send_to_client(&packet);
                    if let Ok(mut guard) = out_pkt.lock() {
                        *guard = packet;
                    }
                    match res {
                        Ok(()) => set_msg(format!(
                            "Unlocked quest {quest_id} (slot {i_slot}, val {i_value})"
                        )),
                        Err(e) => set_msg(format!("Inject failed: {e}")),
                    }
                } else {
                    set_msg(format!("Quest {quest_id} not found in questTree"));
                }
                busy.store(false, Ordering::Release);
            });
    }

    fn trigger_bypass_all(&self) {
        if self.bypass_busy.swap(true, Ordering::AcqRel) {
            return;
        }
        self.set_status("Bypassing all storylines...");

        let bot = Arc::clone(&self.bot);
        let busy = Arc::clone(&self.bypass_busy);
        let status = Arc::clone(&self.status);

        let _ = std::thread::Builder::new()
            .name("grimlite-bypass-all".into())
            .spawn(move || {
                let set_msg = |m: String| {
                    if let Ok(mut s) = status.lock() {
                        *s = m;
                    }
                };
                if !bot.is_logged_in() {
                    set_msg("Not logged in".into());
                    busy.store(false, Ordering::Release);
                    return;
                }
                let Some(flash) = bot.flash() else {
                    set_msg("Game is not loaded".into());
                    busy.store(false, Ordering::Release);
                    return;
                };

                // Mirrors `Travel.cs::bypasses()`: fills strQuests..strQuests7 with 'Z'.
                let mut updated = 0usize;
                for i in 1..=7 {
                    let key = if i == 1 {
                        "strQuests".to_string()
                    } else {
                        format!("strQuests{i}")
                    };
                    let path = format!("world.myAvatar.objData.{key}");
                    if let Ok(raw) = flash.call_str("getGameObject", &[FlashArg::from(path.as_str())]) {
                        let cur = raw.trim().trim_matches('"');
                        if !cur.is_empty() && cur != "null" && cur != "undefined" {
                            let zs = "Z".repeat(cur.len());
                            if flash
                                .call_bool(
                                    "setGameObject",
                                    &[FlashArg::from(path.as_str()), FlashArg::from(zs)],
                                )
                                .unwrap_or(false)
                            {
                                updated += 1;
                            }
                        }
                    }
                }

                if updated > 0 {
                    set_msg(format!(
                        "Bypassed {updated} quest string(s) — relogin to restore"
                    ));
                } else {
                    set_msg("No storyline quest strings found".into());
                }
                busy.store(false, Ordering::Release);
            });
    }

    pub fn show(&mut self, ctx: &Context) {
        if !self.open {
            return;
        }
        let mut open = self.open;
        show_detached_window_sized(
            ctx,
            "grimlite_fast_travel",
            "Fast Travel",
            [335.0, 325.0],
            Some([300.0, 275.0]),
            &mut open,
            |ui| {
                ui.spacing_mut().item_spacing = egui::vec2(5.0, 4.0);
                ui.spacing_mut().button_padding = egui::vec2(6.0, 2.0);
                ui.spacing_mut().interact_size.y = 20.0;
                self.show_body(ui);
            },
        );
        self.open = open;
    }

    fn show_body(&mut self, ui: &mut Ui) {
        // 1. Top segmented tab bar
        egui::Panel::top("grimlite_travel_tabs")
            .frame(egui::Frame::NONE)
            .show_separator_line(false)
            .show(ui, |ui| {
                let count = TravelTab::ALL.len() as f32;
                let spacing = 4.0;
                let tab_w = ((ui.available_width() - spacing * (count - 1.0)) / count).max(60.0);
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = spacing;
                    for &t in TravelTab::ALL {
                        let active = self.tab == t;
                        let btn = egui::Button::new(
                            RichText::new(t.label())
                                .size(11.5)
                                .strong()
                                .color(if active {
                                    Color32::WHITE
                                } else {
                                    theme::Palette::TEXT_MUTED
                                }),
                        )
                        .fill(if active {
                            theme::Palette::ACCENT_SELECT
                        } else {
                            theme::Palette::BG_SURFACE
                        })
                        .stroke(egui::Stroke::new(
                            1.0,
                            if active {
                                theme::Palette::BORDER_FOCUS
                            } else {
                                theme::Palette::BORDER_SUBTLE
                            },
                        ))
                        .corner_radius(egui::CornerRadius::same(4));
                        if ui.add_sized([tab_w, 22.0], btn).clicked() {
                            self.tab = t;
                        }
                    }
                });
                ui.add_space(4.0);
            });

        // 2. Bottom compact status strip
        egui::Panel::bottom("grimlite_travel_status")
            .frame(egui::Frame::NONE)
            .show_separator_line(false)
            .show(ui, |ui| {
                let msg = self.status.lock().map(|s| s.clone()).unwrap_or_default();
                if !msg.is_empty() {
                    ui.add_space(3.0);
                    ui.label(
                        RichText::new(msg)
                            .size(10.5)
                            .color(theme::Palette::TEXT_MUTED),
                    );
                }
            });

        // 3. Active tab body
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE)
            .show(ui, |ui| match self.tab {
                TravelTab::Presets => self.show_presets_tab(ui),
                TravelTab::Bypasser => self.show_bypasser_tab(ui),
                TravelTab::AweShop => self.show_awe_tab(ui),
            });
    }

    fn show_presets_tab(&mut self, ui: &mut Ui) {
        // Controls + optional inline preset editor pinned to top of the Presets tab
        egui::Panel::top("grimlite_travel_presets_top")
            .frame(egui::Frame::NONE)
            .show_separator_line(false)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.checkbox(&mut self.private_room, "Private");
                    ui.add_enabled_ui(self.private_room, |ui| {
                        theme::num_input(ui, &mut self.private_num, 1..=999_999, 72.0);
                    });
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if self.editor_open {
                            if ui
                                .add_sized([58.0, 21.0], egui::Button::new("Close"))
                                .clicked()
                            {
                                self.editor_open = false;
                                self.editing_idx = None;
                            }
                        } else if theme::primary_button(ui, "+ New", [62.0, 21.0])
                            .on_hover_text("Create a new custom travel preset")
                            .clicked()
                        {
                            self.open_new_editor();
                        }
                    });
                });

                if self.editor_open {
                    ui.add_space(4.0);
                    let title = if self.editing_idx.is_some() {
                        "Edit Preset"
                    } else {
                        "New Preset"
                    };
                    theme::card(ui, title, |ui| {
                        // Row 1: Preset Name + Fill from Current location
                        ui.horizontal(|ui| {
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                if ui
                                    .add_sized([56.0, 20.0], egui::Button::new("Current"))
                                    .on_hover_text("Fill Map, Cell, and Pad from current location")
                                    .clicked()
                                {
                                    let snap = self.bot.player_snapshot();
                                    if !snap.map.is_empty() {
                                        self.draft_map = snap.map.clone();
                                        if self.draft_name.trim().is_empty() {
                                            self.draft_name = format!(
                                                "{} ({})",
                                                snap.map,
                                                if snap.cell.is_empty() {
                                                    "Enter"
                                                } else {
                                                    snap.cell.as_str()
                                                }
                                            );
                                        }
                                    }
                                    if !snap.cell.is_empty() {
                                        self.draft_cell = snap.cell;
                                    }
                                    if !snap.pad.is_empty() {
                                        self.draft_pad = snap.pad;
                                    }
                                }
                                ui.add(
                                    TextEdit::singleline(&mut self.draft_name)
                                        .hint_text("Preset name")
                                        .desired_width(f32::INFINITY),
                                );
                            });
                        });

                        // Row 2: Map | Cell | Pad
                        ui.horizontal(|ui| {
                            let avail = (ui.available_width() - 10.0).max(120.0);
                            let map_w = (avail * 0.46).floor();
                            let cell_w = (avail * 0.28).floor();
                            let pad_w = (avail - map_w - cell_w).max(40.0);
                            ui.add(
                                TextEdit::singleline(&mut self.draft_map)
                                    .hint_text("Map")
                                    .desired_width(map_w),
                            );
                            ui.add(
                                TextEdit::singleline(&mut self.draft_cell)
                                    .hint_text("Cell")
                                    .desired_width(cell_w),
                            );
                            ui.add(
                                TextEdit::singleline(&mut self.draft_pad)
                                    .hint_text("Pad")
                                    .desired_width(pad_w),
                            );
                        });

                        // Row 3: Save / Cancel
                        ui.horizontal(|ui| {
                            let save_lbl = if self.editing_idx.is_some() {
                                "Update"
                            } else {
                                "Save"
                            };
                            let half = ((ui.available_width() - 5.0) * 0.5).max(60.0);
                            if theme::primary_button(ui, save_lbl, [half, 21.0]).clicked() {
                                self.save_draft_preset();
                            }
                            if ui
                                .add_sized([half, 21.0], egui::Button::new("Cancel"))
                                .clicked()
                            {
                                self.editor_open = false;
                                self.editing_idx = None;
                            }
                        });
                    });
                }

                ui.add_space(4.0);
            });

        // Scrollable custom presets list filling exact remaining vertical space
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE)
            .show(ui, |ui| {
                let list_h = (ui.available_height() - 6.0).max(60.0);
                let mut travel_idx: Option<usize> = None;
                let mut edit_idx: Option<usize> = None;
                let mut delete_idx: Option<usize> = None;

                theme::list_box(ui, Some(list_h), |ui| {
                    ScrollArea::vertical()
                        .id_salt("grimlite_travel_presets_scroll")
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            ui.spacing_mut().item_spacing.y = 3.0;
                            if self.presets.is_empty() {
                                ui.label(
                                    RichText::new(
                                        "No presets yet — click + New above to add your own.",
                                    )
                                    .small()
                                    .weak(),
                                );
                                return;
                            }

                            for (idx, preset) in self.presets.iter().enumerate() {
                                let is_editing = self.editing_idx == Some(idx) && self.editor_open;
                                theme::list_row(ui, idx, is_editing, 0, |ui| {
                                    ui.horizontal(|ui| {
                                        ui.with_layout(
                                            egui::Layout::right_to_left(egui::Align::Center),
                                            |ui| {
                                                ui.spacing_mut().item_spacing.x = 3.0;
                                                if ui
                                                    .add_sized(
                                                        [20.0, 20.0],
                                                        egui::Button::new(
                                                            RichText::new("×")
                                                                .size(12.0)
                                                                .color(theme::Palette::DANGER),
                                                        ),
                                                    )
                                                    .on_hover_text("Delete preset")
                                                    .clicked()
                                                {
                                                    delete_idx = Some(idx);
                                                }
                                                if ui
                                                    .add_sized(
                                                        [32.0, 20.0],
                                                        egui::Button::new(
                                                            RichText::new("Edit").size(10.5),
                                                        ),
                                                    )
                                                    .on_hover_text("Edit preset")
                                                    .clicked()
                                                {
                                                    edit_idx = Some(idx);
                                                }
                                                if theme::primary_button(ui, "Go", [30.0, 20.0])
                                                    .on_hover_text(format!(
                                                        "Join {} ({}, {})",
                                                        preset.map, preset.cell, preset.pad
                                                    ))
                                                    .clicked()
                                                {
                                                    travel_idx = Some(idx);
                                                }

                                                // Preset name + location summary on the left
                                                ui.with_layout(
                                                    egui::Layout::top_down(egui::Align::LEFT),
                                                    |ui| {
                                                        ui.spacing_mut().item_spacing.y = 0.0;
                                                        let name_resp = ui
                                                            .add(
                                                                egui::Label::new(
                                                                    RichText::new(&preset.name)
                                                                        .size(11.5)
                                                                        .strong()
                                                                        .color(Color32::WHITE),
                                                                )
                                                                .truncate()
                                                                .selectable(false)
                                                                .sense(egui::Sense::click()),
                                                            )
                                                            .on_hover_cursor(
                                                                egui::CursorIcon::PointingHand,
                                                            );
                                                        let sub = format!(
                                                            "{} · {} / {}",
                                                            preset.map, preset.cell, preset.pad
                                                        );
                                                        let sub_resp = ui
                                                            .add(
                                                                egui::Label::new(
                                                                    RichText::new(sub)
                                                                        .size(10.0)
                                                                        .color(
                                                                            theme::Palette::TEXT_MUTED,
                                                                        ),
                                                                )
                                                                .truncate()
                                                                .selectable(false)
                                                                .sense(egui::Sense::click()),
                                                            )
                                                            .on_hover_cursor(
                                                                egui::CursorIcon::PointingHand,
                                                            );
                                                        if name_resp.clicked() || sub_resp.clicked()
                                                        {
                                                            travel_idx = Some(idx);
                                                        }
                                                    },
                                                );
                                            },
                                        );
                                    });
                                });
                            }
                        });
                });

                if let Some(idx) = travel_idx
                    && let Some(p) = self.presets.get(idx).cloned()
                {
                    self.travel_to(&p);
                }
                if let Some(idx) = edit_idx {
                    self.open_edit_editor(idx);
                }
                if let Some(idx) = delete_idx
                    && idx < self.presets.len()
                {
                    let removed = self.presets.remove(idx);
                    if self.editing_idx == Some(idx) {
                        self.editor_open = false;
                        self.editing_idx = None;
                    } else if let Some(ed) = self.editing_idx
                        && ed > idx
                    {
                        self.editing_idx = Some(ed - 1);
                    }
                    save_presets(&self.presets);
                    self.set_status(format!("Deleted preset: {}", removed.name));
                }
            });
    }

    fn show_bypasser_tab(&mut self, ui: &mut Ui) {
        let is_busy = self.bypass_busy.load(Ordering::Acquire);

        // Card 1: Unlock Map by Story Quest ID (mirrors `Travel.cs::btnUnlockMap_Click`)
        theme::card(ui, "Unlock Map (Story Quest ID)", |ui| {
            ui.label(
                RichText::new("Story Quest ID required to unlock a locked map.")
                    .small()
                    .weak(),
            );
            ui.add_space(2.0);
            ui.horizontal(|ui| {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .add_enabled_ui(!is_busy, |ui| {
                            theme::primary_button(
                                ui,
                                if is_busy { "Unlocking..." } else { "Unlock" },
                                [82.0, 21.0],
                            )
                        })
                        .inner
                        .clicked()
                    {
                        self.trigger_unlock_map();
                    }
                    let num_w = ui.available_width().max(80.0);
                    theme::num_input(ui, &mut self.story_quest_id, 0..=1_000_000, num_w);
                });
            });

            ui.add_space(4.0);
            ui.label(RichText::new("Generated packet").small().weak());
            let mut pkt_view = self
                .generated_packet
                .lock()
                .map(|s| s.clone())
                .unwrap_or_default();
            ui.horizontal(|ui| {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let has_pkt = !pkt_view.is_empty();
                    if ui
                        .add_enabled(has_pkt, egui::Button::new("Copy").min_size(egui::vec2(52.0, 21.0)))
                        .clicked()
                    {
                        ui.ctx().copy_text(pkt_view.clone());
                        self.set_status("Copied bypass packet");
                    }
                    ui.add(
                        TextEdit::singleline(&mut pkt_view)
                            .interactive(false)
                            .hint_text("Packet appears here after Unlock")
                            .desired_width(f32::INFINITY),
                    );
                });
            });
        });

        ui.add_space(6.0);

        // Card 2: Bypass All Storylines (mirrors `Travel.cs::bypasses()`)
        theme::card(ui, "Bypass All Storylines", |ui| {
            ui.label(
                RichText::new(
                    "Sets strQuests..strQuests7 to 'Z' to bypass storyline locks. Relogin to restore original progression.",
                )
                .small()
                .weak(),
            );
            ui.add_space(4.0);
            if ui
                .add_enabled_ui(!is_busy, |ui| {
                    theme::primary_button(
                        ui,
                        if is_busy { "Working..." } else { "Bypass All Storylines" },
                        [ui.available_width(), 24.0],
                    )
                })
                .inner
                .clicked()
            {
                self.trigger_bypass_all();
            }
        });
    }

    fn show_awe_tab(&mut self, ui: &mut Ui) {
        theme::card(ui, "Awe Enhancement Shop", |ui| {
            ui.label(
                RichText::new("Select an Awe enhancement type to load its shop directly.")
                    .small()
                    .weak(),
            );
            ui.add_space(4.0);

            let col_w = ((ui.available_width() - 6.0) * 0.5).max(80.0);
            egui::Grid::new("grimlite_awe_enh_grid")
                .num_columns(2)
                .spacing([6.0, 5.0])
                .show(ui, |ui| {
                    for (i, &enh) in AweEnhancement::ALL.iter().enumerate() {
                        let selected = self.awe_kind == enh;
                        let btn = egui::Button::new(
                            RichText::new(format!("{} (#{})", enh.label(), enh.shop_id()))
                                .size(11.5)
                                .color(if selected {
                                    Color32::WHITE
                                } else {
                                    theme::Palette::TEXT_PRIMARY
                                }),
                        )
                        .fill(if selected {
                            theme::Palette::ACCENT_SELECT
                        } else {
                            theme::Palette::BG_INPUT
                        })
                        .stroke(egui::Stroke::new(
                            1.0,
                            if selected {
                                theme::Palette::BORDER_FOCUS
                            } else {
                                theme::Palette::BORDER_SUBTLE
                            },
                        ))
                        .corner_radius(egui::CornerRadius::same(4));
                        if ui.add_sized([col_w, 24.0], btn).clicked() {
                            self.awe_kind = enh;
                        }
                        if i % 2 == 1 {
                            ui.end_row();
                        }
                    }
                });

            ui.add_space(6.0);
            let shop_id = self.awe_kind.shop_id();
            let lbl = format!("Open {} Awe Shop ({shop_id})", self.awe_kind.label());
            if theme::primary_button(ui, &lbl, [ui.available_width(), 24.0]).clicked() {
                self.bot
                    .spawn_flash_action("loadShop", vec![FlashArg::from(shop_id)]);
                self.set_status(format!(
                    "Opening Awe {} Shop ({shop_id})",
                    self.awe_kind.label()
                ));
            }
        });
    }
}

fn json_i32(v: &Value, key: &str) -> i32 {
    v.get(key)
        .and_then(|x| {
            x.as_i64()
                .map(|n| n as i32)
                .or_else(|| x.as_str().and_then(|s| s.trim().parse::<i32>().ok()))
        })
        .unwrap_or(0)
}

fn presets_file_path() -> PathBuf {
    grimlite_bot::options::app_data_dir().join("travel_presets.json")
}

fn serialize_presets(presets: &[TravelPreset]) -> String {
    let arr: Vec<Value> = presets
        .iter()
        .map(|p| {
            json!({
                "name": p.name,
                "map": p.map,
                "cell": p.cell,
                "pad": p.pad,
            })
        })
        .collect();
    serde_json::to_string_pretty(&arr).unwrap_or_else(|_| "[]".into())
}

fn parse_presets_json(raw: &str) -> Option<Vec<TravelPreset>> {
    let arr: Vec<Value> = serde_json::from_str(raw).ok()?;
    let mut out = Vec::with_capacity(arr.len());
    for item in arr {
        let name = item.get("name").and_then(Value::as_str)?.trim().to_string();
        let map = item.get("map").and_then(Value::as_str)?.trim().to_string();
        let cell = item
            .get("cell")
            .and_then(Value::as_str)
            .unwrap_or("Enter")
            .trim()
            .to_string();
        let pad = item
            .get("pad")
            .and_then(Value::as_str)
            .unwrap_or("Spawn")
            .trim()
            .to_string();
        if !map.is_empty() {
            out.push(TravelPreset {
                name: if name.is_empty() { map.clone() } else { name },
                map,
                cell: if cell.is_empty() { "Enter".into() } else { cell },
                pad: if pad.is_empty() { "Spawn".into() } else { pad },
            });
        }
    }
    Some(out)
}

fn load_presets() -> Option<Vec<TravelPreset>> {
    let path = presets_file_path();
    let raw = std::fs::read_to_string(path).ok()?;
    parse_presets_json(&raw)
}

fn save_presets(presets: &[TravelPreset]) {
    let path = presets_file_path();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::write(path, serialize_presets(presets));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_presets_are_tercess_nulgath_and_swindle() {
        let defaults = TravelPreset::defaults();
        assert_eq!(defaults.len(), 2);
        assert_eq!(defaults[0].name, "Tercess (Nulgath)");
        assert_eq!(defaults[0].map, "tercessuinotlim");
        assert_eq!(defaults[0].cell, "Boss2");
        assert_eq!(defaults[0].pad, "Right");
        assert_eq!(defaults[1].name, "Tercess (Swindle)");
        assert_eq!(defaults[1].map, "tercessuinotlim");
        assert_eq!(defaults[1].cell, "Swindle");
        assert_eq!(defaults[1].pad, "Left");
    }

    #[test]
    fn presets_json_roundtrip() {
        let presets = TravelPreset::defaults();
        let json = serialize_presets(&presets);
        let parsed = parse_presets_json(&json).expect("parse presets json");
        assert_eq!(parsed, presets);
    }

    #[test]
    fn awe_enhancement_shop_ids_match_travel_cs() {
        assert_eq!(AweEnhancement::Hybrid.shop_id(), 633);
        assert_eq!(AweEnhancement::Fighter.shop_id(), 635);
        assert_eq!(AweEnhancement::Wizard.shop_id(), 636);
        assert_eq!(AweEnhancement::Thief.shop_id(), 637);
        assert_eq!(AweEnhancement::Healer.shop_id(), 638);
        assert_eq!(AweEnhancement::Lucky.shop_id(), 639);
    }
}
