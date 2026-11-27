//! Compact Client-side Cosmetics tool window:
//! Alignment, Access level, Mute toggle, Username, Guild, Level, FPS, and Gender swap.

use crate::show_detached_window_sized;
use crate::theme;
use egui::{Context, RichText, TextEdit, Ui};
use grimlite_bot::Bot;
use grimlite_bot::host::FlashArg;
use std::sync::Arc;

pub struct CosmeticsWindow {
    bot: Arc<Bot>,
    pub open: bool,
    mute_toggled: bool,
    username: String,
    guild: String,
    level: String,
    fps: i32,
    gender_swapped: bool,
}

impl CosmeticsWindow {
    pub fn new(bot: Arc<Bot>) -> Self {
        Self {
            bot,
            open: false,
            mute_toggled: false,
            username: String::new(),
            guild: String::new(),
            level: String::new(),
            fps: 30,
            gender_swapped: false,
        }
    }

    pub fn show(&mut self, ctx: &Context) {
        if !self.open {
            return;
        }
        let mut open = self.open;
        show_detached_window_sized(
            ctx,
            "grimlite_cosmetics",
            "Cosmetics",
            [285.0, 245.0],
            Some([265.0, 230.0]),
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
        // Card 1: Alignment & Access in compact horizontal rows
        theme::card(ui, "Alignment & Access", |ui| {
            ui.label(RichText::new("Alignment").small().weak());
            let q = ((ui.available_width() - 12.0) / 4.0).max(48.0);
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                if ui.add_sized([q, 21.0], egui::Button::new("Good")).clicked() {
                    self.send_alignment(1);
                }
                if ui.add_sized([q, 21.0], egui::Button::new("Evil")).clicked() {
                    self.send_alignment(2);
                }
                if ui
                    .add_sized([q, 21.0], egui::Button::new("Chaos"))
                    .clicked()
                {
                    self.send_alignment(3);
                }
                if ui
                    .add_sized([q, 21.0], egui::Button::new("Undec."))
                    .on_hover_text("Undecided")
                    .clicked()
                {
                    self.send_alignment(5);
                }
            });

            ui.add_space(2.0);
            ui.label(RichText::new("Access").small().weak());
            let third = ((ui.available_width() - 8.0) / 3.0).max(56.0);
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                if ui
                    .add_sized([third, 21.0], egui::Button::new("Member"))
                    .clicked()
                {
                    self.set_access_level("Member");
                }
                if ui
                    .add_sized([third, 21.0], egui::Button::new("Non-Mem"))
                    .clicked()
                {
                    self.set_access_level("Non Member");
                }
                if ui
                    .add_sized([third, 21.0], egui::Button::new("Moderator"))
                    .clicked()
                {
                    self.set_access_level("Moderator");
                }
            });
        });

        ui.add_space(5.0);

        // Card 2: Client-side overrides
        theme::card(ui, "Client Overrides", |ui| {
            let btn_w = 48.0;

            // Username
            ui.horizontal(|ui| {
                let input_w = (ui.available_width() - btn_w - 5.0).max(80.0);
                ui.add(
                    TextEdit::singleline(&mut self.username)
                        .hint_text("Username")
                        .desired_width(input_w),
                );
                if theme::primary_button(ui, "Set", [btn_w, 21.0]).clicked() {
                    let name = self.username.trim().to_string();
                    if !name.is_empty() {
                        self.bot.spawn_flash_action(
                            "setGameObject",
                            vec![
                                FlashArg::from("world.myAvatar.pMC.pname.ti.text"),
                                FlashArg::from(name.as_str()),
                            ],
                        );
                        self.bot.spawn_flash_action(
                            "setGameObject",
                            vec![
                                FlashArg::from("ui.mcPortrait.strName.text"),
                                FlashArg::from(name.as_str()),
                            ],
                        );
                    }
                }
            });

            // Guild
            ui.horizontal(|ui| {
                let input_w = (ui.available_width() - btn_w - 5.0).max(80.0);
                ui.add(
                    TextEdit::singleline(&mut self.guild)
                        .hint_text("Guild")
                        .desired_width(input_w),
                );
                if theme::primary_button(ui, "Set", [btn_w, 21.0]).clicked() {
                    let guild = self.guild.trim();
                    let formatted = if guild.is_empty() {
                        String::new()
                    } else {
                        format!("< {guild} >")
                    };
                    self.bot.spawn_flash_action(
                        "setGameObject",
                        vec![
                            FlashArg::from("world.myAvatar.pMC.pname.tg.text"),
                            FlashArg::from(formatted.as_str()),
                        ],
                    );
                }
            });

            // Level + FPS compact split row
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                ui.add(
                    TextEdit::singleline(&mut self.level)
                        .hint_text("Lvl")
                        .desired_width(44.0),
                );
                if theme::primary_button(ui, "Set", [38.0, 21.0]).clicked()
                    && let Ok(lvl) = self.level.trim().parse::<i32>()
                {
                    let pkt = format!(
                        r#"{{"t":"xt","b":{{"r":-1,"o":{{"cmd":"levelUp","intExpToLevel":"2000","intLevel":{lvl}}}}}}}"#
                    );
                    let _ = self.bot.capture.send_to_client(&pkt);
                }

                ui.add_space(6.0);
                theme::num_input(ui, &mut self.fps, 1..=60, 44.0);
                ui.label(RichText::new("FPS").small().weak());
                if theme::primary_button(ui, "Set", [ui.available_width().max(38.0), 21.0])
                    .clicked()
                {
                    let fps = self.fps.clamp(1, 60);
                    let mut opts = self.bot.options.get();
                    opts.fps = fps;
                    self.bot.apply_options(opts);
                    self.bot
                        .spawn_flash_action("setFPS", vec![FlashArg::from(fps)]);
                }
            });
        });

        ui.add_space(4.0);

        // Bottom toggles row
        ui.horizontal(|ui| {
            if ui.checkbox(&mut self.mute_toggled, "Toggle Mute").changed() {
                let chat_val = if self.mute_toggled { 0 } else { 2 };
                self.bot.spawn_flash_action(
                    "setGameObject",
                    vec![FlashArg::from("chatF.iChat"), FlashArg::from(chat_val)],
                );
            }
            ui.add_space(8.0);
            if ui
                .checkbox(&mut self.gender_swapped, "Gender swap")
                .changed()
            {
                let uid = *self.bot.state.user_id.read();
                let gender = if self.gender_swapped { "F" } else { "M" };
                let pkt = format!(
                    r#"{{"t":"xt","b":{{"r":-1,"o":{{"cmd":"genderSwap","bitSuccess":1,"gender":"{gender}","intCoins":0,"uid":"{uid}","strHairFileName":"","HairID":"","strHairName":""}}}}}}"#
                );
                let _ = self.bot.capture.send_to_client(&pkt);
            }
        });
    }

    fn send_alignment(&self, value: i32) {
        let pkt = format!("%xt%zm%updateQuest%218701%41%{value}%");
        let _ = self.bot.capture.send_to_server(&pkt);
    }

    fn set_access_level(&self, level: &str) {
        match level {
            "Member" => {
                self.bot.spawn_flash_action(
                    "setGameObject",
                    vec![
                        FlashArg::from("world.myAvatar.objData.iUpgDays"),
                        FlashArg::from(999),
                    ],
                );
                self.bot.spawn_flash_action(
                    "setGameObject",
                    vec![
                        FlashArg::from("world.myAvatar.objData.intAccessLevel"),
                        FlashArg::from(10),
                    ],
                );
                self.bot.spawn_flash_action(
                    "setGameObject",
                    vec![
                        FlashArg::from("world.myAvatar.pMC.pname.ti.textColor"),
                        FlashArg::from(0x8CD5FF_i32),
                    ],
                );
            }
            "Non Member" => {
                self.bot.spawn_flash_action(
                    "setGameObject",
                    vec![
                        FlashArg::from("world.myAvatar.objData.iUpgDays"),
                        FlashArg::from(-1),
                    ],
                );
                self.bot.spawn_flash_action(
                    "setGameObject",
                    vec![
                        FlashArg::from("world.myAvatar.objData.intAccessLevel"),
                        FlashArg::from(10),
                    ],
                );
                self.bot.spawn_flash_action(
                    "setGameObject",
                    vec![
                        FlashArg::from("world.myAvatar.pMC.pname.ti.textColor"),
                        FlashArg::from(0xFFFFFF_i32),
                    ],
                );
            }
            "Moderator" => {
                self.bot.spawn_flash_action(
                    "setGameObject",
                    vec![
                        FlashArg::from("world.myAvatar.objData.intAccessLevel"),
                        FlashArg::from(60),
                    ],
                );
                self.bot.spawn_flash_action(
                    "setGameObject",
                    vec![
                        FlashArg::from("world.myAvatar.pMC.pname.ti.textColor"),
                        FlashArg::from(0xFECA38_i32),
                    ],
                );
            }
            _ => {}
        }
    }
}
