use crate::paths::AppDirs;
use crate::state::Settings;
use crate::tabs::Tab;
use crate::theme::Theme;
use serde::Serialize;
use std::sync::RwLock;
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_runtime::ResizeDirection;

pub struct Core {
    pub settings: Settings,
    pub theme: Theme,
    /// Theme as it was when the editor opened, used by theme_revert.
    pub baseline: Theme,
    pub tabs: Vec<Tab>,
    pub selected: Option<String>,
    pub next_tab_id: usize,
    pub tick: u64,
}

pub struct AppState {
    pub dirs: AppDirs,
    pub core: RwLock<Core>,
}

impl AppState {
    pub fn new(dirs: AppDirs, settings: Settings, theme: Theme) -> Self {
        Self {
            dirs,
            core: RwLock::new(Core {
                settings,
                baseline: theme.clone(),
                theme,
                tabs: Vec::new(),
                selected: None,
                next_tab_id: 0,
                tick: 0,
            }),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct AppError {
    pub code: String,
    pub message: String,
}

impl AppError {
    fn new(code: &str, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
        }
    }
}

impl From<tauri::Error> for AppError {
    fn from(err: tauri::Error) -> Self {
        Self::new("tauri", err.to_string())
    }
}

impl From<std::io::Error> for AppError {
    fn from(err: std::io::Error) -> Self {
        Self::new("io", err.to_string())
    }
}

impl From<serde_json::Error> for AppError {
    fn from(err: serde_json::Error) -> Self {
        Self::new("serde", err.to_string())
    }
}

fn main_window(app: &AppHandle) -> Result<tauri::Window, AppError> {
    app.get_window("main")
        .ok_or_else(|| AppError::new("no_window", "main window is missing"))
}

#[derive(Clone, Serialize)]
pub struct TabMeta {
    id: String,
    url: String,
    title: String,
    alive: bool,
}

#[derive(Clone, Serialize)]
pub struct UrlPayload {
    pub id: String,
    pub url: String,
}

#[derive(Clone, Serialize)]
pub struct TabList {
    tabs: Vec<TabMeta>,
    selected: Option<String>,
}

fn snapshot(core: &Core) -> TabList {
    TabList {
        tabs: core
            .tabs
            .iter()
            .map(|tab| TabMeta {
                id: tab.id.clone(),
                url: tab.url.clone(),
                title: tab.title.clone(),
                alive: tab.alive,
            })
            .collect(),
        selected: core.selected.clone(),
    }
}

/// Tell the chrome UI to rebuild the tab strip.
pub(crate) fn emit_tabs(app: &AppHandle) {
    let state = app.state::<AppState>();
    let list = {
        let core = state.core.read().unwrap_or_else(|e| e.into_inner());
        snapshot(&core)
    };
    let _ = app.emit("tabs:changed", list);
}

/// Only the selected tab is visible; the rest keep running underneath.
pub(crate) fn sync_visibility(app: &AppHandle) {
    let pairs: Vec<(String, Option<String>)> = {
        let state = app.state::<AppState>();
        let core = state.core.read().unwrap_or_else(|e| e.into_inner());
        core.tabs
            .iter()
            .map(|tab| (tab.id.clone(), core.selected.clone()))
            .collect()
    };
    for (id, selected) in pairs {
        if let Some(webview) = app.get_webview(&id) {
            if selected.as_deref() == Some(id.as_str()) {
                let _ = webview.show();
            } else {
                let _ = webview.hide();
            }
        }
    }
}

/// Drop the least recently used background tab once too many are alive.
pub(crate) fn enforce_live_limit(app: &AppHandle) {
    loop {
        let victim = {
            let state = app.state::<AppState>();
            let mut core = state.core.write().unwrap_or_else(|e| e.into_inner());
            let alive = core.tabs.iter().filter(|tab| tab.alive).count();
            let limit = core.settings.max_live_tabs.max(1);
            if alive <= limit {
                None
            } else {
                let selected = core.selected.clone();
                let candidate = core
                    .tabs
                    .iter()
                    .filter(|tab| tab.alive && Some(&tab.id) != selected.as_ref())
                    .min_by_key(|tab| tab.last_active)
                    .map(|tab| tab.id.clone());
                if let Some(id) = &candidate {
                    if let Some(tab) = core.tabs.iter_mut().find(|tab| &tab.id == id) {
                        tab.alive = false;
                    }
                }
                candidate
            }
        };

        match victim {
            Some(id) => {
                if let Some(webview) = app.get_webview(&id) {
                    let _ = webview.close();
                }
                crate::logging::info(&format!("suspended {id}"));
                let _ = app.emit("tab:suspended", &id);
            }
            None => return,
        }
    }
}

pub(crate) fn save_session(app: &AppHandle) {
    let Some(state) = app.try_state::<AppState>() else {
        return;
    };
    let session = {
        let core = state.core.read().unwrap_or_else(|e| e.into_inner());
        let selected = core
            .selected
            .as_ref()
            .and_then(|id| core.tabs.iter().position(|tab| &tab.id == id))
            .unwrap_or(0);
        crate::session::Session {
            schema_version: crate::session::SCHEMA,
            tabs: core.tabs.iter().map(|tab| tab.url.clone()).collect(),
            selected,
        }
    };
    if let Err(err) = session.save(&state.dirs.session_file) {
        crate::logging::warn(&format!("session save failed: {err}"));
    }
}

fn spawn_tab(app: &AppHandle, id: &str, url: &str) -> Result<(), AppError> {
    let win = main_window(app)?;
    let (width, height) = crate::window::logical_size(&win)?;
    crate::tabs::create(&win, id, url, width, (height - crate::window::CHROME_H).max(1.0))?;
    Ok(())
}

#[derive(Serialize)]
pub struct Layout {
    chrome_height: f64,
    width: f64,
    height: f64,
    scale: f64,
    selected: Option<String>,
}

#[tauri::command]
pub async fn layout_get(app: AppHandle, state: State<'_, AppState>) -> Result<Layout, AppError> {
    let win = main_window(&app)?;
    let (width, height) = crate::window::logical_size(&win)?;
    let (chrome_height, selected) = {
        let core = state.core.read().unwrap_or_else(|e| e.into_inner());
        (core.settings.chrome_height, core.selected.clone())
    };
    Ok(Layout {
        chrome_height,
        width,
        height,
        scale: win.scale_factor()?,
        selected,
    })
}

#[tauri::command]
pub async fn window_control(app: AppHandle, action: String) -> Result<(), AppError> {
    let win = main_window(&app)?;
    match action.as_str() {
        "min" => win.minimize()?,
        "max" => {
            if win.is_maximized()? {
                win.unmaximize()?;
            } else {
                win.maximize()?;
            }
        }
        "close" => win.close()?,
        other => return Err(AppError::new("bad_action", format!("unknown action {other}"))),
    }
    Ok(())
}

#[tauri::command]
pub async fn window_drag(app: AppHandle) -> Result<(), AppError> {
    main_window(&app)?.start_dragging()?;
    Ok(())
}

#[tauri::command]
pub async fn window_resize(app: AppHandle, direction: String) -> Result<(), AppError> {
    let dir = match direction.as_str() {
        "n" => ResizeDirection::North,
        "s" => ResizeDirection::South,
        "e" => ResizeDirection::East,
        "w" => ResizeDirection::West,
        "ne" => ResizeDirection::NorthEast,
        "nw" => ResizeDirection::NorthWest,
        "se" => ResizeDirection::SouthEast,
        "sw" => ResizeDirection::SouthWest,
        other => return Err(AppError::new("bad_direction", other)),
    };
    main_window(&app)?.start_resize_dragging(dir)?;
    Ok(())
}

#[tauri::command]
pub async fn tab_list(state: State<'_, AppState>) -> Result<TabList, AppError> {
    let core = state.core.read().unwrap_or_else(|e| e.into_inner());
    Ok(snapshot(&core))
}

#[tauri::command]
pub async fn tab_select(app: AppHandle, state: State<'_, AppState>, id: String) -> Result<(), AppError> {
    let exists = {
        let core = state.core.read().unwrap_or_else(|e| e.into_inner());
        core.tabs.iter().any(|tab| tab.id == id)
    };
    if !exists {
        return Err(AppError::new("no_tab", id));
    }

    {
        let mut core = state.core.write().unwrap_or_else(|e| e.into_inner());
        core.selected = Some(id.clone());
    }

    sync_visibility(&app);
    if let Some(webview) = app.get_webview(&id) {
        let _ = webview.set_focus();
    }
    emit_tabs(&app);
    Ok(())
}

#[tauri::command]
pub async fn tab_new(
    app: AppHandle,
    state: State<'_, AppState>,
    url: Option<String>,
) -> Result<String, AppError> {
    let (id, target) = {
        let mut core = state.core.write().unwrap_or_else(|e| e.into_inner());
        core.next_tab_id += 1;
        let id = format!("tab-{}", core.next_tab_id);
        let target = url.unwrap_or_else(|| core.settings.start_url.clone());
        core.tick += 1;
        let tick = core.tick;
        core.tabs.push(Tab {
            id: id.clone(),
            url: target.clone(),
            title: "New tab".into(),
            alive: true,
            last_active: tick,
        });
        core.selected = Some(id.clone());
        (id, target)
    };

    spawn_tab(&app, &id, &target)?;
    sync_visibility(&app);
    enforce_live_limit(&app);
    emit_tabs(&app);
    save_session(&app);
    Ok(id)
}

#[tauri::command]
pub async fn tab_close(app: AppHandle, state: State<'_, AppState>, id: String) -> Result<(), AppError> {
    if let Some(webview) = app.get_webview(&id) {
        let _ = webview.close();
    }

    let (remaining, next_url) = {
        let mut core = state.core.write().unwrap_or_else(|e| e.into_inner());
        core.tabs.retain(|tab| tab.id != id);
        if core.selected.as_deref() == Some(id.as_str()) {
            core.selected = core.tabs.last().map(|tab| tab.id.clone());
        }
        (core.tabs.len(), core.settings.start_url.clone())
    };

    // Closing the last tab shows a fresh one instead of quitting.
    if remaining == 0 {
        let _ = tab_new(app, state, Some(next_url)).await;
        return Ok(());
    }

    sync_visibility(&app);
    emit_tabs(&app);
    save_session(&app);
    Ok(())
}

#[tauri::command]
pub async fn tab_navigate(app: AppHandle, state: State<'_, AppState>, url: String) -> Result<(), AppError> {
    let target = crate::tabs::parse_url(&url);
    let label = selected_label(&app);
    if label.is_empty() {
        return Ok(());
    }
    {
        let mut core = state.core.write().unwrap_or_else(|e| e.into_inner());
        if let Some(tab) = core.tabs.iter_mut().find(|tab| tab.id == label) {
            tab.url = target.to_string();
        }
    }
    if let Some(webview) = app.get_webview(&label) {
        webview.navigate(target)?;
    }
    Ok(())
}

#[tauri::command]
pub async fn tab_nav(app: AppHandle, action: String) -> Result<(), AppError> {
    let Some(webview) = app.get_webview(&selected_label(&app)) else {
        return Ok(());
    };
    match action.as_str() {
        // Tauri exposes no history API, so drive it from the page.
        "back" => webview.eval("history.back()")?,
        "forward" => webview.eval("history.forward()")?,
        "reload" => webview.reload()?,
        "stop" => webview.eval("window.stop()")?,
        other => return Err(AppError::new("bad_nav", other)),
    }
    Ok(())
}

fn selected_label(app: &AppHandle) -> String {
    let state = app.state::<AppState>();
    let core = state.core.read().unwrap_or_else(|e| e.into_inner());
    core.selected.clone().unwrap_or_default()
}

#[tauri::command]
pub async fn theme_get(state: State<'_, AppState>) -> Result<Theme, AppError> {
    let theme = state
        .core
        .read()
        .unwrap_or_else(|e| e.into_inner())
        .theme
        .clone();
    Ok(theme)
}

#[tauri::command]
pub async fn theme_apply(
    app: AppHandle,
    state: State<'_, AppState>,
    theme: Theme,
    persist: bool,
) -> Result<Theme, AppError> {
    let theme = theme.sanitized();

    {
        let mut core = state.core.write().unwrap_or_else(|e| e.into_inner());
        core.theme = theme.clone();
        if persist {
            core.baseline = theme.clone();
        }
    }

    broadcast_theme(&app, &theme);

    if persist {
        theme.save(&state.dirs.theme_file)?;
        let _ = app.emit("theme:changed", &theme);
    }
    Ok(theme)
}

#[tauri::command]
pub async fn theme_revert(app: AppHandle, state: State<'_, AppState>) -> Result<Theme, AppError> {
    let baseline = {
        let mut core = state.core.write().unwrap_or_else(|e| e.into_inner());
        core.theme = core.baseline.clone();
        core.theme.clone()
    };
    broadcast_theme(&app, &baseline);
    Ok(baseline)
}

fn broadcast_theme(app: &AppHandle, theme: &Theme) {
    let json = serde_json::to_string(theme).unwrap_or_else(|_| "{}".into());
    let script = format!("window.__mixApplyTheme && window.__mixApplyTheme({json})");
    for label in ["chrome", "overlay"] {
        if let Some(webview) = app.get_webview(label) {
            let _ = webview.eval(&script);
        }
    }
}

#[tauri::command]
pub async fn overlay_open(app: AppHandle, kind: String) -> Result<(), AppError> {
    crate::overlay::open(&app, &kind).map_err(|e| AppError::new("overlay", e))
}

#[tauri::command]
pub async fn overlay_close(app: AppHandle) -> Result<(), AppError> {
    crate::overlay::close(&app);
    Ok(())
}

#[derive(Serialize)]
pub struct Paths {
    root: String,
    user_data: String,
    crx: String,
    scripts: String,
    logs: String,
    downloads: String,
    fell_back: bool,
}

#[tauri::command]
pub async fn paths_get(state: State<'_, AppState>) -> Result<Paths, AppError> {
    let dirs = &state.dirs;
    Ok(Paths {
        root: dirs.root.display().to_string(),
        user_data: dirs.user_data.display().to_string(),
        crx: dirs.crx.display().to_string(),
        scripts: dirs.scripts.display().to_string(),
        logs: dirs.logs.display().to_string(),
        downloads: dirs.downloads.display().to_string(),
        fell_back: dirs.fell_back,
    })
}

#[tauri::command]
pub async fn open_dir(state: State<'_, AppState>, which: String) -> Result<(), AppError> {
    let dirs = &state.dirs;
    let path = match which.as_str() {
        "userdata" => &dirs.user_data,
        "crx" => &dirs.crx,
        "scripts" => &dirs.scripts,
        "logs" => &dirs.logs,
        "downloads" => &dirs.downloads,
        other => return Err(AppError::new("bad_dir", other)),
    };
    reveal(path)?;
    Ok(())
}

fn reveal(path: &std::path::Path) -> std::io::Result<()> {
    #[cfg(windows)]
    let mut cmd = {
        let mut c = std::process::Command::new("explorer");
        c.arg(path);
        c
    };
    #[cfg(target_os = "macos")]
    let mut cmd = {
        let mut c = std::process::Command::new("open");
        c.arg(path);
        c
    };
    #[cfg(target_os = "linux")]
    let mut cmd = {
        let mut c = std::process::Command::new("xdg-open");
        c.arg(path);
        c
    };
    cmd.spawn().map(|_| ())
}