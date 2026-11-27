//! Modular packet handlers driven by Ruffle's `CaptureProxy` interceptors.

pub mod anti_counter;
pub mod special;

pub use anti_counter::AntiCounterHandler;
pub use special::{SpecialHandlerManager, SpecialZone};
