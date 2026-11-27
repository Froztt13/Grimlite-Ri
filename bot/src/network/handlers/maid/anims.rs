//! Maid Remake `ct` JSON packet handler for:
//! - Special Anims message triggers (`cbSpecialAnims` + `tbSpecialMsg`)
//! - Taunt Cycle (`tc;<cycle>;<monster>;<second>;<order>`)
//! - Maid Anti-Counter (`"prepares a counter attack"`, `"Counter Attack"` aura--)
//! - Gramiel Safeguard drain target lock (`"Gramiel attempts to drain"`)

use crate::Bot;
use crate::engine::maid::{MaidConfig, TauntCycleSpec};
use serde_json::Value;
use std::sync::Arc;

pub fn handle(bot: &Arc<Bot>, cfg: &MaidConfig, raw: &str) {
    if !raw.contains("\"ct\"") {
        return;
    }
    let Ok(json) = serde_json::from_str::<Value>(raw) else {
        return;
    };
    let Some(obj) = json.pointer("/b/o") else {
        return;
    };
    if obj.get("cmd").and_then(Value::as_str) != Some("ct") {
        return;
    }

    // 1. Inspect `anims` array
    if let Some(anims) = obj.get("anims").and_then(Value::as_array) {
        let special_raw = cfg.special_msg.trim();
        let tc_spec = if cfg.special_anims {
            TauntCycleSpec::parse(special_raw)
        } else {
            None
        };
        let special_triggers: Vec<String> = if cfg.special_anims && tc_spec.is_none() {
            cfg.parsed_special_msgs()
                .into_iter()
                .map(|s| s.to_ascii_lowercase())
                .collect()
        } else {
            Vec::new()
        };

        for anim in anims {
            let Some(msg) = anim.get("msg").and_then(Value::as_str) else {
                continue;
            };
            let msg_lower = msg.to_ascii_lowercase();

            // Maid Anti-Counter: "prepares a counter attack"
            if cfg.anti_counter && msg_lower.contains("prepares a counter attack") {
                bot.maid.trigger_counter_pause(bot, 1500);
            }

            // Gramiel drain lock
            if msg_lower.contains("gramiel attempts to drain") {
                bot.maid.trigger_gramiel_drain_lock(bot);
            }

            // Taunt Cycle (tc;...)
            if let Some(ref spec) = tc_spec {
                bot.maid.trigger_taunt_cycle(bot, spec.clone());
            } else if !special_triggers.is_empty()
                && special_triggers.iter().any(|t| msg_lower.contains(t))
            {
                bot.maid.trigger_force_skill();
                bot.logs
                    .push_debug(format!("[Maid] Special anim matched `{msg}` -> force skill"));
            }
        }
    }

    // 2. Inspect `a` (auras) array for Anti-Counter (`Counter Attack` aura--)
    if cfg.anti_counter
        && let Some(actions) = obj.get("a").and_then(Value::as_array)
    {
        for act in actions {
            let cmd = act.get("cmd").and_then(Value::as_str).unwrap_or_default();
            if cmd == "aura--" {
                if let Some(name) = act.pointer("/aura/nam").and_then(Value::as_str)
                    && name.eq_ignore_ascii_case("Counter Attack")
                {
                    bot.maid.clear_counter_pause(bot);
                }
            }
        }
    }
}
