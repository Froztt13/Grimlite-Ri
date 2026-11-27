//! Bot Manager — Skills tab.

use super::{BotManagerWindow, group_box, two_columns};
use crate::theme;
use egui::{RichText, TextEdit, Ui};
use grimlite_bot::gbot::{BotCommand, BotConfig, SkillEntry};

impl BotManagerWindow {
    // ---- Tab 2: Skills (Modern UX: Quick Rotation, Safe Skill, Skill Command, Skill Set) ----

    pub(super) fn show_skills_tab(&mut self, ui: &mut Ui, cfg: &mut BotConfig) -> bool {
        let mut dirty = false;

        two_columns(ui, |cols| {
            // Column 1: Skill Rotation + Safe Skill + Delay
            group_box(&mut cols[0], "Skill Rotation", |ui| {
                ui.label(RichText::new("Quick add to Skills list").small().weak());
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 4.0;
                    for idx in 1..=4 {
                        if theme::primary_button(ui, &idx.to_string(), [30.0, 22.0]).clicked() {
                            cfg.skills.push(SkillEntry {
                                text: format!("Skill {idx}"),
                                index: idx.to_string(),
                                skill_type: 0,
                                ..Default::default()
                            });
                            dirty = true;
                        }
                    }
                    let rem = ui.available_width().max(58.0);
                    if theme::primary_button(ui, "All (1-4)", [rem, 22.0]).clicked() {
                        for idx in 1..=4 {
                            cfg.skills.push(SkillEntry {
                                text: format!("Skill {idx}"),
                                index: idx.to_string(),
                                skill_type: 0,
                                ..Default::default()
                            });
                        }
                        dirty = true;
                    }
                });

                ui.add_space(4.0);
                ui.separator();
                ui.add_space(2.0);
                ui.label(RichText::new("Safe Skill Rule").small().weak());

                ui.horizontal(|ui| {
                    ui.label("Slot");
                    theme::num_input(ui, &mut self.skill_idx, 0..=5, 38.0);
                    ui.label("Stat");
                    theme::segmented_switch(ui, &mut self.safe_is_mp, "HP", "MP", 84.0);
                });
                ui.horizontal(|ui| {
                    ui.label("When");
                    let mut is_greater = !self.safe_lower;
                    theme::segmented_switch(ui, &mut is_greater, "<= Lower", ">= Greater", 132.0);
                    self.safe_lower = !is_greater;
                    theme::num_input(ui, &mut self.safe_threshold, 1..=100, 40.0);
                    ui.label("%");
                });
                ui.add_space(3.0);
                if theme::primary_button(ui, "+ Add Safe Skill", [ui.available_width(), 22.0])
                    .clicked()
                {
                    let stat = if self.safe_is_mp { "MP" } else { "HP" };
                    let cmp = if self.safe_lower { "<=" } else { ">=" };
                    cfg.skills.push(SkillEntry {
                        text: format!(
                            "[Safe {stat} {cmp} {}%] Skill {}",
                            self.safe_threshold, self.skill_idx
                        ),
                        index: self.skill_idx.to_string(),
                        skill_type: 1,
                        safe_type: if self.safe_lower { 0 } else { 1 },
                        is_safe_mp: self.safe_is_mp,
                        safe_value: self.safe_threshold,
                        safe_health: !self.safe_is_mp,
                        safe_mana: self.safe_is_mp,
                    });
                    dirty = true;
                }

                ui.add_space(4.0);
                ui.separator();
                ui.horizontal(|ui| {
                    ui.label("Skill Delay");
                    if theme::num_input(ui, &mut cfg.skill_delay, 0..=5000, 62.0).changed() {
                        dirty = true;
                    }
                    ui.label("ms");
                });
            });

            // Column 2: Skill Command + Specific Skill Set
            group_box(&mut cols[1], "Skill Command", |ui| {
                ui.label(RichText::new("Target Monster").small().weak());
                ui.add(
                    TextEdit::singleline(&mut self.skill_cmd_monster)
                        .hint_text("Monster (* = random)")
                        .desired_width(f32::INFINITY),
                );
                ui.add_space(3.0);
                ui.horizontal(|ui| {
                    ui.label("Skill Slot");
                    theme::num_input(ui, &mut self.skill_cmd_idx, 0..=5, 42.0);
                    ui.checkbox(&mut self.skill_cmd_wait, "Wait");
                    ui.checkbox(&mut self.skill_cmd_force, "Force");
                });
                ui.add_space(4.0);
                if theme::primary_button(ui, "+ Add Skill Command", [ui.available_width(), 22.0])
                    .clicked()
                {
                    let mon = self.skill_cmd_monster.trim();
                    self.push_command(
                        cfg,
                        BotCommand::UseSkill {
                            monster: if mon.is_empty() {
                                "*".to_string()
                            } else {
                                mon.to_string()
                            },
                            index: self.skill_cmd_idx.to_string(),
                            wait: self.skill_cmd_wait,
                            force: self.skill_cmd_force,
                        },
                    );
                    dirty = true;
                }
            });

            cols[1].add_space(8.0);
            group_box(&mut cols[1], "Skill Set Label", |ui| {
                ui.add(
                    TextEdit::singleline(&mut self.skill_set_name)
                        .hint_text("Skill Set Name (e.g. Boss, Farm)")
                        .desired_width(f32::INFINITY),
                );
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    let half = ((ui.available_width() - 5.0) * 0.5).max(60.0);
                    if theme::primary_button(ui, "+ Add Label", [half, 22.0]).clicked()
                        && !self.skill_set_name.trim().is_empty()
                    {
                        cfg.skills.push(SkillEntry {
                            text: format!("[{}]", self.skill_set_name.trim()),
                            index: String::new(),
                            skill_type: 2,
                            ..Default::default()
                        });
                        dirty = true;
                    }
                    if theme::primary_button(ui, "Use Skill Set", [half, 22.0]).clicked()
                        && !self.skill_set_name.trim().is_empty()
                    {
                        self.push_command(
                            cfg,
                            BotCommand::SkillSet {
                                name: self.skill_set_name.trim().to_string(),
                            },
                        );
                        dirty = true;
                    }
                });
            });
        });

        dirty
    }

}
