//! A download engine of our own.
//!
//! WebView2 hands out a download operation with Pause/Resume on it, but the calls are
//! fragile: accepted before the transfer starts and then ignored, refused while paused, and
//! only callable from the thread that owns the webview. Between them those rules made the
//! controls unreliable in a way no amount of argument-checking could fix.
//!
//! This fetches the file itself, so a pause is a decision the loop takes between chunks:
//! stop reading, keep what is on disk, and resume by asking for the rest with a Range
//! header. That is how a browser download manager works, and it makes the three controls
//! mean what they say.
//!
//! The cost is the browser's cookie jar: this client has its own. Signed download links
//! (the usual case) are fine; a download behind a login needs the webview's own download,
//! which is why the caller decides which engine to use.

use std::collections::HashMap;
use std::io::Write;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use tauri::Emitter;

/// One transfer, whether it is running, paused or finished.
pub struct Job {
    pub id: u64,
    pub url: String,
    pub path: std::path::PathBuf,
    pub total: Option<u64>,
    /// Set while the loop should keep running.
    running: Arc<AtomicBool>,
    /// Set while the user wants it paused: the loop stops but the bytes stay.
    paused: Arc<AtomicBool>,
    /// Set when the file is no longer wanted, partial file and all.
    cancelled: Arc<AtomicBool>,
}

fn jobs() -> &'static Mutex<HashMap<u64, Job>> {
    static JOBS: OnceLock<Mutex<HashMap<u64, Job>>> = OnceLock::new();
    JOBS.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Starts (or restarts) a transfer, continuing from whatever is on disk.
pub fn start(app: tauri::AppHandle, id: u64, url: String, path: std::path::PathBuf) {
    let running = Arc::new(AtomicBool::new(true));
    let paused = Arc::new(AtomicBool::new(false));
    let cancelled = Arc::new(AtomicBool::new(false));

    {
        let Ok(mut map) = jobs().lock() else {
            return;
        };
        if let Some(job) = map.get_mut(&id) {
            job.running = running.clone();
            job.paused = paused.clone();
            job.cancelled = cancelled.clone();
        } else {
            map.insert(
                id,
                Job {
                    id,
                    url: url.clone(),
                    path: path.clone(),
                    total: None,
                    running: running.clone(),
                    paused: paused.clone(),
                    cancelled: cancelled.clone(),
                },
            );
        }
    }

    tauri::async_runtime::spawn(async move {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(600))
            .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) Mix")
            .build();
        let Ok(client) = client else {
            crate::downloads::set_state(&app, id, "failed");
            return;
        };

        loop {
            // Where to continue from: whatever the file already holds.
            let offset = std::fs::metadata(&path).map(|meta| meta.len()).unwrap_or(0);
            let mut request = client.get(&url);
            if offset > 0 {
                request = request.header(reqwest::header::RANGE, format!("bytes={offset}-"));
            }
            let response = match request.send().await {
                Ok(response) => response,
                Err(err) => {
                    crate::logging::warn(&format!("fetch {id} failed: {err}"));
                    crate::downloads::set_state(&app, id, "failed");
                    return;
                }
            };
            if !response.status().is_success() {
                crate::logging::warn(&format!("fetch {id}: http {}", response.status()));
                crate::downloads::set_state(&app, id, "failed");
                return;
            }
            let total = response.content_length().map(|length| length + offset);
            crate::downloads::set_total(&app, id, total);

            let mut file = match std::fs::OpenOptions::new()
                .create(true)
                .append(offset > 0)
                .write(true)
                .truncate(offset == 0)
                .open(&path)
            {
                Ok(file) => file,
                Err(err) => {
                    crate::logging::warn(&format!("fetch {id}: cannot write {}: {err}", path.display()));
                    crate::downloads::set_state(&app, id, "failed");
                    return;
                }
            };

            let mut response = response;
            let mut written = offset;
            let mut last_report = std::time::Instant::now();
            let mut last_bytes = written;

            while let Ok(Some(chunk)) = response.chunk().await {
                if cancelled.load(Ordering::Relaxed) {
                    let _ = file.flush();
                    drop(file);
                    let _ = std::fs::remove_file(&path);
                    crate::downloads::set_state(&app, id, "failed");
                    return;
                }
                if paused.load(Ordering::Relaxed) || !running.load(Ordering::Relaxed) {
                    let _ = file.flush();
                    drop(file);
                    // The bytes on disk are the resume point; the transfer is not over.
                    crate::downloads::set_state(&app, id, "interrupted");
                    return;
                }
                if let Err(err) = file.write_all(&chunk) {
                    crate::logging::warn(&format!("fetch {id}: write failed: {err}"));
                    crate::downloads::set_state(&app, id, "failed");
                    return;
                }
                written += chunk.len() as u64;
                if last_report.elapsed() >= std::time::Duration::from_millis(250) {
                    let speed = ((written - last_bytes) as f64
                        / last_report.elapsed().as_secs_f64()) as u64;
                    crate::downloads::set_progress(&app, id, written, speed);
                    last_report = std::time::Instant::now();
                    last_bytes = written;
                }
            }

            let _ = file.flush();
            drop(file);
            crate::downloads::set_progress(&app, id, written, 0);
            crate::downloads::set_state(&app, id, "finished");
            let _ = app.emit("download:changed", Vec::<serde_json::Value>::new());
            return;
        }
    });
}

/// Stops the loop but keeps the bytes, so it can be resumed.
pub fn pause(id: u64) -> bool {
    let Ok(map) = jobs().lock() else {
        return false;
    };
    match map.get(&id) {
        Some(job) => {
            job.paused.store(true, Ordering::Relaxed);
            true
        }
        None => false,
    }
}

/// Asks the loop to carry on from what is on disk.
pub fn resume(app: tauri::AppHandle, id: u64) -> bool {
    let (url, path) = {
        let Ok(map) = jobs().lock() else {
            return false;
        };
        match map.get(&id) {
            Some(job) => (job.url.clone(), job.path.clone()),
            None => return false,
        }
    };
    start(app, id, url, path);
    true
}

/// Drops the transfer and the partial file with it.
pub fn cancel(id: u64) -> bool {
    let Ok(map) = jobs().lock() else {
        return false;
    };
    match map.get(&id) {
        Some(job) => {
            job.cancelled.store(true, Ordering::Relaxed);
            true
        }
        None => false,
    }
}

/// True when this id belongs to the internal engine rather than the webview's.
pub fn knows(id: u64) -> bool {
    jobs().lock().map(|map| map.contains_key(&id)).unwrap_or(false)
}
