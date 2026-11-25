//! Grimlite Ri application state and embedded host SWF extraction.

use grimlite_bot::Bot;
use std::path::PathBuf;
use std::sync::{Arc, OnceLock};

static BOT: OnceLock<Arc<Bot>> = OnceLock::new();

pub fn bot() -> Arc<Bot> {
    BOT.get_or_init(Bot::new).clone()
}

pub fn data_dir() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("Grimlite-Ri")
}

/// Ensures the embedded `grimlite.swf` is written to the data directory and returns its path.
pub fn ensure_host_swf() -> PathBuf {
    let dir = data_dir();
    let _ = std::fs::create_dir_all(&dir);
    let path = dir.join("grimlite.swf");
    let up_to_date = std::fs::read(&path).is_ok_and(|b| b == grimlite_bot::HOST_SWF);
    if !up_to_date {
        let _ = std::fs::write(&path, grimlite_bot::HOST_SWF);
    }
    path
}
