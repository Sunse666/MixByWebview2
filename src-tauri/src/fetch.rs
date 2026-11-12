//! A download engine with working pause, resume and cancel.

use std::collections::HashMap;
use std::io::Write;
use std::sync::atomic::{AtomicBool, Ordering};
use parking_lot::Mutex;
use std::sync::{Arc, OnceLock};



/// One transfer, whether it is running, paused or finished.
pub struct Job {
    pub url: String,
    pub path: std::path::PathBuf,
    /// Set while the user wants it paused; the bytes stay.
    paused: Arc<AtomicBool>,
    /// Set when the file is no longer wanted, partial file and all.
    cancelled: Arc<AtomicBool>,
    /// True while a loop is actually running.
    active: Arc<AtomicBool>,
    /// The loop, so a pause can stop it at the next await rather than after the next chunk.
    handle: Option<tauri::async_runtime::JoinHandle<()>>,
}

/// Files a transfer is currently writing, so two loops can never share one.
fn claimed() -> &'static Mutex<std::collections::HashSet<String>> {
    static CLAIMED: OnceLock<Mutex<std::collections::HashSet<String>>> = OnceLock::new();
    CLAIMED.get_or_init(|| Mutex::new(std::collections::HashSet::new()))
}

/// Takes ownership of a file for one transfer.
pub fn claim(path: &std::path::Path) -> bool {
    let key = path.to_string_lossy().to_string();
    let mut set = claimed().lock();
    set.insert(key)
}

/// Gives the file back when its transfer is over, so the next attempt can take it.
pub fn unclaim(path: &std::path::Path) {
    release(path);
}

fn release(path: &std::path::Path) {
    let key = path.to_string_lossy().to_string();
    claimed().lock().remove(&key);
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
        let map = jobs().lock();
        if let Some(job) = map.get(&id) {
            if job.active.load(Ordering::Relaxed) {
                job.paused.store(false, Ordering::Relaxed);
                job.cancelled.store(false, Ordering::Relaxed);
                return;
            }
        }
    }

    let active = Arc::new(AtomicBool::new(true));
    {
        let mut map = jobs().lock();
        if let Some(job) = map.get_mut(&id) {
            job.paused = paused.clone();
            job.cancelled = cancelled.clone();
            job.active = active.clone();
        } else {
            map.insert(
                id,
                Job {
                    url: url.clone(),
                    path: path.clone(),
                    paused: paused.clone(),
                    cancelled: cancelled.clone(),
                    active: active.clone(),
                    handle: None,
                },
            );
        }
    }

    let active_guard = active.clone();
    let handle = tauri::async_runtime::spawn(async move {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(600))
            .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) Mix")
            .build();
        let Ok(client) = client else {
            crate::downloads::set_state(&app, id, "failed");
                release(&path);
            return;
        };

        loop {
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
                release(&path);
                    return;
                }
            };
            if !response.status().is_success() {
                crate::logging::warn(&format!("fetch {id}: http {}", response.status()));
                crate::downloads::set_state(&app, id, "failed");
                release(&path);
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
                release(&path);
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
                release(&path);
                    return;
                }
                if paused.load(Ordering::Relaxed) || !running.load(Ordering::Relaxed) {
                    let _ = file.flush();
                    drop(file);
                    crate::downloads::set_state(&app, id, "interrupted");
                    return;
                }
                if let Err(err) = file.write_all(&chunk) {
                    crate::logging::warn(&format!("fetch {id}: write failed: {err}"));
                    crate::downloads::set_state(&app, id, "failed");
                release(&path);
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
            release(&path);
            active_guard.store(false, Ordering::Relaxed);
            return;
        }
    });
    if let Some(job) = jobs().lock().get_mut(&id) {
        job.handle = Some(handle);
    }
}

/// Stops the loop but keeps the bytes, so it can be resumed.
pub fn pause(id: u64) -> bool {
    let mut map = jobs().lock();
    match map.get_mut(&id) {
        Some(job) => {
            job.paused.store(true, Ordering::Relaxed);
            if let Some(handle) = job.handle.take() {
                handle.abort();
            }
            job.active.store(false, Ordering::Relaxed);
            true
        }
        None => false,
    }
}

/// Asks the loop to carry on from what is on disk.
pub fn resume(app: tauri::AppHandle, id: u64) -> bool {
    let (url, path) = {
        let map = jobs().lock();
        match map.get(&id) {
            Some(job) => (job.url.clone(), job.path.clone()),
            None => return false,
        }
    };
    unclaim(&path);
    start(app, id, url, path);
    true
}

/// Drops the transfer and the partial file with it.
pub fn cancel(id: u64) -> bool {
    let map = jobs().lock();
    match map.get(&id) {
        Some(job) => {
            job.cancelled.store(true, Ordering::Relaxed);
            true
        }
        None => false,
    }
}

/// True when a loop for this id is alive right now.
pub fn busy(id: u64) -> bool {
    jobs()
        .lock()
        .get(&id)
        .map(|job| job.active.load(Ordering::Relaxed))
        .unwrap_or(false)
}
