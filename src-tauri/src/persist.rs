//! Writes the history file when nothing has changed for a moment, instead of
//! once per visit: a page load produces a visit, a title and an icon within a
//! second, and each of those used to rewrite the whole file.

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

/// How long the history has to stay still before it is written out.
const QUIET: Duration = Duration::from_millis(1200);

static DIRTY: AtomicBool = AtomicBool::new(false);

/// Marks the in-memory history as newer than what is on disk.
pub fn touch_history() {
    DIRTY.store(true, Ordering::Release);
}

/// Writes history at most once per quiet period, on its own thread so nothing
/// on the ui path waits for the disk. Quitting flushes through the same
/// function, so a normal exit loses nothing.
pub fn spawn_history_flusher(app: tauri::AppHandle) {
    std::thread::spawn(move || loop {
        std::thread::sleep(QUIET);
        if DIRTY.swap(false, Ordering::AcqRel) {
            crate::ipc::flush_history(&app);
        }
    });
}
