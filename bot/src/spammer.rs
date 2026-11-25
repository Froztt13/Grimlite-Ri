//! Packet Spammer sending queued packets directly through `CaptureProxy::send_to_server`.

use crate::capture::CaptureProxy;
use crate::log::LogBuffer;
use parking_lot::RwLock;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Duration;

pub struct PacketSpammer {
    running: Arc<AtomicBool>,
    delay_ms: Arc<AtomicU64>,
    packets: Arc<RwLock<Vec<String>>>,
}

impl Default for PacketSpammer {
    fn default() -> Self {
        Self {
            running: Arc::new(AtomicBool::new(false)),
            delay_ms: Arc::new(AtomicU64::new(1000)),
            packets: Arc::new(RwLock::new(Vec::new())),
        }
    }
}

impl PacketSpammer {
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::Acquire)
    }

    pub fn delay_ms(&self) -> u64 {
        self.delay_ms.load(Ordering::Relaxed)
    }

    pub fn set_delay_ms(&self, ms: u64) {
        self.delay_ms.store(ms.max(50), Ordering::Relaxed);
    }

    pub fn set_packets(&self, list: Vec<String>) {
        *self.packets.write() = list
            .into_iter()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
    }

    pub fn start(&self, capture: Arc<CaptureProxy>, logs: Arc<LogBuffer>) {
        if self.running.swap(true, Ordering::AcqRel) {
            return;
        }
        let running = self.running.clone();
        let delay_ms = self.delay_ms.clone();
        let packets = self.packets.clone();
        std::thread::Builder::new()
            .name("grimlite-spammer".into())
            .spawn(move || {
                logs.push_debug("Packet Spammer started");
                let mut idx = 0usize;
                while running.load(Ordering::Acquire) {
                    let next = {
                        let list = packets.read();
                        if list.is_empty() {
                            None
                        } else {
                            let item = list[idx % list.len()].clone();
                            idx = idx.wrapping_add(1);
                            Some(item)
                        }
                    };
                    if let Some(pkt) = next
                        && let Err(e) = capture.send_to_server(&pkt)
                    {
                        logs.push_debug(format!("[Spammer] {e}"));
                    }
                    let wait = delay_ms.load(Ordering::Relaxed).max(50);
                    let mut slept = 0u64;
                    while slept < wait && running.load(Ordering::Acquire) {
                        let step = (wait - slept).min(50);
                        std::thread::sleep(Duration::from_millis(step));
                        slept += step;
                    }
                }
                logs.push_debug("Packet Spammer stopped");
            })
            .ok();
    }

    pub fn stop(&self) {
        self.running.store(false, Ordering::Release);
    }
}
