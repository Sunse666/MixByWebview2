//! Writes the history file after a quiet period instead of once per visit.

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

/// How long the history has to stay still before it is written out.
const QUIET: Duration = Duration::from_millis(1200);

static DIRTY: AtomicBool = AtomicBool::new(false);

/// Marks the in-memory history as newer than what is on disk.
pub fn touch_history() {
    DIRTY.store(true, Ordering::Release);
}

/// Writes history at most once per quiet period, on its own thread.
pub fn spawn_history_flusher(app: tauri::AppHandle) {
    std::thread::spawn(move || loop {
        std::thread::sleep(QUIET);
        if DIRTY.swap(false, Ordering::AcqRel) {
            crate::ipc::flush_history(&app);
        }
    });
}
