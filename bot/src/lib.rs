//! Grimlite Ri core bot engine (no Ruffle or UI dependencies).

pub mod capture;
pub mod flash;
pub mod gbot;
pub mod host;
pub mod log;
pub mod options;
pub mod runner;
pub mod spammer;
pub mod state;
pub mod tamper;

use crate::capture::CaptureProxy;
use crate::flash::Flash;
use crate::gbot::BotConfig;
use crate::host::{FlashArg, FlashHost};
use crate::log::LogBuffer;
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
    game_loaded: AtomicBool,
    worker_started: AtomicBool,
}

impl Bot {
    pub fn new() -> Arc<Self> {
        let capture = Arc::new(CaptureProxy::default());
        let logs = Arc::new(LogBuffer::default());
        let state = Arc::new(GameState::default());
        let tamper = Arc::new(TamperEngine::default());

        capture.add_interceptor(Arc::new(StatePacketInterceptor::new(
            state.clone(),
            logs.clone(),
        )));
        capture.add_interceptor(tamper.clone());
        capture.start();

        Arc::new(Self {
            host: RwLock::new(None),
            capture,
            logs,
            state,
            options: Arc::new(OptionsStore::default()),
            tamper,
            spammer: Arc::new(PacketSpammer::default()),
            runner: Arc::new(BotRunner::default()),
            game_loaded: AtomicBool::new(false),
            worker_started: AtomicBool::new(false),
        })
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

    pub fn start_bot(self: &Arc<Self>) {
        self.runner.start(Arc::clone(self));
    }

    pub fn stop_bot(&self) {
        self.runner.stop();
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
                    let Some(flash) = bot.flash() else {
                        continue;
                    };

                    // Poll batched player state
                    if let Ok(snap) = flash.call_json::<PlayerSnapshot>("getPlayerState", &[]) {
                        bot.state.update_player(snap);
                    }

                    // Enforce changed options
                    let desired = bot.options.get();
                    if applied.as_ref() != Some(&desired) {
                        let _ = flash.call_bool(
                            "setInfiniteRange",
                            &[FlashArg::Bool(desired.infinite_range)],
                        );
                        let _ = flash.call_bool(
                            "setEnemyMagnet",
                            &[FlashArg::Bool(desired.enemy_magnet)],
                        );
                        let _ =
                            flash.call_bool("setLagKiller", &[FlashArg::Bool(desired.lag_killer)]);
                        let _ = flash.call_bool(
                            "setDisableAnimations",
                            &[FlashArg::Bool(desired.disable_animations)],
                        );
                        let _ = flash.call_bool(
                            "setSkipCutscenes",
                            &[FlashArg::Bool(desired.skip_cutscenes)],
                        );
                        let _ =
                            flash.call_bool("setWalkSpeed", &[FlashArg::from(desired.walk_speed)]);
                        let _ = flash.call_bool("setFPS", &[FlashArg::from(desired.fps)]);
                        applied = Some(desired.clone());
                    }

                    // Periodic provoke monsters (every ~1s while logged in)
                    if desired.provoke_monsters
                        && tick.is_multiple_of(2)
                        && bot.state.snapshot().logged_in
                    {
                        let _ = flash.call_bool("aggroMonsters", &[]);
                    }
                }
            });
    }
}
