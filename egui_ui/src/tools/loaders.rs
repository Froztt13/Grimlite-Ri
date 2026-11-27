//! Compact Loader / Grabber tool window (mirrors Grimlite-Li `Loaders.cs` + `Grabber.cs`):
//! - Loader section: Hair shop / Shop / Quest (+ batch count, Force Accept, Ghost) / Armor customizer
//! - Grabbed tree view with search filter and full right-click context menus
//! - Grabber controls: Shop items, Quest IDs, Quest items/drop rates, Inventory, Temp, Bank, Monsters, All Monsters

use crate::show_detached_window_sized;
use crate::theme;
use egui::{Color32, Context, OpenUrl, RichText, ScrollArea, TextEdit, Ui};
use grimlite_bot::Bot;
use grimlite_bot::gbot::{BotCommand, QuestEntry};
use grimlite_bot::grabber::{
    GrabItemMeta, GrabKind, GrabNode, GrabNodeAction, GrabOrderBy, fetch_grabbed,
};
use grimlite_bot::host::FlashArg;
use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

#[derive(Clone, Copy, PartialEq, Eq)]
enum LoadKind {
    HairShop,
    Shop,
    Quest,
    ArmorCustomizer,
}

impl LoadKind {
    const ALL: &[Self] = &[
        Self::HairShop,
        Self::Shop,
        Self::Quest,
        Self::ArmorCustomizer,
    ];

    fn label(self) -> &'static str {
        match self {
            Self::HairShop => "Hair shop",
            Self::Shop => "Shop",
            Self::Quest => "Quest",
            Self::ArmorCustomizer => "Armor customizer",
        }
    }
}

pub struct LoadersWindow {
    bot: Arc<Bot>,
    pub open: bool,

    // Loader state
    load_input: String,
    load_kind: LoadKind,
    quest_count: i32,
    ghost_accept: bool,

    // Grabber state
    grab_kind: GrabKind,
    order_by: GrabOrderBy,
    search_text: String,
    grabbed: Arc<Mutex<Vec<GrabNode>>>,
    grabbing: Arc<AtomicBool>,
    selected_key: Option<String>,
    expanded_keys: HashSet<String>,
    status: String,
}

impl LoadersWindow {
    pub fn new(bot: Arc<Bot>) -> Self {
        Self {
            bot,
            open: false,
            load_input: String::new(),
            load_kind: LoadKind::Quest,
            quest_count: 1,
            ghost_accept: false,
            grab_kind: GrabKind::QuestIds,
            order_by: GrabOrderBy::Id,
            search_text: String::new(),
            grabbed: Arc::new(Mutex::new(Vec::new())),
            grabbing: Arc::new(AtomicBool::new(false)),
            selected_key: None,
            expanded_keys: HashSet::new(),
            status: String::new(),
        }
    }

    pub fn show(&mut self, ctx: &Context) {
        if !self.open {
            return;
        }
        let mut open = self.open;
        show_detached_window_sized(
            ctx,
            "grimlite_loaders",
            "Loader / Grabber",
            [330.0, 420.0],
            Some([300.0, 350.0]),
            &mut open,
            |ui| {
                ui.spacing_mut().item_spacing = egui::vec2(5.0, 4.0);
                ui.spacing_mut().button_padding = egui::vec2(5.0, 2.0);
                ui.spacing_mut().interact_size.y = 20.0;
                self.show_body(ui);
            },
        );
        self.open = open;
    }

    fn show_body(&mut self, ui: &mut Ui) {
        let is_quest = self.load_kind == LoadKind::Quest;

        // 1. Loader card
        theme::card(ui, "Loader", |ui| {
            let right_w = 72.0;

            // Row 1: ID input + quest count
            ui.horizontal(|ui| {
                let input_w = (ui.available_width() - right_w - 5.0).max(100.0);
                let hint = match self.load_kind {
                    LoadKind::Quest => "Quest ID (or 1,2,3)",
                    LoadKind::Shop => "Shop ID",
                    LoadKind::HairShop => "Hair shop ID",
                    LoadKind::ArmorCustomizer => "No ID needed",
                };
                ui.add_enabled(
                    self.load_kind != LoadKind::ArmorCustomizer,
                    TextEdit::singleline(&mut self.load_input)
                        .hint_text(hint)
                        .desired_width(input_w),
                );
                ui.add_enabled_ui(is_quest, |ui| {
                    theme::num_input(ui, &mut self.quest_count, 1..=30, right_w);
                });
            });

            // Row 2: Load type dropdown + F Accept
            ui.horizontal(|ui| {
                let cb_w = (ui.available_width() - right_w - 5.0).max(100.0);
                theme::dropdown(ui, "loader_kind_cb", self.load_kind.label(), cb_w, |ui| {
                    for &k in LoadKind::ALL {
                        ui.selectable_value(&mut self.load_kind, k, k.label());
                    }
                });
                ui.add_enabled_ui(is_quest, |ui| {
                    if ui
                        .add_sized([right_w, 21.0], egui::Button::new("Accept"))
                        .on_hover_text("Load and force-accept quest batch")
                        .clicked()
                    {
                        self.trigger_force_accept();
                    }
                });
            });

            // Row 3: Load button + Ghost checkbox
            ui.horizontal(|ui| {
                let btn_w = (ui.available_width() - right_w - 5.0).max(100.0);
                if theme::primary_button(ui, "Load", [btn_w, 21.0]).clicked() {
                    self.trigger_load();
                }
                ui.add_enabled_ui(is_quest, |ui| {
                    ui.checkbox(&mut self.ghost_accept, "Ghost");
                });
            });
        });

        ui.add_space(4.0);

        // 2. Grabbed Tree + Search + Grab controls
        let bottom_h = 84.0;
        let tree_h = (ui.available_height() - bottom_h).max(110.0);

        theme::list_box(ui, Some(tree_h), |ui| {
            let nodes = self.grabbed.lock().map(|g| g.clone()).unwrap_or_default();
            let query = self.search_text.trim().to_ascii_lowercase();
            ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.spacing_mut().item_spacing.y = 2.0;
                    if self.grabbing.load(Ordering::Acquire) {
                        ui.label(RichText::new("Grabbing from game...").small().weak());
                    } else if nodes.is_empty() {
                        ui.label(
                            RichText::new("Empty — select a type below and click Grab.\nRight-click any item for actions.")
                                .small()
                                .weak(),
                        );
                    } else {
                        let mut row_idx = 0usize;
                        for (i, node) in nodes.iter().enumerate() {
                            if !query.is_empty() && !node_matches(node, &query) {
                                continue;
                            }
                            let path = i.to_string();
                            self.render_node(ui, node, &path, 0, &mut row_idx, &query);
                        }
                        if row_idx == 0 {
                            ui.label(RichText::new("No matching entries.").small().weak());
                        }
                    }
                });
        });

        ui.add_space(4.0);

        // Search input
        ui.horizontal(|ui| {
            let clear_w = 46.0;
            let w = (ui.available_width() - clear_w - 5.0).max(100.0);
            ui.add(
                TextEdit::singleline(&mut self.search_text)
                    .hint_text("Filter grabbed items...")
                    .desired_width(w),
            );
            if ui
                .add_sized([clear_w, 21.0], egui::Button::new("Clear"))
                .clicked()
            {
                self.search_text.clear();
            }
        });

        // Grab type + Order By
        ui.horizontal(|ui| {
            let order_w = 68.0;
            let cb_w = (ui.available_width() - order_w - 5.0).max(100.0);
            theme::dropdown(ui, "grabber_kind_cb", self.grab_kind.label(), cb_w, |ui| {
                for &k in GrabKind::ALL {
                    ui.selectable_value(&mut self.grab_kind, k, k.label());
                }
            });
            ui.add_enabled_ui(self.grab_kind.supports_order_by(), |ui| {
                theme::dropdown(
                    ui,
                    "grabber_order_cb",
                    self.order_by.label(),
                    order_w,
                    |ui| {
                        for &o in GrabOrderBy::ALL {
                            ui.selectable_value(&mut self.order_by, o, o.label());
                        }
                    },
                );
            });
        });

        // Grab button + status
        ui.horizontal(|ui| {
            let is_busy = self.grabbing.load(Ordering::Acquire);
            if ui
                .add_enabled_ui(!is_busy, |ui| {
                    theme::primary_button(
                        ui,
                        if is_busy { "Grabbing..." } else { "Grab" },
                        [96.0, 22.0],
                    )
                })
                .inner
                .clicked()
            {
                self.trigger_grab();
            }
            if !self.status.is_empty() {
                ui.label(RichText::new(&self.status).small().weak());
            }
        });
    }

    fn render_node(
        &mut self,
        ui: &mut Ui,
        node: &GrabNode,
        path: &str,
        depth: usize,
        row_idx: &mut usize,
        query: &str,
    ) {
        let has_children = !node.children.is_empty();
        let is_expanded = has_children && (!query.is_empty() || self.expanded_keys.contains(path));
        let is_selected = self.selected_key.as_deref() == Some(path);
        let is_match = !query.is_empty() && node.label.to_ascii_lowercase().contains(query);

        let cur_row = *row_idx;
        *row_idx += 1;

        let prefix = if has_children {
            if is_expanded { "▾ " } else { "▸ " }
        } else {
            ""
        };
        let indent = (depth as i8 * 14).min(56);
        let highlight = is_match.then_some(theme::Palette::BORDER_FOCUS);
        let resp = theme::list_text_row(
            ui,
            cur_row,
            is_selected,
            indent,
            format!("{prefix}{}", node.label),
            highlight,
        );

        if resp.clicked() {
            self.selected_key = Some(path.to_string());
            if has_children {
                if self.expanded_keys.contains(path) {
                    self.expanded_keys.remove(path);
                } else {
                    self.expanded_keys.insert(path.to_string());
                }
            }
        }
        if resp.secondary_clicked() {
            self.selected_key = Some(path.to_string());
        }
        resp.context_menu(|ui| self.show_context_menu(ui, &node.action, &node.label));

        if is_expanded {
            for (ci, child) in node.children.iter().enumerate() {
                if !query.is_empty() && !node_matches(child, query) && !is_match {
                    continue;
                }
                let child_path = format!("{path}.{ci}");
                self.render_node(ui, child, &child_path, depth + 1, row_idx, query);
            }
        }
    }

    fn show_context_menu(&mut self, ui: &mut Ui, action: &GrabNodeAction, fallback_label: &str) {
        match action {
            GrabNodeAction::None => {
                if ui.button("Copy text").clicked() {
                    ui.ctx().copy_text(fallback_label.to_string());
                    ui.close();
                }
            }
            GrabNodeAction::WikiCopy { name } => {
                self.menu_wiki_and_copy(ui, name);
            }
            GrabNodeAction::Shop { shop_id, name } => {
                if ui.button("Load Shop").clicked() {
                    self.bot
                        .spawn_flash_action("loadShop", vec![FlashArg::from(*shop_id)]);
                    self.status = format!("Loading shop {shop_id}");
                    ui.close();
                }
                ui.separator();
                self.menu_wiki_and_copy(ui, name);
            }
            GrabNodeAction::ShopItem {
                shop_id,
                item_id,
                shop_item_id,
                name,
                qty,
                cost,
                is_ac,
            } => {
                let buy_label = if *is_ac && *cost > 0 {
                    format!("Buy This Item ({cost} AC!)")
                } else {
                    "Buy This Item".to_string()
                };
                if ui
                    .button(if *is_ac && *cost > 0 {
                        RichText::new(buy_label).color(Color32::from_rgb(251, 191, 36))
                    } else {
                        RichText::new(buy_label)
                    })
                    .clicked()
                {
                    self.bot.spawn_flash_action(
                        "BuyItemQty",
                        vec![
                            FlashArg::from(*item_id),
                            FlashArg::from(*shop_item_id),
                            FlashArg::from(*qty),
                        ],
                    );
                    self.status = format!("Buying {name}");
                    ui.close();
                }
                if ui.button("Add buy command").clicked() {
                    let mut cfg = self.bot.runner.config();
                    cfg.commands.push(BotCommand::Buy {
                        shop_id: *shop_id,
                        item_name: name.clone(),
                        qty: *qty,
                    });
                    self.bot.runner.update_config(cfg);
                    self.status = format!("Added Buy: {name}");
                    ui.close();
                }
                ui.separator();
                self.menu_wiki_and_copy(ui, name);
            }
            GrabNodeAction::Quest {
                quest_id,
                i_slot,
                i_value,
                reqs,
            } => {
                if ui.button("Add to quest list").clicked() {
                    let mut cfg = self.bot.runner.config();
                    if !cfg.quests.iter().any(|q| q.id == *quest_id) {
                        cfg.quests.push(QuestEntry {
                            id: *quest_id,
                            item_id: None,
                            safe_relogin: false,
                        });
                        self.bot.runner.update_config(cfg);
                    }
                    self.status = format!("Added quest {quest_id}");
                    ui.close();
                }
                if ui.button("Accept Quest").clicked() {
                    self.bot
                        .spawn_flash_action("acceptQuest", vec![FlashArg::from(*quest_id)]);
                    self.status = format!("Accepted quest {quest_id}");
                    ui.close();
                }
                if ui.button("Complete Quest (Once)").clicked() {
                    self.bot.spawn_flash_action(
                        "completeQuest",
                        vec![FlashArg::from(*quest_id), FlashArg::from(-1)],
                    );
                    self.status = format!("Completed quest {quest_id}");
                    ui.close();
                }
                if ui.button("Complete Quest (Maxed)").clicked() {
                    self.bot.spawn_flash_action(
                        "Complete",
                        vec![
                            FlashArg::from(*quest_id),
                            FlashArg::from(-1),
                            FlashArg::from(-1),
                        ],
                    );
                    self.status = format!("Completed quest {quest_id} (max)");
                    ui.close();
                }
                if ui.button("Load Quest").clicked() {
                    self.bot
                        .spawn_flash_action("loadQuest", vec![FlashArg::from(*quest_id)]);
                    self.status = format!("Loaded quest {quest_id}");
                    ui.close();
                }
                if !reqs.is_empty() {
                    ui.separator();
                    if ui.button("Copy all Reqs").clicked() {
                        ui.ctx().copy_text(format_items_clipboard(reqs));
                        self.status = "Copied quest reqs".into();
                        ui.close();
                    }
                    if ui.button("Copy Bypass Packet").clicked() {
                        let pkt = format!(
                            r#"{{"t":"xt","b":{{"r":-1,"o":{{"cmd":"updateQuest","iValue":{i_value},"iIndex":{i_slot}}}}}}}"#
                        );
                        ui.ctx().copy_text(pkt);
                        self.status = "Copied bypass packet".into();
                        ui.close();
                    }
                }
            }
            GrabNodeAction::ItemList { items } => {
                if ui.button("Add all to whitelist").clicked() {
                    let mut cfg = self.bot.runner.config();
                    for it in items {
                        if !it.is_temp
                            && !cfg.drops.iter().any(|d| d.eq_ignore_ascii_case(&it.name))
                        {
                            cfg.drops.push(it.name.clone());
                        }
                    }
                    self.bot.runner.update_config(cfg);
                    self.status = format!("Added {} items to whitelist", items.len());
                    ui.close();
                }
                if ui.button("Copy all").clicked() {
                    ui.ctx().copy_text(format_items_clipboard(items));
                    self.status = "Copied item list".into();
                    ui.close();
                }
            }
            GrabNodeAction::Item { name, is_temp } => {
                if !*is_temp && ui.button("Add to whitelist").clicked() {
                    let mut cfg = self.bot.runner.config();
                    if !cfg.drops.iter().any(|d| d.eq_ignore_ascii_case(name)) {
                        cfg.drops.push(name.clone());
                        self.bot.runner.update_config(cfg);
                    }
                    self.status = format!("Whitelisted {name}");
                    ui.close();
                }
                self.menu_wiki_and_copy(ui, name);
            }
        }
    }

    fn menu_wiki_and_copy(&mut self, ui: &mut Ui, name: &str) {
        if ui.button("Copy name to clipboard").clicked() {
            ui.ctx().copy_text(name.to_string());
            self.status = format!("Copied: {name}");
            ui.close();
        }
        if ui.button("Go to Wikipage").clicked() {
            let url = format!("https://aqwwiki.wikidot.com/{}", name.replace(' ', "+"));
            ui.ctx().open_url(OpenUrl::new_tab(url));
            ui.close();
        }
        if ui.button("Search on Wiki").clicked() {
            let url = format!(
                "https://aqwwiki.wikidot.com/search:site/q/{}",
                name.replace(' ', "+")
            );
            ui.ctx().open_url(OpenUrl::new_tab(url));
            ui.close();
        }
    }

    fn trigger_load(&mut self) {
        let raw = self.load_input.trim().to_string();
        match self.load_kind {
            LoadKind::HairShop => {
                if let Ok(id) = raw.parse::<i32>() {
                    self.bot.spawn_flash_action(
                        "callGameFunction",
                        vec![
                            FlashArg::from("world.sendLoadHairShopRequest"),
                            FlashArg::from(id),
                        ],
                    );
                    self.status = format!("Hair shop {id}");
                } else {
                    self.status = "Invalid Hair Shop ID".into();
                }
            }
            LoadKind::Shop => {
                if let Ok(id) = raw.parse::<i32>() {
                    self.bot
                        .spawn_flash_action("loadShop", vec![FlashArg::from(id)]);
                    self.status = format!("Shop {id}");
                } else {
                    self.status = "Invalid Shop ID".into();
                }
            }
            LoadKind::Quest => {
                let ids = parse_quest_ids(&raw, self.quest_count);
                if ids.is_empty() {
                    self.status = "Invalid Quest ID".into();
                    return;
                }
                let bot = Arc::clone(&self.bot);
                self.status = format!("Loading {} quest(s)", ids.len());
                let _ = std::thread::Builder::new()
                    .name("grimlite-load-quests".into())
                    .spawn(move || {
                        if let Some(flash) = bot.flash() {
                            for id in ids {
                                let _ = flash.call_bool("loadQuest", &[FlashArg::from(id)]);
                            }
                        }
                    });
            }
            LoadKind::ArmorCustomizer => {
                self.bot.spawn_flash_action(
                    "callGameFunction",
                    vec![FlashArg::from("openArmorCustomize")],
                );
                self.status = "Armor customizer".into();
            }
        }
    }

    fn trigger_force_accept(&mut self) {
        let ids = parse_quest_ids(self.load_input.trim(), self.quest_count);
        if ids.is_empty() {
            self.status = "Invalid Quest ID".into();
            return;
        }
        let bot = Arc::clone(&self.bot);
        let ghost = self.ghost_accept;
        self.status = format!("Force accepting {} quest(s)", ids.len());
        let _ = std::thread::Builder::new()
            .name("grimlite-force-accept".into())
            .spawn(move || {
                let Some(flash) = bot.flash() else {
                    return;
                };
                for &id in &ids {
                    let _ = flash.call_bool("loadQuest", &[FlashArg::from(id)]);
                }
                std::thread::sleep(Duration::from_millis(900));
                for id in ids {
                    let in_prog = flash
                        .call_bool("isQuestInProgress", &[FlashArg::from(id)])
                        .unwrap_or(false);
                    if !in_prog {
                        let _ = flash.call_bool("acceptQuest", &[FlashArg::from(id)]);
                        std::thread::sleep(Duration::from_millis(if ghost { 500 } else { 900 }));
                    }
                }
            });
    }

    fn trigger_grab(&mut self) {
        if self.grabbing.swap(true, Ordering::AcqRel) {
            return;
        }
        self.selected_key = None;
        self.expanded_keys.clear();
        let bot = Arc::clone(&self.bot);
        let kind = self.grab_kind;
        let order = self.order_by;
        let out = Arc::clone(&self.grabbed);
        let busy = Arc::clone(&self.grabbing);
        self.status = format!("Grabbing {}...", kind.label());

        let _ = std::thread::Builder::new()
            .name("grimlite-grabber".into())
            .spawn(move || {
                let nodes = fetch_grabbed(&bot, kind, order);
                if let Ok(mut guard) = out.lock() {
                    *guard = nodes;
                }
                busy.store(false, Ordering::Release);
            });
    }
}

fn parse_quest_ids(raw: &str, count: i32) -> Vec<i32> {
    if raw.contains(',') {
        raw.split(',')
            .filter_map(|s| s.trim().parse::<i32>().ok())
            .collect()
    } else if let Ok(start) = raw.parse::<i32>() {
        let n = count.clamp(1, 30);
        (0..n).map(|i| start + i).collect()
    } else {
        Vec::new()
    }
}

fn node_matches(node: &GrabNode, query: &str) -> bool {
    if node.label.to_ascii_lowercase().contains(query) {
        return true;
    }
    node.children.iter().any(|c| node_matches(c, query))
}

fn format_items_clipboard(items: &[GrabItemMeta]) -> String {
    let names: Vec<&str> = items.iter().map(|i| i.name.as_str()).collect();
    let qtys: Vec<String> = items.iter().map(|i| i.qty.to_string()).collect();
    format!(
        "\"ItemName\" : \"{}\",\n\"Quantity\" : \"{}\"",
        names.join(","),
        qtys.join(",")
    )
}
