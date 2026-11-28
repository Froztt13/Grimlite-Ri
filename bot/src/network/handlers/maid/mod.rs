//! Maid Remake packet handlers mirroring `Grimlite-Li/Grimoire/Networking/Handlers/Maid/`.
//! Each handler owns its own packet parsing and logic; `MaidPacketRouter` dispatches inbound traffic.

pub mod anims;
pub mod cell_jumper;
pub mod copy_walk;
pub mod join_map;
pub mod party;
pub mod warning_msg;

use crate::Bot;
use crate::capture::{Interceptor, MessageInfo};
use parking_lot::RwLock;
use std::sync::Weak;

/// Central `CaptureProxy` interceptor for all Maid Remake packet handlers.
#[derive(Default)]
pub struct MaidPacketRouter {
    bot: RwLock<Option<Weak<Bot>>>,
}

impl MaidPacketRouter {
    pub fn attach_bot(&self, weak: Weak<Bot>) {
        *self.bot.write() = Some(weak);
    }
}

impl Interceptor for MaidPacketRouter {
    fn intercept(&self, message: &mut MessageInfo, outbound: bool) {
        if outbound {
            return;
        }
        let Some(bot) = self.bot.read().as_ref().and_then(Weak::upgrade) else {
            return;
        };

        let cfg = bot.maid.config();
        let running = bot.maid.is_running();

        // Party invite auto-accept and /p party commands are always enabled.
        party::handle_party_invitation(&bot, &message.content);
        party::handle_party_chat(&bot, &message.content);

        if !running {
            return;
        }

        let raw = &message.content;
        if raw.starts_with("%xt%") {
            let parts: Vec<&str> = raw.split('%').collect();
            let cmd = parts.get(2).copied().unwrap_or_default();
            match cmd {
                "uotls" => {
                    cell_jumper::handle(&bot, &cfg, &parts);
                    if cfg.copy_walk {
                        copy_walk::handle(&bot, &cfg, &parts);
                    }
                }
                "exitArea" => {
                    // %xt%exitArea%-1%<uid>%<username>%
                    let target = cfg.target.trim();
                    if !target.is_empty()
                        && !bot.maid.is_unfollow()
                        && !bot.maid.is_on_pause()
                        && parts
                            .get(5)
                            .is_some_and(|u| u.trim().eq_ignore_ascii_case(target))
                    {
                        let target_name = target.to_string();
                        let b = std::sync::Arc::clone(&bot);
                        let _ = std::thread::Builder::new()
                            .name("maid-follow-map".into())
                            .spawn(move || {
                                if let Some(flash) = b.flash() {
                                    let _ = flash.call_str("CancelAutoAttack", &[]);
                                    let _ = flash.call_str("CancelTarget", &[]);
                                    let snap = b.player_snapshot();
                                    if !snap.cell.is_empty() {
                                        let pad = if snap.pad.is_empty() {
                                            "Left"
                                        } else {
                                            snap.pad.as_str()
                                        };
                                        let _ = flash.call_str(
                                            "moveToCell",
                                            &[
                                                crate::host::FlashArg::from(snap.cell.as_str()),
                                                crate::host::FlashArg::from(pad),
                                            ],
                                        );
                                    }
                                }
                                std::thread::sleep(std::time::Duration::from_millis(250));
                                let pkt = format!("%xt%zm%cmd%1%goto%{target_name}%");
                                let _ = b.capture.send_to_server(&pkt);
                            });
                    }
                }
                "server" => {
                    join_map::handle(&bot, &cfg, raw);
                }
                "warning" => {
                    warning_msg::handle(&bot, &cfg, &parts);
                }
                _ => {}
            }
        } else if raw.trim_start().starts_with('{') {
            anims::handle(&bot, &cfg, raw);
        }
    }
}
