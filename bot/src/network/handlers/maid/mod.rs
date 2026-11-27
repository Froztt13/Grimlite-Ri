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
