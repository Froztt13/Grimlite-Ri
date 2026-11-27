//! Bot Manager — Item tab.

use super::{BotManagerWindow, group_box, two_columns};
use crate::theme;
use egui::{RichText, TextEdit, Ui};
use grimlite_bot::gbot::{BotCommand, BotConfig};

impl BotManagerWindow {
    // ---- Tab 4: Item ----

    pub(super) fn show_item_tab(&mut self, ui: &mut Ui, cfg: &mut BotConfig) -> bool {
        let mut dirty = false;
        two_columns(ui, |cols| {
            group_box(&mut cols[0], "Item Actions", |ui| {
                ui.label(RichText::new("Item Name").small().weak());
                ui.add(
                    TextEdit::singleline(&mut self.item_name)
                        .hint_text("Item name (also used by Buy)")
                        .desired_width(f32::INFINITY),
                );
                ui.add_space(5.0);
                let half = ((ui.available_width() - 5.0) * 0.5).max(60.0);
                let trimmed = self.item_name.trim().to_string();

                ui.horizontal(|ui| {
                    if theme::primary_button(ui, "Get Drop", [half, 22.0]).clicked()
                        && !trimmed.is_empty()
                    {
                        self.push_command(
                            cfg,
                            BotCommand::GetDrop {
                                item_name: trimmed.clone(),
                            },
                        );
                        dirty = true;
                    }
                    if theme::primary_button(ui, "Equip", [half, 22.0]).clicked()
                        && !trimmed.is_empty()
                    {
                        self.push_command(
                            cfg,
                            BotCommand::Equip {
                                item_name: trimmed.clone(),
                            },
                        );
                        dirty = true;
                    }
                });
                ui.add_space(3.0);
                ui.horizontal(|ui| {
                    if theme::primary_button(ui, "To Bank", [half, 22.0]).clicked()
                        && !trimmed.is_empty()
                    {
                        self.push_command(
                            cfg,
                            BotCommand::BankTransfer {
                                item_name: trimmed.clone(),
                                to_bank: true,
                            },
                        );
                        dirty = true;
                    }
                    if theme::primary_button(ui, "To Inv", [half, 22.0]).clicked()
                        && !trimmed.is_empty()
                    {
                        self.push_command(
                            cfg,
                            BotCommand::BankTransfer {
                                item_name: trimmed.clone(),
                                to_bank: false,
                            },
                        );
                        dirty = true;
                    }
                });
                ui.add_space(3.0);
                ui.horizontal(|ui| {
                    if theme::primary_button(ui, "Sell", [half, 22.0]).clicked()
                        && !trimmed.is_empty()
                    {
                        self.push_command(
                            cfg,
                            BotCommand::Sell {
                                item_name: trimmed.clone(),
                            },
                        );
                        dirty = true;
                    }
                    if theme::primary_button(ui, "+ Whitelist", [half, 22.0]).clicked()
                        && !trimmed.is_empty()
                    {
                        if !cfg.drops.iter().any(|d| d.eq_ignore_ascii_case(&trimmed)) {
                            cfg.drops.push(trimmed.clone());
                            dirty = true;
                        }
                    }
                });
            });

            cols[0].add_space(8.0);
            group_box(&mut cols[0], "Map Item", |ui| {
                ui.horizontal(|ui| {
                    ui.add(
                        TextEdit::singleline(&mut self.map_item_id)
                            .hint_text("Map Item ID (e.g. 1234)")
                            .desired_width((ui.available_width() - 96.0).max(60.0)),
                    );
                    let rem = ui.available_width().max(88.0);
                    if theme::primary_button(ui, "+ Map Item", [rem, 22.0]).clicked()
                        && let Ok(id) = self.map_item_id.trim().parse::<i32>()
                    {
                        self.push_command(cfg, BotCommand::MapItem { item_id: id });
                        dirty = true;
                    }
                });
            });

            group_box(&mut cols[1], "Shop", |ui| {
                ui.horizontal(|ui| {
                    ui.label("Shop ID");
                    ui.add(
                        TextEdit::singleline(&mut self.shop_id)
                            .hint_text("ID")
                            .desired_width(68.0),
                    );
                    ui.add_space(4.0);
                    ui.label("Qty");
                    ui.add(
                        TextEdit::singleline(&mut self.buy_qty)
                            .hint_text("1")
                            .desired_width(ui.available_width().max(38.0)),
                    );
                });
                ui.add_space(5.0);
                let half = ((ui.available_width() - 5.0) * 0.5).max(60.0);
                ui.horizontal(|ui| {
                    if theme::primary_button(ui, "Load Shop", [half, 22.0]).clicked()
                        && let Ok(id) = self.shop_id.trim().parse::<i32>()
                    {
                        self.push_command(cfg, BotCommand::LoadShop { shop_id: id });
                        dirty = true;
                    }
                    if theme::primary_button(ui, "Buy Item", [half, 22.0]).clicked()
                        && let Ok(id) = self.shop_id.trim().parse::<i32>()
                        && !self.item_name.trim().is_empty()
                    {
                        let qty = self.buy_qty.trim().parse::<i32>().unwrap_or(1);
                        self.push_command(
                            cfg,
                            BotCommand::Buy {
                                shop_id: id,
                                item_name: self.item_name.trim().to_string(),
                                qty,
                            },
                        );
                        dirty = true;
                    }
                });
            });

            cols[1].add_space(8.0);
            group_box(&mut cols[1], "Drop Rules", |ui| {
                dirty |= ui
                    .checkbox(&mut cfg.enable_pickup, "Enable Pickup")
                    .changed();
                dirty |= ui
                    .checkbox(&mut cfg.enable_pickup_ac_tagged, "Pickup All AC Tagged")
                    .changed();
                dirty |= ui
                    .checkbox(&mut cfg.enable_pickup_all, "Pickup All Drops")
                    .changed();
                dirty |= ui
                    .checkbox(&mut cfg.enable_rejection, "Reject Unlisted Drops")
                    .changed();
            });
        });
        dirty
    }

}
