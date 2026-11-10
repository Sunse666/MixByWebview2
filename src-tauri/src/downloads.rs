use crate::ipc::AppState;
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use tauri::{webview::DownloadEvent, Emitter, Manager, Webview};

#[derive(Debug, Clone, Serialize)]
pub struct DownloadTask {
    pub id: u64,
    pub url: String,
    pub file_name: String,
    pub path: String,
    /// started | finished | failed
    pub state: String,
    /// Bytes written so far; WebView2 gives no total, so there is no percentage.
    pub bytes: u64,
    /// Bytes per second over the last sample; zero when nothing is moving.
    pub speed: u64,
    /// True while the download operation is paused.
    pub paused: bool,
}

static NEXT_ID: AtomicU64 = AtomicU64::new(1);
/// The download operations this session has started, so they can be paused and resumed.
///
/// WebView2 hands out ICoreWebView2DownloadOperation through DownloadStarting, and that is
/// the only handle with Pause/Resume/Cancel on it. The webview builder's own callback only
/// reports that a download began, which is why the list could show progress but not stop it.
#[cfg(windows)]
pub mod ops {
    use std::collections::HashMap;
    use std::sync::{Mutex, OnceLock};
    use webview2_com::Microsoft::Web::WebView2::Win32::{
        ICoreWebView2DownloadOperation, ICoreWebView2_4,
    };
    use webview2_com::DownloadStartingEventHandler;
    use windows_core::{Interface, PWSTR};

    /// COM interfaces from one webview are usable from the threads that ask about them;
    /// the map is behind a mutex, so the only thing being asserted is that the interface
    /// is free-threaded, which the download objects are.
    struct Ops(Mutex<HashMap<String, ICoreWebView2DownloadOperation>>);
    unsafe impl Send for Ops {}
    unsafe impl Sync for Ops {}

    static OPS: OnceLock<Ops> = OnceLock::new();

    fn ops() -> &'static Ops {
        OPS.get_or_init(|| Ops(Mutex::new(HashMap::new())))
    }

    /// Remembers the operation under the url it fetches.
    ///
    /// Not under the result path: that is chosen by the webview's own handler, and the
    /// app rewrites it to the portable downloads folder, so the two disagree. The url is
    /// the one thing both sides see the same.
    fn remember(args: &webview2_com::Microsoft::Web::WebView2::Win32::ICoreWebView2DownloadStartingEventArgs) -> Option<String> {
        let operation = unsafe { args.DownloadOperation().ok()? };
        let mut uri = PWSTR::null();
        unsafe { operation.Uri(&mut uri).ok()? };
        if uri.is_null() {
            return None;
        }
        let key = unsafe { uri.to_string().ok()? };
        ops().0.lock().ok()?.insert(key.clone(), operation);
        Some(key)
    }

    /// Attaches the handler that keeps a handle on every download this page starts.
    pub fn watch(webview: &tauri::Webview) {
        // The handler is built inside the closure: it holds a COM pointer and is not Send,
        // and WebView2 keeps its own reference once the event is hooked up.
        let _ = webview.with_webview(|platform| unsafe {
            let controller = platform.controller();
            let Ok(core) = controller.CoreWebView2() else {
                return;
            };
            let handler = DownloadStartingEventHandler::create(Box::new(move |_sender, args| {
                if let Some(args) = args {
                    let key = remember(&args);
                if let Some(key) = key {
                    crate::logging::info(&format!("download operation: {key}"));
                }
                }
                Ok(())
            }));
            if let Ok(core) = core.cast::<ICoreWebView2_4>() {
                let mut token = 0;
                let _ = core.add_DownloadStarting(&handler, &mut token);
            }
        });
    }

    /// Runs something against the operation for this url, and says what actually happened.
    ///
    /// The result used to be "the operation was found", which reported success for a call
    /// the browser had refused. Now it carries the state the operation was in and the error
    /// the call returned, so a failure can be read instead of guessed at.
    fn with_op(
        url: &str,
        run: impl FnOnce(&ICoreWebView2DownloadOperation) -> windows_core::Result<()>,
    ) -> Result<String, String> {
        let map = ops().0.lock().map_err(|_| "lock poisoned".to_string())?;
        let operation = map.get(url).ok_or_else(|| "no operation for that url".to_string())?;
        let state = unsafe {
            let mut state = webview2_com::Microsoft::Web::WebView2::Win32::COREWEBVIEW2_DOWNLOAD_STATE::default();
            operation.State(&mut state).map(|_| state.0).unwrap_or(-1)
        };
        // 0 created, 1 in progress, 2 interrupted, 3 completed.
        let label = match state {
            0 => "created",
            1 => "in progress",
            2 => "interrupted",
            3 => "completed",
            _ => "unknown",
        };
        match run(operation) {
            Ok(()) => Ok(label.to_string()),
            Err(err) => Err(format!("{label}: {err}")),
        }
    }

    pub fn pause(url: &str) -> Result<String, String> {
        with_op(url, |operation| unsafe { operation.Pause() })
    }

    pub fn resume(url: &str) -> Result<String, String> {
        with_op(url, |operation| unsafe { operation.Resume() })
    }

    pub fn cancel(url: &str) -> Result<String, String> {
        with_op(url, |operation| unsafe { operation.Cancel() })
    }
}

#[cfg(not(windows))]
pub mod ops {
    pub fn watch(_webview: &tauri::Webview) {}
    pub fn pause(_url: &str) -> Result<String, String> {
        Err("not supported on this platform".into())
    }
    pub fn resume(_url: &str) -> Result<String, String> {
        Err("not supported on this platform".into())
    }
    pub fn cancel(_url: &str) -> Result<String, String> {
        Err("not supported on this platform".into())
    }
}

const KEEP: usize = 50;

/// WebView2 has no download UI of its own beyond a prompt, so we take the
/// destination, record the task and let the chrome strip show it.
pub fn handle(webview: Webview, event: DownloadEvent<'_>) -> bool {
    let app = webview.app_handle().clone();

    match event {
        DownloadEvent::Requested { url, destination } => {
            let Some(state) = app.try_state::<AppState>() else {
                return true;
            };
            // WebView2 already resolved Content-Disposition into a file name; keep it
            // and only move the file into our portable folder.
            let suggested = destination
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or_default()
                .to_string();
            let target = target_for(&state.dirs.downloads, url.as_str(), &suggested);
            *destination = target.clone();
            record(&app, Some(url.as_str()), Some(&target), "started");
            true
        }
        DownloadEvent::Finished { url, path, success } => {
            let state = if success { "finished" } else { "failed" };
            record(&app, Some(url.as_str()), path.as_deref(), state);
            success
        }
        // The enum is non_exhaustive; unknown events keep the default behaviour.
        _ => true,
    }
}

/// Records a file that we wrote ourselves (image save), so it shows up in the
/// same list as the webview downloads.
/// Fills in the byte count for a finished task from the file on disk.
fn finish_size(path: &str) -> u64 {
    std::fs::metadata(path).map(|meta| meta.len()).unwrap_or(0)
}

pub fn record_done(app: &tauri::AppHandle, url: &str, path: &Path) {
    record(app, Some(url), Some(path), "started");
    record(app, Some(url), Some(path), "finished");
}

fn record(app: &tauri::AppHandle, url: Option<&str>, path: Option<&Path>, state: &str) {
    let Some(guard) = app.try_state::<AppState>() else {
        return;
    };

    let tasks = {
        let mut tasks = guard.downloads.write().unwrap_or_else(|e| e.into_inner());
        match state {
            "started" => {
                let target = path.map(Path::to_path_buf).unwrap_or_default();
                tasks.push(DownloadTask {
                    id: NEXT_ID.fetch_add(1, Ordering::Relaxed),
                    url: url.unwrap_or_default().to_string(),
                    file_name: file_name(&target),
                    path: target.display().to_string(),
                    state: state.to_string(),
                    bytes: 0,
                    speed: 0,
                    paused: false,
                });
                if tasks.len() > KEEP {
                    let drop_count = tasks.len() - KEEP;
                    tasks.drain(0..drop_count);
                }
            }
            _ => {
                // Finish the most recent task for this url; path may be absent on macOS.
                let target = tasks
                    .iter()
                    .rposition(|task| Some(task.url.as_str()) == url && task.state == "started");
                if let Some(index) = target {
                    // Pausing makes the browser report the transfer as not successful. It is
                    // interrupted, not failed, and it keeps its resume button - marking it
                    // failed is what made the download look broken and unresumable.
                    let interrupted = tasks[index].paused && state != "finished";
                    tasks[index].state = if interrupted {
                        "interrupted".to_string()
                    } else {
                        state.to_string()
                    };
                    if let Some(path) = path {
                        tasks[index].path = path.display().to_string();
                        tasks[index].file_name = file_name(path);
                        tasks[index].bytes = finish_size(&path.display().to_string());
                    }
                }
            }
        }
        tasks.clone()
    };

    crate::logging::info(&format!("download {state}: {}", url.unwrap_or_default()));
    let _ = app.emit("download:changed", tasks);
}

/// WebView2 reports only start and finish, so the size on disk is polled to
/// give the strip something to show while a download runs.
pub fn start_progress_watch(app: tauri::AppHandle) {
    std::thread::spawn(move || loop {
        std::thread::sleep(std::time::Duration::from_millis(500));

        let Some(state) = app.try_state::<AppState>() else {
            return;
        };
        let mut changed = false;
        let tasks = {
            let mut tasks = state.downloads.write().unwrap_or_else(|e| e.into_inner());
            for task in tasks
                .iter_mut()
                .filter(|task| task.state == "started" || task.state == "interrupted")
            {
                let size = std::fs::metadata(&task.path).map(|meta| meta.len()).unwrap_or(0);
                if size != task.bytes {
                    // The poll runs every 500 ms, so the difference is a rate over half a
                    // second; a partly written chunk still produces a sensible number, and
                    // a stall shows as zero rather than as the last happy value.
                    let delta = size.saturating_sub(task.bytes);
                    task.speed = delta.saturating_mul(2);
                    task.bytes = size;
                    changed = true;
                } else if task.speed != 0 {
                    task.speed = 0;
                    changed = true;
                }
            }
            tasks.clone()
        };
        if changed {
            let _ = app.emit("download:changed", tasks);
        }
    });
}

/// Prefer the name the server suggested, fall back to the URL, then avoid
/// overwriting an existing file.
pub fn target_for(dir: &Path, url: &str, suggested: &str) -> PathBuf {
    let name = if suggested.trim().is_empty() {
        derive_name(url)
    } else {
        sanitize(suggested)
    };

    let candidate = dir.join(&name);
    if !candidate.exists() {
        return candidate;
    }

    let (stem, ext) = split_name(&name);
    for counter in 1..1000 {
        let candidate = dir.join(format!("{stem} ({counter}){ext}"));
        if !candidate.exists() {
            return candidate;
        }
    }
    candidate
}

fn derive_name(url: &str) -> String {
    let raw = url.split(['?', '#']).next().unwrap_or_default();
    let last = raw
        .rsplit('/')
        .find(|segment| !segment.is_empty())
        .unwrap_or("download");
    sanitize(last)
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_default()
}

fn split_name(name: &str) -> (String, String) {
    match name.rsplit_once('.') {
        Some((stem, ext)) if !stem.is_empty() && !ext.is_empty() => {
            (stem.to_string(), format!(".{ext}"))
        }
        _ => (name.to_string(), String::new()),
    }
}

fn sanitize(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    for ch in name.chars() {
        if ch.is_ascii_alphanumeric() || matches!(ch, '.' | '-' | '_' | ' ') {
            out.push(ch);
        } else {
            out.push('-');
        }
    }
    if out.len() > 120 {
        out.truncate(120);
    }
    let trimmed = out.trim_matches(['.', ' ', '-']);
    if trimmed.is_empty() {
        "download".to_string()
    } else {
        trimmed.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("mix-dl-test-{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("scratch");
        dir
    }

    #[test]
    fn keeps_plain_names() {
        assert_eq!(sanitize("report.pdf"), "report.pdf");
        assert_eq!(sanitize("my file 1.zip"), "my file 1.zip");
    }

    #[test]
    fn strips_dangerous_characters() {
        assert_eq!(sanitize("../../etc/passwd"), "etc-passwd");
        assert_eq!(sanitize("a/b\\c:d"), "a-b-c-d");
        assert_eq!(sanitize("..."), "download");
        assert_eq!(sanitize(""), "download");
    }

    #[test]
    fn derives_name_from_url() {
        let dir = scratch("names");
        let path = target_for(&dir, "https://example.com/files/report.pdf?token=1", "");
        assert_eq!(path.file_name().unwrap(), "report.pdf");
    }

    #[test]
    fn prefers_the_suggested_name() {
        let dir = scratch("suggested");
        let path = target_for(&dir, "https://example.com/download?id=7", "Mozilla Installer.exe");
        assert_eq!(path.file_name().unwrap(), "Mozilla Installer.exe");
    }

    #[test]
    fn avoids_overwriting() {
        let dir = scratch("collide");
        std::fs::write(dir.join("report.pdf"), b"x").expect("seed");
        let path = target_for(&dir, "https://example.com/report.pdf", "");
        assert_eq!(path.file_name().unwrap(), "report (1).pdf");
    }
}