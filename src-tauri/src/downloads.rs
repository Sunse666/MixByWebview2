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
    /// Stopped by the user, bytes kept, resumable.
    Paused,
    /// Stopped by the network or the server, bytes kept, resumable.
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
            Phase::Paused => "paused",
            Phase::Interrupted => "interrupted",
            Phase::Finished => "finished",
            Phase::Failed => "failed",
        }
    }

    pub fn parse(text: &str) -> Phase {
        match text {
            "started" => Phase::Started,
            "paused" => Phase::Paused,
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
        matches!(self, Phase::Paused | Phase::Interrupted | Phase::Failed)
    }

    /// What a pause or resume asked for should look like once applied.
    ///
    /// The two stoppage states are not the same thing to a reader: one was asked for, the
    /// other happened. They only looked alike because both keep the bytes on disk.
    pub fn after_pause(paused: bool) -> Phase {
        if paused {
            Phase::Paused
        } else {
            Phase::Started
        }
    }
}

fn is_false(value: &bool) -> bool {
    !*value
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
    /// Set when the tab that started this download was private. The row is shown while
    /// the browser runs, but is not written to the list on disk.
    #[serde(default, skip_serializing_if = "is_false")]
    pub incognito: bool,
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
    /// Why the last attempt failed, in words a reader can act on.
    ///
    /// Only meaningful while `state` is `failed`. A row that says "failed" and nothing else
    /// leaves the reader guessing whether to wait, sign in, or give up; this is what the
    /// downloads page shows under the name.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub error: String,
    /// The page whose cookies this download is entitled to.
    ///
    /// A retry has to send what the first attempt sent, or a file behind a login comes back
    /// as the login page. The cookies themselves are deliberately not stored: this list is
    /// written to `downloads.json`, which the backup export packages up, so persisting them
    /// would put a working session into a zip the reader mails to their other machine. The
    /// page is recorded instead, and the cookies are read from it again when needed - which
    /// is both smaller and safe to keep on disk.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub source: String,
}

static NEXT_ID: AtomicU64 = AtomicU64::new(1);
/// The download operations this session has started, by url.
#[cfg(windows)]
#[allow(dead_code)]
pub mod ops {
    use std::collections::HashMap;
    use std::sync::{Mutex, OnceLock};
    use tauri::Manager;
    use webview2_com::DownloadStartingEventHandler;
    use webview2_com::Microsoft::Web::WebView2::Win32::{
        ICoreWebView2DownloadOperation, ICoreWebView2_4,
    };
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
    fn remember(
        args: &webview2_com::Microsoft::Web::WebView2::Win32::ICoreWebView2DownloadStartingEventArgs,
    ) -> Option<String> {
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
        let label = webview.label().to_string();
        let _ = webview.with_webview(move |platform| unsafe {
            let controller = platform.controller();
            let Ok(core) = controller.CoreWebView2() else {
                return;
            };
            let app = app.clone();
            let downloads = downloads.clone();
            let label = label.clone();
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
                // The work starts on a thread of its own rather than here: this handler
                // runs on the WebView2 thread, and collecting the cookies asks that same
                // thread a question - which it could not answer while it was in here.
                let _ = args.SetCancel(true);
                crate::logging::info(&format!("download via engine: {}", target.display()));
                let app = app.clone();
                let label = label.clone();
                std::thread::spawn(move || {
                    crate::downloads::start_with_engine_from(&app, &url, &target, &label);
                });
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
        let operation = map
            .get(url)
            .ok_or_else(|| "no operation for that url".to_string())?;
        let state = unsafe {
            let mut state =
                webview2_com::Microsoft::Web::WebView2::Win32::COREWEBVIEW2_DOWNLOAD_STATE::default(
                );
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
        with_op(url, |operation| unsafe { operation.Cancel() })
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

/// Drops the oldest rows once the list is longer than it is allowed to be.
///
/// One rule rather than the same `drain` written at each place a task is added. The rows it
/// drops are only forgotten, never deleted: the files they describe are the reader's.
fn prune(tasks: &mut Vec<DownloadTask>) {
    if tasks.len() > KEEP {
        let drop_count = tasks.len() - KEEP;
        tasks.drain(0..drop_count);
    }
}

/// Chooses where a download lands, and lets it run.
///
/// On Windows this is only the second half of the story: `ops::watch` hangs a second
/// handler on the same `DownloadStarting` event, cancels the browser's own transfer and
/// fetches the file with the internal engine instead. That engine records the task, so
/// recording it here as well left two rows in the list for every download - one that
/// belonged to a transfer which was cancelled a moment later, and one that finished.
///
/// The other platforms have no such handler (`ops::watch` is empty there), so this
/// remains the only place a task is written down.
pub fn handle(webview: Webview, event: DownloadEvent<'_>) -> bool {
    let app = webview.app_handle().clone();

    // True on the platforms where this handler is the one that owns the task row.
    let owns_the_row = !cfg!(windows);

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
            if owns_the_row {
                record(&app, Some(url.as_str()), Some(&target), "started");
            }
            true
        }
        DownloadEvent::Finished { url, path, success } => {
            if owns_the_row {
                let state = if success { "finished" } else { "failed" };
                record(&app, Some(url.as_str()), path.as_deref(), state);
            }
            success
        }
        _ => true,
    }
}

/// Where the download list is kept between runs.
///
/// Under UserData, with the other state: it used to sit in the app root beside the exe,
/// which put it outside both the backup and the clear-data action - a list of everything
/// downloaded survived both.
fn history_file(dirs: &crate::paths::AppDirs) -> std::path::PathBuf {
    dirs.user_data.join("downloads.json")
}
/// The same list under the name it had before the move, read once so a profile that is
/// upgraded does not lose the downloads it remembers.
fn legacy_history_file(dirs: &crate::paths::AppDirs) -> std::path::PathBuf {
    dirs.root.join("downloads.json")
}

/// Writes the list, so the downloads page still shows what happened last time.
///
/// A download started from a private tab is left out: the row is on screen while the
/// browser runs, and a file listing every private download on disk is the one thing
/// private mode exists to prevent.
pub fn save_history(app: &tauri::AppHandle) {
    let Some(state) = app.try_state::<AppState>() else {
        return;
    };
    let tasks: Vec<DownloadTask> = state
        .downloads
        .read()
        .iter()
        .filter(|task| !task.incognito)
        .cloned()
        .collect();
    let path = history_file(&state.dirs);
    if let Ok(bytes) = serde_json::to_vec_pretty(&tasks) {
        if let Err(err) = crate::paths::write_atomic(&path, &bytes) {
            crate::logging::warn(&format!("cannot save downloads: {err}"));
        }
    }
}

/// Reads the list back; anything that was running comes back as interrupted.
pub fn load_history(dirs: &crate::paths::AppDirs) -> Vec<DownloadTask> {
    // A profile written before the move keeps its list in the app root. It is read from
    // there once and then written to the new place, so the list is not lost on upgrade.
    let legacy = legacy_history_file(dirs);
    let text = match std::fs::read_to_string(history_file(dirs)) {
        Ok(text) => text,
        Err(_) => {
            let Ok(text) = std::fs::read_to_string(&legacy) else {
                return Vec::new();
            };
            let _ = std::fs::remove_file(&legacy);
            text
        }
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
    // The counter starts at 1 in a fresh process, so a list read back from disk shares its
    // ids with the first download of this session. Every command that takes an id - pause,
    // resume, cancel, open, retry - then acts on whichever row it finds first, which is often
    // the historical one: pausing this session's transfer paused a file from last week.
    // Measured once with eight rows on disk, all of them id 1.
    let tasks = dedupe(tasks);
    reserve_ids(tasks.iter().map(|task| task.id + 1).max().unwrap_or(1));
    let mut tasks = tasks;
    prune(&mut tasks);
    tasks
}

/// Makes every id in a restored list unique, and hands the highest back.
///
/// A list written by a build without this already contains duplicates, and they would
/// survive every later save. The first row keeps its id so a reader's existing rows stay
/// recognisable; the rest are given fresh ones past the highest seen.
fn dedupe(tasks: Vec<DownloadTask>) -> Vec<DownloadTask> {
    let highest = tasks.iter().map(|task| task.id).max().unwrap_or(0);
    let mut next = highest + 1;
    let mut seen = std::collections::HashSet::new();
    tasks
        .into_iter()
        .map(|mut task| {
            if !seen.insert(task.id) {
                task.id = next;
                next += 1;
                seen.insert(task.id);
            }
            task
        })
        .collect()
}

/// Pushes the id counter past what a restored list already uses.
fn reserve_ids(next: u64) {
    let mut current = NEXT_ID.load(Ordering::Relaxed);
    while current < next {
        match NEXT_ID.compare_exchange(current, next, Ordering::Relaxed, Ordering::Relaxed) {
            Ok(_) => return,
            Err(actual) => current = actual,
        }
    }
}

/// Marks rows that claim to be running when nothing is.
pub fn reconcile(app: &tauri::AppHandle) {
    let Some(state) = app.try_state::<AppState>() else {
        return;
    };
    let stale: Vec<u64> = {
        let tasks = state.downloads.read();
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
    let tasks = state.downloads.read();
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
        let mut tasks = state.downloads.write();
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
        let mut tasks = state.downloads.write();
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
        let mut tasks = state.downloads.write();
        if let Some(task) = tasks.get_mut(index) {
            task.state = next.to_string();
            task.paused = next == "paused";
            if next == "finished" {
                task.bytes = std::fs::metadata(&task.path)
                    .map(|meta| meta.len())
                    .unwrap_or(task.bytes);
                task.total = Some(task.bytes);
                // A file that arrived is not a file that failed.
                task.error.clear();
            }
            if next != "started" {
                task.speed = 0;
            }
            if next == "started" {
                // Asking again clears the reason the last attempt gave: it is about to be
                // either fixed or replaced.
                task.error.clear();
            }
        }
        tasks.clone()
    };
    crate::logging::info(&format!("download {id} -> {next}"));
    let _ = app.emit("download:changed", tasks);
    save_history(app);
}

/// The size the first attempt learned, for a resume to compare against.
///
/// Needed because a 416 does not always carry `Content-Range` - bilibili's CDN sends none -
/// and without it "the file is already whole" cannot be told from "the file is a leftover of
/// something else".
pub fn recorded_total(app: &tauri::AppHandle, id: u64) -> Option<u64> {
    app.try_state::<AppState>()?
        .downloads
        .read()
        .iter()
        .find(|task| task.id == id)
        .and_then(|task| task.total)
}

/// Marks a task failed, with the reason kept for the page to show.
///
/// Written in one pass rather than `set_state` followed by a second update: that wrote the
/// list to disk twice for one failure, and the first copy was the one without the reason, so
/// a reader who opened the file saw `failed` and nothing else.
pub fn set_failed(app: &tauri::AppHandle, id: u64, reason: &str) {
    let Some(index) = task_index(app, id) else {
        return;
    };
    let Some(state) = app.try_state::<AppState>() else {
        return;
    };
    let tasks = {
        let mut tasks = state.downloads.write();
        if let Some(task) = tasks.get_mut(index) {
            task.state = Phase::Failed.as_str().to_string();
            task.paused = false;
            task.speed = 0;
            task.error = reason.to_string();
        }
        tasks.clone()
    };
    crate::logging::warn(&format!("download {id} failed: {reason}"));
    let _ = app.emit("download:changed", tasks);
    save_history(app);
}

/// Starts a transfer with the internal engine and records it as a task.
pub fn start_with_engine(app: &tauri::AppHandle, url: &str, path: &Path) -> u64 {
    start_with_engine_from(app, url, path, "")
}

/// Starts a transfer, telling the server what the page would have sent.
///
/// `tab` is the webview the download was started from: it supplies the referrer and the
/// cookies. Both are best effort - a tab that is already gone sends nothing, which is
/// what every download did before.
pub fn start_with_engine_from(app: &tauri::AppHandle, url: &str, path: &Path, tab: &str) -> u64 {
    let request = request_for(app, tab);
    let existing = {
        let Some(state) = app.try_state::<AppState>() else {
            return 0;
        };
        let tasks = state.downloads.read();
        tasks
            .iter()
            .rev()
            .find(|task| task.url == url && Phase::parse(&task.state).is_retryable())
            .map(|task| (task.id, task.path.clone(), task.state.clone()))
    };
    let (id, path) = match existing {
        Some((id, held, held_state)) if !held.is_empty() => {
            let held = Path::new(&held);
            crate::logging::info(&format!("download {id} continued for {}", held.display()));
            set_state(app, id, retry_phase(Phase::parse(&held_state)).as_str());
            (id, held.to_path_buf())
        }
        _ => {
            let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
            record_into(app, id, url, path, is_private(app, tab), tab);
            (id, path.to_path_buf())
        }
    };
    let path = path.as_path();
    if !crate::fetch::claim(path) {
        crate::logging::warn(&format!(
            "download {id}: {} is already being written",
            path.display()
        ));
        // The row is told, not just the log: this used to return the id as if the transfer
        // had started, so the list showed a download that would never move and nothing said
        // why. It happens when the same url is asked for twice at once.
        set_failed(app, id, "this file is already being downloaded");
        return id;
    }
    crate::fetch::start_with(
        app.clone(),
        id,
        url.to_string(),
        path.to_path_buf(),
        request,
    );
    id
}

/// What the tab with this label would send, found from a task's recorded source.
///
/// The label a download recorded may belong to a tab that has since been closed or
/// navigated; a label that no longer resolves sends nothing, which is what every download
/// did before this existed. A tab that is still open sends its current cookies, which is
/// what the reader would expect from pressing retry in the same session.
pub(crate) fn request_for_task(app: &tauri::AppHandle, source: &str) -> crate::fetch::Request {
    request_for(app, source)
}

/// The tab label a task recorded as its origin, or empty.
pub fn task_source(app: &tauri::AppHandle, id: u64) -> String {
    app.try_state::<AppState>()
        .and_then(|state| {
            state
                .downloads
                .read()
                .iter()
                .find(|task| task.id == id)
                .map(|task| task.source.clone())
        })
        .unwrap_or_default()
}

/// True when the tab behind a download was a private one.
///
/// A label that no longer resolves counts as public, which is the behaviour every
/// download had before this was tracked.
fn is_private(app: &tauri::AppHandle, tab: &str) -> bool {
    if tab.is_empty() {
        return false;
    }
    app.try_state::<AppState>()
        .and_then(|state| {
            state
                .try_read_core(crate::ipc::window_of_tab(tab))
                .and_then(|core| {
                    core.tabs
                        .iter()
                        .find(|entry| entry.id == tab)
                        .map(|entry| entry.incognito)
                })
        })
        .unwrap_or(false)
}

/// What the tab with this label would send along with its next request.
///
/// Cookies are read over the DevTools protocol because the webview exposes no other
/// interface for them, and they stay inside the process. A tab that is gone, a
/// background webview that refuses the call and a non-Windows build all produce an
/// empty context: the download then works exactly as it did before.
pub(crate) fn request_for(app: &tauri::AppHandle, tab: &str) -> crate::fetch::Request {
    if tab.is_empty() {
        return crate::fetch::Request::default();
    }
    let Some(webview) = app.get_webview(tab) else {
        return crate::fetch::Request::default();
    };
    let page = webview.url().map(|url| url.to_string()).unwrap_or_default();
    crate::fetch::Request {
        referer: if page.starts_with("http") {
            Some(page)
        } else {
            None
        },
        cookie: crate::fetch::cookies_for(&webview),
    }
}

/// Adds a task with an id of the caller's choosing, so the engine and the task agree.
///
/// `source` is the webview label the download came from. It is recorded so a retry - this
/// session or a later one - can ask that page for its cookies again.
fn record_into(
    app: &tauri::AppHandle,
    id: u64,
    url: &str,
    path: &Path,
    incognito: bool,
    source: &str,
) {
    let Some(state) = app.try_state::<AppState>() else {
        return;
    };
    let tasks = {
        let mut tasks = state.downloads.write();
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
            incognito,
            error: String::new(),
            source: source.to_string(),
        });
        prune(&mut tasks);
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
    record_from(app, url, path, state, false)
}

/// Records a download, saying whether the tab behind it was private.
fn record_from(
    app: &tauri::AppHandle,
    url: Option<&str>,
    path: Option<&Path>,
    state: &str,
    incognito: bool,
) {
    let Some(guard) = app.try_state::<AppState>() else {
        return;
    };

    let tasks = {
        let mut tasks = guard.downloads.write();
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
                    incognito,
                    error: String::new(),
                    // This path is the one that records a download the browser itself
                    // started (the non-Windows branch of `handle`), and it has no webview
                    // label to attribute it to.
                    source: String::new(),
                });
                prune(&mut tasks);
            }
            _ => {
                let target = tasks
                    .iter()
                    .rposition(|task| Some(task.url.as_str()) == url && task.state == "started");
                if let Some(index) = target {
                    // The browser reported this download, but the bytes are written by
                    // the engine: the event says the browser stopped caring, not that the
                    // file is done. Whatever the engine last wrote is the truer answer, and
                    // it is the only one that can tell a pause from a broken connection.
                    let engine = Phase::parse(&tasks[index].state);
                    let keep_engine = matches!(engine, Phase::Paused | Phase::Interrupted);
                    tasks[index].state = if keep_engine {
                        engine.as_str().to_string()
                    } else {
                        state.to_string()
                    };
                    tasks[index].paused = engine == Phase::Paused;
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
            let mut tasks = state.downloads.write();
            for task in tasks.iter_mut().filter(|task| {
                task.state == "started" || task.state == "interrupted" || task.state == "paused"
            }) {
                let size = std::fs::metadata(&task.path)
                    .map(|meta| meta.len())
                    .unwrap_or(0);
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

/// Makes a suggested name safe to put on disk.
///
/// The rule is the one Windows itself enforces, not "ascii only": the characters a file name
/// may not contain are `< > : " / \ | ? *` and the control range, and everything else is
/// allowed. Replacing every non-ascii character with a dash used to turn `中文标题.html` into
/// `html` - the whole name gone, only the extension left - and `Rust 程序设计.pdf` into
/// `Rust ----.pdf`, on a browser whose interface is half Chinese.
///
/// Length is counted in characters, not bytes: the 255-character limit is a character count,
/// and cutting a name at 120 *bytes* split multi-byte characters and chopped CJK names to
/// about 40 of them.
fn sanitize(name: &str) -> String {
    // Room for the " (12)" a collision adds and for the extension, inside the 255 Windows
    // allows. Counted in characters, which is what that limit means.
    const MAX_CHARS: usize = 120;

    let mut out = String::with_capacity(name.len());
    for ch in name.chars() {
        let illegal = matches!(
            ch,
            '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*' | '\0'
        ) || (ch as u32) < 0x20;
        if illegal {
            out.push('-');
        } else {
            out.push(ch);
        }
    }
    // A name that is nothing but dots, dashes and spaces, or empty, still has to become
    // something. A leading dash is also trimmed because a substituted separator can put one
    // there (`../../etc/passwd` becomes `..-..-etc-passwd`), and a name starting with a dash
    // is a command-line hazard if anything downstream passes it to a shell.
    let trimmed = out.trim_matches(['.', ' ', '-']);
    if trimmed.is_empty() {
        return "download".to_string();
    }
    // Truncate by characters, keeping the extension: a name cut mid-character is a name the
    // filesystem refuses, and losing the extension loses the association with a program.
    let (stem, extension) = split_name(trimmed);
    if trimmed.chars().count() <= MAX_CHARS {
        return trimmed.to_string();
    }
    // `split_name` keeps the dot with the extension, so this is the whole budget.
    let keep = MAX_CHARS.saturating_sub(extension.chars().count());
    let stem: String = stem.chars().take(keep.max(1)).collect();
    let trimmed_stem = stem.trim_end_matches(['.', ' ']);
    let trimmed_stem = if trimmed_stem.is_empty() {
        "download"
    } else {
        trimmed_stem
    };
    format!("{trimmed_stem}{extension}")
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
    fn ids_restored_from_disk_do_not_collide_with_this_session_s() {
        // The counter starts at 1 in a fresh process, so a list read back from disk used to
        // share its ids with the first download of the session: every command that takes an
        // id then found the historical row first. Measured once with eight rows on disk, all
        // of them id 1.
        let restored = dedupe(vec![row(1), row(1), row(1), row(7)]);
        let ids: Vec<u64> = restored.iter().map(|task| task.id).collect();
        assert_eq!(ids.len(), 4);
        let unique: std::collections::HashSet<u64> = ids.iter().copied().collect();
        assert_eq!(unique.len(), 4, "got {ids:?}");

        // And the one that was already 7 keeps it, so its row is still recognised.
        assert!(ids.contains(&7));
    }

    #[test]
    fn reserving_ids_never_moves_the_counter_backwards() {
        // Two lists read in sequence must not hand the same id out twice.
        reserve_ids(100);
        assert!(NEXT_ID.load(Ordering::Relaxed) >= 100);
        reserve_ids(3);
        assert!(
            NEXT_ID.load(Ordering::Relaxed) >= 100,
            "a smaller list must not lower the counter"
        );
    }

    fn row(id: u64) -> DownloadTask {
        DownloadTask {
            id,
            url: format!("https://example.com/{id}"),
            file_name: format!("{id}.bin"),
            path: format!("/tmp/{id}.bin"),
            state: "finished".to_string(),
            bytes: 1,
            speed: 0,
            paused: false,
            total: Some(1),
            incognito: false,
            error: String::new(),
            source: String::new(),
        }
    }

    #[test]
    fn the_list_is_capped_at_the_newest_rows() {
        // The cap used to be spelled out at each place a task was added, and `load_history`
        // had none at all: a list on disk written by an older build could come back longer
        // than the cap and stay that way. One rule, applied on the way in.
        let mut tasks: Vec<DownloadTask> = (0..KEEP + 12)
            .map(|index| DownloadTask {
                id: index as u64,
                url: format!("https://example.com/{index}"),
                file_name: format!("{index}.bin"),
                path: format!("/tmp/{index}.bin"),
                state: "finished".to_string(),
                bytes: 1,
                speed: 0,
                paused: false,
                total: Some(1),
                incognito: false,
                error: String::new(),
                source: String::new(),
            })
            .collect();
        prune(&mut tasks);
        assert_eq!(tasks.len(), KEEP);
        // The oldest go first, so what survives is the newest.
        assert_eq!(tasks.first().map(|task| task.id), Some(12));
        assert_eq!(tasks.last().map(|task| task.id), Some(KEEP as u64 + 11));
    }

    #[test]
    fn strips_dangerous_characters() {
        assert_eq!(sanitize("../../etc/passwd"), "etc-passwd");
        assert_eq!(sanitize("a/b\\c:d"), "a-b-c-d");
        assert_eq!(sanitize("..."), "download");
        assert_eq!(sanitize(""), "download");
        // The set Windows actually forbids, and nothing beyond it.
        assert_eq!(sanitize("a<b>c\"d|e?f*g"), "a-b-c-d-e-f-g");
        // A tab or a newline in a suggested name is a control character, not a space.
        assert_eq!(sanitize("a\tb\nc"), "a-b-c");
    }

    #[test]
    fn keeps_names_that_are_not_ascii() {
        // The regression: every non-ascii character became a dash, so a CJK title lost its
        // whole name and kept only the extension - `中文标题.html` was saved as `html`.
        assert_eq!(sanitize("中文标题.html"), "中文标题.html");
        assert_eq!(sanitize("报告 final.pdf"), "报告 final.pdf");
        assert_eq!(sanitize("Rust 程序设计.pdf"), "Rust 程序设计.pdf");
        assert_eq!(sanitize("café résumé.txt"), "café résumé.txt");
        assert_eq!(sanitize("Ελληνικά.md"), "Ελληνικά.md");

        // The forbidden characters are still replaced inside a non-ascii name.
        assert_eq!(sanitize("标题/第一章.pdf"), "标题-第一章.pdf");
    }

    #[test]
    fn a_long_name_is_cut_by_characters_and_keeps_its_extension() {
        // 200 CJK characters plus an extension: 200 characters, so it has to be cut, and
        // the cut has to leave a whole number of characters behind or the filesystem
        // rejects the result.
        let stem = "标".repeat(200);
        let name = sanitize(&format!("{stem}.pdf"));
        assert!(name.ends_with(".pdf"), "extension survived: {name}");
        assert_eq!(
            name.chars().count(),
            120,
            "counted in characters, not bytes"
        );
        assert!(!name.contains('\u{FFFD}'), "no character was split");

        // A name already inside the limit is untouched.
        let short = sanitize(&format!("{}.pdf", "标".repeat(20)));
        assert_eq!(short.chars().count(), 24);
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
        let path = target_for(
            &dir,
            "https://example.com/download?id=7",
            "Mozilla Installer.exe",
        );
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
        // A pause the user asked for reads differently from a connection that went
        // away, but both keep their bytes and both can be resumed.
        assert_eq!(Phase::after_pause(true), Phase::Paused);
        assert_eq!(Phase::after_pause(false), Phase::Started);
        assert!(Phase::Paused.is_retryable());
        assert!(Phase::Interrupted.is_retryable());
        assert!(!Phase::Paused.is_live());
        assert!(!Phase::Interrupted.is_live());
    }

    #[test]
    fn the_two_stoppages_stay_apart_on_the_way_to_disk() {
        assert_eq!(Phase::parse("paused"), Phase::Paused);
        assert_eq!(Phase::parse("interrupted"), Phase::Interrupted);
        assert_ne!(Phase::Paused.as_str(), Phase::Interrupted.as_str());
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
