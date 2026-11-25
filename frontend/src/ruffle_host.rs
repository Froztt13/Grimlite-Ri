//! Marshals `grimlite_bot::host::FlashHost` calls onto Ruffle's engine/main thread
//! and executes `Player::call_internal_interface`.

use grimlite_bot::host::{BotError, FlashArg, FlashHost};
use ruffle_core::Player;
use ruffle_core::external::Value as ExternalValue;
use serde_json::Value as JsonValue;
use std::collections::BTreeMap;
use std::sync::mpsc::{self, SyncSender};
use std::thread::ThreadId;

/// A pending call from a bot/worker thread to Ruffle's `Player`.
pub struct FlashCallRequest {
    pub name: String,
    pub args: Vec<FlashArg>,
    pub reply: SyncSender<Result<String, BotError>>,
}

/// Implements `FlashHost` by dispatching `FlashCallRequest`s to the winit/engine loop.
pub struct RuffleFlashHost {
    engine_thread: ThreadId,
    dispatch: Box<dyn Fn(FlashCallRequest) -> Result<(), String> + Send + Sync>,
}

impl RuffleFlashHost {
    pub fn new<F>(engine_thread: ThreadId, dispatch: F) -> Self
    where
        F: Fn(FlashCallRequest) -> Result<(), String> + Send + Sync + 'static,
    {
        Self {
            engine_thread,
            dispatch: Box::new(dispatch),
        }
    }
}

impl FlashHost for RuffleFlashHost {
    fn call(&self, name: &str, args: &[FlashArg]) -> Result<String, BotError> {
        if std::thread::current().id() == self.engine_thread {
            return Err(BotError::FlashCall(
                name.to_string(),
                "cannot call Flash from the engine/UI thread (would deadlock)".to_string(),
            ));
        }
        let (tx, rx) = mpsc::sync_channel(1);
        let req = FlashCallRequest {
            name: name.to_string(),
            args: args.to_vec(),
            reply: tx,
        };
        (self.dispatch)(req).map_err(|e| BotError::FlashCall(name.to_string(), e))?;
        rx.recv()
            .map_err(|_| BotError::FlashCall(name.to_string(), "player dropped reply".to_string()))?
    }
}

/// Runs on the engine/main thread to answer a `FlashCallRequest` against the active `Player`.
pub fn answer_flash_call(player: Option<&mut Player>, req: FlashCallRequest) {
    let result = match player {
        Some(player) => {
            let ext_args: Vec<ExternalValue> = req.args.iter().map(flash_arg_to_external).collect();
            let val = player.call_internal_interface(&req.name, ext_args);
            Ok(external_to_string(&val))
        }
        None => Err(BotError::NoHost),
    };
    let _ = req.reply.send(result);
}

fn flash_arg_to_external(arg: &FlashArg) -> ExternalValue {
    match arg {
        FlashArg::Null => ExternalValue::Null,
        FlashArg::Bool(b) => ExternalValue::Bool(*b),
        FlashArg::Number(n) => ExternalValue::Number(*n),
        FlashArg::String(s) => ExternalValue::String(s.clone()),
        FlashArg::Json(v) => json_to_external(v),
    }
}

fn json_to_external(v: &JsonValue) -> ExternalValue {
    match v {
        JsonValue::Null => ExternalValue::Null,
        JsonValue::Bool(b) => ExternalValue::Bool(*b),
        JsonValue::Number(n) => ExternalValue::Number(n.as_f64().unwrap_or(0.0)),
        JsonValue::String(s) => ExternalValue::String(s.clone()),
        JsonValue::Array(arr) => ExternalValue::List(arr.iter().map(json_to_external).collect()),
        JsonValue::Object(map) => {
            let mut out = BTreeMap::new();
            for (k, val) in map {
                out.insert(k.clone(), json_to_external(val));
            }
            ExternalValue::Object(out)
        }
    }
}

fn external_to_string(v: &ExternalValue) -> String {
    match v {
        ExternalValue::Undefined | ExternalValue::Null => String::new(),
        ExternalValue::Bool(b) => b.to_string(),
        ExternalValue::Number(n) => n.to_string(),
        ExternalValue::String(s) => s.clone(),
        ExternalValue::Object(_) | ExternalValue::List(_) => format!("{v:?}"),
    }
}
