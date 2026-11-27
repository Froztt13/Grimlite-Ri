//! Real-time DPS Meter driven directly by incoming AQW combat packets (`ct`) via `CaptureProxy`.

use crate::capture::{Interceptor, MessageInfo};
use parking_lot::Mutex;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

#[derive(Clone, Debug, Default)]
pub struct DpsSnapshot {
    pub active: bool,
    pub total_damage: i64,
    pub hits: u64,
    pub peak_hit: i64,
    pub elapsed_secs: f64,
    pub dps: f64,
}

#[derive(Default)]
struct DpsInner {
    started_at: Option<Instant>,
    total_damage: i64,
    hits: u64,
    peak_hit: i64,
}

pub struct DpsMeter {
    enabled: AtomicBool,
    inner: Mutex<DpsInner>,
}

impl Default for DpsMeter {
    fn default() -> Self {
        Self {
            enabled: AtomicBool::new(true),
            inner: Mutex::new(DpsInner::default()),
        }
    }
}

impl DpsMeter {
    pub fn is_enabled(&self) -> bool {
        self.enabled.load(Ordering::Relaxed)
    }

    pub fn set_enabled(&self, on: bool) {
        self.enabled.store(on, Ordering::Relaxed);
    }

    pub fn reset(&self) {
        *self.inner.lock() = DpsInner::default();
    }

    pub fn snapshot(&self) -> DpsSnapshot {
        let g = self.inner.lock();
        let elapsed = g
            .started_at
            .map(|t| t.elapsed().as_secs_f64())
            .unwrap_or(0.0);
        let dps = if elapsed > 0.1 {
            g.total_damage as f64 / elapsed
        } else {
            g.total_damage as f64
        };
        DpsSnapshot {
            active: self.is_enabled(),
            total_damage: g.total_damage,
            hits: g.hits,
            peak_hit: g.peak_hit,
            elapsed_secs: elapsed,
            dps,
        }
    }

    pub fn record_hit(&self, dmg: i64) {
        if dmg <= 0 || !self.is_enabled() {
            return;
        }
        let mut g = self.inner.lock();
        if g.started_at.is_none() {
            g.started_at = Some(Instant::now());
        }
        g.total_damage += dmg;
        g.hits += 1;
        if dmg > g.peak_hit {
            g.peak_hit = dmg;
        }
    }
}

/// Passive `CaptureProxy` interceptor that extracts combat damage numbers from `ct` packets.
pub struct DpsPacketInterceptor {
    meter: Arc<DpsMeter>,
}

impl DpsPacketInterceptor {
    pub fn new(meter: Arc<DpsMeter>) -> Self {
        Self { meter }
    }
}

impl Interceptor for DpsPacketInterceptor {
    fn priority(&self) -> i32 {
        -50
    }

    fn intercept(&self, message: &mut MessageInfo, outbound: bool) {
        if outbound || !message.send || !self.meter.is_enabled() {
            return;
        }
        let text = message.content.trim();
        if !text.starts_with('{') || !text.contains("\"ct\"") {
            return;
        }
        let Ok(v) = serde_json::from_str::<serde_json::Value>(text) else {
            return;
        };
        let Some(o) = v.get("b").and_then(|b| b.get("o")) else {
            return;
        };
        if o.get("cmd").and_then(|c| c.as_str()) != Some("ct") {
            return;
        }
        if let Some(actions) = o.get("a").and_then(|a| a.as_array()) {
            for act in actions {
                // Player-to-monster damage: cInf starts with "p:" and tInf starts with "m:"
                let caster = act.get("cInf").and_then(|s| s.as_str()).unwrap_or("");
                let target = act.get("tInf").and_then(|s| s.as_str()).unwrap_or("");
                if caster.starts_with("p:") && target.starts_with("m:") {
                    if let Some(hp) = act.get("hp").and_then(|h| h.as_i64()) {
                        if hp > 0 {
                            self.meter.record_hit(hp);
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_combat_damage_packets() {
        let meter = Arc::new(DpsMeter::default());
        let interceptor = DpsPacketInterceptor::new(meter.clone());
        let mut msg = MessageInfo::new(
            r#"{"t":"xt","b":{"r":-1,"o":{"cmd":"ct","a":[{"cInf":"p:123","tInf":"m:1","hp":4500},{"cInf":"m:1","tInf":"p:123","hp":200}]}}}"#,
        );
        interceptor.intercept(&mut msg, false);
        let snap = meter.snapshot();
        assert_eq!(snap.total_damage, 4500);
        assert_eq!(snap.hits, 1);
        assert_eq!(snap.peak_hit, 4500);
    }
}
