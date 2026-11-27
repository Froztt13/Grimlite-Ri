//! Anti-Counter Attack packet handler (mirrors Grimlite-Li `CmdKill.AntiCounterHandler`).
//! Listens directly to incoming AQW server JSON packets via Ruffle's `CaptureProxy`.

use crate::capture::{Interceptor, MessageInfo};
use crate::log::LogBuffer;
use parking_lot::RwLock;
use serde_json::Value;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

type TriggerCallback = Arc<dyn Fn() + Send + Sync>;

pub struct AntiCounterHandler {
    paused: AtomicBool,
    logs: Arc<LogBuffer>,
    on_counter_active: RwLock<Option<TriggerCallback>>,
}

impl AntiCounterHandler {
    pub fn new(logs: Arc<LogBuffer>) -> Self {
        Self {
            paused: AtomicBool::new(false),
            logs,
            on_counter_active: RwLock::new(None),
        }
    }

    /// Registers a non-blocking callback invoked when a monster prepares a counter attack.
    pub fn set_on_counter_active(&self, cb: TriggerCallback) {
        *self.on_counter_active.write() = Some(cb);
    }

    /// Returns `true` while a monster's Counter Attack aura is active and combat should pause.
    pub fn is_paused(&self) -> bool {
        self.paused.load(Ordering::Acquire)
    }

    /// Resets paused state (e.g. when starting/stopping combat or changing maps).
    pub fn reset(&self) {
        self.paused.store(false, Ordering::Release);
    }

    fn handle_json(&self, root: &Value) {
        let Some(data) = root.pointer("/b/o") else {
            return;
        };

        // 1. Detect counter attack warning in `anims`
        if let Some(anims) = data.get("anims").and_then(Value::as_array) {
            for anim in anims {
                if let Some(msg) = anim.get("msg").and_then(Value::as_str)
                    && msg.to_ascii_lowercase().contains("prepares a counter attack")
                {
                    let was_paused = self.paused.swap(true, Ordering::AcqRel);
                    if !was_paused {
                        self.logs.push_debug("[AntiCounter] Counter Attack: active");
                    }
                    if let Some(cb) = self.on_counter_active.read().as_ref() {
                        cb();
                    }
                }
            }
        }

        // 2. Detect counter attack fade in `a` (auras)
        if let Some(auras) = data.get("a").and_then(Value::as_array) {
            for entry in auras {
                let name = entry
                    .get("aura")
                    .and_then(|a| a.get("nam"))
                    .and_then(Value::as_str);
                let cmd = entry.get("cmd").and_then(Value::as_str);
                if name == Some("Counter Attack") && cmd.is_some_and(|c| c.contains("aura-")) {
                    if self.paused.swap(false, Ordering::AcqRel) {
                        self.logs.push_debug("[AntiCounter] Counter Attack: fades");
                    }
                    break;
                }
            }
        }
    }
}

impl Interceptor for AntiCounterHandler {
    fn intercept(&self, message: &mut MessageInfo, outbound: bool) {
        if outbound || !message.content.starts_with('{') {
            return;
        }
        // Fast path: skip JSON parsing unless packet mentions anims or Counter Attack
        if !message.content.contains("anims") && !message.content.contains("Counter Attack") {
            return;
        }
        if let Ok(root) = serde_json::from_str::<Value>(&message.content) {
            self.handle_json(&root);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;

    #[test]
    fn pauses_on_counter_attack_and_resumes_on_fade() {
        let logs = Arc::new(LogBuffer::default());
        let handler = AntiCounterHandler::new(logs);
        let triggers = Arc::new(AtomicUsize::new(0));
        let t_clone = triggers.clone();
        handler.set_on_counter_active(Arc::new(move || {
            t_clone.fetch_add(1, Ordering::Relaxed);
        }));

        let mut warn = MessageInfo::new(
            r#"{"t":"xt","b":{"r":-1,"o":{"cmd":"ct","anims":[{"msg":"Ultra Boss prepares a counter attack!"}]}}}"#,
        );
        handler.intercept(&mut warn, false);
        assert!(handler.is_paused());
        assert_eq!(triggers.load(Ordering::Relaxed), 1);

        let mut fade = MessageInfo::new(
            r#"{"t":"xt","b":{"r":-1,"o":{"cmd":"ct","a":[{"cmd":"aura-","aura":{"nam":"Counter Attack"}}]}}}"#,
        );
        handler.intercept(&mut fade, false);
        assert!(!handler.is_paused());
    }
}
