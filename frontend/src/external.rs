//! Ruffle `ExternalInterfaceProvider` forwarding inbound AS3 calls to `grimlite_bot::Bot`.

use grimlite_bot::Bot;
use ruffle_core::context::UpdateContext;
use ruffle_core::external::{ExternalInterfaceProvider, Value as ExternalValue};
use std::sync::Arc;

pub struct GrimliteExternalInterface {
    bot: Arc<Bot>,
}

impl GrimliteExternalInterface {
    pub fn new(bot: Arc<Bot>) -> Self {
        Self { bot }
    }
}

impl ExternalInterfaceProvider for GrimliteExternalInterface {
    fn call_method(
        &self,
        _context: &mut UpdateContext<'_>,
        name: &str,
        args: &[ExternalValue],
    ) -> ExternalValue {
        let string_args: Vec<String> = args.iter().map(external_to_string).collect();
        match self.bot.on_flash_call(name, &string_args) {
            Some(s) => ExternalValue::String(s),
            None => ExternalValue::Null,
        }
    }

    fn on_callback_available(&self, _name: &str) {}

    fn get_id(&self) -> Option<String> {
        Some("grimlite-ri".to_string())
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
