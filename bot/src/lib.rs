//! Grimlite Ri core bot engine (no Ruffle or UI dependencies).

pub mod commands;
pub mod engine;
pub mod game;
pub mod log;
pub mod network;

// Convenience re-exports for clean crate-root imports
pub use engine::{auto_attack, gbot, maid, options, runner};
pub use game::{flash, grabber, host, state};
pub use network::{capture, handlers, spammer, tamper};

use crate::auto_attack::AutoAttack;
use crate::capture::CaptureProxy;
use crate::flash::Flash;
use crate::gbot::BotConfig;
use crate::handlers::{AntiCounterHandler, MaidPacketRouter, SpecialHandlerManager};
use crate::host::{FlashArg, FlashHost};
use crate::log::LogBuffer;
use crate::maid::MaidEngine;
use crate::options::{BotOptions, OptionsStore};
use crate::runner::BotRunner;
use crate::spammer::PacketSpammer;
use crate::state::{GameState, PlayerSnapshot, StatePacketInterceptor};
use crate::tamper::TamperEngine;
use parking_lot::RwLock;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

/// Embedded host SWF compiled from `host/as3/` via `cargo run -p grimlite_host`.
pub const HOST_SWF: &[u8] = include_bytes!("../assets/grimlite.swf");

/// Shared handle to a single Grimlite Ri bot instance.
pub struct Bot {
    host: RwLock<Option<Arc<dyn FlashHost>>>,
    pub capture: Arc<CaptureProxy>,
    pub logs: Arc<LogBuffer>,
    pub state: Arc<GameState>,
    pub options: Arc<OptionsStore>,
    pub tamper: Arc<TamperEngine>,
    pub spammer: Arc<PacketSpammer>,
    pub runner: Arc<BotRunner>,
    pub auto_attack: Arc<AutoAttack>,
    pub maid: Arc<MaidEngine>,
    pub anti_counter: Arc<AntiCounterHandler>,
    pub special_handler: Arc<SpecialHandlerManager>,
    game_loaded: AtomicBool,
    worker_started: AtomicBool,
}

impl Bot {
    pub fn new() -> Arc<Self> {
        let capture = Arc::new(CaptureProxy::default());
        let logs = Arc::new(LogBuffer::default());
        let state = Arc::new(GameState::default());
        let tamper = Arc::new(TamperEngine::default());
        let anti_counter = Arc::new(AntiCounterHandler::new(logs.clone()));
        let special_handler = Arc::new(SpecialHandlerManager::new(logs.clone()));
        let maid_router = Arc::new(MaidPacketRouter::default());

        capture.add_interceptor(Arc::new(StatePacketInterceptor::new(
            state.clone(),
            logs.clone(),
        )));
        capture.add_interceptor(anti_counter.clone());
        capture.add_interceptor(special_handler.clone());
        capture.add_interceptor(maid_router.clone());
        capture.add_interceptor(tamper.clone());
        capture.start();

        let bot = Arc::new(Self {
            host: RwLock::new(None),
            capture,
            logs,
            state,
            options: Arc::new(OptionsStore::default()),
            tamper,
            spammer: Arc::new(PacketSpammer::default()),
            runner: Arc::new(BotRunner::default()),
            auto_attack: Arc::new(AutoAttack::default()),
            maid: Arc::new(MaidEngine::default()),
            anti_counter: anti_counter.clone(),
            special_handler: special_handler.clone(),
            game_loaded: AtomicBool::new(false),
            worker_started: AtomicBool::new(false),
        });

        let weak_bot = Arc::downgrade(&bot);
        special_handler.attach_bot(weak_bot.clone());
        maid_router.attach_bot(weak_bot.clone());
        anti_counter.set_on_counter_active(Arc::new(move || {
            if let Some(b) = weak_bot.upgrade() {
                let enabled = b.auto_attack.is_running()
                    || b.options.get().anti_counter
                    || (b.runner.is_running() && b.runner.config().anti_counter);
                if enabled {
                    b.spawn_flash_action("cancelTarget", vec![]);
                }
            }
        }));

        bot
    }

    /// Runs a one-off Flash call on a background thread so the UI thread never blocks.
    pub fn spawn_flash_action(self: &Arc<Self>, name: &'static str, args: Vec<FlashArg>) {
        let bot = Arc::clone(self);
        let _ = std::thread::Builder::new()
            .name("grimlite-action".into())
            .spawn(move || {
                if let Some(flash) = bot.flash() {
                    if let Err(e) = flash.call_str(name, &args) {
                        bot.logs.push_debug(format!("[Action {name}] {e}"));
                    }
                }
            });
    }

    /// Queries the current map's frame labels from Flash (`getCells`) and stores them in `GameState`.
    pub fn refresh_map_cells(self: &Arc<Self>) {
        let bot = Arc::clone(self);
        let _ = std::thread::Builder::new()
            .name("grimlite-cells".into())
            .spawn(move || {
                if let Some(flash) = bot.flash()
                    && let Ok(raw) = flash.call_str("getCells", &[])
                    && let Ok(parsed) = serde_json::from_str::<Vec<String>>(&raw)
                    && !parsed.is_empty()
                {
                    *bot.state.map_cells.write() = parsed;
                }
            });
    }

    pub fn load_gbot_file(&self, path: &std::path::Path) -> Result<(), String> {
        let raw = std::fs::read_to_string(path).map_err(|e| format!("read error: {e}"))?;
        let cfg = BotConfig::from_gbot_json(&raw).map_err(|e| format!("parse error: {e}"))?;
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("bot.gbot")
            .to_string();
        self.logs.push_debug(format!(
            "[Bot] Loaded `{name}` ({} commands, {} drops)",
            cfg.commands.len(),
            cfg.drops.len()
        ));
        self.runner.load_config(name, cfg);
        Ok(())
    }

    pub fn save_gbot_file(&self, path: &std::path::Path) -> Result<(), String> {
        let cfg = self.runner.config();
        let json = cfg
            .to_gbot_json()
            .map_err(|e| format!("serialize error: {e}"))?;
        std::fs::write(path, json).map_err(|e| format!("write error: {e}"))?;
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("bot.gbot")
            .to_string();
        self.logs.push_debug(format!("[Bot] Saved `{name}`"));
        Ok(())
    }

    pub fn start_bot(self: &Arc<Self>) {
        if !self.is_logged_in() {
            return;
        }
        self.runner.start(Arc::clone(self));
    }

    pub fn stop_bot(&self) {
        let was_running = self.runner.is_running();
        self.runner.stop();
        if was_running && self.is_logged_in() {
            let snap = self.state.snapshot();
            let cell = snap.cell;
            let pad = if snap.pad.is_empty() {
                "Left".to_string()
            } else {
                snap.pad
            };
            if let Some(flash) = self.flash() {
                let _ = std::thread::Builder::new()
                    .name("grimlite-stop-disengage".into())
                    .spawn(move || {
                        let _ = flash.call_str("CancelAutoAttack", &[]);
                        let _ = flash.call_str("CancelTarget", &[]);
                        if !cell.is_empty() {
                            let _ = flash.call_str(
                                "Jump",
                                &[FlashArg::from(cell.as_str()), FlashArg::from(pad.as_str())],
                            );
                        }
                    });
            }
        }
    }

    pub fn is_logged_in(&self) -> bool {
        self.state.snapshot().logged_in
    }

    pub fn set_host(self: &Arc<Self>, host: Arc<dyn FlashHost>) {
        *self.host.write() = Some(host);
        if !self.worker_started.swap(true, Ordering::AcqRel) {
            self.start_background_worker();
        }
    }

    pub fn flash(&self) -> Option<Flash> {
        self.host.read().clone().map(Flash::new)
    }

    pub fn is_game_loaded(&self) -> bool {
        self.game_loaded.load(Ordering::Acquire)
    }

    pub fn set_game_loaded(&self, loaded: bool) {
        self.game_loaded.store(loaded, Ordering::Release);
    }

    pub fn player_snapshot(&self) -> PlayerSnapshot {
        self.state.snapshot()
    }

    pub fn apply_options(&self, opts: BotOptions) {
        self.options.set(opts);
    }

    pub fn start_spammer(&self, packets: Vec<String>, delay_ms: u64) {
        self.spammer.set_packets(packets);
        self.spammer.set_delay_ms(delay_ms);
        self.spammer.start(self.capture.clone(), self.logs.clone());
    }

    pub fn start_spammer_queue(&self, packets: Vec<crate::spammer::QueuedPacket>, delay_ms: u64) {
        self.spammer.set_queued_packets(packets);
        self.spammer.set_delay_ms(delay_ms);
        self.spammer.start(self.capture.clone(), self.logs.clone());
    }

    pub fn stop_spammer(&self) {
        self.spammer.stop();
    }

    /// Called by Ruffle's `ExternalInterfaceProvider` while the Player is locked.
    /// Must NEVER block or call back into Flash.
    pub fn on_flash_call(&self, name: &str, args: &[String]) -> Option<String> {
        match name {
            "requestLoadGame" => {
                self.logs.push_debug("Host SWF ready (requestLoadGame)");
                None
            }
            "pre-load" => {
                self.logs.push_debug("Fetching AQW game client...");
                None
            }
            "loaded" => {
                self.game_loaded.store(true, Ordering::Release);
                self.logs.push_debug("AQW Game.swf loaded");
                None
            }
            "playerState" => {
                if let Some(raw) = args.first()
                    && let Ok(snap) = serde_json::from_str::<PlayerSnapshot>(raw)
                {
                    let logged_in = snap.logged_in;
                    self.state.update_player(snap);
                    if !logged_in {
                        self.auto_attack.stop_on_logout(self);
                        if self.runner.is_running() && !self.runner.config().auto_relogin {
                            self.logs.push_debug(
                                "[Bot] Stopped (logged out, auto-relogin disabled)",
                            );
                            self.stop_bot();
                        }
                    }
                }
                None
            }
            "debug" => {
                if let Some(msg) = args.first() {
                    self.logs.push_debug(msg.clone());
                }
                None
            }
            _ => None,
        }
    }

    fn start_background_worker(self: &Arc<Self>) {
        let bot = Arc::clone(self);
        let _ = std::thread::Builder::new()
            .name("grimlite-worker".into())
            .spawn(move || {
                let mut applied: Option<BotOptions> = None;
                let mut tick: u64 = 0;
                loop {
                    std::thread::sleep(Duration::from_millis(500));
                    tick = tick.wrapping_add(1);
                    if !bot.is_game_loaded() {
                        continue;
                    }
                    let desired = bot.options.get();
                    let options_changed = applied.as_ref() != Some(&desired);
                    let need_provoke = desired.provoke_monsters
                        && tick.is_multiple_of(2)
                        && bot.state.snapshot().logged_in;
                    let need_follow = desired.follow_enabled
                        && tick.is_multiple_of(4)
                        && bot.state.snapshot().logged_in;

                    // Fast path: player state and walk speed are maintained inside AS3 `onFrame`
                    // (`playerState` push-on-change + `optWalkSpeedEnabled`), so when no options
                    // changed and neither provoke nor follow is active, zero main-thread Flash
                    // calls or event-loop wakeups occur!
                    if !options_changed && !need_provoke && !need_follow {
                        continue;
                    }

                    let Some(flash) = bot.flash() else {
                        continue;
                    };

                    // Enforce changed options
                    if options_changed {
                        let image_changed = applied.as_ref().is_none_or(|a| {
                            a.lag_killer_image != desired.lag_killer_image
                                || a.lag_killer_image_rev != desired.lag_killer_image_rev
                        });
                        if image_changed {
                            match &desired.lag_killer_image {
                                Some(path) if !path.trim().is_empty() => {
                                    match std::fs::read(path) {
                                        Ok(bytes) => {
                                            let b64 = encode_base64(&bytes);
                                            let _ = flash.call_str(
                                                "setLagKillerImage",
                                                &[FlashArg::from(b64)],
                                            );
                                        }
                                        Err(e) => {
                                            bot.logs.push_debug(format!(
                                                "[LagKiller] Failed to read `{path}`: {e}"
                                            ));
                                        }
                                    }
                                }
                                _ => {
                                    let _ = flash
                                        .call_str("setLagKillerImage", &[FlashArg::from("")]);
                                }
                            }
                        }
                        if desired.infinite_range {
                            let _ = flash.call_bool(
                                "setInfiniteRange",
                                &[FlashArg::Bool(true)],
                            );
                        }
                        let _ = flash
                            .call_bool("setEnemyMagnet", &[FlashArg::Bool(desired.enemy_magnet)]);
                        let _ =
                            flash.call_bool("setLagKiller", &[FlashArg::Bool(desired.lag_killer)]);
                        let _ = flash.call_bool(
                            "setHidePlayers",
                            &[FlashArg::Bool(desired.hide_players)],
                        );
                        let _ = flash.call_bool(
                            "setDisableAnimations",
                            &[FlashArg::Bool(desired.disable_animations)],
                        );
                        let _ = flash.call_bool(
                            "setSkipCutscenes",
                            &[FlashArg::Bool(desired.skip_cutscenes)],
                        );
                        let _ = flash.call_bool(
                            "setWalkSpeedOpt",
                            &[
                                FlashArg::Bool(desired.walk_speed_enabled),
                                FlashArg::from(desired.walk_speed.clamp(1, 99)),
                            ],
                        );
                        let _ = flash.call_bool("setFPS", &[FlashArg::from(desired.fps)]);
                        applied = Some(desired.clone());
                    }

                    // Periodic provoke monsters (every ~1s while logged in)
                    if need_provoke {
                        let _ = flash.call_bool("aggroMonsters", &[]);
                    }

                    // Periodic Follow Player check (every ~2s while logged in, mirrors BotManager.cs setFollowHandler)
                    if need_follow {
                        let target = desired.follow_player.trim().to_ascii_lowercase();
                        if !target.is_empty() && target != "player name" {
                            let in_map = flash
                                .call_str("PlayersInMap", &[])
                                .ok()
                                .and_then(|raw| serde_json::from_str::<Vec<String>>(&raw).ok())
                                .map(|list| {
                                    list.iter().any(|p| p.trim().eq_ignore_ascii_case(&target))
                                })
                                .unwrap_or(true);
                            if !in_map {
                                let _ = flash.call_bool("cancelTarget", &[]);
                                let _ = flash.call_bool(
                                    "moveToCell",
                                    &[FlashArg::from("Blank"), FlashArg::from("Spawn")],
                                );
                                std::thread::sleep(Duration::from_millis(600));
                                let _ = flash.call_str("GoTo", &[FlashArg::from(target.as_str())]);
                            }
                        }
                    }
                }
            });
    }
}

fn encode_base64(input: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(input.len().div_ceil(3) * 4);
    for chunk in input.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = if chunk.len() > 1 { chunk[1] as u32 } else { 0 };
        let b2 = if chunk.len() > 2 { chunk[2] as u32 } else { 0 };
        let triple = (b0 << 16) | (b1 << 8) | b2;
        out.push(TABLE[((triple >> 18) & 0x3F) as usize] as char);
        out.push(TABLE[((triple >> 12) & 0x3F) as usize] as char);
        if chunk.len() > 1 {
            out.push(TABLE[((triple >> 6) & 0x3F) as usize] as char);
        } else {
            out.push('=');
        }
        if chunk.len() > 2 {
            out.push(TABLE[(triple & 0x3F) as usize] as char);
        } else {
            out.push('=');
        }
    }
    out
}
