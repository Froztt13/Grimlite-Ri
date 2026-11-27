//! Grimlite-Li inspired Bot Manager Editor (split per tab under `bot_managers/`):
//! - Left column: persistent full-width list explorer (`explorer.rs`)
//! - Right column: tabbed command & config builders (`combat`, `skills`, `map`, `item`,
//!   `quest`, `misc`, `misc2`, `options`, `bots`, `info`).

mod bots;
mod combat;
mod explorer;
mod info;
mod item;
mod map;
mod misc;
mod misc2;
mod options;
mod quest;
mod skills;

use crate::command_editor::CommandEditorWindow;
use crate::show_detached_window_sized;
use crate::theme;
use egui::{Context, ScrollArea, Ui};
use grimlite_bot::Bot;
use grimlite_bot::gbot::{BotCommand, QuestEntry, SkillEntry, StatementKind};
use grimlite_bot::handlers::SpecialZone;
use std::collections::BTreeSet;
use std::path::PathBuf;
use std::sync::Arc;


pub type FileDialogFn = Box<dyn Fn() -> Option<PathBuf> + Send + Sync>;

#[derive(Clone, Copy, PartialEq, Eq)]
enum ListKind {
    Commands,
    Skills,
    Quests,
    Drops,
}

impl ListKind {
    const ALL: &[Self] = &[Self::Commands, Self::Skills, Self::Quests, Self::Drops];

    fn label(self) -> &'static str {
        match self {
            Self::Commands => "Commands",
            Self::Skills => "Skills",
            Self::Quests => "Quests",
            Self::Drops => "Drops",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum RightTab {
    Combat,
    Skills,
    Map,
    Item,
    Quest,
    Misc,
    Misc2,
    Options,
    Bots,
    Info,
}

impl RightTab {
    const ALL: &[Self] = &[
        Self::Combat,
        Self::Skills,
        Self::Map,
        Self::Item,
        Self::Quest,
        Self::Misc,
        Self::Misc2,
        Self::Options,
        Self::Bots,
        Self::Info,
    ];

    fn label(self) -> &'static str {
        match self {
            Self::Combat => "Combat",
            Self::Skills => "Skills",
            Self::Map => "Map",
            Self::Item => "Item",
            Self::Quest => "Quest",
            Self::Misc => "Misc",
            Self::Misc2 => "Misc 2",
            Self::Options => "Options",
            Self::Bots => "Bots",
            Self::Info => "Info",
        }
    }
}

#[derive(Clone, Default)]
enum ClipboardBuffer {
    #[default]
    Empty,
    Commands(Vec<BotCommand>),
    Skills(Vec<SkillEntry>),
    Quests(Vec<QuestEntry>),
    Drops(Vec<String>),
}

pub struct BotManagerWindow {
    bot: Arc<Bot>,
    pick_gbot: Option<FileDialogFn>,
    save_gbot: Option<FileDialogFn>,

    // Left list explorer state (multi-select + copy/paste + reactive search)
    list_kind: ListKind,
    selected: BTreeSet<usize>,
    anchor: Option<usize>,
    last_scrolled_idx: Option<usize>,
    clipboard: ClipboardBuffer,
    cmd_held: bool,
    search_text: String,
    clear_all_lists: bool,
    command_editor: CommandEditorWindow,

    // Right tab state
    tab: RightTab,
    status: String,

    // Combat inputs
    combat_monster: String,
    kf_is_temp: bool,
    kf_add_whitelist: bool,
    kf_monster: String,
    kf_item: String,
    kf_qty: String,

    // Skills tab inputs
    skill_idx: i32,
    safe_is_mp: bool,
    safe_lower: bool,
    safe_threshold: i32,
    skill_cmd_monster: String,
    skill_cmd_idx: i32,
    skill_cmd_wait: bool,
    skill_cmd_force: bool,
    skill_set_name: String,

    // Map inputs
    join_map: String,
    join_cell: String,
    join_pad: String,
    jump_cell: String,
    jump_pad: String,
    walk_x: i32,
    walk_y: i32,
    map_item_id: String,

    // Item inputs
    item_name: String,
    shop_id: String,
    buy_qty: String,

    // Quest inputs
    quest_id: String,
    quest_item_id: String,
    quest_complete_qty: i32,
    auto_quest_id: String,
    auto_quest_item_id: String,

    // Misc inputs
    delay_ms: String,
    beep_times: i32,
    misc_level: String,
    misc_fps: i32,
    goto_player_name: String,
    label_name: String,
    packet_text: String,
    packet_to_client: bool,
    log_text: String,

    // Misc 2 (Statement, Messages, Load Map, Temp Integer, Index, Special Handlers) inputs
    stmt_category: &'static str,
    stmt_kind: StatementKind,
    stmt_v1: String,
    stmt_v2: String,
    client_msg_text: String,
    load_map_swf: String,
    temp_int_name: String,
    temp_int_val: i32,
    index_val: i32,
    special_zone: SpecialZone,

    // Options tab live toggle ("Enable options without starting")
    enable_options_live: bool,
}

impl BotManagerWindow {
    pub fn new(
        bot: Arc<Bot>,
        pick_gbot: Option<FileDialogFn>,
        save_gbot: Option<FileDialogFn>,
    ) -> Self {
        Self {
            bot,
            pick_gbot,
            save_gbot,
            list_kind: ListKind::Commands,
            selected: BTreeSet::new(),
            anchor: None,
            last_scrolled_idx: None,
            clipboard: ClipboardBuffer::Empty,
            cmd_held: false,
            search_text: String::new(),
            clear_all_lists: false,
            command_editor: CommandEditorWindow::default(),
            tab: RightTab::Combat,
            status: String::new(),
            combat_monster: "*".into(),
            kf_is_temp: false,
            kf_add_whitelist: true,
            kf_monster: "*".into(),
            kf_item: String::new(),
            kf_qty: "1".into(),
            skill_idx: 1,
            safe_is_mp: false,
            safe_lower: true,
            safe_threshold: 60,
            skill_cmd_monster: "*".into(),
            skill_cmd_idx: 1,
            skill_cmd_wait: false,
            skill_cmd_force: false,
            skill_set_name: String::new(),
            join_map: "battleon-99999".into(),
            join_cell: "Enter".into(),
            join_pad: "Spawn".into(),
            jump_cell: "Enter".into(),
            jump_pad: "Spawn".into(),
            walk_x: 480,
            walk_y: 350,
            map_item_id: String::new(),
            item_name: String::new(),
            shop_id: String::new(),
            buy_qty: "1".into(),
            quest_id: String::new(),
            quest_item_id: "-1".into(),
            quest_complete_qty: 1,
            auto_quest_id: String::new(),
            auto_quest_item_id: "-1".into(),
            delay_ms: "1000".into(),
            beep_times: 10,
            misc_level: String::new(),
            misc_fps: 30,
            goto_player_name: String::new(),
            label_name: String::new(),
            packet_text: String::new(),
            packet_to_client: false,
            log_text: String::new(),
            stmt_category: "Item",
            stmt_kind: StatementKind::InInventory,
            stmt_v1: String::new(),
            stmt_v2: "1".into(),
            client_msg_text: String::new(),
            load_map_swf: String::new(),
            temp_int_name: String::new(),
            temp_int_val: 0,
            index_val: 0,
            special_zone: SpecialZone::UltraDage,
            enable_options_live: false,
        }
    }

    pub fn show(&mut self, ctx: &Context, open: &mut bool) {
        show_detached_window_sized(
            ctx,
            "grimlite_bot_manager",
            "Bot",
            [790.0, 385.0],
            Some([740.0, 360.0]),
            open,
            |ui| {
                // Compact density across the entire Bot Manager
                ui.spacing_mut().item_spacing = egui::vec2(5.0, 4.0);
                ui.spacing_mut().button_padding = egui::vec2(6.0, 2.0);
                ui.spacing_mut().interact_size.y = 20.0;

                let mut cfg = self.bot.runner.config();
                let mut dirty = false;
                self.cmd_held = ui.input(|i| i.modifiers.command);

                // Keyboard shortcuts for the list (Cmd/Ctrl+Up/Down, Cmd/Ctrl+C/V, Delete)
                dirty |= self.handle_list_shortcuts(ui, &mut cfg);

                // Left column: List Explorer (Commands / Skills / Quests / Drops)
                egui::Panel::left("bot_mgr_left_list")
                    .default_size(255.0)
                    .min_size(220.0)
                    .max_size(350.0)
                    .show(ui, |ui| {
                        ui.spacing_mut().item_spacing = egui::vec2(4.0, 3.0);
                        ui.spacing_mut().button_padding = egui::vec2(5.0, 2.0);
                        dirty |= self.show_left_explorer(ui, &mut cfg);
                    });

                // Right column: Tab bar + active tab content
                egui::CentralPanel::default().show(ui, |ui| {
                    ui.spacing_mut().item_spacing = egui::vec2(5.0, 4.0);
                    ui.spacing_mut().button_padding = egui::vec2(6.0, 2.0);
                    ui.horizontal_wrapped(|ui| {
                        ui.spacing_mut().item_spacing.x = 2.0;
                        for &t in RightTab::ALL {
                            ui.selectable_value(&mut self.tab, t, t.label());
                        }
                    });
                    ui.separator();

                    ScrollArea::vertical()
                        .id_salt("bot_mgr_right_scroll")
                        .auto_shrink([false, false])
                        .show(ui, |ui| match self.tab {
                            RightTab::Combat => dirty |= self.show_combat_tab(ui, &mut cfg),
                            RightTab::Skills => dirty |= self.show_skills_tab(ui, &mut cfg),
                            RightTab::Map => dirty |= self.show_map_tab(ui, &mut cfg),
                            RightTab::Item => dirty |= self.show_item_tab(ui, &mut cfg),
                            RightTab::Quest => dirty |= self.show_quest_tab(ui, &mut cfg),
                            RightTab::Misc => dirty |= self.show_misc_tab(ui, &mut cfg),
                            RightTab::Misc2 => dirty |= self.show_misc2_tab(ui, &mut cfg),
                            RightTab::Options => dirty |= self.show_options_tab(ui, &mut cfg),
                            RightTab::Bots => self.show_bots_tab(ui, &mut cfg, &mut dirty),
                            RightTab::Info => self.show_info_tab(ui, &cfg),
                        });
                });

                if dirty {
                    self.bot.runner.update_config(cfg);
                }
            },
        );

        // Render detached Command Editor window (opened by double-clicking a list item)
        let mut cfg = self.bot.runner.config();
        if self.command_editor.show(ctx, &mut cfg) {
            self.bot.runner.update_config(cfg);
        }
        if !*open {
            self.command_editor.close();
        }
    }

}

pub(super) fn two_columns(ui: &mut Ui, add_contents: impl FnOnce(&mut [Ui])) {
    ui.scope(|ui| {
        ui.spacing_mut().item_spacing.x = 10.0;
        ui.columns(2, |cols| {
            for col in cols.iter_mut() {
                col.spacing_mut().item_spacing.x = 5.0;
            }
            add_contents(cols);
        });
    });
}

pub(super) fn group_box(ui: &mut Ui, title: &str, add_contents: impl FnOnce(&mut Ui)) {
    theme::card(ui, title, add_contents);
}

pub(super) fn non_empty_or(s: &str, fallback: &str) -> String {
    let t = s.trim();
    if t.is_empty() {
        fallback.to_string()
    } else {
        t.to_string()
    }
}
