//! Thread-safe log buffers for Debug & Bot messages.

use chrono::Local;
use parking_lot::Mutex;
use std::collections::VecDeque;

const MAX_LINES: usize = 500;

#[derive(Default)]
pub struct LogBuffer {
    debug: Mutex<VecDeque<String>>,
}

impl LogBuffer {
    pub fn push_debug(&self, message: impl AsRef<str>) {
        let line = format!("[{}] {}", Local::now().format("%H:%M:%S"), message.as_ref());
        let mut buf = self.debug.lock();
        if buf.len() >= MAX_LINES {
            buf.pop_front();
        }
        buf.push_back(line);
    }

    pub fn debug_lines(&self) -> Vec<String> {
        self.debug.lock().iter().cloned().collect()
    }

    pub fn clear_debug(&self) {
        self.debug.lock().clear();
    }
}
