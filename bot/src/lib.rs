//! Grimlite Ri core bot engine (no Ruffle or UI dependencies).

pub mod capture;
pub mod host;
pub mod log;

use crate::capture::CaptureProxy;
use crate::host::FlashHost;
use crate::log::LogBuffer;
use parking_lot::RwLock;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

/// Embedded host SWF compiled from `host/as3/` via `cargo run -p grimlite_host`.
pub const HOST_SWF: &[u8] = include_bytes!("../assets/grimlite.swf");

/// Shared handle to a single Grimlite Ri bot instance.
pub struct Bot {
    host: RwLock<Option<Arc<dyn FlashHost>>>,
    pub capture: Arc<CaptureProxy>,
    pub logs: LogBuffer,
    game_loaded: AtomicBool,
    logged_in: AtomicBool,
    username: RwLock<String>,
}

impl Bot {
    pub fn new() -> Arc<Self> {
        let capture = Arc::new(CaptureProxy::default());
        // Enable capture by default so packet handlers & logger work immediately without reconnect.
        capture.start();
        Arc::new(Self {
            host: RwLock::new(None),
            capture,
            logs: LogBuffer::default(),
            game_loaded: AtomicBool::new(false),
            logged_in: AtomicBool::new(false),
            username: RwLock::new(String::new()),
        })
    }

    pub fn set_host(&self, host: Arc<dyn FlashHost>) {
        *self.host.write() = Some(host);
    }

    pub fn host(&self) -> Option<Arc<dyn FlashHost>> {
        self.host.read().clone()
    }

    pub fn is_game_loaded(&self) -> bool {
        self.game_loaded.load(Ordering::Acquire)
    }

    pub fn is_logged_in(&self) -> bool {
        self.logged_in.load(Ordering::Acquire)
    }

    pub fn username(&self) -> String {
        self.username.read().clone()
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
}
