//! A download engine with working pause, resume and cancel.

use parking_lot::Mutex;
use std::collections::HashMap;
use std::io::Write;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock};

/// What the page that started a download would have sent with it.
///
/// The engine is not the webview: it has no cookie jar and no referrer of its own, so a
/// link behind a login used to come back as the site's login page saved under the file's
/// name. The values are collected once, when the download starts, from the page that
/// asked for it.
#[derive(Debug, Clone, Default)]
pub struct Request {
    /// The page the download was started from, sent as the referrer.
    pub referer: Option<String>,
    /// Name=value pairs, already joined for one Cookie header.
    pub cookie: Option<String>,
}

impl Request {
    /// True when it has nothing the page would have sent anyway.
    pub fn is_empty(&self) -> bool {
        self.referer.is_none() && self.cookie.is_none()
    }
}

/// One transfer, whether it is running, paused or finished.
pub struct Job {
    pub url: String,
    pub path: std::path::PathBuf,
    /// What the page would have sent along with the request.
    pub request: Request,
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

/// Sent as the user agent. A browser string rather than "Mix": plenty of file hosts
/// answer a non-browser agent with an error page that still has a 200 status.
pub(crate) const USER_AGENT: &str =
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) \
     Chrome/131.0.0.0 Safari/537.36 Edg/131.0.0.0";

/// True when the file being written is a web page on purpose, so an html answer is the
/// file rather than a login wall. "Save page as" lands here.
fn wants_html(path: &std::path::Path) -> bool {
    matches!(
        path.extension()
            .and_then(|value| value.to_str())
            .map(|value| value.to_ascii_lowercase())
            .as_deref(),
        Some("html") | Some("htm") | Some("xhtml")
    )
}

/// True when the answer is a document rather than the file that was asked for.
///
/// A login wall, an error page and a "link expired" notice all arrive as 200 with an
/// html body, so the only way to notice is what the server says it is sending.
fn is_html(response: &reqwest::Response) -> bool {
    response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .map(|value| value.to_ascii_lowercase().contains("text/html"))
        .unwrap_or(false)
}

/// Starts (or restarts) a transfer, continuing from whatever is on disk.
///
/// `request` carries what the page would have sent; an empty one is a plain fetch.
pub fn start_with(
    app: tauri::AppHandle,
    id: u64,
    url: String,
    path: std::path::PathBuf,
    request: Request,
) {
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
            // The values are collected from the page at the call site, so a retry that
            // comes from the downloads list carries none. Only replace what is there with
            // something that is not empty, or a retry would wipe the first attempt's
            // credentials and fail the way the original problem did.
            if !request.is_empty() {
                job.request = request.clone();
            }
        } else {
            map.insert(
                id,
                Job {
                    url: url.clone(),
                    path: path.clone(),
                    request: request.clone(),
                    paused: paused.clone(),
                    cancelled: cancelled.clone(),
                    active: active.clone(),
                    handle: None,
                },
            );
        }
    }

    let request_headers = {
        let map = jobs().lock();
        map.get(&id)
            .map(|job| job.request.clone())
            .unwrap_or_default()
    };

    let active_guard = active.clone();
    let handle = tauri::async_runtime::spawn(async move {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(600))
            .user_agent(USER_AGENT)
            .build();
        let Ok(client) = client else {
            crate::downloads::set_state(&app, id, "failed");
            release(&path);
            return;
        };

        // One transfer, continuing from whatever byte the file already holds. This is a
        // block rather than a loop because every way out of it is a return: the resume
        // path is a second call to start_with(), not a second pass here.
        {
            let offset = std::fs::metadata(&path).map(|meta| meta.len()).unwrap_or(0);
            let mut builder = client.get(&url);
            if offset > 0 {
                builder = builder.header(reqwest::header::RANGE, format!("bytes={offset}-"));
            }
            // What the page would have sent. Without these a link behind a login answers
            // with the login page, and a hotlink-protected file answers with a refusal.
            if let Some(referer) = request_headers.referer.as_deref() {
                builder = builder.header(reqwest::header::REFERER, referer);
            }
            if let Some(cookie) = request_headers.cookie.as_deref() {
                builder = builder.header(reqwest::header::COOKIE, cookie);
            }
            let response = match builder.send().await {
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
            // A server that ignores Range answers 200 with the whole file. Appending that
            // to the bytes already on disk produced a file that was the head of the file
            // followed by all of it, and the byte count then matched the announced length,
            // so nothing downstream could tell. The partial file is dropped instead, and
            // the transfer starts over from zero.
            let resumed = offset > 0 && response.status() == reqwest::StatusCode::PARTIAL_CONTENT;
            let offset = if resumed { offset } else { 0 };
            if !resumed {
                let _ = std::fs::remove_file(&path);
            }
            // A saved web page is a real thing to download, so an html answer is only
            // refused when the file was not asked for as one.
            if is_html(&response) && !wants_html(&path) {
                crate::logging::warn(&format!(
                    "fetch {id}: server answered with a web page, not the file"
                ));
                crate::downloads::set_state(&app, id, "failed");
                release(&path);
                return;
            }
            let mut response = response;
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
                    crate::logging::warn(&format!(
                        "fetch {id}: cannot write {}: {err}",
                        path.display()
                    ));
                    crate::downloads::set_state(&app, id, "failed");
                    release(&path);
                    return;
                }
            };

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
                if paused.load(Ordering::Relaxed) {
                    let _ = file.flush();
                    drop(file);
                    // The user asked for this one, which reads differently from a
                    // connection that went away: the buttons offered are not the same.
                    crate::downloads::set_state(&app, id, "paused");
                    // The claim has to go back: `pause` aborts the loop, so the release at
                    // the end of it never runs. Leaving the file claimed made every later
                    // attempt to fetch it a silent no-op - the download appeared to start
                    // and then simply never moved, and starting it again did nothing at all.
                    release(&path);
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
                        / last_report.elapsed().as_secs_f64())
                        as u64;
                    crate::downloads::set_progress(&app, id, written, speed);
                    last_report = std::time::Instant::now();
                    last_bytes = written;
                }
            }

            let _ = file.flush();
            drop(file);
            crate::downloads::set_progress(&app, id, written, 0);
            // A connection that drops ends the chunk stream without an error, so the byte
            // count is what says whether the file is whole. Calling a short file finished
            // is how a truncated download used to look like a good one.
            if total.map(|size| written >= size).unwrap_or(true) {
                crate::downloads::set_state(&app, id, "finished");
            } else {
                crate::logging::warn(&format!("fetch {id}: stopped at {written} of {total:?}"));
                crate::downloads::set_state(&app, id, "interrupted");
            }
            release(&path);
            active_guard.store(false, Ordering::Relaxed);
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
///
/// The request context is carried over rather than rebuilt: `pause` aborts the loop, so
/// by the time this runs there may be no webview left to read the cookies back from.
///
/// A task that outlived the process has no job at all - `jobs()` is empty after a restart -
/// and this used to answer `false` for it. The row still said "Resume", the page swallowed
/// the false as if it were success, and pressing it did nothing whatsoever. Resuming is
/// therefore allowed to adopt a task it has never seen: the list on disk knows the url and
/// the path, which is everything a transfer needs.
pub fn resume(app: tauri::AppHandle, id: u64, known: Option<(String, std::path::PathBuf)>) -> bool {
    let from_job = {
        let map = jobs().lock();
        map.get(&id)
            .map(|job| (job.url.clone(), job.path.clone(), job.request.clone()))
    };
    let (url, path, request) = match from_job {
        Some((url, path, request)) => (url, path, request),
        None => {
            // Nothing this session started: take the url and path the task list remembers.
            let Some((url, path)) = known else {
                crate::logging::warn(&format!("download {id}: nothing known to resume"));
                return false;
            };
            (url, path, Request::default())
        }
    };
    unclaim(&path);
    start_with(app, id, url, path, request);
    true
}

/// The cookies this webview would send with its next request, joined for one header.
///
/// Read through the DevTools protocol because the webview offers no other way to ask,
/// and nothing here second-guesses the expiry: what the browser is holding is what it
/// would have sent, and filtering it again is how a download ends up unauthenticated.
#[cfg(windows)]
pub fn cookies_for(webview: &tauri::Webview) -> Option<String> {
    let json = crate::screenshot::call_timeout(
        webview,
        "Network.getCookies",
        "{}",
        std::time::Duration::from_secs(2),
    )?;
    let value: serde_json::Value = serde_json::from_str(&json).ok()?;
    let cookies = value.get("cookies")?.as_array()?;
    let mut pairs: Vec<String> = Vec::new();
    for cookie in cookies {
        let (Some(name), Some(content)) = (
            cookie.get("name").and_then(|item| item.as_str()),
            cookie.get("value").and_then(|item| item.as_str()),
        ) else {
            continue;
        };
        if name.is_empty() {
            continue;
        }
        // A value with a line break would split the header into a second one.
        if name.contains([';', '\r', '\n']) || content.contains([';', '\r', '\n']) {
            continue;
        }
        pairs.push(format!("{name}={content}"));
    }
    if pairs.is_empty() {
        return None;
    }
    Some(pairs.join("; "))
}

#[cfg(not(windows))]
pub fn cookies_for(_webview: &tauri::Webview) -> Option<String> {
    None
}

/// Drops the transfer and the partial file with it.
///
/// The flag alone was not enough. It is only read inside the chunk loop, so a task that was
/// paused - or one left over from a previous process, which has no loop at all - never saw
/// it: the row stayed, the file stayed, and Cancel looked like a dead button. An abort covers
/// the paused case, and the caller removes what is on disk for the rest.
pub fn cancel(id: u64) -> bool {
    let mut map = jobs().lock();
    match map.get_mut(&id) {
        Some(job) => {
            job.cancelled.store(true, Ordering::Relaxed);
            if let Some(handle) = job.handle.take() {
                handle.abort();
            }
            job.active.store(false, Ordering::Relaxed);
            true
        }
        // No loop: nothing to stop, but there may still be bytes on disk, which is the
        // caller's business to remove.
        None => false,
    }
}

/// Forgets a job entirely, so the id can be started fresh later.
pub fn forget(id: u64) {
    if let Some(mut job) = jobs().lock().remove(&id) {
        if let Some(handle) = job.handle.take() {
            handle.abort();
        }
        release(&job.path);
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
