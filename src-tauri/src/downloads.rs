use crate::ipc::AppState;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use tauri::{webview::DownloadEvent, Emitter, Manager, Webview};

/// What a download is doing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    /// The loop should be making progress.
    Started,
    /// Stopped on purpose, bytes kept, resumable.
    Interrupted,
    /// All bytes are on disk.
    Finished,
    /// Stopped for a reason that may not come back.
    Failed,
}

impl Phase {
    pub fn as_str(self) -> &'static str {
        match self {
            Phase::Started => "started",
            Phase::Interrupted => "interrupted",
            Phase::Finished => "finished",
            Phase::Failed => "failed",
        }
    }

    pub fn parse(text: &str) -> Phase {
        match text {
            "started" => Phase::Started,
            "interrupted" => Phase::Interrupted,
            "finished" => Phase::Finished,
            _ => Phase::Failed,
        }
    }

    /// True when a loop ought to exist for this task.
    pub fn is_live(self) -> bool {
        self == Phase::Started
    }

    /// True when asking again should continue this task rather than start another.
    pub fn is_retryable(self) -> bool {
        matches!(self, Phase::Interrupted | Phase::Failed)
    }

    /// What a pause or resume asked for should look like once applied.
    pub fn after_pause(paused: bool) -> Phase {
        if paused {
            Phase::Interrupted
        } else {
            Phase::Started
        }
    }
}

/// Turns a failed or interrupted task back into a running one.
pub fn retry_phase(phase: Phase) -> Phase {
    if phase.is_retryable() {
        Phase::Started
    } else {
        phase
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadTask {
    pub id: u64,
    pub url: String,
    pub file_name: String,
    pub path: String,
    /// started | finished | failed
    pub state: String,
    /// Bytes written so far.
    pub bytes: u64,
    /// Bytes per second over the last sample.
    pub speed: u64,
    /// True while the download operation is paused.
    pub paused: bool,
    /// Size the server announced, when it did.
    pub total: Option<u64>,
}

static NEXT_ID: AtomicU64 = AtomicU64::new(1);
/// The download operations this session has started, by url.
#[cfg(windows)]
#[allow(dead_code)]
pub mod ops {
    use std::collections::HashMap;
    use std::sync::{Mutex, OnceLock};
    use webview2_com::Microsoft::Web::WebView2::Win32::{
        ICoreWebView2DownloadOperation, ICoreWebView2_4,
    };
    use webview2_com::DownloadStartingEventHandler;
    use tauri::Manager;
    use windows_core::{Interface, PWSTR};

    /// The file name the browser would have used, taken from its chosen path.
    fn suggested_name(
        args: &webview2_com::Microsoft::Web::WebView2::Win32::ICoreWebView2DownloadStartingEventArgs,
    ) -> String {
        let mut path = PWSTR::null();
        if unsafe { args.ResultFilePath(&mut path) }.is_err() || path.is_null() {
            return String::new();
        }
        unsafe { path.to_string() }
            .ok()
            .and_then(|text| {
                std::path::Path::new(&text)
                    .file_name()
                    .map(|name| name.to_string_lossy().to_string())
            })
            .unwrap_or_default()
    }

    /// COM operation handles, shared across threads.
    struct Ops(Mutex<HashMap<String, ICoreWebView2DownloadOperation>>);
    unsafe impl Send for Ops {}
    unsafe impl Sync for Ops {}

    static OPS: OnceLock<Ops> = OnceLock::new();

    fn ops() -> &'static Ops {
        OPS.get_or_init(|| Ops(Mutex::new(HashMap::new())))
    }

    /// Remembers the operation under the url it fetches.
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
        let Some(state) = webview.app_handle().try_state::<crate::ipc::AppState>() else {
            return;
        };
        let app = webview.app_handle().clone();
        let downloads = state.dirs.downloads.clone();
        let _ = webview.with_webview(move |platform| unsafe {
            let controller = platform.controller();
            let Ok(core) = controller.CoreWebView2() else {
                return;
            };
            let app = app.clone();
            let downloads = downloads.clone();
            let handler = DownloadStartingEventHandler::create(Box::new(move |_sender, args| {
                let Some(args) = args else {
                    return Ok(());
                };
                let Ok(operation) = args.DownloadOperation() else {
                    return Ok(());
                };
                let mut uri = PWSTR::null();
                if operation.Uri(&mut uri).is_err() || uri.is_null() {
                    return Ok(());
                }
                let Ok(url) = uri.to_string() else {
                    return Ok(());
                };
                let suggested = suggested_name(&args);
                let target = crate::downloads::target_for(&downloads, &url, &suggested);
                let _ = args.SetCancel(true);
                crate::logging::info(&format!("download via engine: {}", target.display()));
                crate::downloads::start_with_engine(&app, &url, &target);
                Ok(())
            }));
            if let Ok(core) = core.cast::<ICoreWebView2_4>() {
                let mut token = 0;
                let _ = core.add_DownloadStarting(&handler, &mut token);
            }
        });
    }

    /// Runs something against the operation for this url.
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

    /// Pauses, and says whether it took effect.
    pub fn pause(url: &str) -> Result<String, String> {
        for _ in 0..10 {
            let state = with_op(url, |operation| unsafe { operation.Pause() })?;
            if state == "interrupted" || state == "completed" {
                return Ok(state);
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
            let now = state_of(url)?;
            if now == "interrupted" || now == "completed" {
                return Ok(now);
            }
        }
        Err(format!("still {} after pausing", state_of(url)?))
    }

    pub fn resume(url: &str) -> Result<String, String> {
        let state = with_op(url, |operation| unsafe { operation.Resume() })?;
        Ok(state)
    }

    /// Cancels, resuming first when the transfer is paused.
    pub fn cancel(url: &str) -> Result<String, String> {
        let state = state_of(url)?;
        if state == "interrupted" {
            let _ = with_op(url, |operation| unsafe { operation.Resume() });
            std::thread::sleep(std::time::Duration::from_millis(120));
        }
        match with_op(url, |operation| unsafe { operation.Cancel() }) {
            Ok(state) => Ok(state),
            Err(err) => Err(err),
        }
    }

    /// The state the browser reports for this operation.
    pub fn state_of(url: &str) -> Result<String, String> {
        with_op(url, |_| Ok(()))
    }
}

#[cfg(not(windows))]
#[allow(dead_code)]
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
    pub fn state_of(_url: &str) -> Result<String, String> {
        Err("not supported on this platform".into())
    }
}

const KEEP: usize = 50;

/// Takes the destination, records the task and lets the strip show it.
pub fn handle(webview: Webview, event: DownloadEvent<'_>) -> bool {
    let app = webview.app_handle().clone();

    match event {
        DownloadEvent::Requested { url, destination } => {
            let Some(state) = app.try_state::<AppState>() else {
                return true;
            };
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
        _ => true,
    }
}



/// Where the download list is kept between runs.
fn history_file(dirs: &crate::paths::AppDirs) -> std::path::PathBuf {
    dirs.root.join("downloads.json")
}

/// Writes the list, so the downloads page still shows what happened last time.
pub fn save_history(app: &tauri::AppHandle) {
    let Some(state) = app.try_state::<AppState>() else {
        return;
    };
    let tasks = state.downloads.read().unwrap_or_else(|e| e.into_inner()).clone();
    let path = history_file(&state.dirs);
    if let Ok(bytes) = serde_json::to_vec_pretty(&tasks) {
        if let Err(err) = crate::paths::write_atomic(&path, &bytes) {
            crate::logging::warn(&format!("cannot save downloads: {err}"));
        }
    }
}

/// Reads the list back; anything that was running comes back as interrupted.
pub fn load_history(dirs: &crate::paths::AppDirs) -> Vec<DownloadTask> {
    let Ok(text) = std::fs::read_to_string(history_file(dirs)) else {
        return Vec::new();
    };
    let Ok(mut tasks) = serde_json::from_str::<Vec<DownloadTask>>(&text) else {
        return Vec::new();
    };
    for task in tasks.iter_mut() {
        if Phase::parse(&task.state).is_live() {
            task.state = Phase::Interrupted.as_str().to_string();
            task.speed = 0;
        }
    }
    tasks
}


/// Marks rows that claim to be running when nothing is.
pub fn reconcile(app: &tauri::AppHandle) {
    let Some(state) = app.try_state::<AppState>() else {
        return;
    };
    let stale: Vec<u64> = {
        let tasks = state.downloads.read().unwrap_or_else(|e| e.into_inner());
        tasks
            .iter()
            .filter(|task| Phase::parse(&task.state).is_live() && !crate::fetch::busy(task.id))
            .map(|task| task.id)
            .collect()
    };
    for id in stale {
        crate::logging::info(&format!("download {id} was not actually running"));
        set_state(app, id, "interrupted");
    }
}

/// The task for this id, if it still exists.
fn task_index(app: &tauri::AppHandle, id: u64) -> Option<usize> {
    let state = app.try_state::<AppState>()?;
    let tasks = state.downloads.read().unwrap_or_else(|e| e.into_inner());
    tasks.iter().position(|task| task.id == id)
}

/// Records the size the server announced, which is what makes a percentage possible.
pub fn set_total(app: &tauri::AppHandle, id: u64, total: Option<u64>) {
    let Some(index) = task_index(app, id) else {
        return;
    };
    let Some(state) = app.try_state::<AppState>() else {
        return;
    };
    let tasks = {
        let mut tasks = state.downloads.write().unwrap_or_else(|e| e.into_inner());
        if let Some(task) = tasks.get_mut(index) {
            task.total = total;
        }
        tasks.clone()
    };
    let _ = app.emit("download:changed", tasks);
}

/// Updates how far a transfer has got and how fast it is going.
pub fn set_progress(app: &tauri::AppHandle, id: u64, bytes: u64, speed: u64) {
    let Some(index) = task_index(app, id) else {
        return;
    };
    let Some(state) = app.try_state::<AppState>() else {
        return;
    };
    let mut changed = false;
    let tasks = {
        let mut tasks = state.downloads.write().unwrap_or_else(|e| e.into_inner());
        if let Some(task) = tasks.get_mut(index) {
            if task.bytes != bytes || task.speed != speed {
                task.bytes = bytes;
                task.speed = speed;
                changed = true;
            }
        }
        tasks.clone()
    };
    if changed {
        let _ = app.emit("download:changed", tasks);
    }
}

/// Moves a task to another state, and reports it.
pub fn set_state(app: &tauri::AppHandle, id: u64, next: &str) {
    let next = Phase::parse(next);
    let next = next.as_str();
    let Some(index) = task_index(app, id) else {
        return;
    };
    let Some(state) = app.try_state::<AppState>() else {
        return;
    };
    let tasks = {
        let mut tasks = state.downloads.write().unwrap_or_else(|e| e.into_inner());
        if let Some(task) = tasks.get_mut(index) {
            task.state = next.to_string();
            task.paused = next == "interrupted";
            if next == "finished" {
                task.bytes = std::fs::metadata(&task.path)
                    .map(|meta| meta.len())
                    .unwrap_or(task.bytes);
                task.total = Some(task.bytes);
            }
            if next != "started" {
                task.speed = 0;
            }
        }
        tasks.clone()
    };
    crate::logging::info(&format!("download {id} -> {next}"));
    let _ = app.emit("download:changed", tasks);
    save_history(app);
}

/// Starts a transfer with the internal engine and records it as a task.
pub fn start_with_engine(app: &tauri::AppHandle, url: &str, path: &Path) -> u64 {
    let existing = {
        let Some(state) = app.try_state::<AppState>() else {
            return 0;
        };
        let tasks = state.downloads.read().unwrap_or_else(|e| e.into_inner());
        tasks
            .iter()
            .rev()
            .find(|task| task.url == url && Phase::parse(&task.state).is_retryable())
            .map(|task| (task.id, task.path.clone(), task.state.clone()))
    };
    let (id, path) = match existing {
        Some((id, held, held_state)) if !held.is_empty() => {
            let held = Path::new(&held);
            crate::logging::info(&format!(
                "download {id} continued for {}",
                held.display()
            ));
            set_state(app, id, retry_phase(Phase::parse(&held_state)).as_str());
            (id, held.to_path_buf())
        }
        _ => {
            let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
            record_into(app, id, url, path);
            (id, path.to_path_buf())
        }
    };
    let path = path.as_path();
    if !crate::fetch::claim(path) {
        crate::logging::warn(&format!(
            "download {id}: {} is already being written",
            path.display()
        ));
        return id;
    }
    crate::fetch::start(app.clone(), id, url.to_string(), path.to_path_buf());
    id
}

/// Adds a task with an id of the caller's choosing, so the engine and the task agree.
fn record_into(app: &tauri::AppHandle, id: u64, url: &str, path: &Path) {
    let Some(state) = app.try_state::<AppState>() else {
        return;
    };
    let tasks = {
        let mut tasks = state.downloads.write().unwrap_or_else(|e| e.into_inner());
        tasks.push(DownloadTask {
            id,
            url: url.to_string(),
            file_name: file_name(path),
            path: path.display().to_string(),
            state: "started".to_string(),
            bytes: 0,
            speed: 0,
            paused: false,
            total: None,
        });
        if tasks.len() > KEEP {
            let drop_count = tasks.len() - KEEP;
            tasks.drain(0..drop_count);
        }
        tasks.clone()
    };
    let _ = app.emit("download:changed", tasks);
}
/// Fills in the byte count for a finished task from the file on disk.
fn finish_size(path: &str) -> u64 {
    std::fs::metadata(path).map(|meta| meta.len()).unwrap_or(0)
}

/// Records a file the app wrote itself, so it shows up in the list.
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
                    total: None,
                });
                if tasks.len() > KEEP {
                    let drop_count = tasks.len() - KEEP;
                    tasks.drain(0..drop_count);
                }
            }
            _ => {
                let target = tasks
                    .iter()
                    .rposition(|task| Some(task.url.as_str()) == url && task.state == "started");
                if let Some(index) = target {
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
    save_history(app);
}

/// Polls the size on disk while a download runs.
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

/// Picks the file name and avoids overwriting an existing file.
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

    #[test]
    fn a_pause_leaves_something_resumable() {
        assert_eq!(Phase::after_pause(true), Phase::Interrupted);
        assert_eq!(Phase::after_pause(false), Phase::Started);
        assert!(Phase::Interrupted.is_retryable());
        assert!(!Phase::Interrupted.is_live());
    }

    #[test]
    fn only_a_running_task_has_a_loop() {
        assert!(Phase::Started.is_live());
        assert!(!Phase::Finished.is_live());
        assert!(!Phase::Failed.is_live());
        assert!(!Phase::Interrupted.is_live());
    }

    #[test]
    fn a_finished_download_is_not_retried_in_place() {
        assert!(!Phase::Finished.is_retryable());
        assert_eq!(retry_phase(Phase::Finished), Phase::Finished);
        assert_eq!(retry_phase(Phase::Failed), Phase::Started);
        assert_eq!(retry_phase(Phase::Interrupted), Phase::Started);
    }

    #[test]
    fn unknown_text_reads_as_failed() {
        assert_eq!(Phase::parse("interrupted"), Phase::Interrupted);
        assert_eq!(Phase::parse("finished"), Phase::Finished);
        assert_eq!(Phase::parse("something else"), Phase::Failed);
        assert_eq!(Phase::parse(""), Phase::Failed);
    }

    #[test]
    fn every_phase_survives_a_round_trip() {
        for phase in [
            Phase::Started,
            Phase::Interrupted,
            Phase::Finished,
            Phase::Failed,
        ] {
            assert_eq!(Phase::parse(phase.as_str()), phase);
        }
    }
}