//! Live Packet Tamperer / Interceptor rules applied in-process via `CaptureProxy`.

use crate::capture::{Interceptor, MessageInfo};
use parking_lot::RwLock;
use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TamperDirection {
    Inbound,
    Outbound,
    Both,
}

impl TamperDirection {
    pub fn matches(self, outbound: bool) -> bool {
        match self {
            Self::Inbound => !outbound,
            Self::Outbound => outbound,
            Self::Both => true,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Inbound => "IN",
            Self::Outbound => "OUT",
            Self::Both => "BOTH",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TamperAction {
    Block,
    Replace { from: String, to: String },
}

#[derive(Clone, Debug)]
pub struct TamperRule {
    pub id: u64,
    pub enabled: bool,
    pub direction: TamperDirection,
    pub contains: String,
    pub action: TamperAction,
}

#[derive(Default)]
pub struct TamperEngine {
    rules: RwLock<Vec<TamperRule>>,
    next_id: AtomicU64,
}

impl TamperEngine {
    pub fn add_rule(
        &self,
        direction: TamperDirection,
        contains: String,
        action: TamperAction,
    ) -> u64 {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed) + 1;
        self.rules.write().push(TamperRule {
            id,
            enabled: true,
            direction,
            contains,
            action,
        });
        id
    }

    pub fn remove_rule(&self, id: u64) {
        self.rules.write().retain(|r| r.id != id);
    }

    pub fn set_enabled(&self, id: u64, enabled: bool) {
        if let Some(r) = self.rules.write().iter_mut().find(|r| r.id == id) {
            r.enabled = enabled;
        }
    }

    pub fn rules(&self) -> Vec<TamperRule> {
        self.rules.read().clone()
    }
}

impl Interceptor for TamperEngine {
    fn priority(&self) -> i32 {
        0
    }

    fn intercept(&self, message: &mut MessageInfo, outbound: bool) {
        let rules = self.rules.read();
        for rule in rules.iter() {
            if !rule.enabled || !rule.direction.matches(outbound) {
                continue;
            }
            if !rule.contains.is_empty() && !message.content.contains(&rule.contains) {
                continue;
            }
            match &rule.action {
                TamperAction::Block => {
                    message.send = false;
                    return;
                }
                TamperAction::Replace { from, to } => {
                    if !from.is_empty() {
                        message.content = message.content.replace(from, to);
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
    fn tamper_engine_replaces_and_blocks() {
        let engine = TamperEngine::default();
        engine.add_rule(
            TamperDirection::Outbound,
            "old".to_string(),
            TamperAction::Replace {
                from: "old".to_string(),
                to: "new".to_string(),
            },
        );
        engine.add_rule(
            TamperDirection::Inbound,
            "kick".to_string(),
            TamperAction::Block,
        );

        let mut out = MessageInfo::new("%xt%zm%old%1%");
        engine.intercept(&mut out, true);
        assert!(out.send);
        assert_eq!(out.content, "%xt%zm%new%1%");

        let mut inc = MessageInfo::new("%xt%kick%-1%");
        engine.intercept(&mut inc, false);
        assert!(!inc.send);
    }
}
