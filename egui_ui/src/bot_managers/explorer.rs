//! Left list explorer (`Commands` / `Skills` / `Quests` / `Drops`), clipboard, shortcuts, and syntax colors.

use super::{BotManagerWindow, ClipboardBuffer, ListKind};
use crate::theme;
use egui::{Color32, Context, Key, RichText, ScrollArea, TextEdit, Ui};
use grimlite_bot::gbot::{BotCommand, BotConfig, QuestEntry, SkillEntry, StatementKind};
use std::collections::BTreeSet;

impl BotManagerWindow {
    pub(super) fn handle_list_shortcuts(&mut self, ui: &Ui, cfg: &mut BotConfig) -> bool {
        if ui.ctx().egui_wants_keyboard_input() {
            return false;
        }
        let mut dirty = false;
        let (
            cmd_up,
            cmd_down,
            do_copy_plain,
            do_copy,
            do_cut,
            do_dup,
            pasted_text,
            do_paste_key,
            do_delete,
        ) = ui.input(|i| {
            let cmd = i.modifiers.command;
            let shift = i.modifiers.shift;
            let up = cmd && i.key_pressed(Key::ArrowUp);
            let down = cmd && i.key_pressed(Key::ArrowDown);
            let copy_plain = cmd && shift && i.key_pressed(Key::C);
            let copy = !copy_plain
                && (i.events.iter().any(|e| matches!(e, egui::Event::Copy))
                    || (cmd && i.key_pressed(Key::C)));
            let cut = i.events.iter().any(|e| matches!(e, egui::Event::Cut))
                || (cmd && i.key_pressed(Key::X));
            let dup = cmd && i.key_pressed(Key::D);
            let pasted = i.events.iter().find_map(|e| match e {
                egui::Event::Paste(text) => Some(text.clone()),
                _ => None,
            });
            let paste_key = cmd && i.key_pressed(Key::V);
            let del = i.key_pressed(Key::Delete) || i.key_pressed(Key::Backspace);
            (up, down, copy_plain, copy, cut, dup, pasted, paste_key, del)
        });

        if cmd_up {
            dirty |= self.move_selected_up(cfg);
        }
        if cmd_down {
            dirty |= self.move_selected_down(cfg);
        }
        if do_copy_plain {
            self.copy_selected_plain_text(ui.ctx(), cfg);
        } else if do_copy {
            self.copy_selected(ui.ctx(), cfg);
        } else if do_cut {
            self.copy_selected(ui.ctx(), cfg);
            dirty |= self.remove_selected(cfg);
        } else if do_dup {
            self.copy_selected(ui.ctx(), cfg);
            dirty |= self.paste_clipboard(cfg, None);
        }
        if let Some(text) = pasted_text {
            dirty |= self.paste_clipboard(cfg, Some(&text));
        } else if do_paste_key {
            dirty |= self.paste_clipboard(cfg, None);
        }
        if do_delete {
            dirty |= self.remove_selected(cfg);
        }
        dirty
    }

    pub(super) fn show_left_explorer(&mut self, ui: &mut Ui, cfg: &mut BotConfig) -> bool {
        let mut dirty = false;
        let running = self.bot.runner.is_running();
        let active_idx = self.bot.runner.current_index();

        // Bottom controls pinned cleanly so they are NEVER clipped
        egui::Panel::bottom("bot_mgr_left_controls")
            .resizable(false)
            .show_separator_line(false)
            .show(ui, |ui| {
                ui.add_space(3.0);
                let total_w = ui.available_width();
                let gap = 4.0;
                let half_w = ((total_w - gap) * 0.5).max(80.0);

                // Row 1 (50% - 50%): [v] [^] (50%) | [Search... reactive] (50%)
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = gap;
                    let qtr_w = ((half_w - gap) * 0.5).max(24.0);
                    let len = self.current_list_len(cfg);
                    let can_down = self
                        .selected
                        .iter()
                        .next_back()
                        .is_some_and(|&i| i + 1 < len);
                    let can_up = self
                        .selected
                        .iter()
                        .next()
                        .is_some_and(|&i| i > 0 && i < len);

                    if theme::primary_button_enabled(ui, can_down, "v", [qtr_w, 20.0])
                        .on_hover_text("Move down (Cmd/Ctrl + Down)")
                        .clicked()
                    {
                        dirty |= self.move_selected_down(cfg);
                    }
                    if theme::primary_button_enabled(ui, can_up, "^", [qtr_w, 20.0])
                        .on_hover_text("Move up (Cmd/Ctrl + Up)")
                        .clicked()
                    {
                        dirty |= self.move_selected_up(cfg);
                    }
                    ui.add(
                        TextEdit::singleline(&mut self.search_text)
                            .hint_text("Search...")
                            .desired_width(half_w),
                    );
                });

                // Row 2 (50% - 50%): [Clear] [ ] All (50%) | [Remove] (50%)
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = gap;
                    ui.allocate_ui_with_layout(
                        egui::vec2(half_w, 20.0),
                        egui::Layout::left_to_right(egui::Align::Center),
                        |ui| {
                            ui.spacing_mut().item_spacing.x = gap;
                            let clear_w = (half_w - 42.0).max(38.0);
                            if theme::primary_button(ui, "Clear", [clear_w, 20.0]).clicked() {
                                if self.clear_all_lists {
                                    cfg.commands.clear();
                                    cfg.skills.clear();
                                    cfg.quests.clear();
                                    cfg.drops.clear();
                                } else {
                                    match self.list_kind {
                                        ListKind::Commands => cfg.commands.clear(),
                                        ListKind::Skills => cfg.skills.clear(),
                                        ListKind::Quests => cfg.quests.clear(),
                                        ListKind::Drops => cfg.drops.clear(),
                                    }
                                }
                                self.selected.clear();
                                self.anchor = None;
                                dirty = true;
                            }
                            ui.checkbox(&mut self.clear_all_lists, "All");
                        },
                    );

                    let len = self.current_list_len(cfg);
                    let can_remove = self.selected.iter().any(|&i| i < len);
                    if theme::danger_button(ui, "Remove", can_remove, [half_w, 20.0]).clicked() {
                        dirty |= self.remove_selected(cfg);
                    }
                });

                // Row 3 (50% - 50%): [Commands ▾] (50%) | [Start Bot / Bot: ON] (50%)
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = gap;
                    let prev_kind = self.list_kind;
                    theme::dropdown(
                        ui,
                        "bot_mgr_list_kind",
                        self.list_kind.label(),
                        half_w,
                        |ui| {
                            for &k in ListKind::ALL {
                                ui.selectable_value(&mut self.list_kind, k, k.label());
                            }
                        },
                    );
                    if self.list_kind != prev_kind {
                        self.selected.clear();
                        self.anchor = None;
                    }

                    let has_cmds = !cfg.commands.is_empty();
                    let logged_in = self.bot.is_logged_in();
                    if theme::toggle_button(
                        ui,
                        running,
                        running || (logged_in && has_cmds),
                        "Start Bot",
                        "Bot: ON",
                        Some([half_w, 20.0]),
                    )
                    .clicked()
                    {
                        if running {
                            self.bot.stop_bot();
                        } else {
                            self.bot.start_bot();
                        }
                    }
                });
            });

        // Framed full-width List Box occupying exact remaining height
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE)
            .show(ui, |ui| {
                egui::Frame::group(ui.style())
                    .inner_margin(egui::Margin::symmetric(3, 3))
                    .show(ui, |ui| {
                        ui.set_min_size(ui.available_size());
                        ScrollArea::vertical()
                            .id_salt("bot_mgr_explorer_list")
                            .auto_shrink([false, false])
                            .show(ui, |ui| {
                                ui.spacing_mut().item_spacing.y = 2.0;
                                let len = self.current_list_len(cfg);
                                if len == 0 {
                                    ui.label(RichText::new("  (empty)").small().weak());
                                    return;
                                }
                                let q = self.search_text.trim().to_lowercase();
                                let mut shown = 0;
                                for i in 0..len {
                                    let label = self.format_list_item(cfg, i);
                                    if !q.is_empty() && !label.to_lowercase().contains(&q) {
                                        continue;
                                    }
                                    shown += 1;
                                    let is_selected = self.selected.contains(&i);
                                    let is_cur = self.list_kind == ListKind::Commands
                                        && running
                                        && i == active_idx;

                                    let row_fill = if is_selected {
                                        Color32::from_rgb(16, 82, 118)
                                    } else if is_cur {
                                        Color32::from_rgb(20, 62, 42)
                                    } else if shown % 2 == 0 {
                                        Color32::from_rgba_unmultiplied(255, 255, 255, 6)
                                    } else {
                                        Color32::TRANSPARENT
                                    };

                                    let text_color = if is_selected {
                                        Color32::WHITE
                                    } else if is_cur {
                                        Color32::from_rgb(120, 225, 150)
                                    } else {
                                        self.item_text_color(cfg, i)
                                    };

                                    let index_color = if is_selected {
                                        Color32::from_rgb(220, 238, 255)
                                    } else if is_cur {
                                        Color32::from_rgb(120, 225, 150)
                                    } else {
                                        Color32::from_rgb(0xD9, 0xEE, 0xCB)
                                    };

                                    let prefix = if is_cur { "> " } else { "" };
                                    let mut job = egui::text::LayoutJob::default();
                                    let mono = egui::FontId::monospace(11.5);
                                    job.append(
                                        &format!("{prefix}[{i:02}] "),
                                        0.0,
                                        egui::TextFormat::simple(mono.clone(), index_color),
                                    );
                                    job.append(
                                        &label,
                                        0.0,
                                        egui::TextFormat::simple(mono, text_color),
                                    );

                                    let resp = egui::Frame::new()
                                        .fill(row_fill)
                                        .corner_radius(3.0)
                                        .inner_margin(egui::Margin::symmetric(6, 4))
                                        .show(ui, |ui| {
                                            ui.set_min_width(ui.available_width());
                                            ui.add(
                                                egui::Label::new(job).truncate().selectable(false),
                                            )
                                        })
                                        .response
                                        .interact(egui::Sense::click())
                                        .on_hover_text(&label);

                                    if resp.double_clicked() {
                                        match self.list_kind {
                                            ListKind::Commands => {
                                                self.command_editor
                                                    .open_command(i, cfg.commands[i].clone());
                                            }
                                            ListKind::Skills => {
                                                self.command_editor
                                                    .open_skill(i, cfg.skills[i].clone());
                                            }
                                            ListKind::Quests => {
                                                self.command_editor
                                                    .open_quest(i, cfg.quests[i].clone());
                                            }
                                            ListKind::Drops => {
                                                self.command_editor
                                                    .open_drop(i, cfg.drops[i].clone());
                                            }
                                        }
                                    } else if resp.clicked() {
                                        // Release focus from any TextEdit so Cmd/Ctrl+C/V/Up/Down work immediately
                                        if let Some(focused) = ui.ctx().memory(|m| m.focused()) {
                                            ui.ctx().memory_mut(|m| m.surrender_focus(focused));
                                        }
                                        let (shift, cmd) = ui.input(|inp| {
                                            (inp.modifiers.shift, inp.modifiers.command)
                                        });
                                        if shift {
                                            let anchor = self.anchor.unwrap_or(i);
                                            let (lo, hi) = if anchor <= i {
                                                (anchor, i)
                                            } else {
                                                (i, anchor)
                                            };
                                            self.selected.clear();
                                            for idx in lo..=hi {
                                                self.selected.insert(idx);
                                            }
                                        } else if cmd {
                                            if !self.selected.remove(&i) {
                                                self.selected.insert(i);
                                            }
                                            self.anchor = Some(i);
                                        } else {
                                            self.selected.clear();
                                            self.selected.insert(i);
                                            self.anchor = Some(i);
                                        }
                                    }
                                }
                                if shown == 0 {
                                    ui.label(RichText::new("  (no matches)").small().weak());
                                }
                            });
                    });
            });

        dirty
    }

    fn current_list_len(&self, cfg: &BotConfig) -> usize {
        match self.list_kind {
            ListKind::Commands => cfg.commands.len(),
            ListKind::Skills => cfg.skills.len(),
            ListKind::Quests => cfg.quests.len(),
            ListKind::Drops => cfg.drops.len(),
        }
    }

    fn item_text_color(&self, cfg: &BotConfig, idx: usize) -> Color32 {
        match self.list_kind {
            ListKind::Commands => cfg
                .commands
                .get(idx)
                .map(command_color)
                .unwrap_or(Color32::from_rgb(0xDC, 0xDC, 0xDC)),
            ListKind::Skills => match cfg.skills.get(idx).map(|s| s.skill_type).unwrap_or(0) {
                2 => Color32::from_rgb(0xCE, 0xAC, 0x90),
                1 => Color32::from_rgb(0xAB, 0xE1, 0xD7),
                _ => Color32::from_rgb(0xF5, 0x9D, 0xD9),
            },
            ListKind::Quests => Color32::from_rgb(0xC9, 0xAB, 0xB0),
            ListKind::Drops => Color32::from_rgb(0xE1, 0x97, 0xCF),
        }
    }

    fn format_list_item(&self, cfg: &BotConfig, idx: usize) -> String {
        match self.list_kind {
            ListKind::Commands => cfg.commands[idx].to_string(),
            ListKind::Skills => {
                let s = &cfg.skills[idx];
                if s.skill_type == 2 {
                    format!("[{}]", s.text)
                } else if s.safe_health || s.safe_mana {
                    let kind = if s.safe_mana { "MP" } else { "HP" };
                    format!("Safe {} ({kind} <= {}%)", s.index, s.safe_value)
                } else if s.text.is_empty() {
                    format!("Skill {}", s.index)
                } else {
                    format!("{}: {}", s.index, s.text)
                }
            }
            ListKind::Quests => {
                let q = &cfg.quests[idx];
                match &q.item_id {
                    Some(item) if !item.is_empty() => format!("Quest {} (item {item})", q.id),
                    _ => format!("Quest {}", q.id),
                }
            }
            ListKind::Drops => cfg.drops[idx].clone(),
        }
    }

    fn swap_items(&self, cfg: &mut BotConfig, a: usize, b: usize) {
        match self.list_kind {
            ListKind::Commands => cfg.commands.swap(a, b),
            ListKind::Skills => cfg.skills.swap(a, b),
            ListKind::Quests => cfg.quests.swap(a, b),
            ListKind::Drops => cfg.drops.swap(a, b),
        }
    }

    fn move_selected_up(&mut self, cfg: &mut BotConfig) -> bool {
        let len = self.current_list_len(cfg);
        let indices: Vec<usize> = self.selected.iter().copied().filter(|&i| i < len).collect();
        if indices.is_empty() || indices[0] == 0 {
            return false;
        }
        let mut next = BTreeSet::new();
        for i in indices {
            self.swap_items(cfg, i, i - 1);
            next.insert(i - 1);
        }
        self.selected = next;
        self.anchor = self.anchor.map(|a| a.saturating_sub(1));
        true
    }

    fn move_selected_down(&mut self, cfg: &mut BotConfig) -> bool {
        let len = self.current_list_len(cfg);
        let indices: Vec<usize> = self.selected.iter().copied().filter(|&i| i < len).collect();
        if indices.is_empty() || *indices.last().unwrap_or(&0) + 1 >= len {
            return false;
        }
        let mut next = BTreeSet::new();
        for i in indices.into_iter().rev() {
            self.swap_items(cfg, i, i + 1);
            next.insert(i + 1);
        }
        self.selected = next;
        self.anchor = self.anchor.map(|a| (a + 1).min(len - 1));
        true
    }

    fn remove_selected(&mut self, cfg: &mut BotConfig) -> bool {
        let len = self.current_list_len(cfg);
        let indices: Vec<usize> = self.selected.iter().copied().filter(|&i| i < len).collect();
        if indices.is_empty() {
            return false;
        }
        let first = indices[0];
        for i in indices.into_iter().rev() {
            match self.list_kind {
                ListKind::Commands => {
                    cfg.commands.remove(i);
                }
                ListKind::Skills => {
                    cfg.skills.remove(i);
                }
                ListKind::Quests => {
                    cfg.quests.remove(i);
                }
                ListKind::Drops => {
                    cfg.drops.remove(i);
                }
            }
        }
        self.selected.clear();
        let new_len = self.current_list_len(cfg);
        if new_len > 0 {
            let sel = first.min(new_len - 1);
            self.selected.insert(sel);
            self.anchor = Some(sel);
        } else {
            self.anchor = None;
        }
        true
    }

    fn copy_selected_plain_text(&self, ctx: &Context, cfg: &BotConfig) {
        let len = self.current_list_len(cfg);
        let lines: Vec<String> = self
            .selected
            .iter()
            .copied()
            .filter(|&i| i < len)
            .map(|i| self.format_list_item(cfg, i))
            .collect();
        if !lines.is_empty() {
            ctx.copy_text(lines.join("\n"));
        }
    }

    fn copy_selected(&mut self, ctx: &Context, cfg: &BotConfig) {
        let len = self.current_list_len(cfg);
        let indices: Vec<usize> = self.selected.iter().copied().filter(|&i| i < len).collect();
        if indices.is_empty() {
            return;
        }
        let mut partial = BotConfig::default();
        self.clipboard = match self.list_kind {
            ListKind::Commands => {
                let items: Vec<BotCommand> = indices
                    .into_iter()
                    .map(|i| cfg.commands[i].clone())
                    .collect();
                partial.commands = items.clone();
                ClipboardBuffer::Commands(items)
            }
            ListKind::Skills => {
                let items: Vec<SkillEntry> =
                    indices.into_iter().map(|i| cfg.skills[i].clone()).collect();
                partial.skills = items.clone();
                ClipboardBuffer::Skills(items)
            }
            ListKind::Quests => {
                let items: Vec<QuestEntry> =
                    indices.into_iter().map(|i| cfg.quests[i].clone()).collect();
                partial.quests = items.clone();
                ClipboardBuffer::Quests(items)
            }
            ListKind::Drops => {
                let items: Vec<String> =
                    indices.into_iter().map(|i| cfg.drops[i].clone()).collect();
                partial.drops = items.clone();
                ClipboardBuffer::Drops(items)
            }
        };
        // Mirror Grimlite-Li: also serialize selected items as .gbot JSON to the OS clipboard
        if let Ok(json) = partial.to_gbot_json() {
            ctx.copy_text(json);
        }
    }

    fn paste_clipboard(&mut self, cfg: &mut BotConfig, os_text: Option<&str>) -> bool {
        let len = self.current_list_len(cfg);
        let insert_at = self
            .selected
            .iter()
            .next_back()
            .map_or(len, |&last| (last + 1).min(len));

        // 1. Prefer OS clipboard .gbot JSON (Grimlite-Li compatible) if valid
        if let Some(raw) = os_text
            && let Ok(parsed) = BotConfig::from_gbot_json(raw)
        {
            let count = match self.list_kind {
                ListKind::Commands if !parsed.commands.is_empty() => {
                    for (offset, item) in parsed.commands.iter().cloned().enumerate() {
                        cfg.commands.insert(insert_at + offset, item);
                    }
                    parsed.commands.len()
                }
                ListKind::Skills if !parsed.skills.is_empty() => {
                    for (offset, item) in parsed.skills.iter().cloned().enumerate() {
                        cfg.skills.insert(insert_at + offset, item);
                    }
                    parsed.skills.len()
                }
                ListKind::Quests if !parsed.quests.is_empty() => {
                    for (offset, item) in parsed.quests.iter().cloned().enumerate() {
                        cfg.quests.insert(insert_at + offset, item);
                    }
                    parsed.quests.len()
                }
                ListKind::Drops if !parsed.drops.is_empty() => {
                    for (offset, item) in parsed.drops.iter().cloned().enumerate() {
                        cfg.drops.insert(insert_at + offset, item);
                    }
                    parsed.drops.len()
                }
                _ => 0,
            };
            if count > 0 {
                self.selected.clear();
                for idx in insert_at..insert_at + count {
                    self.selected.insert(idx);
                }
                self.anchor = Some(insert_at);
                return true;
            }
        }

        // 2. Fallback to internal clipboard buffer
        let count = match (&self.list_kind, &self.clipboard) {
            (ListKind::Commands, ClipboardBuffer::Commands(items)) if !items.is_empty() => {
                for (offset, item) in items.iter().cloned().enumerate() {
                    cfg.commands.insert(insert_at + offset, item);
                }
                items.len()
            }
            (ListKind::Skills, ClipboardBuffer::Skills(items)) if !items.is_empty() => {
                for (offset, item) in items.iter().cloned().enumerate() {
                    cfg.skills.insert(insert_at + offset, item);
                }
                items.len()
            }
            (ListKind::Quests, ClipboardBuffer::Quests(items)) if !items.is_empty() => {
                for (offset, item) in items.iter().cloned().enumerate() {
                    cfg.quests.insert(insert_at + offset, item);
                }
                items.len()
            }
            (ListKind::Drops, ClipboardBuffer::Drops(items)) if !items.is_empty() => {
                for (offset, item) in items.iter().cloned().enumerate() {
                    cfg.drops.insert(insert_at + offset, item);
                }
                items.len()
            }
            _ => return false,
        };

        self.selected.clear();
        for idx in insert_at..insert_at + count {
            self.selected.insert(idx);
        }
        self.anchor = Some(insert_at);
        true
    }

    pub(super) fn push_command(&mut self, cfg: &mut BotConfig, cmd: BotCommand) {
        if self.cmd_held
            && self.list_kind == ListKind::Commands
            && let Some(&last) = self.selected.iter().next_back()
            && last < cfg.commands.len()
        {
            cfg.commands.insert(last + 1, cmd);
            self.selected.clear();
            self.selected.insert(last + 1);
            self.anchor = Some(last + 1);
        } else {
            cfg.commands.push(cmd);
        }
    }

}

/// Command syntax colors matching Grimlite-Li `config.cfg` / `BotManager.lstCommands_DrawItem`.
fn command_color(cmd: &BotCommand) -> Color32 {
    let rgb = |hex: u32| {
        Color32::from_rgb(
            ((hex >> 16) & 0xFF) as u8,
            ((hex >> 8) & 0xFF) as u8,
            (hex & 0xFF) as u8,
        )
    };
    match cmd {
        // Combat
        BotCommand::Kill { .. } => rgb(0xF2BA8F),
        BotCommand::KillFor { .. } => rgb(0xF6D8CF),
        BotCommand::ShortHunt { .. } | BotCommand::QuestHunt { .. } => rgb(0x9999FF),
        BotCommand::Attack { .. } => rgb(0xA6B0A3),
        BotCommand::UseSkill { .. } => rgb(0xF59DD9),
        BotCommand::SkillSet { .. } => rgb(0xCEAC90),
        BotCommand::Rest { .. } => rgb(0xABE1D7),
        BotCommand::CancelTarget => rgb(0xE2A588),

        // Map
        BotCommand::Join { .. } => rgb(0xFC92B5),
        BotCommand::MoveToCell { .. } => rgb(0x93D5B8),
        BotCommand::Walk { .. } => rgb(0xCAEC9E),
        BotCommand::SetSpawn => rgb(0xBCA38A),
        BotCommand::Yulgar => rgb(0xA3B5D6),

        // Item
        BotCommand::GetDrop { .. } => rgb(0x8FC5C6),
        BotCommand::LoadShop { .. } => rgb(0xF7D4C2),
        BotCommand::Buy { .. } => rgb(0xB889D9),
        BotCommand::Sell { .. } => rgb(0xF89F7F),
        BotCommand::Equip { .. } => rgb(0x91A9A7),
        BotCommand::BankTransfer { .. } => rgb(0x809393),
        BotCommand::MapItem { .. } => rgb(0x8BBDD7),
        BotCommand::Whitelist { .. } => rgb(0xE197CF),

        // Quest
        BotCommand::AcceptQuest { .. } => rgb(0xC9ABB0),
        BotCommand::CompleteQuest { .. } => rgb(0xA8E8AF),
        BotCommand::AddQuestList { .. } => rgb(0xC9ABB0),
        BotCommand::RemoveQuestList { .. } => rgb(0xF88495),

        // Misc
        BotCommand::Delay { .. } => rgb(0xFAF1DD),
        BotCommand::Label { .. } => rgb(0x87A8CF),
        BotCommand::GotoLabel { .. } => rgb(0xBDD0E6),
        BotCommand::Index { .. } => rgb(0xD9EECB),
        BotCommand::Packet { .. } => rgb(0xADEB9F),
        BotCommand::Int { .. } => rgb(0xC3FEB1),
        BotCommand::SetVar { .. } => rgb(0x90B396),
        BotCommand::Log { .. } => rgb(0xA9FEA0),
        BotCommand::Provoke { .. } => rgb(0xB9EBE3),
        BotCommand::SetFPS { .. } => rgb(0xD5D1EA),
        BotCommand::Logout => rgb(0xE2A588),
        BotCommand::Beep { .. } => rgb(0xE5F68B),
        BotCommand::GotoPlayer { .. } => rgb(0xD2E580),
        BotCommand::ClearTemp => rgb(0xBBAD92),
        BotCommand::ClientMessage { .. } => rgb(0xF196EF),
        BotCommand::LoadMap { .. } => rgb(0x93D5B8),
        BotCommand::SpecialHandler { .. } => rgb(0x8080FF),
        BotCommand::Stop => rgb(0x92C2C7),
        BotCommand::Restart => rgb(0xF8DFB6),
        BotCommand::Blank { .. } => rgb(0x9CB685),

        // Statements
        BotCommand::Statement { kind, .. } => statement_color(kind),

        BotCommand::Unknown { .. } => theme::Palette::TEXT_MUTED,
    }
}

fn statement_color(kind: &StatementKind) -> Color32 {
    let hex: u32 = match kind {
        StatementKind::InInventory => 0xE9FBE0,
        StatementKind::NotInInventory => 0xB4BDCE,
        StatementKind::InTemp => 0x96C08D,
        StatementKind::NotInTemp => 0xA8CB8B,
        StatementKind::InBank => 0xD789C8,
        StatementKind::NotInBank => 0xE488FA,
        StatementKind::InBankOrInvent => 0xC8ECA2,
        StatementKind::NotInBankAndInvent => 0xB4BDCE,
        StatementKind::MapIs => 0xB981BE,
        StatementKind::MapIsNot => 0xB28DA9,
        StatementKind::CellIs => 0xB6A89F,
        StatementKind::CellIsNot => 0xCEE8FE,
        StatementKind::QuestInProgress => 0x97C0FD,
        StatementKind::QuestNotInProgress => 0xF88495,
        StatementKind::QuestCompleted => 0x86EC9E,
        StatementKind::QuestNotCompleted => 0xE6CBE9,
        StatementKind::MonsterInRoom => 0xADB28D,
        StatementKind::MonsterNotInRoom => 0xA1DCE6,
        StatementKind::HasTarget => 0xE8C9FB,
        StatementKind::NotHasTarget => 0xF7FCBB,
        StatementKind::HealthLessThan => 0xB08EB6,
        StatementKind::HealthGreaterThan => 0xACD9BC,
        StatementKind::ManaLessThan => 0x86CEB7,
        StatementKind::ManaGreaterThan => 0xAC86DC,
        StatementKind::GoldGreaterThan => 0xF68EA5,
        StatementKind::GoldLessThan => 0xB4A193,
        StatementKind::LevelGreaterThan => 0xE5E2DB,
        StatementKind::LevelLessThan => 0x93C6EB,
        StatementKind::IntIs => 0xF69BB3,
        StatementKind::IntIsNot => 0xA183C9,
        StatementKind::IntGreaterThan => 0xCDB2F0,
        StatementKind::IntLessThan => 0xCF89CF,
        StatementKind::IntEqualInt => 0x9E80D6,
        StatementKind::IntNotEqualInt => 0xE29ED9,
        StatementKind::VarEqualVar => 0x90B396,
        StatementKind::VarNotEqualVar => 0xF9F9E1,
        StatementKind::NameEquals => 0x98B2D4,
    };
    Color32::from_rgb(
        ((hex >> 16) & 0xFF) as u8,
        ((hex >> 8) & 0xFF) as u8,
        (hex & 0xFF) as u8,
    )
}
