use crate::extensions::ExtInfo;
use crate::library::{Bookmark, Visit};
use crate::paths::AppDirs;
use crate::state::Settings;
use crate::tabs::Tab;
use crate::theme::Theme;
use serde::{Deserialize, Serialize};
use std::sync::RwLock;
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_runtime::ResizeDirection;

#[derive(Default)]
pub struct Core {
    pub tabs: Vec<Tab>,
    pub selected: Option<String>,
    pub next_tab_id: usize,
    pub tick: u64,
}

pub struct AppState {
    pub dirs: AppDirs,
    /// One tab set per window, keyed by the window label.
    pub cores: parking_lot::RwLock<std::collections::HashMap<String, Core>>,
    pub settings: parking_lot::RwLock<Settings>,
    pub theme: parking_lot::RwLock<Theme>,
    pub baseline: parking_lot::RwLock<Theme>,
    pub downloads: RwLock<Vec<crate::downloads::DownloadTask>>,
    pub menu_context: RwLock<MenuContext>,
    /// Overlay webview label per window, so each window has its own menu.
    pub overlay: RwLock<std::collections::HashMap<String, String>>,
    pub permissions: crate::permissions::PermissionStore,
    pub snap: std::sync::Mutex<crate::snap::SnapState>,
    pub opts: crate::tabs::WebviewOpts,
    pub extensions: Vec<ExtInfo>,
    pub history: RwLock<Vec<Visit>>,
    pub bookmarks: RwLock<Vec<Bookmark>>,
    pub scripts: Vec<crate::userscripts::ScriptInfo>,
}

impl AppState {
    pub fn new(
        dirs: AppDirs,
        settings: Settings,
        theme: Theme,
        extensions: Vec<ExtInfo>,
        userscripts: Vec<crate::userscripts::Userscript>,
    ) -> Self {
        let mut opts = crate::tabs::WebviewOpts::for_dirs(&dirs, &extensions);
        let scripts = userscripts.iter().map(|script| script.info()).collect();
        let scripts_js = crate::userscripts::loader_js(&userscripts);
        // The loader travels with every tab webview; keeping it in the options means
        // tab creation does not have to reach back into state for it.
        opts.userscripts = scripts_js.clone();
        let history = crate::library::load_visits(&dirs.history_file);
        let bookmarks = crate::library::load_bookmarks(&dirs.bookmarks_file);
        let mut cores = std::collections::HashMap::new();
        cores.insert("w1".to_string(), Core::default());

        Self {
            opts,
            extensions,
            history: RwLock::new(history),
            bookmarks: RwLock::new(bookmarks),
            scripts,
            dirs,
            cores: parking_lot::RwLock::new(cores),
            settings: parking_lot::RwLock::new(settings),
            baseline: parking_lot::RwLock::new(theme.clone()),
            theme: parking_lot::RwLock::new(theme),
            downloads: RwLock::new(Vec::new()),
            menu_context: RwLock::new(MenuContext::default()),
            overlay: RwLock::new(std::collections::HashMap::new()),
            permissions: crate::permissions::PermissionStore::default(),
            snap: std::sync::Mutex::new(crate::snap::SnapState::default()),
        }
    }

    pub fn add_window(&self, label: &str) {
        self.cores
            .write()
            .entry(label.to_string())
            .or_insert_with(Core::default);
    }

    pub fn remove_window(&self, label: &str) {
        self.cores.write().remove(label);
    }

    /// Tab state of one window. Panics only if a window exists without a core,
    /// which would be a bug in open/close bookkeeping.
    pub fn try_read_core(&self, label: &str) -> Option<parking_lot::MappedRwLockReadGuard<'_, Core>> {
        parking_lot::RwLockReadGuard::try_map(self.cores.read(), |map| map.get(label)).ok()
    }

    pub fn try_write_core(&self, label: &str) -> Option<parking_lot::MappedRwLockWriteGuard<'_, Core>> {
        parking_lot::RwLockWriteGuard::try_map(self.cores.write(), |map| map.get_mut(label)).ok()
    }

    pub fn read_core(&self, label: &str) -> parking_lot::MappedRwLockReadGuard<'_, Core> {
        parking_lot::RwLockReadGuard::map(self.cores.read(), |map| {
            map.get(label)
                .unwrap_or_else(|| panic!("window {label} has no tab state"))
        })
    }

    pub fn write_core(&self, label: &str) -> parking_lot::MappedRwLockWriteGuard<'_, Core> {
        parking_lot::RwLockWriteGuard::map(self.cores.write(), |map| {
            map.get_mut(label)
                .unwrap_or_else(|| panic!("window {label} has no tab state"))
        })
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct SettingsPatch {
    pub restore_session: Option<bool>,
    pub start_url: Option<String>,
    pub max_live_tabs: Option<usize>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MenuContext {
    pub link: Option<String>,
    pub image: Option<String>,
    pub selection: Option<String>,
    pub editable: bool,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct MenuRequest {
    pub dismiss: Option<bool>,
    pub x: Option<f64>,
    pub y: Option<f64>,
    pub link: Option<String>,
    pub image: Option<String>,
    pub selection: Option<String>,
    pub editable: Option<bool>,
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

fn window_of(window: &tauri::Window) -> tauri::Window {
    window.clone()
}

/// Webview labels carry the window they belong to: "w1-tab-3".
pub fn tab_label(window: &str, index: usize) -> String {
    format!("{window}-tab-{index}")
}

pub fn window_of_tab(id: &str) -> &str {
    id.split("-tab-").next().unwrap_or("w1")
}

#[derive(Clone, Serialize)]
pub struct TabMeta {
    id: String,
    url: String,
    title: String,
    alive: bool,
    incognito: bool,
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
                incognito: tab.incognito,
            })
            .collect(),
        selected: core.selected.clone(),
    }
}

/// Tell the chrome UI to rebuild the tab strip.
pub(crate) fn emit_tabs(app: &AppHandle, label: &str) {
    let state = app.state::<AppState>();
    let list = {
        let core = state.read_core(label);
        snapshot(&core)
    };
    let _ = app.emit("tabs:changed", list);
}

/// Only the selected tab is visible; the rest keep running underneath.
pub(crate) fn sync_visibility(app: &AppHandle, label: &str) {
    let pairs: Vec<(String, Option<String>)> = {
        let state = app.state::<AppState>();
        let core = state.read_core(label);
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
pub(crate) fn enforce_live_limit(app: &AppHandle, label: &str) {
    loop {
        let victim = {
            let state = app.state::<AppState>();
            let mut core = state.write_core(label);
            let alive = core.tabs.iter().filter(|tab| tab.alive).count();
            let limit = state.settings.read().max_live_tabs.max(1);
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

fn window_geometry(app: &AppHandle, label: &str) -> Option<crate::session::WindowGeometry> {
    let win = app.get_window(label)?;
    let position = win.outer_position().ok()?;
    let size = win.inner_size().ok()?;
    Some(crate::session::WindowGeometry {
        x: position.x,
        y: position.y,
        width: size.width,
        height: size.height,
        maximized: win.is_maximized().unwrap_or(false),
    })
}

pub(crate) fn save_session(app: &AppHandle, label: &str) {
    let Some(state) = app.try_state::<AppState>() else {
        return;
    };
    let session = {
        let core = state.read_core(label);
        let selected = core
            .selected
            .as_ref()
            .and_then(|id| core.tabs.iter().position(|tab| &tab.id == id))
            .unwrap_or(0);
        crate::session::Session {
            schema_version: crate::session::SCHEMA,
            tabs: core
                .tabs
                .iter()
                .filter(|tab| !tab.incognito)
                .map(|tab| tab.url.clone())
                .collect(),
            selected,
            window: window_geometry(app, label),
        }
    };
    if let Err(err) = session.save(&state.dirs.session_file) {
        crate::logging::warn(&format!("session save failed: {err}"));
    }
}

fn spawn_tab(
    app: &AppHandle,
    label: &str,
    id: &str,
    url: &str,
    incognito: bool,
) -> Result<(), AppError> {
    let win = app
        .get_window(label)
        .ok_or_else(|| AppError::new("no_window", "window is missing"))?;
    let opts = app.state::<AppState>().opts.clone();
    let (width, height) = crate::window::logical_size(&win)?;
    crate::tabs::create(
        &win,
        id,
        url,
        width,
        (height - crate::window::CHROME_H).max(1.0),
        &opts,
        incognito,
    )?;
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
pub async fn layout_get(app: AppHandle, window: tauri::Window, state: State<'_, AppState>) -> Result<Layout, AppError> {
    let win = window_of(&window);
    let (width, height) = crate::window::logical_size(&win)?;
    let (chrome_height, selected) = {
        let core = state.read_core(window.label());
        (state.settings.read().chrome_height, core.selected.clone())
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
pub async fn window_control(
    app: AppHandle,
    window: tauri::Window,
    action: String,
) -> Result<(), AppError> {
    let win = window_of(&window);
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
pub async fn window_drag(app: AppHandle, window: tauri::Window) -> Result<(), AppError> {
    window.start_dragging()?;
    Ok(())
}

#[tauri::command]
pub async fn window_resize(app: AppHandle, direction: String, window: tauri::Window) -> Result<(), AppError> {
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
    window.start_resize_dragging(dir)?;
    Ok(())
}

#[tauri::command]
pub async fn tab_list(window: tauri::Window, state: State<'_, AppState>) -> Result<TabList, AppError> {
    let core = state.read_core(window.label());
    Ok(snapshot(&core))
}

#[tauri::command]
pub async fn tab_select(app: AppHandle, window: tauri::Window, state: State<'_, AppState>, id: String) -> Result<(), AppError> {
    let exists = {
        let core = state.read_core(window.label());
        core.tabs.iter().any(|tab| tab.id == id)
    };
    if !exists {
        return Err(AppError::new("no_tab", id));
    }

    {
        let mut core = state.write_core(window.label());
        core.selected = Some(id.clone());
    }

    sync_visibility(&app, window.label());
    if let Some(webview) = app.get_webview(&id) {
        let _ = webview.set_focus();
    }
    emit_tabs(&app, window.label());
    Ok(())
}

#[tauri::command]
pub async fn tab_new(
    app: AppHandle,
    window: tauri::Window, state: State<'_, AppState>,
    url: Option<String>,
    incognito: Option<bool>,
) -> Result<String, AppError> {
    let (id, target) = {
        let mut core = state.write_core(window.label());
        core.next_tab_id += 1;
        let id = format!("tab-{}", core.next_tab_id);
        let target = url.unwrap_or_else(|| state.settings.read().start_url.clone());
        core.tick += 1;
        let tick = core.tick;
        core.tabs.push(Tab {
            id: id.clone(),
            url: target.clone(),
            title: if incognito.unwrap_or(false) {
                "Incognito".into()
            } else {
                "New tab".into()
            },
            zoom: 1.0,
            incognito: incognito.unwrap_or(false),
            alive: true,
            last_active: tick,
        });
        core.selected = Some(id.clone());
        (id, target)
    };

    spawn_tab(&app, window.label(), &id, &target, incognito.unwrap_or(false))?;
    crate::logging::info(&format!("tab created {id} -> {target}"));
    sync_visibility(&app, window.label());
    enforce_live_limit(&app, window.label());
    emit_tabs(&app, window.label());
    save_session(&app, window.label());
    Ok(id)
}

#[tauri::command]
pub async fn tab_close(app: AppHandle, window: tauri::Window, state: State<'_, AppState>, id: String) -> Result<(), AppError> {
    if let Some(webview) = app.get_webview(&id) {
        let _ = webview.close();
    }

    let (remaining, next_url) = {
        let mut core = state.write_core(window.label());
        core.tabs.retain(|tab| tab.id != id);
        if core.selected.as_deref() == Some(id.as_str()) {
            core.selected = core.tabs.last().map(|tab| tab.id.clone());
        }
        (core.tabs.len(), state.settings.read().start_url.clone())
    };

    // Closing the last tab shows a fresh one instead of quitting.
    if remaining == 0 {
        let _ = tab_new(app, window.clone(), state, Some(next_url), None).await;
        return Ok(());
    }

    sync_visibility(&app, window.label());
    emit_tabs(&app, window.label());
    save_session(&app, window.label());
    Ok(())
}

#[tauri::command]
pub async fn tab_navigate(app: AppHandle, window: tauri::Window, state: State<'_, AppState>, url: String) -> Result<(), AppError> {
    let target = crate::tabs::parse_url(&url);
    let label = selected_label(&state, &window);
    if label.is_empty() {
        return Ok(());
    }
    {
        let mut core = state.write_core(window.label());
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
pub async fn tab_nav(
    app: AppHandle,
    window: tauri::Window,
    state: State<'_, AppState>,
    action: String,
) -> Result<(), AppError> {
    let Some(webview) = app.get_webview(&selected_label(&state, &window)) else {
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

fn selected_label(state: &AppState, window: &tauri::Window) -> String {
    state
        .read_core(window.label())
        .selected
        .clone()
        .unwrap_or_default()
}

#[tauri::command]
pub async fn theme_get(window: tauri::Window, state: State<'_, AppState>) -> Result<Theme, AppError> {
    Ok(state.theme.read().clone())
}

#[tauri::command]
pub async fn theme_apply(
    app: AppHandle,
    window: tauri::Window, state: State<'_, AppState>,
    theme: Theme,
    persist: bool,
) -> Result<Theme, AppError> {
    let theme = theme.sanitized();

    {
        *state.theme.write() = theme.clone();
        if persist {
            *state.baseline.write() = theme.clone();
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
pub async fn theme_revert(app: AppHandle, window: tauri::Window, state: State<'_, AppState>) -> Result<Theme, AppError> {
    let baseline = {
        let value = state.baseline.read().clone();
        *state.theme.write() = value.clone();
        value
    };
    broadcast_theme(&app, &baseline);
    Ok(baseline)
}

fn broadcast_theme(app: &AppHandle, theme: &Theme) {
    let json = serde_json::to_string(theme).unwrap_or_else(|_| "{}".into());
    let script = format!("window.__mixApplyTheme && window.__mixApplyTheme({json})");

    let mut labels = vec!["w1-chrome".to_string()];
    for label in app
        .state::<AppState>()
        .overlay
        .read()
        .unwrap_or_else(|e| e.into_inner())
        .values()
        .cloned()
        .collect::<Vec<_>>()
    {
        labels.push(label);
    }

    for label in labels {
        if let Some(webview) = app.get_webview(&label) {
            let _ = webview.eval(&script);
        }
    }
}

#[tauri::command]
pub async fn download_list(window: tauri::Window, state: State<'_, AppState>) -> Result<Vec<crate::downloads::DownloadTask>, AppError> {
    Ok(state.downloads.read().unwrap_or_else(|e| e.into_inner()).clone())
}

#[tauri::command]
pub async fn download_open_dir(window: tauri::Window, state: State<'_, AppState>, id: u64) -> Result<(), AppError> {
    let path = {
        let tasks = state.downloads.read().unwrap_or_else(|e| e.into_inner());
        tasks
            .iter()
            .find(|task| task.id == id)
            .map(|task| task.path.clone())
    };
    match path {
        Some(path) if !path.is_empty() => reveal(std::path::Path::new(&path)),
        _ => reveal(&state.dirs.downloads),
    }?;
    Ok(())
}

#[tauri::command]
pub async fn shortcut(app: AppHandle, window: tauri::Window, state: State<'_, AppState>, action: String) -> Result<(), AppError> {
    crate::logging::info(&format!("shortcut {action}"));
    match action.as_str() {
        "new_tab" => {
            tab_new(app, window.clone(), state, None, None).await?;
        }
        "new_incognito" => {
            tab_new(app, window.clone(), state, None, Some(true)).await?;
        }
        "new_window" => {
            window_new(app, state).await?;
        }
        "bookmark" => {
            let _ = bookmark_toggle(app.clone(), window.clone(), state).await?;
        }
        "close_tab" => {
            let id = selected_label(&state, &window);
            if !id.is_empty() {
                tab_close(app, window.clone(), state, id).await?;
            }
        }
        "next_tab" => cycle_tab(&app, &window, 1).await,
        "prev_tab" => cycle_tab(&app, &window, -1).await,
        "focus_url" => {
            let _ = app.emit("ui:focus-url", ());
        }
        "reload" => {
            if let Some(webview) = app.get_webview(&selected_label(&state, &window)) {
                let _ = webview.reload();
            }
        }
        "zoom_in" => zoom_by(&app, window.label(), 0.1)?,
        "zoom_out" => zoom_by(&app, window.label(), -0.1)?,
        "zoom_reset" => set_zoom(&app, window.label(), 1.0)?,
        "print" => {
            if let Some(webview) = app.get_webview(&selected_label(&state, &window)) {
                let _ = webview.print();
            }
        }
        "fullscreen" => {
            if let Some(win) = app.get_window(window.label()) {
                let next = !win.is_fullscreen().unwrap_or(false);
                let _ = win.set_fullscreen(next);
            }
        }
        "downloads" => {
            if let Err(err) = reveal(&app.state::<AppState>().dirs.downloads) {
                crate::logging::warn(&format!("cannot open downloads: {err}"));
            }
        }
        "devtools" => {
            #[cfg(debug_assertions)]
            if let Some(webview) = app.get_webview(&selected_label(&state, &window)) {
                webview.open_devtools();
            }
        }
        other => return Err(AppError::new("bad_shortcut", other)),
    }
    Ok(())
}

async fn cycle_tab(app: &AppHandle, window: &tauri::Window, delta: isize) {
    let next = {
        let state = app.state::<AppState>();
        let core = state.read_core(window.label());
        if core.tabs.len() < 2 {
            return;
        }
        let current = core
            .selected
            .as_ref()
            .and_then(|id| core.tabs.iter().position(|tab| &tab.id == id))
            .unwrap_or(0);
        let len = core.tabs.len() as isize;
        let index = (current as isize + delta).rem_euclid(len);
        Some(core.tabs[index as usize].id.clone())
    };

    if let Some(id) = next {
        let state = app.state::<AppState>();
        let _ = tab_select(app.clone(), window.clone(), state, id).await;
    }
}

fn zoom_by(app: &AppHandle, label: &str, delta: f64) -> Result<(), AppError> {
    let current = {
        let state = app.state::<AppState>();
        let core = state.read_core(label);
        let label = core.selected.clone();
        label
            .and_then(|id| core.tabs.iter().find(|tab| tab.id == id).map(|tab| tab.zoom))
            .unwrap_or(1.0)
    };
    set_zoom(app, label, current + delta)
}

fn set_zoom(app: &AppHandle, label: &str, level: f64) -> Result<(), AppError> {
    let level = (level * 10.0).round() / 10.0;
    let level = level.clamp(0.5, 2.0);
    let id = app
        .state::<AppState>()
        .read_core(label)
        .selected
        .clone()
        .unwrap_or_default();
    if id.is_empty() {
        return Ok(());
    }
    {
        let state = app.state::<AppState>();
        let mut core = state.write_core(label);
        if let Some(tab) = core.tabs.iter_mut().find(|tab| tab.id == id) {
            tab.zoom = level;
        }
    }
    if let Some(webview) = app.get_webview(&id) {
        webview.set_zoom(level)?;
    }
    crate::logging::info(&format!("zoom -> {level:.1}"));
    Ok(())
}

/// Pages report their context menu here; dismiss=true closes an open menu.
#[tauri::command]
pub async fn context_menu(
    app: AppHandle,
    window: tauri::Window, state: State<'_, AppState>,
    request: MenuRequest,
) -> Result<(), AppError> {
    if request.dismiss.unwrap_or(false) {
        crate::overlay::close(&app, window.label());
        return Ok(());
    }

    let context = MenuContext {
        link: request.link.filter(|value| !value.is_empty()),
        image: request.image.filter(|value| !value.is_empty()),
        selection: request
            .selection
            .filter(|value| !value.trim().is_empty())
            .map(|value| value.chars().take(200).collect()),
        editable: request.editable.unwrap_or(false),
    };
    {
        let mut slot = state.menu_context.write().unwrap_or_else(|e| e.into_inner());
        *slot = context.clone();
    }

    // Page coordinates are relative to the tab webview, which starts below chrome.
    let win = window_of(&window);
    let (width, height) = crate::window::logical_size(&win)?;
    let menu_w = 232.0;
    let menu_h = 260.0;
    let x = (request.x.unwrap_or(0.0) + 80.0).clamp(4.0, (width - menu_w - 4.0).max(4.0));
    let y = (crate::window::CHROME_H + request.y.unwrap_or(0.0))
        .clamp(4.0, (height - menu_h - 4.0).max(4.0));

    crate::logging::info(&format!(
        "context menu at {x:.0},{y:.0} link={} image={} sel={}",
        context.link.is_some(),
        context.image.is_some(),
        context.selection.is_some()
    ));
    if let Err(err) = crate::overlay::open_at(&app, &window, "context_menu", x, y, menu_w, menu_h) {
        crate::logging::warn(&format!("context menu overlay failed: {err}"));
        return Err(AppError::new("overlay", err));
    }
    Ok(())
}

#[tauri::command]
pub async fn menu_context_get(window: tauri::Window, state: State<'_, AppState>) -> Result<MenuContext, AppError> {
    Ok(state.menu_context.read().unwrap_or_else(|e| e.into_inner()).clone())
}

/// Asked from the page before a sensitive API is used. Waits for the user.
#[tauri::command]
pub async fn permission_request(
    app: AppHandle,
    window: tauri::Window,
    origin: String,
    kind: String,
) -> Result<bool, AppError> {
    let key = crate::permissions::key(&origin, &kind);
    if let Some(known) = crate::permissions::remembered(&app, &key) {
        return Ok(known);
    }

    let (id, receiver) = crate::permissions::open_prompt(&app);
    let win = window_of(&window);
    let (width, _) = crate::window::logical_size(&win)?;
    let prompt_w = 380.0;
    let prompt_h = 150.0;
    let x = ((width - prompt_w) / 2.0).max(0.0);
    let hash = format!(
        "permission&id={id}&origin={}&kind={}",
        crate::permissions::encode(&origin),
        crate::permissions::encode(&kind)
    );
    let url = format!("overlay.html#{hash}");

    crate::overlay::open_url(
        &app,
        &window,
        &url,
        x,
        crate::window::CHROME_H + 48.0,
        prompt_w,
        prompt_h,
    )
    .map_err(|err| AppError::new("overlay", err))?;

    crate::logging::info(&format!("permission prompt {key}"));
    let allow = tauri::async_runtime::spawn_blocking(move || {
        receiver
            .recv_timeout(std::time::Duration::from_secs(120))
            .unwrap_or(false)
    })
    .await
    .unwrap_or(false);

    crate::permissions::remember(&app, &key, allow);
    crate::overlay::close(&app, window.label());
    crate::logging::info(&format!("permission {key} -> {allow}"));
    Ok(allow)
}

#[tauri::command]
pub async fn permission_reply(
    app: AppHandle,
    window: tauri::Window,
    id: u64,
    allow: bool,
) -> Result<(), AppError> {
    if !crate::permissions::answer(&app, id, allow) {
        crate::logging::warn(&format!("permission reply for unknown prompt {id}"));
    }
    crate::overlay::close(&app, window.label());
    Ok(())
}

#[tauri::command]
pub async fn settings_get(window: tauri::Window, state: State<'_, AppState>) -> Result<Settings, AppError> {
    Ok(state.settings.read().clone())
}

#[tauri::command]
pub async fn settings_set(
    window: tauri::Window, state: State<'_, AppState>,
    patch: SettingsPatch,
) -> Result<Settings, AppError> {
    let settings = {
        let mut guard = state.settings.write();
        if let Some(value) = patch.restore_session {
            guard.restore_session = value;
        }
        if let Some(value) = patch.start_url {
            if !value.trim().is_empty() {
                guard.start_url = value;
            }
        }
        if let Some(value) = patch.max_live_tabs {
            guard.max_live_tabs = value.clamp(1, 64);
        }
        guard.clone()
    };

    settings.save(&state.dirs.settings_file)?;
    crate::logging::info(&format!(
        "settings saved: restore_session={} max_live_tabs={}",
        settings.restore_session, settings.max_live_tabs
    ));
    Ok(settings)
}

#[tauri::command]
pub async fn autostart_get(app: AppHandle) -> Result<bool, AppError> {
    use tauri_plugin_autostart::ManagerExt;
    app.autolaunch()
        .is_enabled()
        .map_err(|err| AppError::new("autostart", err.to_string()))
}

#[tauri::command]
pub async fn autostart_set(app: AppHandle, enabled: bool) -> Result<bool, AppError> {
    use tauri_plugin_autostart::ManagerExt;
    let manager = app.autolaunch();
    let result = if enabled {
        manager.enable()
    } else {
        manager.disable()
    };
    result.map_err(|err| AppError::new("autostart", err.to_string()))?;
    let now = manager
        .is_enabled()
        .map_err(|err| AppError::new("autostart", err.to_string()))?;
    crate::logging::info(&format!("autostart -> {now}"));
    Ok(now)
}

#[tauri::command]
pub async fn snap(app: AppHandle, direction: String) -> Result<(), AppError> {
    let dir = match direction.as_str() {
        "left" => crate::snap::Direction::Left,
        "right" => crate::snap::Direction::Right,
        "up" => crate::snap::Direction::Up,
        "down" => crate::snap::Direction::Down,
        other => return Err(AppError::new("bad_direction", other)),
    };
    crate::snap::cycle(&app, dir).map_err(|err| AppError::new("snap", err))
}

#[tauri::command]
pub async fn ext_list(window: tauri::Window, state: State<'_, AppState>) -> Result<Vec<ExtInfo>, AppError> {
    Ok(state.extensions.clone())
}

#[derive(Serialize)]
pub struct AppInfo {
    version: &'static str,
    git: &'static str,
    os: &'static str,
    webview: String,
}

#[tauri::command]
pub async fn zoom_get(window: tauri::Window, state: State<'_, AppState>) -> Result<f64, AppError> {
    let core = state.read_core(window.label());
    let zoom = core
        .selected
        .as_ref()
        .and_then(|id| core.tabs.iter().find(|tab| &tab.id == id))
        .map(|tab| tab.zoom)
        .unwrap_or(1.0);
    Ok(zoom)
}

#[tauri::command]
pub async fn zoom_step(app: AppHandle, delta: f64, window: tauri::Window, state: State<'_, AppState>) -> Result<f64, AppError> {
    let current = {
        let state = app.state::<AppState>();
        let core = state.read_core(window.label());
        core.selected
            .as_ref()
            .and_then(|id| core.tabs.iter().find(|tab| &tab.id == id))
            .map(|tab| tab.zoom)
            .unwrap_or(1.0)
    };
    set_zoom(&app, window.label(), current + delta)?;
    Ok(((current + delta) * 10.0).round() / 10.0)
}

#[tauri::command]
pub async fn tab_print(
    app: AppHandle,
    window: tauri::Window,
    state: State<'_, AppState>,
) -> Result<(), AppError> {
    if let Some(webview) = app.get_webview(&selected_label(&state, &window)) {
        webview.print()?;
    }
    Ok(())
}

#[tauri::command]
pub async fn window_fullscreen(
    app: AppHandle,
    window: tauri::Window,
    enabled: Option<bool>,
) -> Result<bool, AppError> {
    let win = window_of(&window);
    let next = enabled.unwrap_or(!win.is_fullscreen()?);
    win.set_fullscreen(next)?;
    Ok(next)
}

/// Wipes cookies, storage and caches for every webview we own.
#[tauri::command]
pub async fn app_clear_data(app: AppHandle, window: tauri::Window, state: State<'_, AppState>) -> Result<(), AppError> {
    let labels: Vec<String> = {
        let state = app.state::<AppState>();
        let core = state.read_core(window.label());
        core.tabs.iter().map(|tab| tab.id.clone()).collect()
    };

    for label in labels {
        if let Some(webview) = app.get_webview(&label) {
            let _ = webview.clear_all_browsing_data();
        }
    }
    for label in ["w1-chrome"] {
        if let Some(webview) = app.get_webview(label) {
            let _ = webview.clear_all_browsing_data();
        }
    }
    crate::logging::info("browsing data cleared");
    Ok(())
}

pub(crate) fn record_visit(app: &AppHandle, url: &str, title: &str) {
    if !crate::library::worth_recording(url) {
        return;
    }
    let Some(state) = app.try_state::<AppState>() else {
        return;
    };
    let snapshot = {
        let mut visits = state.history.write().unwrap_or_else(|e| e.into_inner());
        crate::library::push_visit(&mut visits, url, title);
        visits.clone()
    };
    if let Err(err) = crate::library::save_visits(&state.dirs.history_file, &snapshot) {
        crate::logging::warn(&format!("history save failed: {err}"));
    }
}

#[tauri::command]
pub async fn history_list(window: tauri::Window, state: State<'_, AppState>, limit: Option<usize>) -> Result<Vec<Visit>, AppError> {
    let visits = state.history.read().unwrap_or_else(|e| e.into_inner());
    let take = limit.unwrap_or(500).min(visits.len());
    Ok(visits.iter().take(take).cloned().collect())
}

#[tauri::command]
pub async fn history_clear(window: tauri::Window, state: State<'_, AppState>) -> Result<(), AppError> {
    {
        let mut visits = state.history.write().unwrap_or_else(|e| e.into_inner());
        visits.clear();
    }
    crate::library::save_visits(&state.dirs.history_file, &[])?;
    crate::logging::info("history cleared");
    Ok(())
}

#[tauri::command]
pub async fn bookmark_list(window: tauri::Window, state: State<'_, AppState>) -> Result<Vec<Bookmark>, AppError> {
    Ok(state.bookmarks.read().unwrap_or_else(|e| e.into_inner()).clone())
}

/// Toggles the selected tab and reports whether it is bookmarked now.
#[tauri::command]
pub async fn bookmark_toggle(app: AppHandle, window: tauri::Window, state: State<'_, AppState>) -> Result<bool, AppError> {
    let (url, title) = {
        let core = state.read_core(window.label());
        match core
            .selected
            .as_ref()
            .and_then(|id| core.tabs.iter().find(|tab| &tab.id == id))
        {
            Some(tab) => (tab.url.clone(), tab.title.clone()),
            None => return Ok(false),
        }
    };

    let (added, snapshot) = {
        let mut bookmarks = state.bookmarks.write().unwrap_or_else(|e| e.into_inner());
        let added = crate::library::toggle_bookmark(&mut bookmarks, &url, &title);
        (added, bookmarks.clone())
    };
    crate::library::save_bookmarks(&state.dirs.bookmarks_file, &snapshot)?;
    crate::logging::info(&format!("bookmark {url} -> {added}"));
    let _ = app.emit("bookmarks:changed", added);
    Ok(added)
}

#[tauri::command]
pub async fn bookmark_remove(
    app: AppHandle,
    window: tauri::Window, state: State<'_, AppState>,
    url: String,
) -> Result<(), AppError> {
    let snapshot = {
        let mut bookmarks = state.bookmarks.write().unwrap_or_else(|e| e.into_inner());
        bookmarks.retain(|bookmark| bookmark.url != url);
        bookmarks.clone()
    };
    crate::library::save_bookmarks(&state.dirs.bookmarks_file, &snapshot)?;
    let _ = app.emit("bookmarks:changed", false);
    Ok(())
}

#[tauri::command]
pub async fn diag_export(app: AppHandle) -> Result<String, AppError> {
    let path = crate::diag::export(&app).map_err(|err| AppError::new("diag", err))?;
    crate::logging::info(&format!("diagnostics written: {}", path.display()));
    Ok(path.display().to_string())
}

#[tauri::command]
pub async fn app_info() -> Result<AppInfo, AppError> {
    Ok(AppInfo {
        version: env!("CARGO_PKG_VERSION"),
        git: env!("MIX_GIT"),
        os: std::env::consts::OS,
        webview: tauri::webview_version().unwrap_or_else(|_| "unknown".into()),
    })
}

/// Quits for real: the window close button only hides to the tray.
#[tauri::command]
pub async fn app_quit(app: AppHandle, window: tauri::Window) -> Result<(), AppError> {
    save_session(&app, window.label());
    crate::logging::info("quit from the app menu");
    app.exit(0);
    Ok(())
}

/// The chrome ui has no console we can read, so it reports through here.
#[tauri::command]
pub async fn ui_log(message: String) -> Result<(), AppError> {
    crate::logging::info(&format!("ui: {message}"));
    Ok(())
}

#[tauri::command]
pub async fn script_list(window: tauri::Window, state: State<'_, AppState>) -> Result<Vec<crate::userscripts::ScriptInfo>, AppError> {
    Ok(state.scripts.clone())
}

/// Opens another window with its own tab strip.
#[tauri::command]
pub async fn window_new(app: AppHandle, state: State<'_, AppState>) -> Result<String, AppError> {
    let taken: Vec<String> = state.cores.read().keys().cloned().collect();
    let mut index = 2;
    let label = loop {
        let candidate = format!("w{index}");
        if !taken.contains(&candidate) {
            break candidate;
        }
        index += 1;
    };

    let url = state.settings.read().start_url.clone();
    crate::window::open_window(&app, &label, vec![url], 0, None)?;
    crate::logging::info(&format!("window {label} opened"));
    Ok(label)
}

#[tauri::command]
pub async fn overlay_open(
    app: AppHandle,
    window: tauri::Window,
    kind: String,
) -> Result<(), AppError> {
    crate::overlay::open(&app, &window, &kind).map_err(|e| AppError::new("overlay", e))
}

#[tauri::command]
pub async fn overlay_close(app: AppHandle, window: tauri::Window) -> Result<(), AppError> {
    crate::overlay::close(&app, window.label());
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
pub async fn paths_get(window: tauri::Window, state: State<'_, AppState>) -> Result<Paths, AppError> {
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
pub async fn open_dir(window: tauri::Window, state: State<'_, AppState>, which: String) -> Result<(), AppError> {
    let dirs = &state.dirs;
    let path = match which.as_str() {
        "userdata" => &dirs.user_data,
        "crx" => &dirs.crx,
        "scripts" => &dirs.scripts,
        "logs" => &dirs.logs,
        "downloads" => &dirs.downloads,
        "diag" => &dirs.diag,
        "root" => &dirs.root,
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