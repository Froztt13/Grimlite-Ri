//! FlashHost abstraction: allows testing `grimlite_bot` without Ruffle or a GPU.

use serde_json::Value;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum BotError {
    #[error("Flash host is not connected")]
    NoHost,
    #[error("Flash call `{0}` failed: {1}")]
    FlashCall(String, String),
    #[error("Bot stopped")]
    Stopped,
}

#[derive(Clone, Debug)]
pub enum FlashArg {
    Null,
    Bool(bool),
    Number(f64),
    String(String),
    Json(Value),
}

impl From<&str> for FlashArg {
    fn from(s: &str) -> Self {
        Self::String(s.to_string())
    }
}

impl From<String> for FlashArg {
    fn from(s: String) -> Self {
        Self::String(s)
    }
}

impl From<bool> for FlashArg {
    fn from(b: bool) -> Self {
        Self::Bool(b)
    }
}

impl From<i32> for FlashArg {
    fn from(n: i32) -> Self {
        Self::Number(f64::from(n))
    }
}

/// Implemented by `grimlite_frontend` to marshal calls onto Ruffle's engine thread.
pub trait FlashHost: Send + Sync {
    fn call(&self, name: &str, args: &[FlashArg]) -> Result<String, BotError>;
}
