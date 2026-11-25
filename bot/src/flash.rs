//! Typed helpers for invoking Grimlite Ri's host SWF callbacks.

use crate::host::{BotError, FlashArg, FlashHost};
use serde::de::DeserializeOwned;
use std::sync::Arc;

pub struct Flash {
    host: Arc<dyn FlashHost>,
}

impl Flash {
    pub fn new(host: Arc<dyn FlashHost>) -> Self {
        Self { host }
    }

    pub fn call_str(&self, name: &str, args: &[FlashArg]) -> Result<String, BotError> {
        self.host.call(name, args)
    }

    pub fn call_bool(&self, name: &str, args: &[FlashArg]) -> Result<bool, BotError> {
        let s = self.call_str(name, args)?;
        Ok(s.eq_ignore_ascii_case("true"))
    }

    pub fn call_json<T: DeserializeOwned>(
        &self,
        name: &str,
        args: &[FlashArg],
    ) -> Result<T, BotError> {
        let raw = self.call_str(name, args)?;
        serde_json::from_str(&raw).map_err(|e| BotError::FlashCall(name.to_string(), e.to_string()))
    }

    pub fn get<T: DeserializeOwned>(&self, path: &str) -> Result<T, BotError> {
        self.call_json("getGameObject", &[FlashArg::from(path)])
    }

    pub fn set(&self, path: &str, value: FlashArg) -> Result<bool, BotError> {
        self.call_bool("setGameObject", &[FlashArg::from(path), value])
    }

    pub fn call_game_function(&self, path: &str, extra: &[FlashArg]) -> Result<String, BotError> {
        if extra.is_empty() {
            self.call_str("callGameFunction0", &[FlashArg::from(path)])
        } else {
            let mut args = Vec::with_capacity(extra.len() + 1);
            args.push(FlashArg::from(path));
            args.extend_from_slice(extra);
            self.call_str("callGameFunction", &args)
        }
    }
}
