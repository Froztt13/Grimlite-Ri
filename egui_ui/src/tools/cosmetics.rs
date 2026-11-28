//! Compact Client-side Cosmetics tool window:
//! Alignment, Access level, Mute toggle, Username, Guild, Level, FPS, and Gender swap.

use crate::show_detached_window_sized;
use crate::theme;
use egui::{Context, RichText, TextEdit, Ui};
use grimlite_bot::Bot;
use grimlite_bot::host::FlashArg;
use std::sync::{Arc, Mutex};

#[derive(Default)]
struct SyncedCosmetics {
    is_unmuted: Option<bool>,
    is_female: Option<bool>,
}

pub struct CosmeticsWindow {
    bot: Arc<Bot>,
    pub open: bool,
    was_open: bool,
    last_synced_user: String,
    is_unmuted: bool,
    is_female: bool,
    mute_touched: bool,
    gender_touched: bool,
    pending_sync: Arc<Mutex<Option<SyncedCosmetics>>>,
    username: String,
    guild: String,
    level: String,
}

impl CosmeticsWindow {
    pub fn new(bot: Arc<Bot>) -> Self {
        Self {
            bot,
            open: false,
            was_open: false,
            last_synced_user: String::new(),
            is_unmuted: true,
            is_female: false,
            mute_touched: false,
            gender_touched: false,
            pending_sync: Arc::new(Mutex::new(None)),
            username: String::new(),
            guild: String::new(),
            level: String::new(),
        }
    }

    fn trigger_sync(&mut self) {
        let bot = Arc::clone(&self.bot);
        let slot = Arc::clone(&self.pending_sync);
        let _ = std::thread::Builder::new()
            .name("grimlite-cosmetics-sync".into())
            .spawn(move || {
                let Some(flash) = bot.flash() else {
                    return;
                };
                let is_unmuted = flash
                    .call_str("getGameObject", &[FlashArg::from("chatF.iChat")])
                    .ok()
                    .and_then(|raw| parse_ichat_unmuted(&raw));
                let is_female = flash
                    .call_str(
                        "getGameObject",
                        &[FlashArg::from("world.myAvatar.objData.strGender")],
                    )
                    .ok()
                    .and_then(|raw| parse_gender_female(&raw));
                if let Ok(mut guard) = slot.lock() {
                    *guard = Some(SyncedCosmetics {
                        is_unmuted,
                        is_female,
                    });
                }
            });
    }

    pub fn show(&mut self, ctx: &Context) {
        if !self.open {
            self.was_open = false;
            return;
        }

        // Sync initial Mute/Unmute and Male/Female state from Flash whenever the window opens
        // or the logged-in player changes.
        let snap = self.bot.player_snapshot();
        let just_opened = !self.was_open;
        self.was_open = true;
        if snap.logged_in && (just_opened || snap.username != self.last_synced_user) {
            self.last_synced_user = snap.username.clone();
            self.mute_touched = false;
            self.gender_touched = false;
            self.trigger_sync();
        } else if !snap.logged_in {
            self.last_synced_user.clear();
        }

        if let Ok(mut guard) = self.pending_sync.lock()
            && let Some(synced) = guard.take()
        {
            if let Some(unmuted) = synced.is_unmuted
                && !self.mute_touched
            {
                self.is_unmuted = unmuted;
            }
            if let Some(female) = synced.is_female
                && !self.gender_touched
            {
                self.is_female = female;
            }
        }

        let mut open = self.open;
        show_detached_window_sized(
            ctx,
            "grimlite_cosmetics",
            "Cosmetics",
            [285.0, 248.0],
            Some([265.0, 232.0]),
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

            // Level
            ui.horizontal(|ui| {
                let input_w = (ui.available_width() - btn_w - 5.0).max(80.0);
                ui.add(
                    TextEdit::singleline(&mut self.level)
                        .hint_text("Level")
                        .desired_width(input_w),
                );
                if theme::primary_button(ui, "Set", [btn_w, 21.0]).clicked()
                    && let Ok(lvl) = self.level.trim().parse::<i32>()
                {
                    let pkt = format!(
                        r#"{{"t":"xt","b":{{"r":-1,"o":{{"cmd":"levelUp","intExpToLevel":"2000","intLevel":{lvl}}}}}}}"#
                    );
                    let _ = self.bot.capture.send_to_client(&pkt);
                }
            });
        });

        ui.add_space(4.0);

        // Bottom segmented switches: Mute / Unmute and Male / Female
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            let half = ((ui.available_width() - 6.0) * 0.5).max(108.0);

            let prev_unmuted = self.is_unmuted;
            theme::segmented_switch(ui, &mut self.is_unmuted, "Mute", "Unmute", half)
                .on_hover_text("Toggle client-side chat mute (chatF.iChat)");
            if self.is_unmuted != prev_unmuted {
                self.mute_touched = true;
                let chat_val = if self.is_unmuted { 2 } else { 0 };
                self.bot.spawn_flash_action(
                    "setGameObject",
                    vec![FlashArg::from("chatF.iChat"), FlashArg::from(chat_val)],
                );
            }

            let prev_female = self.is_female;
            theme::segmented_switch(ui, &mut self.is_female, "Male", "Female", half)
                .on_hover_text("Client-side gender swap");
            if self.is_female != prev_female {
                self.gender_touched = true;
                let uid = *self.bot.state.user_id.read();
                let gender = if self.is_female { "F" } else { "M" };
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

fn parse_ichat_unmuted(raw: &str) -> Option<bool> {
    let cleaned = raw.trim().trim_matches('"').trim();
    cleaned.parse::<i32>().ok().map(|v| v > 0)
}

fn parse_gender_female(raw: &str) -> Option<bool> {
    let cleaned = raw.trim().trim_matches('"').trim();
    if cleaned.eq_ignore_ascii_case("F") {
        Some(true)
    } else if cleaned.eq_ignore_ascii_case("M") {
        Some(false)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_flash_chat_mute_and_gender_values() {
        assert_eq!(parse_ichat_unmuted("0"), Some(false));
        assert_eq!(parse_ichat_unmuted("\"0\""), Some(false));
        assert_eq!(parse_ichat_unmuted("2"), Some(true));
        assert_eq!(parse_ichat_unmuted("1"), Some(true));
        assert_eq!(parse_ichat_unmuted("null"), None);

        assert_eq!(parse_gender_female("M"), Some(false));
        assert_eq!(parse_gender_female("\"M\""), Some(false));
        assert_eq!(parse_gender_female("F"), Some(true));
        assert_eq!(parse_gender_female("\"f\""), Some(true));
        assert_eq!(parse_gender_female(""), None);
    }
}
