//! Standalone Auto Attack loop toggled directly from the top navigation bar.
//! Integrates `AntiCounterHandler` (pauses combat during Counter Attack) and only casts
//! skills when an active monster target exists.

use crate::Bot;
use crate::host::FlashArg;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

#[derive(Default)]
pub struct AutoAttack {
    running: Arc<AtomicBool>,
}

impl AutoAttack {
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::Acquire)
    }

    pub fn toggle(&self, bot: Arc<Bot>) {
        if self.is_running() {
            self.stop(bot);
        } else {
            self.start(bot);
        }
    }

    pub fn start(&self, bot: Arc<Bot>) {
        if !bot.is_logged_in() {
            return;
        }
        if self.running.swap(true, Ordering::AcqRel) {
            return;
        }
        bot.anti_counter.reset();
        let running = self.running.clone();
        let _ = std::thread::Builder::new()
            .name("grimlite-auto-attack".into())
            .spawn(move || {
                bot.logs.push_debug("[Auto Attack] Started");
                let mut skill_idx = 0usize;
                while running.load(Ordering::Acquire) {
                    if !bot.is_logged_in() {
                        running.store(false, Ordering::Release);
                        bot.anti_counter.reset();
                        bot.logs.push_debug("[Auto Attack] Stopped (logged out)");
                        return;
                    }

                    if let Some(flash) = bot.flash() {
                        // Pause completely while monster's Counter Attack aura is active
                        if bot.anti_counter.is_paused() {
                            if flash.call_bool("hasTarget", &[]).unwrap_or(false) {
                                let _ = flash.call_bool("cancelTarget", &[]);
                            }
                            sleep_step(100, &running);
                            continue;
                        }

                        // Acquire a target first; never spam skills if no monster is targeted
                        let mut has_target = flash.call_bool("hasTarget", &[]).unwrap_or(false);
                        if !has_target {
                            let attacked = flash
                                .call_bool("attackMonster", &[FlashArg::from("*")])
                                .unwrap_or(false);
                            if attacked {
                                has_target = flash.call_bool("hasTarget", &[]).unwrap_or(false);
                            }
                        }

                        if has_target {
                            let key = ((skill_idx % 4) + 1).to_string();
                            skill_idx = skill_idx.wrapping_add(1);
                            let _ = flash.call_bool("useSkill", &[FlashArg::from(key)]);
                        }
                    }

                    sleep_step(100, &running);
                }
                bot.anti_counter.reset();
                bot.logs.push_debug("[Auto Attack] Stopped");
            });
    }

    pub fn stop(&self, bot: Arc<Bot>) {
        if self.running.swap(false, Ordering::AcqRel) {
            bot.anti_counter.reset();
            bot.spawn_flash_action("cancelTarget", vec![]);
        }
    }

    pub fn stop_on_logout(&self, bot: &Bot) {
        if self.running.swap(false, Ordering::AcqRel) {
            bot.anti_counter.reset();
            bot.logs.push_debug("[Auto Attack] Stopped (logged out)");
        }
    }
}

fn sleep_step(ms: u64, running: &AtomicBool) {
    let mut slept = 0u64;
    while slept < ms && running.load(Ordering::Acquire) {
        let step = (ms - slept).min(50);
        std::thread::sleep(Duration::from_millis(step));
        slept += step;
    }
}
