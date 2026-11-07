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
    /// Tab ids by recency, most recent first; Ctrl+Tab walks this.
    pub mru: Vec<String>,
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
    /// host -> favicon url, shared by history, bookmarks and the new tab page.
    pub favicons: RwLock<crate::favicons::Favicons>,
    pub suggest_text: parking_lot::RwLock<String>,
    pub suggest_width: parking_lot::RwLock<f64>,
    /// Recently closed tabs, newest last, so Ctrl+Shift+T can bring them back.
    pub closed_tabs: RwLock<Vec<(String, String)>>,
    pub zoom: parking_lot::RwLock<crate::zoom::ZoomStore>,
    /// Window label -> text typed while the address bar owns the keyboard.
    pub url_edit: RwLock<std::collections::HashMap<String, String>>,
    /// Tab id -> the url it is currently trying to load.
    /// Origin of the bundled pages, e.g. http://tauri.localhost/
    pub app_base: parking_lot::RwLock<String>,
    pub pending: RwLock<std::collections::HashMap<String, String>>,
    /// Cached "can we reach this host" answers, so navigation is not slowed down.
    pub reachability: crate::net::Reachability,
    /// Result of the last in-page translation, polled by the bubble.
    pub translate_result: parking_lot::RwLock<(usize, Option<String>)>,
    /// Tabs whose text is currently replaced by the translator.
    pub translated_tabs: parking_lot::RwLock<std::collections::HashSet<String>>,
    /// Windows whose page is showing a fullscreen element; the chrome hides.
    pub fullscreen: parking_lot::RwLock<std::collections::HashSet<String>>,
    /// Whether the last in-page search matched anything.
    pub find_hit: parking_lot::RwLock<bool>,
    /// Tab id -> the url whose error page is already showing.
    pub error_shown: RwLock<std::collections::HashMap<String, String>>,
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
            favicons: RwLock::new(crate::favicons::Favicons::load(&dirs.favicons_file)),
        history: RwLock::new(history),
            suggest_text: parking_lot::RwLock::new(String::new()),
            suggest_width: parking_lot::RwLock::new(420.0),
            closed_tabs: RwLock::new(Vec::new()),
            zoom: parking_lot::RwLock::new(crate::zoom::ZoomStore::load(&dirs.zoom_file)),
            url_edit: RwLock::new(std::collections::HashMap::new()),
            app_base: parking_lot::RwLock::new(String::new()),
            pending: RwLock::new(std::collections::HashMap::new()),
            reachability: crate::net::Reachability::default(),
            translate_result: parking_lot::RwLock::new((0, None)),
            translated_tabs: parking_lot::RwLock::new(std::collections::HashSet::new()),
            fullscreen: parking_lot::RwLock::new(std::collections::HashSet::new()),
            find_hit: parking_lot::RwLock::new(true),
            error_shown: RwLock::new(std::collections::HashMap::new()),
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
    pub show_bookmarks: Option<bool>,
    pub toolbar: Option<Vec<String>>,
    pub baidu_appid: Option<String>,
    pub baidu_secret: Option<String>,
    pub translate_to: Option<String>,
    pub new_tab_page: Option<bool>,
    pub search_engine: Option<String>,
    pub shortcuts: Option<std::collections::BTreeMap<String, String>>,
    pub custom_search: Option<String>,
    pub gestures: Option<bool>,
    pub super_drag: Option<bool>,
    pub drag_hint: Option<bool>,
    pub drag_threshold: Option<u32>,
    pub drag_map: Option<std::collections::BTreeMap<String, String>>,
    pub gesture_trail: Option<bool>,
    pub gesture_color: Option<String>,
    pub gesture_width: Option<u32>,
    pub gesture_threshold: Option<u32>,
    pub gesture_map: Option<std::collections::BTreeMap<String, String>>,
    pub gesture_blocked: Option<String>,
    pub tab_double_click_close: Option<bool>,
    pub tab_show_close: Option<bool>,
    pub tab_close_goes_left: Option<bool>,
    pub newtab_show_search: Option<bool>,
    pub newtab_show_sites: Option<bool>,
    pub newtab_url: Option<String>,
    pub newtab_tiles: Option<u32>,
    pub newtab_columns: Option<u32>,
    pub newtab_radius: Option<u32>,
    pub menu_bar: Option<bool>,
    pub language: Option<String>,
    pub hidden_extensions: Option<Vec<String>>,
    pub disabled_extensions: Option<Vec<String>>,
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

/// Percent encodes a query the way a search box would.
pub(crate) fn encode_query(text: &str) -> String {
    let mut out = String::new();
    for byte in text.as_bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(*byte as char)
            }
            b' ' => out.push('+'),
            other => out.push_str(&format!("%{other:02X}")),
        }
    }
    out
}

/// Keys arrive in different spellings; store one form: "ctrl+shift+t".
fn normalize_combo(raw: &str) -> String {
    let mut parts: Vec<String> = raw
        .split('+')
        .map(|part| part.trim().to_ascii_lowercase())
        .filter(|part| !part.is_empty())
        .map(|part| match part.as_str() {
            "control" => "ctrl".to_string(),
            "esc" => "escape".to_string(),
            "del" => "delete".to_string(),
            other => other.to_string(),
        })
        .collect();
    parts.sort_by_key(|part| match part.as_str() {
        "ctrl" => 0,
        "alt" => 1,
        "shift" => 2,
        "meta" => 3,
        _ => 4,
    });
    parts.join("+")
}

impl AppError {
    pub(crate) fn new(code: &str, message: impl Into<String>) -> Self {
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

/// The window the user is working in; falls back to the primary one.
pub fn focused_window(app: &AppHandle) -> Option<tauri::Window> {
    app.windows()
        .into_values()
        .find(|win| win.is_focused().unwrap_or(false))
        .or_else(|| app.get_window("w1"))
}

/// The menu bar sits between the tab strip and the address field.
/// The menu bar was removed; kept at zero so old settings cannot add a gap.
pub const MENU_BAR_HEIGHT: f64 = 0.0;

/// Height of the chrome strip: the base plus the optional bookmarks row.
pub fn chrome_height(app: &AppHandle) -> f64 {
    match app.try_state::<AppState>() {
        Some(state) => {
            let settings = state.settings.read();
            settings.chrome_height
        + if settings.show_bookmarks { 30.0 } else { 0.0 }
        }
        None => crate::window::CHROME_H,
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
    pinned: bool,
    icon: Option<String>,
    loading: bool,
}

#[derive(Clone, Serialize)]
pub struct LoadingPayload {
    pub id: String,
    pub loading: bool,
}

#[derive(Clone, Serialize)]
pub struct IconPayload {
    pub id: String,
    pub icon: Option<String>,
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
                pinned: tab.pinned,
                icon: tab.icon.clone(),
                loading: tab.loading,
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
    let _ = app.emit_to(label, "tabs:changed", list);
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
                let _ = app.emit_to(crate::ipc::window_of_tab(&id), "tab:suspended", &id);
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

/// Writes every window (tabs, selected index, geometry) to session.json.
pub(crate) fn save_sessions(app: &AppHandle) {
    let state = app.state::<AppState>();

    let mut windows: Vec<crate::session::WindowSession> = Vec::new();
    for (label, core) in state.cores.read().iter() {
        let tabs: Vec<crate::session::SessionTab> = core
            .tabs
            .iter()
            .filter(|tab| !tab.incognito)
            .map(|tab| crate::session::SessionTab::Full {
                url: tab.url.clone(),
                scroll: tab.scroll,
                pinned: tab.pinned,
            })
            .collect();
        if tabs.is_empty() {
            continue;
        }
        let selected = core
            .selected
            .as_ref()
            .and_then(|id| core.tabs.iter().position(|tab| &tab.id == id))
            .unwrap_or(0);
        windows.push(crate::session::WindowSession {
            tabs,
            selected,
            window: window_geometry(app, label),
        });
    }

    let session = crate::session::Session {
        schema_version: crate::session::SCHEMA,
        windows,
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
) -> Result<tauri::Webview, AppError> {
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
        (height - chrome_height(app)).max(1.0),
        &opts,
        incognito,
    )
    .map_err(AppError::from)
}





#[tauri::command]
pub async fn window_control(
    _app: AppHandle,
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
pub async fn window_drag(_app: AppHandle, window: tauri::Window) -> Result<(), AppError> {
    window.start_dragging()?;
    Ok(())
}

#[tauri::command]
pub async fn window_resize(_app: AppHandle, direction: String, window: tauri::Window) -> Result<(), AppError> {
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
        touch_mru(&mut core.mru, &id);
    }

    sync_visibility(&app, window.label());
    if let Some(webview) = app.get_webview(&id) {
        let _ = webview.set_focus();
        let scroll = state
            .try_read_core(window.label())
            .and_then(|core| core.tabs.iter().find(|tab| tab.id == id).map(|tab| tab.scroll))
            .unwrap_or(0.0);
        if scroll > 0.0 {
            let _ = webview.eval(&format!("window.scrollTo(0, {scroll});"));
        }
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
    // When true the new tab is created behind the current one.
    background: Option<bool>,
) -> Result<String, AppError> {
    let (id, target, selection_before) = {
        let mut core = state.write_core(window.label());
        core.next_tab_id += 1;
        // Labels carry the window so per-window lookups keep working.
    let id = tab_label(window.label(), core.next_tab_id);
        let target = match url {
        Some(input) => {
            if crate::tabs::parse_url(&input).is_none() {
                return Err(AppError::new("bad_url", format!("not a web address: {input}")));
            }
            input
        }
        None => {
            let settings = state.settings.read();
            if !settings.newtab_url.is_empty() {
                settings.newtab_url.clone()
            } else if settings.new_tab_page {
                "http://tauri.localhost/pages/newtab.html".to_string()
            } else {
                settings.start_url.clone()
            }
        }
    };
        core.tick += 1;
        let tick = core.tick;
        let previous = core.selected.clone();
        core.tabs.push(Tab {
            id: id.clone(),
            url: target.clone(),
            title: if incognito.unwrap_or(false) {
                "Incognito".into()
            } else {
                "New tab".into()
            },
            zoom: 1.0,
            pinned: false,
            incognito: incognito.unwrap_or(false),
            icon: None,
            loading: false,
            scroll: 0.0,
            alive: true,
            last_active: tick,
        });
        core.selected = Some(id.clone());
        touch_mru(&mut core.mru, &id);
        (id, target, previous)
    };

    let created = spawn_tab(&app, window.label(), &id, &target, incognito.unwrap_or(false))?;

    if background.unwrap_or(false) {
        // Created behind the current tab: put the selection back where it was.
        {
            let mut core = state.write_core(window.label());
            core.selected = selection_before;
        }
        sync_visibility(&app, window.label());
        emit_tabs(&app, window.label());
        save_sessions(&app);
        crate::logging::info(&format!("tab {id} opened in the background"));
        return Ok(id);
    }

    let _ = created.set_focus();
    crate::logging::info(&format!("tab created {id} -> {target}"));
    sync_visibility(&app, window.label());
    enforce_live_limit(&app, window.label());
    emit_tabs(&app, window.label());
    save_sessions(&app);
    Ok(id)
}
/// Pins a tab to the left strip, or releases it.
#[tauri::command]
pub async fn tab_pin(
    window: tauri::Window,
    state: State<'_, AppState>,
    id: String,
    pinned: bool,
) -> Result<(), AppError> {
    {
        let mut core = state.write_core(window.label());
        if let Some(tab) = core.tabs.iter_mut().find(|tab| tab.id == id) {
            tab.pinned = pinned;
        }
    }
    crate::logging::info(&format!("tab {id} pinned={pinned}"));
    Ok(())
}
#[tauri::command]
pub async fn tab_close(app: AppHandle, window: tauri::Window, state: State<'_, AppState>, id: String) -> Result<(), AppError> {
    if let Some(webview) = app.get_webview(&id) {
        let _ = webview.close();
    }

    let (remaining, next_url) = {
        let mut core = state.write_core(window.label());
        if let Some(index) = core.tabs.iter().position(|tab| tab.id == id) {
            let closed = core.tabs[index].clone();
            if !closed.incognito {
                let mut history = state.closed_tabs.write().unwrap_or_else(|e| e.into_inner());
                history.push((closed.url, closed.title));
                if history.len() > 20 {
                    history.remove(0);
                }
            }
        }
        let closed_at = core.tabs.iter().position(|tab| tab.id == id);
        core.tabs.retain(|tab| tab.id != id);
        core.mru.retain(|entry| entry != &id);
        if core.selected.as_deref() == Some(id.as_str()) {
            let go_left = state.settings.read().tab_close_goes_left;
            core.selected = match closed_at {
                // Step to the neighbour on the chosen side, staying in range.
                Some(index) if go_left => core
                    .tabs
                    .get(index.saturating_sub(1))
                    .map(|tab| tab.id.clone()),
                Some(index) => core.tabs.get(index).map(|tab| tab.id.clone()),
                None => None,
            }
            .or_else(|| core.tabs.last().map(|tab| tab.id.clone()));
        }
        (core.tabs.len(), state.settings.read().start_url.clone())
    };

    // Closing the last tab shows a fresh one instead of quitting.
    if remaining == 0 {
        let _ = tab_new(app, window.clone(), state, Some(next_url), None, None).await;
        return Ok(());
    }

    sync_visibility(&app, window.label());
    emit_tabs(&app, window.label());
    save_sessions(&app);
    Ok(())
}

#[tauri::command]
pub async fn tab_navigate(app: AppHandle, window: tauri::Window, state: State<'_, AppState>, url: String) -> Result<(), AppError> {
    // A dead host would land on WebView2's own error page, which we cannot
    // replace, so answer it ourselves before navigating.
    if url.starts_with("http") && !is_internal_url(&url) && !state.reachability.check(&url) {
        let id = selected_label(&state, &window);
        if !id.is_empty() {
            crate::logging::info(&format!("unreachable: {url}"));
            show_error_page(&app, &id, url.trim());
        }
        return Ok(());
    }

    let Some(target) = crate::tabs::parse_url(&url) else {
        crate::logging::warn(&format!("rejected address: {url}"));
        let id = selected_label(&state, &window);
        if !id.is_empty() {
            show_error_page(&app, &id, url.trim());
        }
        return Err(AppError::new("bad_url", format!("not a web address: {url}")));
    };
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
        // Without this the first click after a menu action only activates the
        // webview; the click never reaches the page.
        let _ = webview.set_focus();
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

pub(crate) fn selected_label(state: &AppState, window: &tauri::Window) -> String {
    state
        .read_core(window.label())
        .selected
        .clone()
        .unwrap_or_default()
}

#[tauri::command]
pub async fn theme_get(_window: tauri::Window, state: State<'_, AppState>) -> Result<Theme, AppError> {
    Ok(state.theme.read().clone())
}

#[tauri::command]
pub async fn theme_apply(
    app: AppHandle,
    _window: tauri::Window, state: State<'_, AppState>,
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
pub async fn theme_revert(app: AppHandle, _window: tauri::Window, state: State<'_, AppState>) -> Result<Theme, AppError> {
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
pub async fn download_list(_window: tauri::Window, state: State<'_, AppState>) -> Result<Vec<crate::downloads::DownloadTask>, AppError> {
    Ok(state.downloads.read().unwrap_or_else(|e| e.into_inner()).clone())
}

/// Zips the whole profile so it can be restored on another machine.
#[tauri::command]
pub async fn backup_export(_window: tauri::Window, state: State<'_, AppState>) -> Result<String, AppError> {
    let default_name = format!(
        "Mix-backup-{}.zip",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|value| value.as_secs())
            .unwrap_or(0)
    );
    let Some(target) = rfd::FileDialog::new()
        .set_file_name(&default_name)
        .add_filter("zip", &["zip"])
        .save_file()
    else {
        return Err(AppError::new("cancelled", "no file chosen"));
    };

    let file = std::fs::File::create(&target).map_err(|err| AppError::new("backup", err.to_string()))?;
    let mut zip = zip::ZipWriter::new(file);
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);

    // Only data, never the WebView2 profile: it is huge and machine specific.
    let skip = ["profile", "profile-ui", "logs", "diag", "extensions", "downloads"];
    let mut count = 0;
    let mut stack = vec![state.dirs.user_data.clone()];

    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let name = path.file_name().and_then(|value| value.to_str()).unwrap_or_default();
            if path.is_dir() {
                if dir == state.dirs.user_data && skip.contains(&name) {
                    continue;
                }
                stack.push(path);
                continue;
            }
            let Ok(relative) = path.strip_prefix(&state.dirs.user_data) else {
                continue;
            };
            let entry_name = relative.display().to_string().replace('\\', "/");
            let Ok(bytes) = std::fs::read(&path) else {
                continue;
            };
            if zip.start_file(entry_name, options).is_err() {
                continue;
            }
            let _ = std::io::Write::write_all(&mut zip, &bytes);
            count += 1;
        }
    }

    zip.finish().map_err(|err| AppError::new("backup", err.to_string()))?;
    crate::logging::info(&format!("backup written: {} ({count} files)", target.display()));
    Ok(target.display().to_string())
}

/// Restores a backup over the current profile; the app restarts afterwards.
#[tauri::command]
pub async fn backup_import(_window: tauri::Window, state: State<'_, AppState>) -> Result<String, AppError> {
    let Some(source) = rfd::FileDialog::new()
        .add_filter("zip", &["zip"])
        .pick_file()
    else {
        return Err(AppError::new("cancelled", "no file chosen"));
    };

    let file = std::fs::File::open(&source).map_err(|err| AppError::new("backup", err.to_string()))?;
    let mut archive = zip::ZipArchive::new(file).map_err(|err| AppError::new("backup", err.to_string()))?;

    let mut restored = 0;
    for index in 0..archive.len() {
        let Ok(mut entry) = archive.by_index(index) else {
            continue;
        };
        let Some(name) = entry.enclosed_name() else {
            continue;
        };
        let target = state.dirs.user_data.join(&name);
        if let Some(parent) = target.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let mut buffer = Vec::new();
        if std::io::Read::read_to_end(&mut entry, &mut buffer).is_err() {
            continue;
        }
        if std::fs::write(&target, buffer).is_ok() {
            restored += 1;
        }
    }

    crate::logging::info(&format!(
        "backup restored: {} ({restored} files)",
        source.display()
    ));
    Ok(format!("{restored} files"))
}

/// Makes Mix a candidate default browser, then opens the Windows picker: the
/// app must never take handlers away from the user silently.
#[tauri::command]
pub async fn register_browser() -> Result<String, AppError> {
    let exe = crate::browser_reg::register()?;
    #[cfg(windows)]
    let _ = std::process::Command::new("cmd")
        .args(["/C", "start", "", "ms-settings:defaultapps"])
        .spawn();
    Ok(exe)
}

/// Reveals a file in Explorer, or opens its folder when it is missing.
pub(crate) fn reveal_path(path: &std::path::Path) -> Result<(), AppError> {
    #[cfg(windows)]
    {
        let target = if path.is_dir() {
            path.to_path_buf()
        } else {
            path.parent()
                .map(|parent| parent.to_path_buf())
                .unwrap_or_else(|| path.to_path_buf())
        };
        std::process::Command::new("explorer")
            .arg(target)
            .spawn()
            .map(|_| ())
            .map_err(|err| AppError::new("open", err.to_string()))
    }
    #[cfg(not(windows))]
    {
        let _ = path;
        Err(AppError::new("open", "not supported on this platform"))
    }
}

/// Opens the file a download produced.
/// Drops finished entries from the list; files on disk stay.
#[tauri::command]
pub async fn download_clear(_window: tauri::Window, state: State<'_, AppState>) -> Result<(), AppError> {
    let mut tasks = state.downloads.write().unwrap_or_else(|e| e.into_inner());
    tasks.retain(|task| task.state == "started");
    Ok(())
}

#[tauri::command]
pub async fn download_open(
    _window: tauri::Window,
    state: State<'_, AppState>,
    id: u64,
) -> Result<(), AppError> {
    let path = {
        let tasks = state.downloads.read().unwrap_or_else(|e| e.into_inner());
        tasks
            .iter()
            .find(|task| task.id == id)
            .map(|task| task.path.clone())
    };
    let Some(path) = path else {
        return Err(AppError::new("bad_download", "unknown download"));
    };
    reveal_path(std::path::Path::new(&path))
}
#[tauri::command]
pub async fn download_open_dir(_window: tauri::Window, state: State<'_, AppState>, id: u64) -> Result<(), AppError> {
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

/// Opens a search in a new tab using the configured engine.
#[tauri::command]
pub async fn search_new_tab(
    app: AppHandle,
    window: tauri::Window,
    state: State<'_, AppState>,
    text: String,
) -> Result<(), AppError> {
    let query = text.trim().to_string();
    if query.is_empty() {
        return Ok(());
    }
    let url = {
        let settings = state.settings.read();
        let template = settings.custom_search.trim();
        if template.contains("%s") {
            template.replace("%s", &encode_query(&query))
        } else {
            let base = match settings.search_engine.as_str() {
                "bing" => "https://www.bing.com/search?q=",
                "google" => "https://www.google.com/search?q=",
                "ddg" => "https://duckduckgo.com/?q=",
                _ => "https://www.baidu.com/s?wd=",
            };
            format!("{base}{}", encode_query(&query))
        }
    };
    crate::logging::info(&format!("search: {query}"));
    tab_new(app, window, state, Some(url), None, None).await?;
    Ok(())
}
/// Shows what the drag engine sees under the pointer for a few seconds.
#[tauri::command]
pub async fn drag_probe(
    app: AppHandle,
    window: tauri::Window,
    state: State<'_, AppState>,
) -> Result<(), AppError> {
    let label = selected_label(&state, &window);
    let Some(webview) = app.get_webview(&label) else {
        return Ok(());
    };
    let _ = webview.eval(crate::tabs::DRAG_PROBE);
    crate::logging::info("drag probe injected");
    Ok(())
}
#[tauri::command]
pub async fn shortcut(app: AppHandle, window: tauri::Window, state: State<'_, AppState>, action: String) -> Result<(), AppError> {
    if action != "dismiss" {
        crate::logging::info(&format!("shortcut {action}"));
    }
    match action.as_str() {
        "new_tab" => {
            tab_new(app, window.clone(), state, None, None, None).await?;
        }
        "new_incognito" => {
            tab_new(app, window.clone(), state, None, Some(true), None).await?;
        }
        "dismiss" => {
            crate::overlay::close(&app, window.label());
        }
        "reopen_tab" => {
            let _ = tab_reopen(app, window, state).await?;
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
            // Focus the chrome webview, and arm the page side key bridge so typing
            // works even when WebView2 keeps the keyboard on the page.
            let _ = window.set_focus();
            if let Some(chrome) = app.get_webview(&format!("{}-chrome", window.label())) {
                let _ = chrome.set_focus();
            }
            set_url_edit(&app, &window, true);
            let _ = app.emit_to(window.label(), "ui:focus-url", ());
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
        // Not gated on debug: F12 must work in the packaged build too.
        // Gesture and palette actions that reuse existing commands.
        "stop" => {
            let target = selected_label(&state, &window);
            if let Some(webview) = app.get_webview(&target) {
                let _ = webview.eval("window.stop()");
            }
        }
        "copy_url" => {
            let url = {
                let core = state.read_core(window.label());
                core.tabs
                    .iter()
                    .find(|tab| Some(&tab.id) == core.selected.as_ref())
                    .map(|tab| tab.url.clone())
                    .unwrap_or_default()
            };
            if !url.is_empty() {
                let mut board =
                    arboard::Clipboard::new().map_err(|err| AppError::new("clipboard", err.to_string()))?;
                board
                    .set_text(url)
                    .map_err(|err| AppError::new("clipboard", err.to_string()))?;
            }
        }
        "duplicate_tab" => {
            let url = {
                let core = state.read_core(window.label());
                core.tabs
                    .iter()
                    .find(|tab| Some(&tab.id) == core.selected.as_ref())
                    .map(|tab| tab.url.clone())
                    .unwrap_or_default()
            };
            tab_new(app, window.clone(), state, Some(url), None, None).await?;
        }
        "close_other_tabs" => {
            let keep = selected_label(&state, &window);
            let others: Vec<String> = {
                let core = state.read_core(window.label());
                core.tabs
                    .iter()
                    .filter(|tab| tab.id != keep)
                    .map(|tab| tab.id.clone())
                    .collect()
            };
            for id in others {
                let state = app.state::<AppState>();
                tab_close(app.clone(), window.clone(), state, id).await?;
            }
        }
        "toggle_pin" => {
            let (id, pinned) = {
                let core = state.read_core(window.label());
                match core.tabs.iter().find(|tab| Some(&tab.id) == core.selected.as_ref()) {
                    Some(tab) => (tab.id.clone(), tab.pinned),
                    None => (String::new(), false),
                }
            };
            if !id.is_empty() {
                let mut core = state.write_core(window.label());
                if let Some(tab) = core.tabs.iter_mut().find(|tab| tab.id == id) {
                    tab.pinned = !pinned;
                }
            }
        }
        "minimize" => window_of(&window).minimize()?,
        "maximize" => {
            let win = window_of(&window);
            if win.is_maximized()? {
                win.unmaximize()?;
            } else {
                win.maximize()?;
            }
        }
        "close_window" => window_of(&window).close()?,
        "open_home" | "open_history" | "open_bookmarks" | "open_settings" | "open_extensions" | "open_downloads" => {
            let page = match action.as_str() {
                "open_home" => state.settings.read().start_url.clone(),
                "open_history" => "http://tauri.localhost/pages/history.html".to_string(),
                "open_bookmarks" => "http://tauri.localhost/pages/bookmarks.html".to_string(),
                "open_settings" => "http://tauri.localhost/pages/settings.html".to_string(),
                "open_extensions" => "http://tauri.localhost/pages/extensions.html".to_string(),
                _ => "http://tauri.localhost/pages/downloads.html".to_string(),
            };
            tab_new(app, window.clone(), state, Some(page), None, None).await?;
        }
        "translate" => {
            let target = selected_label(&state, &window);
            if let Some(webview) = app.get_webview(&target) {
                let _ = webview.eval("window.__mixStartTranslate && window.__mixStartTranslate()");
            }
        }
        "devtools" => {
            let target = selected_label(&state, &window);
            match app.get_webview(&target) {
                Some(webview) => {
                    webview.open_devtools();
                    crate::logging::info(&format!("devtools opened for {target}"));
                }
                None => crate::logging::warn(&format!("devtools: no webview for {target}")),
            }
        }
        other => return Err(AppError::new("bad_shortcut", other)),
    }
    Ok(())
}

/// Ctrl+Tab follows the recency list, so it jumps to the last used tab.
fn next_by_recency(mru: &[String], delta: isize) -> Option<String> {
    if mru.len() < 2 {
        return None;
    }
    if delta >= 0 {
        Some(mru[1].clone())
    } else {
        Some(mru[mru.len() - 1].clone())
    }
}

fn touch_mru(mru: &mut Vec<String>, id: &str) {
    mru.retain(|entry| entry != id);
    mru.insert(0, id.to_string());
}

async fn cycle_tab(app: &AppHandle, window: &tauri::Window, delta: isize) {
    let next = {
        let state = app.state::<AppState>();
        let core = state.read_core(window.label());
        next_by_recency(&core.mru, delta)
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
    let level = crate::zoom::normalize(level);
    let state = app.state::<AppState>();

    let (id, url) = {
        let core = state.read_core(label);
        match core
            .selected
            .as_ref()
            .and_then(|id| core.tabs.iter().find(|tab| &tab.id == id))
        {
            Some(tab) => (tab.id.clone(), tab.url.clone()),
            None => return Ok(()),
        }
    };

    {
        let mut core = state.write_core(label);
        if let Some(tab) = core.tabs.iter_mut().find(|tab| tab.id == id) {
            tab.zoom = level;
        }
    }
    if let Some(webview) = app.get_webview(&id) {
        webview.set_zoom(level)?;
    }

    // The level belongs to the site, not the tab, so it survives both.
    {
        let mut store = state.zoom.write();
        if store.remember(&url, level) {
            if let Err(err) = store.save(&state.dirs.zoom_file) {
                crate::logging::warn(&format!("zoom save failed: {err}"));
            }
        }
    }

    crate::logging::info(&format!("zoom -> {level:.1} for {url}"));
    Ok(())
}

/// Pages report their context menu here; dismiss=true closes an open menu.
/// Pages served by the app itself: no DNS, no TCP, so never probed.
pub(crate) fn is_internal_url(url: &str) -> bool {
    url.contains("tauri.localhost") || url.starts_with("data:") || url.starts_with("about:")
}

/// The chrome strip is only as tall as the toolbar, so a menu drawn inside it
/// would be clipped; it gets its own overlay like the page menu.
#[tauri::command]
pub async fn chrome_menu(
    app: AppHandle,
    window: tauri::Window,
    kind: String,
    x: f64,
    y: f64,
) -> Result<(), AppError> {
    let label = match kind.as_str() {
        "url" => "url_menu",
        _ => "chrome_menu",
    };
    let (menu_w, menu_h) = if label == "url_menu" {
        (190.0, 150.0)
    } else {
        (190.0, 110.0)
    };
    let _ = app.emit_to(window.label(), "chrome-menu", ());
    let _ = crate::overlay::open_at(&app, &window, label, x, y, menu_w, menu_h);
    Ok(())
}

/// The address field cannot read the clipboard itself: the webview clipboard API
/// is gated behind a permission prompt that never appears, so go through the OS.
#[tauri::command]
pub async fn clipboard_read() -> Result<String, AppError> {
    let mut board = arboard::Clipboard::new().map_err(|err| AppError::new("clipboard", err.to_string()))?;
    board.get_text().map_err(|err| AppError::new("clipboard", err.to_string()))
}

#[tauri::command]
pub async fn clipboard_write(text: String) -> Result<(), AppError> {
    let mut board = arboard::Clipboard::new().map_err(|err| AppError::new("clipboard", err.to_string()))?;
    board.set_text(text).map_err(|err| AppError::new("clipboard", err.to_string()))
}

/// Runs a text edit in the chrome page, which owns the address field.
#[tauri::command]
pub async fn url_action(
    app: AppHandle,
    window: tauri::Window,
    action: String,
) -> Result<(), AppError> {
    if let Some(chrome) = app.get_webview(&format!("{}-chrome", window.label())) {
        let script = format!("window.__mixUrlAction && window.__mixUrlAction(\"{action}\")");
        let _ = chrome.eval(&script);
    }
    Ok(())
}

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
    // Height follows the entries that actually apply, so the panel has no dead space.
    let mut rows = 5.0; // back, forward, reload, print, inspect
    let mut groups = 2.0;
    if context.link.is_some() { rows += 2.0; groups += 1.0; }
    if context.image.is_some() { rows += 2.0; groups += 1.0; }
    if context.selection.is_some() { rows += 1.0; groups += 1.0; }
    if context.editable { rows += 3.0; groups += 1.0; }

    let menu_w = 250.0;
    let menu_h = rows * 30.0 + groups * 11.0 + 12.0;
    let x = (request.x.unwrap_or(0.0) + 80.0).clamp(4.0, (width - menu_w - 4.0).max(4.0));
    let y = (chrome_height(&app) + request.y.unwrap_or(0.0))
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
pub async fn menu_context_get(_window: tauri::Window, state: State<'_, AppState>) -> Result<MenuContext, AppError> {
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
        chrome_height(&app) + 48.0,
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
pub async fn settings_get(_window: tauri::Window, state: State<'_, AppState>) -> Result<Settings, AppError> {
    Ok(state.settings.read().clone())
}

#[tauri::command]
pub async fn settings_set(
    _window: tauri::Window, state: State<'_, AppState>,
    patch: SettingsPatch,
) -> Result<Settings, AppError> {
    let settings = {
        let mut guard = state.settings.write();
        if let Some(value) = patch.restore_session {
            guard.restore_session = value;
        }
        if let Some(value) = patch.show_bookmarks {
            guard.show_bookmarks = value;
        }
        if let Some(value) = patch.baidu_appid {
            guard.baidu_appid = value.trim().to_string();
        }
        if let Some(value) = patch.baidu_secret {
            guard.baidu_secret = value.trim().to_string();
        }
        if let Some(value) = patch.drag_hint {
            guard.drag_hint = value;
        }
        if let Some(value) = patch.drag_threshold {
            guard.drag_threshold = value.clamp(8, 120);
        }
        if let Some(value) = patch.drag_map {
            guard.drag_map = value;
        }
        if let Some(value) = patch.super_drag {
            guard.super_drag = value;
        }
        if let Some(value) = patch.gesture_trail {
            guard.gesture_trail = value;
        }
        if let Some(value) = patch.gesture_color {
            guard.gesture_color = value.trim().to_string();
        }
        if let Some(value) = patch.gesture_width {
            guard.gesture_width = value.clamp(1, 10);
        }
        if let Some(value) = patch.gesture_threshold {
            guard.gesture_threshold = value.clamp(10, 200);
        }
        if let Some(value) = patch.gesture_map {
            guard.gesture_map = value;
        }
        if let Some(value) = patch.gesture_blocked {
            guard.gesture_blocked = value.trim().to_string();
        }
        if let Some(value) = patch.gestures {
            guard.gestures = value;
        }
        if let Some(value) = patch.tab_double_click_close {
            guard.tab_double_click_close = value;
        }
        if let Some(value) = patch.tab_show_close {
            guard.tab_show_close = value;
        }
        if let Some(value) = patch.tab_close_goes_left {
            guard.tab_close_goes_left = value;
        }
        if let Some(value) = patch.custom_search {
            guard.custom_search = value.trim().to_string();
        }
        if let Some(value) = patch.newtab_show_search {
            guard.newtab_show_search = value;
        }
        if let Some(value) = patch.newtab_show_sites {
            guard.newtab_show_sites = value;
        }
        if let Some(value) = patch.newtab_url {
            guard.newtab_url = value.trim().to_string();
        }
        if let Some(value) = patch.newtab_tiles {
            guard.newtab_tiles = value.clamp(1, 40);
        }
        if let Some(value) = patch.newtab_columns {
            guard.newtab_columns = value.clamp(1, 12);
        }
        if let Some(value) = patch.newtab_radius {
            guard.newtab_radius = value.min(50);
        }
        if let Some(value) = patch.shortcuts {
            guard.shortcuts = value
                .into_iter()
                .filter(|(_, combo)| !combo.trim().is_empty())
                .map(|(action, combo)| (action, normalize_combo(&combo)))
                .collect();
        }
        if let Some(value) = patch.search_engine {
            guard.search_engine = match value.as_str() {
                "bing" | "google" | "ddg" => value,
                _ => "baidu".to_string(),
            };
        }
        if let Some(value) = patch.new_tab_page {
            guard.new_tab_page = value;
        }
        if let Some(value) = patch.menu_bar {
            guard.menu_bar = value;
        }
        if let Some(value) = patch.language {
            guard.language = match value.as_str() {
                "zh" => "zh".to_string(),
                _ => "en".to_string(),
            };
        }
        if let Some(value) = patch.translate_to {
            if !value.trim().is_empty() {
                guard.translate_to = value.trim().to_string();
            }
        }
        if let Some(value) = patch.toolbar {
            // Only ids we know how to draw survive a hand edited settings file.
            let known = ["downloads", "translate", "find", "zoom", "bookmark", "extensions"];
            guard.toolbar = value
                .into_iter()
                .filter(|id| known.contains(&id.as_str()))
                .collect();
        }
        if let Some(value) = patch.hidden_extensions {
            guard.hidden_extensions = value;
        }
        if let Some(value) = patch.disabled_extensions {
            guard.disabled_extensions = value;
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

/// The extensions WebView2 actually loaded, with the ids it assigned.
#[tauri::command]
pub async fn extension_live_list(
    app: AppHandle,
    window: tauri::Window,
    state: State<'_, AppState>,
) -> Result<Vec<crate::ext_live::LiveExtension>, AppError> {
    let label = selected_label(&state, &window);
    let Some(webview) = app.get_webview(&label) else {
        return Ok(Vec::new());
    };
    let listed = crate::ext_live::list(&webview);
    crate::logging::info(&format!("live extensions: {}", listed.len()));
    Ok(listed)
}

/// Enables or disables a loaded extension without restarting the app.
#[tauri::command]
pub async fn extension_live_set(
    app: AppHandle,
    window: tauri::Window,
    state: State<'_, AppState>,
    id: String,
    enabled: bool,
) -> Result<bool, AppError> {
    let label = selected_label(&state, &window);
    let Some(webview) = app.get_webview(&label) else {
        return Ok(false);
    };
    let ok = crate::ext_live::set_enabled(&webview, &id, enabled);
    crate::logging::info(&format!("extension {id} enabled={enabled}: {ok}"));
    Ok(ok)
}
#[tauri::command]
pub async fn ext_list(_window: tauri::Window, state: State<'_, AppState>) -> Result<Vec<ExtInfo>, AppError> {
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
pub async fn zoom_step(app: AppHandle, delta: f64, window: tauri::Window, _state: State<'_, AppState>) -> Result<f64, AppError> {
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

#[derive(Serialize)]
pub struct Suggestion {
    pub url: String,
    pub title: String,
    pub kind: String,
}

/// History first, then bookmarks, deduped by url and ranked by how well the
/// text matches the start of the url or the title.
fn collect_suggestions(state: &AppState, text: &str, limit: usize) -> Vec<Suggestion> {
    let needle = text.trim().to_lowercase();
    let mut found: Vec<Suggestion> = Vec::new();

    let history = state.history.read().unwrap_or_else(|e| e.into_inner()).clone();
    for visit in history {
        found.push(Suggestion {
            url: visit.url,
            title: visit.title,
            kind: "history".into(),
        });
    }
    for bookmark in state.bookmarks.read().unwrap_or_else(|e| e.into_inner()).iter() {
        found.push(Suggestion {
            url: bookmark.url.clone(),
            title: bookmark.title.clone(),
            kind: "bookmark".into(),
        });
    }

    let mut seen: Vec<String> = Vec::new();
    found.retain(|item| {
        if seen.contains(&item.url) {
            return false;
        }
        seen.push(item.url.clone());
        true
    });

    if !needle.is_empty() {
        found.retain(|item| {
            let url = item.url.to_lowercase();
            let title = item.title.to_lowercase();
            url.contains(&needle) || title.contains(&needle)
        });
        found.sort_by_key(|item| {
            let url = item.url.to_lowercase();
            let title = item.title.to_lowercase();
            if url.contains(&needle) { 0 } else if title.contains(&needle) { 1 } else { 2 }
        });
    }

    found.truncate(limit);
    found
}

/// Opens the dropdown under the address bar.
#[tauri::command]
pub async fn suggest_open(
    app: AppHandle,
    window: tauri::Window,
    x: f64,
    width: f64,
) -> Result<(), AppError> {
    if app
        .state::<AppState>()
        .overlay
        .read()
        .unwrap_or_else(|e| e.into_inner())
        .contains_key(window.label())
    {
        return Ok(());
    }
    *app.state::<AppState>().suggest_width.write() = width;
    crate::overlay::open_at(
        &app,
        &window,
        "suggest",
        x,
        chrome_height(&app) + 2.0,
        width,
        48.0,
    )
    .map_err(|e| AppError::new("overlay", e))
}

/// Recomputes the dropdown and pushes it into the overlay page.
#[tauri::command]
pub async fn suggest_feed(
    app: AppHandle,
    window: tauri::Window,
    state: State<'_, AppState>,
    text: String,
) -> Result<usize, AppError> {
    *state.suggest_text.write() = text.clone();
    Ok(push_suggestions(&app, window.label(), &state))
}

/// Re-sends the current suggestions to a page that just finished loading.
#[tauri::command]
pub async fn suggest_refresh(
    app: AppHandle,
    window: tauri::Window,
    state: State<'_, AppState>,
) -> Result<usize, AppError> {
    Ok(push_suggestions(&app, window.label(), &state))
}

fn push_suggestions(app: &AppHandle, window_label: &str, state: &AppState) -> usize {
    let text = state.suggest_text.read().clone();
    let items = collect_suggestions(state, &text, 8);
    let count = items.len();

    let label = app
        .state::<AppState>()
        .overlay
        .read()
        .unwrap_or_else(|e| e.into_inner())
        .get(window_label)
        .cloned();

    let Some(label) = label else {
        return 0;
    };
    let Some(webview) = app.get_webview(&label) else {
        return 0;
    };

    let json = serde_json::to_string(&items).unwrap_or_else(|_| "[]".into());
    let rows = count as f64;
    // 34px per row plus 6px padding top and bottom; no scrollbar wanted.
    let height = if rows == 0.0 { 40.0 } else { rows * 34.0 + 14.0 };
    let width = *state.suggest_width.read();
    let _ = webview.set_size(tauri::LogicalSize::new(width, height));
    let _ = webview.eval(&format!("window.__mixSuggest && window.__mixSuggest({json})"));
    count
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
    _app: AppHandle,
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
pub async fn app_clear_data(app: AppHandle, window: tauri::Window, _state: State<'_, AppState>) -> Result<(), AppError> {
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

/// Icons are fetched once and stored as files: the site may change or vanish,
/// and pages should not depend on a remote URL staying alive.
fn cache_icon(app: &AppHandle, host: &str, url: &str) {
    if host.is_empty() || !url.starts_with("http") {
        return;
    }
    let Some(state) = app.try_state::<AppState>() else {
        return;
    };
    let dir = state.dirs.favicons_dir.clone();
    if crate::favicons::cache_path(&dir, host).is_some() {
        return;
    }
    crate::logging::info(&format!("favicon download: {host} <- {url}"));
    let handle = app.clone();
    let host = host.to_string();
    let url = url.to_string();
    tauri::async_runtime::spawn(async move {
        if crate::favicons::cache_path(&dir, &host).is_some() {
            return;
        }
        let Ok(client) = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .build()
        else {
            return;
        };
        let response = match client.get(&url).send().await {
            Ok(response) => response,
            Err(err) => {
                crate::logging::warn(&format!("favicon fetch failed: {host}: {err}"));
                return;
            }
        };
        let content_type = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .unwrap_or_default()
            .to_string();
        let bytes = match response.bytes().await {
            Ok(bytes) => bytes,
            Err(err) => {
                crate::logging::warn(&format!("favicon body failed: {host}: {err}"));
                return;
            }
        };
        match crate::favicons::store_icon(&dir, &host, &bytes, &content_type) {
            Some(path) => crate::logging::info(&format!("favicon cached: {}", path.display())),
            None => crate::logging::warn(&format!(
                "favicon not stored: {host} ({} bytes, {content_type})",
                bytes.len()
            )),
        }
        // The refresh of the page happens on the next open; no event needed.
        let _ = handle;
    });
}

/// The document title arrives after the url change that recorded the visit, so
/// the newest entry for that url still carries the previous page's title.
pub(crate) fn attach_title(app: &AppHandle, url: &str, title: &str) {
    if title.trim().is_empty() {
        return;
    }
    let Some(state) = app.try_state::<AppState>() else {
        return;
    };
    let snapshot = {
        let mut visits = state.history.write().unwrap_or_else(|e| e.into_inner());
        let Some(visit) = visits.iter_mut().find(|visit| visit.url == url) else {
            return;
        };
        if visit.title == title {
            return;
        }
        visit.title = title.to_string();
        visits.clone()
    };
    let _ = crate::library::save_visits(&state.dirs.history_file, &snapshot);
}

/// The favicon arrives after the visit is recorded, so patch the newest match.
pub(crate) fn attach_icon(app: &AppHandle, url: &str, icon: &str) {
    let Some(state) = app.try_state::<AppState>() else {
        return;
    };
    let snapshot = {
        let mut visits = state.history.write().unwrap_or_else(|e| e.into_inner());
        let mut changed = false;
        for visit in visits.iter_mut() {
            if visit.url == url && visit.icon.as_deref() != Some(icon) {
                visit.icon = Some(icon.to_string());
                changed = true;
                break;
            }
        }
        if !changed {
            return;
        }
        visits.clone()
    };
    let _ = crate::library::save_visits(&state.dirs.history_file, &snapshot);
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

/// host -> icon url, used by the bookmarks and new tab pages.
#[tauri::command]
pub async fn favicon_map(state: State<'_, AppState>) -> Result<std::collections::BTreeMap<String, String>, AppError> {
    let icons = state.favicons.read().unwrap_or_else(|e| e.into_inner());
    let mut out = std::collections::BTreeMap::new();
    for (host, remote) in icons.hosts.iter() {
        let value = crate::favicons::cache_path(&state.dirs.favicons_dir, host)
            .and_then(|path| crate::favicons::data_url(&path))
            .unwrap_or_else(|| remote.clone());
        out.insert(host.clone(), value);
    }
    Ok(out)
}

#[tauri::command]
pub async fn history_list(_window: tauri::Window, state: State<'_, AppState>, limit: Option<usize>) -> Result<Vec<Visit>, AppError> {
    let visits = state.history.read().unwrap_or_else(|e| e.into_inner());
    let icons = state.favicons.read().unwrap_or_else(|e| e.into_inner());
    let take = limit.unwrap_or(500).min(visits.len());
    Ok(visits
        .iter()
        .take(take)
        .map(|visit| {
            let mut copy = visit.clone();
            if copy.icon.is_none() {
                let host = crate::favicons::host_of(&visit.url);
                copy.icon = icons.get(&host).map(str::to_string);
            }
            copy
        })
        .collect())
}

/// Removes the rows the history page has selected.
#[tauri::command]
pub async fn history_remove(
    _window: tauri::Window,
    state: State<'_, AppState>,
    entries: Vec<(String, u64)>,
) -> Result<usize, AppError> {
    let removed = {
        let mut visits = state.history.write().unwrap_or_else(|e| e.into_inner());
        let before = visits.len();
        visits.retain(|visit| {
            !entries
                .iter()
                .any(|(url, at)| visit.url == *url && visit.at == *at)
        });
        // Timestamps can drift after a title or icon backfill; fall back to the
        // url alone so the button keeps working.
        if visits.len() == before {
            visits.retain(|visit| !entries.iter().any(|(url, _)| visit.url == *url));
        }
        before - visits.len()
    };
    let snapshot = state.history.read().unwrap_or_else(|e| e.into_inner()).clone();
    crate::library::save_visits(&state.dirs.history_file, &snapshot)?;
    crate::logging::info(&format!("history: removed {removed} entries"));
    Ok(removed)
}

/// Clears the selected kinds of local data and reports what was cleared.
#[tauri::command]
pub async fn clear_data(
    app: AppHandle,
    _window: tauri::Window,
    state: State<'_, AppState>,
    kinds: Vec<String>,
) -> Result<Vec<String>, AppError> {
    let mut done = Vec::new();

    if kinds.iter().any(|kind| kind == "history") {
        state.history.write().unwrap_or_else(|e| e.into_inner()).clear();
        crate::library::save_visits(&state.dirs.history_file, &[])?;
        done.push("history".to_string());
    }

    if kinds.iter().any(|kind| kind == "bookmarks") {
        state.bookmarks.write().unwrap_or_else(|e| e.into_inner()).clear();
        crate::library::save_bookmarks(&state.dirs.bookmarks_file, &[])?;
        let _ = app.emit("bookmarks:changed", false);
        done.push("bookmarks".to_string());
    }

    if kinds.iter().any(|kind| kind == "favicons") {
        state
            .favicons
            .write()
            .unwrap_or_else(|e| e.into_inner())
            .hosts
            .clear();
        let _ = std::fs::remove_dir_all(&state.dirs.favicons_dir);
        let empty = state.favicons.read().unwrap_or_else(|e| e.into_inner()).clone();
        empty.save(&state.dirs.favicons_file);
        done.push("favicons".to_string());
    }

    if kinds.iter().any(|kind| kind == "session") {
        let _ = std::fs::remove_file(&state.dirs.session_file);
        done.push("session".to_string());
    }

    if kinds.iter().any(|kind| kind == "zoom") {
        let mut zoom = state.zoom.write();
        zoom.forget_all();
        let _ = zoom.save(&state.dirs.zoom_file);
        done.push("zoom".to_string());
    }

    crate::logging::info(&format!("cleared: {}", done.join(", ")));
    Ok(done)
}

#[tauri::command]
pub async fn history_clear(_window: tauri::Window, state: State<'_, AppState>) -> Result<(), AppError> {
    {
        let mut visits = state.history.write().unwrap_or_else(|e| e.into_inner());
        visits.clear();
    }
    crate::library::save_visits(&state.dirs.history_file, &[])?;
    crate::logging::info("history cleared");
    Ok(())
}

#[tauri::command]
pub async fn bookmark_list(_window: tauri::Window, state: State<'_, AppState>) -> Result<Vec<Bookmark>, AppError> {
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

/// Adds a bookmark for an explicit url; used when something is dropped on the bar.
#[tauri::command]
pub async fn bookmark_add(
    app: AppHandle,
    state: State<'_, AppState>,
    url: String,
    title: String,
) -> Result<(), AppError> {
    if !url.starts_with("http://") && !url.starts_with("https://") {
        return Ok(());
    }
    let snapshot = {
        let mut bookmarks = state.bookmarks.write().unwrap_or_else(|e| e.into_inner());
        if !bookmarks.iter().any(|bookmark| bookmark.url == url) {
            crate::library::toggle_bookmark(&mut bookmarks, &url, &title);
        }
        bookmarks.clone()
    };
    crate::library::save_bookmarks(&state.dirs.bookmarks_file, &snapshot)?;
    let _ = app.emit("bookmarks:changed", true);
    Ok(())
}

#[tauri::command]
pub async fn bookmark_move(
    app: AppHandle,
    state: State<'_, AppState>,
    from: String,
    to: usize,
) -> Result<(), AppError> {
    let snapshot = {
        let mut bookmarks = state.bookmarks.write().unwrap_or_else(|e| e.into_inner());
        move_bookmark(&mut bookmarks, &from, to);
        bookmarks.clone()
    };
    crate::library::save_bookmarks(&state.dirs.bookmarks_file, &snapshot)?;
    let _ = app.emit("bookmarks:changed", false);
    Ok(())
}

/// Swaps WebView2's own error page for ours; returns the url it replaced.
pub fn show_error_page(app: &AppHandle, id: &str, failed_url: &str) -> Option<String> {
    let state = app.try_state::<AppState>()?;
    {
        let mut shown = state.error_shown.write().unwrap_or_else(|e| e.into_inner());
        if shown.get(id).map(String::as_str) == Some(failed_url) {
            return None;
        }
        shown.insert(id.to_string(), failed_url.to_string());
    }

    let base = state.app_base.read().clone();
    if base.is_empty() {
        return None;
    }
    let webview = app.get_webview(id)?;
    let page = format!("{base}error.html#{}", crate::permissions::encode(failed_url));
    if let Ok(url) = tauri::Url::parse(&page) {
        let _ = webview.navigate(url);
        crate::logging::info(&format!("error page for {failed_url}"));
        return Some(failed_url.to_string());
    }
    None
}
fn move_bookmark(bookmarks: &mut Vec<Bookmark>, from: &str, to: usize) -> bool {
    let Some(index) = bookmarks.iter().position(|bookmark| bookmark.url == from) else {
        return false;
    };
    let target = to.min(bookmarks.len().saturating_sub(1));
    if index == target {
        return false;
    }
    let item = bookmarks.remove(index);
    bookmarks.insert(target, item);
    true
}

/// Renames a bookmark, and optionally re-points it at another url.
/// Moves a bookmark into a folder; an empty name means the top level.
#[tauri::command]
pub async fn bookmark_set_folder(
    app: AppHandle,
    _window: tauri::Window,
    state: State<'_, AppState>,
    url: String,
    folder: String,
) -> Result<bool, AppError> {
    let changed = {
        let mut list = state.bookmarks.write().unwrap_or_else(|e| e.into_inner());
        let Some(item) = list.iter_mut().find(|item| item.url == url) else {
            return Err(AppError::new("not_found", "no such bookmark"));
        };
        let next = folder.trim().to_string();
        if item.folder == next {
            false
        } else {
            item.folder = next;
            true
        }
    };
    if changed {
        let snapshot = state.bookmarks.read().unwrap_or_else(|e| e.into_inner()).clone();
        crate::library::save_bookmarks(&state.dirs.bookmarks_file, &snapshot)?;
        let _ = app.emit("bookmarks:changed", false);
    }
    Ok(changed)
}
#[tauri::command]
pub async fn bookmark_update(
    app: AppHandle,
    _window: tauri::Window,
    state: State<'_, AppState>,
    url: String,
    title: String,
    new_url: Option<String>,
) -> Result<bool, AppError> {
    let changed = {
        let mut list = state.bookmarks.write().unwrap_or_else(|e| e.into_inner());
        let Some(item) = list.iter_mut().find(|item| item.url == url) else {
            return Err(AppError::new("not_found", "no such bookmark"));
        };
        let trimmed = title.trim().to_string();
        let mut changed = false;
        if !trimmed.is_empty() && item.title != trimmed {
            item.title = trimmed;
            changed = true;
        }
        if let Some(next) = new_url {
            let next = next.trim().to_string();
            if !next.is_empty() && item.url != next {
                item.url = next;
                changed = true;
            }
        }
        changed
    };

    if changed {
        let snapshot = state.bookmarks.read().unwrap_or_else(|e| e.into_inner()).clone();
        crate::library::save_bookmarks(&state.dirs.bookmarks_file, &snapshot)?;
        let _ = app.emit("bookmarks:changed", false);
    }
    Ok(changed)
}
#[tauri::command]
pub async fn bookmark_remove(
    app: AppHandle,
    _window: tauri::Window, state: State<'_, AppState>,
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

/// Writes the current theme next to the settings so it can be shared.
#[tauri::command]
pub async fn theme_export(app: AppHandle, _window: tauri::Window) -> Result<String, AppError> {
    let state = app.state::<AppState>();
    let theme = state.theme.read().clone();
    let path = state.dirs.user_data.join("theme-export.json");
    theme.save(&path)?;
    crate::logging::info(&format!("theme exported to {}", path.display()));
    Ok(path.display().to_string())
}

/// Applies a pasted theme. Invalid JSON is rejected and the old theme stays.
#[tauri::command]
pub async fn theme_import(
    app: AppHandle,
    window: tauri::Window,
    state: State<'_, AppState>,
    json: String,
) -> Result<Theme, AppError> {
    let parsed: Theme = serde_json::from_str(&json)
        .map_err(|err| AppError::new("bad_theme", format!("not valid theme json: {err}")))?;
    let theme = parsed.sanitized();

    *state.theme.write() = theme.clone();
    *state.baseline.write() = theme.clone();
    theme.save(&state.dirs.theme_file)?;
    broadcast_theme(&app, &theme);
    let _ = window.emit("theme:changed", &theme);
    crate::logging::info("theme imported");
    Ok(theme)
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

/// Restarts so extension changes are picked up: WebView2 loads them at startup.
#[tauri::command]
pub async fn app_restart(app: AppHandle, _window: tauri::Window) -> Result<(), AppError> {
    let exe = std::env::current_exe().map_err(|err| AppError::new("restart", err.to_string()))?;
    let mut command = std::process::Command::new(exe);
    if let Ok(dir) = std::env::current_dir() {
        command.current_dir(dir);
    }
    command.spawn().map_err(|err| AppError::new("restart", err.to_string()))?;
    app.exit(0);
    Ok(())
}

/// Quits for real: the window close button only hides to the tray.
#[tauri::command]
pub async fn app_quit(app: AppHandle, _window: tauri::Window) -> Result<(), AppError> {
    save_sessions(&app);
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

/// Userscripts are files, so disabling means renaming the suffix: the scanner
/// only picks up names ending in ".user.js".
#[tauri::command]
pub async fn script_set_enabled(
    _window: tauri::Window,
    path: String,
    enabled: bool,
) -> Result<String, AppError> {
    let source = std::path::PathBuf::from(&path);
    let name = source
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default()
        .to_string();
    let target = if enabled {
        if name.ends_with(".user.js") {
            source.clone()
        } else if name.ends_with(".user.js.disabled") {
            source.with_file_name(name.trim_end_matches(".disabled"))
        } else {
            return Err(AppError::new("bad_script", "unexpected file name"));
        }
    } else if name.ends_with(".user.js.disabled") {
        source.clone()
    } else if name.ends_with(".user.js") {
        source.with_file_name(format!("{name}.disabled"))
    } else {
        return Err(AppError::new("bad_script", "unexpected file name"));
    };

    if target != source {
        std::fs::rename(&source, &target).map_err(|err| AppError::new("script", err.to_string()))?;
    }
    crate::logging::info(&format!("userscript {} -> {}", source.display(), target.display()));
    Ok(target.display().to_string())
}

#[tauri::command]
pub async fn script_remove(_window: tauri::Window, path: String) -> Result<(), AppError> {
    std::fs::remove_file(&path).map_err(|err| AppError::new("script", err.to_string()))?;
    crate::logging::info(&format!("userscript removed: {path}"));
    Ok(())
}

#[tauri::command]
pub async fn script_list(_window: tauri::Window, state: State<'_, AppState>) -> Result<Vec<crate::userscripts::ScriptInfo>, AppError> {
    Ok(state.scripts.clone())
}

/// Opens another window with its own tab strip.
/// Reopens the most recently closed tab.
/// Pages report their favicon here so the tab strip can show it.
fn move_tab(tabs: &mut Vec<Tab>, id: &str, to: usize) -> bool {
    let Some(from) = tabs.iter().position(|tab| tab.id == id) else {
        return false;
    };
    let target = to.min(tabs.len().saturating_sub(1));
    if from == target {
        return false;
    }
    let tab = tabs.remove(from);
    tabs.insert(target, tab);
    true
}

/// Moves a tab to a new index; the strip calls this after a drag.
/// Pages forward typed characters here while the address bar owns the keyboard,
/// which avoids depending on webview focus.
#[tauri::command]
pub async fn url_type(
    app: AppHandle,
    window: tauri::Window,
    state: State<'_, AppState>,
    text: String,
) -> Result<(), AppError> {
    let next = {
        let mut edit = state.url_edit.write().unwrap_or_else(|e| e.into_inner());
        let Some(buffer) = edit.get_mut(window.label()) else {
            return Ok(());
        };
        if text == "\u{8}" {
            buffer.pop();
        } else {
            buffer.push_str(&text);
        }
        buffer.clone()
    };
    crate::logging::info(&format!("url_type -> {next:?}"));
    let _ = app.emit_to(window.label(), "url:text", next);
    Ok(())
}

#[tauri::command]
pub async fn url_key(
    app: AppHandle,
    window: tauri::Window,
    _state: State<'_, AppState>,
    key: String,
) -> Result<(), AppError> {
    match key.as_str() {
        "enter" => {
            exit_url_edit(&app, window.label());
            let _ = app.emit_to(window.label(), "url:commit", ());
        }
        "escape" => {
            exit_url_edit(&app, window.label());
            let _ = app.emit_to(window.label(), "url:cancel", ());
        }
        _ => {}
    }
    Ok(())
}

#[tauri::command]
pub async fn url_blur(app: AppHandle, window: tauri::Window) -> Result<(), AppError> {
    exit_url_edit(&app, window.label());
    Ok(())
}

/// Turns the page-side key bridge on or off for every tab in a window.
pub(crate) fn set_url_edit(app: &AppHandle, window: &tauri::Window, on: bool) {
    let state = app.state::<AppState>();
    let tabs: Vec<String> = match state.try_read_core(window.label()) {
        Some(core) => core.tabs.iter().map(|tab| tab.id.clone()).collect(),
        None => Vec::new(),
    };
    let script = if on { "window.__mixUrlMode = true" } else { "window.__mixUrlMode = false" };
    let count = tabs.len();
    for id in tabs {
        if let Some(webview) = app.get_webview(&id) {
            let _ = webview.eval(script);
        }
    }
    crate::logging::info(&format!("url edit {on} for {} ({count} tabs)", window.label()));
    if on {
        state
            .url_edit
            .write()
            .unwrap_or_else(|e| e.into_inner())
            .insert(window.label().to_string(), String::new());
    }
}

fn exit_url_edit(app: &AppHandle, window_label: &str) {
    if let Some(state) = app.try_state::<AppState>() {
        state
            .url_edit
            .write()
            .unwrap_or_else(|e| e.into_inner())
            .remove(window_label);
    }
    if let Some(win) = app.get_window(window_label) {
        set_url_edit(app, &win, false);
    }
}

/// Pages report their scroll offset here so it survives a tab switch.
#[tauri::command]
pub async fn tab_scroll(
    _app: AppHandle,
    webview: tauri::Webview,
    state: State<'_, AppState>,
    y: f64,
) -> Result<(), AppError> {
    let id = webview.label().to_string();

    let owner = window_of_tab(&id).to_string();
    if let Some(mut core) = state.try_write_core(&owner) {
        if let Some(tab) = core.tabs.iter_mut().find(|tab| tab.id == id) {
            tab.scroll = y.max(0.0);
        }
    }
    Ok(())
}

/// Opens an overlay at a point the caller measured, used by the menu bar.
#[tauri::command]
pub async fn overlay_open_at(
    app: AppHandle,
    window: tauri::Window,
    kind: String,
    x: f64,
    y: f64,
) -> Result<(), AppError> {
    // Height per group, so a short menu does not open a tall empty panel.
    let menu_h = match kind.split("group=").nth(1).unwrap_or("file") {
        "file" => 322.0,
        "edit" => 128.0,
        "view" => 150.0,
        "history" => 96.0,
        "bookmarks" => 96.0,
        "tools" => 254.0,
        _ => 96.0,
    };
    let (menu_w, menu_h) = (232.0, menu_h);
    let _ = crate::overlay::open_at(&app, &window, &kind, x, y, menu_w, menu_h);
    Ok(())
}

/// Fullscreen video only fills the page webview; the strip has to get out of the
/// way, otherwise the player keeps the window frame around it.
#[tauri::command]
pub async fn page_fullscreen(
    app: AppHandle,
    webview: tauri::Webview,
    state: State<'_, AppState>,
    on: bool,
) -> Result<(), AppError> {
    let owner = window_of_tab(webview.label()).to_string();
    {
        let mut set = state.fullscreen.write();
        if on {
            set.insert(owner.clone());
        } else {
            set.remove(&owner);
        }
    }
    let _ = crate::window::relayout(&app, &owner);
    Ok(())
}

/// Gives the keyboard to the page once the address bar is done.
/// The chrome strip reports how tall it actually is, so the page never
/// starts inside an empty band caused by DPI rounding.
#[tauri::command]
pub async fn chrome_fit(app: AppHandle, window: tauri::Window, state: State<'_, AppState>, height: f64) -> Result<(), AppError> {
    let wanted = height.clamp(24.0, 160.0);
    {
        let mut settings = state.settings.write();
        if (settings.chrome_height - wanted).abs() < 1.0 {
            return Ok(());
        }
        settings.chrome_height = wanted;
    }
    crate::logging::info(&format!("chrome fit: {wanted}"));
    let label = window_of(&window).label().to_string();
    crate::window::relayout(&app, &label)?;
    Ok(())
}
#[tauri::command]
pub async fn focus_page(
    app: AppHandle,
    window: tauri::Window,
    state: State<'_, AppState>,
) -> Result<(), AppError> {
    let id = selected_label(&state, &window);
    if let Some(webview) = app.get_webview(&id) {
        let _ = webview.set_focus();
    }
    Ok(())
}

/// Starts the in-page translator. The page collects its own text and calls back;
/// eval cannot return a value, so the flow is push based.
#[tauri::command]
pub async fn translate_page_inplace(
    app: AppHandle,
    window: tauri::Window,
    state: State<'_, AppState>,
) -> Result<(), AppError> {
    *state.translate_result.write() = (0, None);
    let id = selected_label(&state, &window);
    let Some(webview) = app.get_webview(&id) else {
        return Ok(());
    };
    let _ = webview.eval("window.__mixStartTranslate && window.__mixStartTranslate()");
    Ok(())
}

/// Asked by the bubble to show progress without depending on events.
#[tauri::command]
pub async fn translate_status(
    state: State<'_, AppState>,
) -> Result<(usize, Option<String>), AppError> {
    Ok(state.translate_result.read().clone())
}

/// Puts the original text back.
#[tauri::command]
pub async fn translate_restore(
    app: AppHandle,
    window: tauri::Window,
    state: State<'_, AppState>,
) -> Result<(), AppError> {
    let id = selected_label(&state, &window);
    if let Some(webview) = app.get_webview(&id) {
        let _ = webview.eval("window.__mixStopTranslate && window.__mixStopTranslate()");
    }
    state.translated_tabs.write().remove(&id);
    Ok(())
}

/// Whether the selected tab currently shows translated text.
#[tauri::command]
pub async fn translate_state(
    window: tauri::Window,
    state: State<'_, AppState>,
) -> Result<bool, AppError> {
    let id = selected_label(&state, &window);
    Ok(state.translated_tabs.read().contains(&id))
}

/// The page reports how many nodes it replaced, and Chrome shows a hint.
#[tauri::command]
pub async fn translate_done(
    app: AppHandle,
    webview: tauri::Webview,
    count: usize,
    error: Option<String>,
) -> Result<(), AppError> {
    let tab = webview.label().to_string();
    if let Some(state) = app.try_state::<AppState>() {
        *state.translate_result.write() = (count, error.clone());
        let mut tabs = state.translated_tabs.write();
        if error.is_none() && count > 0 {
            tabs.insert(tab.clone());
        } else if error.is_some() {
            tabs.remove(&tab);
        }
    }
    let owner = window_of_tab(webview.label()).to_string();
    if let Some(message) = &error {
        crate::logging::warn(&format!("translation failed: {message}"));
        // Child webviews drop events, so the hint is pushed in directly.
        if let Some(chrome) = app.get_webview(&format!("{owner}-chrome")) {
            let text = serde_json::to_string(message).unwrap_or_else(|_| "\"failed\"".into());
            let _ = chrome.eval(&format!(
                "window.__mixTranslateFailed && window.__mixTranslateFailed({text})"
            ));
        }
    } else {
        crate::logging::info(&format!("translated {count} text nodes"));
    }
    let _ = app.emit_to(owner, "translate:done", (count, error));
    Ok(())
}
/// Translates a batch of page strings through the Microsoft engine Edge uses.
#[tauri::command]
pub async fn translate_texts(
    _app: AppHandle,
    _window: tauri::Window,
    state: State<'_, AppState>,
    texts: Vec<String>,
    to: Option<String>,
) -> Result<Vec<String>, AppError> {
    let target = to.unwrap_or_else(|| {
        let saved = state.settings.read().translate_to.clone();
        match saved.as_str() {
            "" => "zh-Hans".to_string(),
            "zh" => "zh-Hans".to_string(),
            other => other.to_string(),
        }
    });

    let trimmed: Vec<String> = texts
        .into_iter()
        .map(|text| text.trim().to_string())
        .collect();
    let started = std::time::Instant::now();
    let out = crate::translate::translate(trimmed, &target).await;
    let done = out.iter().filter(|text| !text.is_empty()).count();
    crate::logging::info(&format!(
        "translated {done}/{} blocks to {target} in {} ms",
        out.len(),
        started.elapsed().as_millis()
    ));
    Ok(out)
}
/// Page translation goes through a web proxy: WebView2 has no translate API.
#[tauri::command]
pub async fn translate_page(
    app: AppHandle,
    window: tauri::Window,
    state: State<'_, AppState>,
    target: Option<String>,
) -> Result<String, AppError> {
    let text = match target {
        Some(value) => value,
        None => {
            let id = selected_label(&state, &window);
            state
                .try_read_core(window.label())
                .and_then(|core| core.tabs.iter().find(|tab| tab.id == id).map(|tab| tab.url.clone()))
                .unwrap_or_default()
        }
    };
    let text = text.trim().to_string();
    if text.is_empty() {
        return Err(AppError::new("bad_url", "nothing to translate"));
    }

    let encoded = crate::permissions::encode(&text);
    let proxy = if text.starts_with("http://") || text.starts_with("https://") {
        // Whole page: Baidu renders the translated document in place.
        format!(
            "https://fanyi.baidu.com/transpage?query={encoded}&source=url&ie=utf8&from=auto&to=zh&render=1"
        )
    } else {
        // Plain text: the translator page with the text prefilled.
        format!("https://fanyi.baidu.com/mtpe-individual/multimodal?query={encoded}&lang=auto2zh")
    };

    let page = tab_new(app, window, state, Some(proxy.clone()), None, None).await?;
    crate::logging::info(&format!("translate {text} as {page}"));
    Ok(proxy)
}
#[tauri::command]
pub async fn download_url(
    app: AppHandle,
    state: State<'_, AppState>,
    url: String,
    file_name: Option<String>,
) -> Result<String, AppError> {
    if !url.starts_with("http://") && !url.starts_with("https://") {
        return Err(AppError::new("bad_url", "only http(s) files can be saved"));
    }

    let response = reqwest::get(&url)
        .await
        .map_err(|err| AppError::new("net", err.to_string()))?;
    if !response.status().is_success() {
        return Err(AppError::new(
            "net",
            format!("server answered {}", response.status()),
        ));
    }

    let bytes = response
        .bytes()
        .await
        .map_err(|err| AppError::new("net", err.to_string()))?;
    // 128 MB is plenty for an image and keeps a stray click from filling the disk.
    if bytes.len() > 128 * 1024 * 1024 {
        return Err(AppError::new("too_big", "file is larger than 128 MB"));
    }

    let suggested = file_name.unwrap_or_default();
    let target = crate::downloads::target_for(&state.dirs.downloads, &url, &suggested);
    std::fs::write(&target, &bytes)?;
    crate::downloads::record_done(&app, &url, &target);
    crate::logging::info(&format!("saved {}", target.display()));
    Ok(target.display().to_string())
}

/// Drives the browser's own find through the page: there is no API to open the
/// WebView2 find bar, so we keep our own box and call window.find.
#[tauri::command]
pub async fn find_in_page(
    app: AppHandle,
    window: tauri::Window,
    state: State<'_, AppState>,
    text: String,
    forward: Option<bool>,
    restart: Option<bool>,
) -> Result<(), AppError> {
    let id = selected_label(&state, &window);
    let Some(webview) = app.get_webview(&id) else {
        return Ok(());
    };
    // JSON encoded so quotes or backslashes in the query cannot break the script.
    let query = serde_json::to_string(&text).unwrap_or_else(|_| "\"\"".into());
    let back = forward.map(|value| !value).unwrap_or(false);
    let again = restart.unwrap_or(false);
    let script = format!(
        "(function(){{var hit=window.find({query}, false, {back}, true, {again}, false, false);\
window.__TAURI_INTERNALS__.invoke('find_result', {{ hit: Boolean(hit) }}).catch(function(){{}});}})()"
    );
    let _ = webview.eval(&script);
    Ok(())
}

/// The page reports the outcome of a find, because eval cannot return a value.
#[tauri::command]
pub async fn find_result(state: State<'_, AppState>, hit: bool) -> Result<(), AppError> {
    *state.find_hit.write() = hit;
    Ok(())
}

#[tauri::command]
pub async fn find_status(state: State<'_, AppState>) -> Result<bool, AppError> {
    Ok(*state.find_hit.read())
}


#[tauri::command]
pub async fn page_failed(
    app: AppHandle,
    webview: tauri::Webview,
    state: State<'_, AppState>,
    code: String,
) -> Result<(), AppError> {
    let id = webview.label().to_string();
    let failed = state
        .pending
        .read()
        .unwrap_or_else(|e| e.into_inner())
        .get(&id)
        .cloned()
        .unwrap_or_default();
    if failed.is_empty() {
        return Ok(());
    }
    crate::logging::info(&format!("page failed: {failed} ({code})"));
    show_error_page(&app, &id, &failed);
    Ok(())
}

#[tauri::command]
pub async fn tab_move(
    app: AppHandle,
    window: tauri::Window,
    state: State<'_, AppState>,
    id: String,
    to: usize,
) -> Result<(), AppError> {
    {
        let mut core = state.write_core(window.label());
        move_tab(&mut core.tabs, &id, to);
    }
    emit_tabs(&app, window.label());
    save_sessions(&app);
    Ok(())
}

#[tauri::command]
pub async fn tab_icon(
    app: AppHandle,
    webview: tauri::Webview,
    state: State<'_, AppState>,
    url: String,
) -> Result<(), AppError> {
    let id = webview.label().to_string();
    let icon = url.trim().to_string();
    if icon.is_empty() || !icon.starts_with("http") {
        return Ok(());
    }

    let page_url = state
        .try_read_core(&window_of_tab(&id))
        .and_then(|core| core.tabs.iter().find(|tab| tab.id == id).map(|tab| tab.url.clone()))
        .unwrap_or_default();
    if !page_url.is_empty() {
        attach_icon(&app, &page_url, &icon);
        let host = crate::favicons::host_of(&page_url);
        let changed = {
            let state = app.try_state::<AppState>();
            match state {
                Some(state) => state
                    .favicons
                    .write()
                    .unwrap_or_else(|e| e.into_inner())
                    .set(&host, &icon),
                None => false,
            }
        };
        if changed {
            if let Some(state) = app.try_state::<AppState>() {
                state
                    .favicons
                    .read()
                    .unwrap_or_else(|e| e.into_inner())
                    .save(&state.dirs.favicons_file);
            }
        }
        // Outside the branch: the map may already know this host but the file
        // cache can still be missing (fresh profile, deleted folder).
        cache_icon(&app, &host, &icon);
    }

    let owner = window_of_tab(&id).to_string();
    if let Some(mut core) = state.try_write_core(&owner) {
        if let Some(tab) = core.tabs.iter_mut().find(|tab| tab.id == id) {
            tab.icon = Some(icon.clone());
        }
    }
    let _ = app.emit_to(owner, "tab:icon", IconPayload { id, icon: Some(icon) });
    Ok(())
}
#[tauri::command]
pub async fn tab_reopen(
    app: AppHandle,
    window: tauri::Window,
    state: State<'_, AppState>,
) -> Result<bool, AppError> {
    let Some((url, _title)) = state
        .closed_tabs
        .write()
        .unwrap_or_else(|e| e.into_inner())
        .pop()
    else {
        return Ok(false);
    };
    tab_new(app, window, state, Some(url), None, None).await?;
    Ok(true)
}

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
    crate::window::open_window(&app, &label, vec![(url, 0.0, false)], 0, None)?;
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

/// Opens the overlay, or closes it when it is already open for this window.
#[tauri::command]
pub async fn overlay_toggle(
    app: AppHandle,
    window: tauri::Window,
    kind: String,
) -> Result<bool, AppError> {
    let open = app
        .state::<AppState>()
        .overlay
        .read()
        .unwrap_or_else(|e| e.into_inner())
        .contains_key(window.label());
    if open {
        crate::overlay::close(&app, window.label());
        return Ok(false);
    }
    crate::overlay::open(&app, &window, &kind).map_err(|e| AppError::new("overlay", e))?;
    Ok(true)
}

#[tauri::command]
pub async fn overlay_close(app: AppHandle, window: tauri::Window) -> Result<(), AppError> {
    crate::overlay::close(&app, window.label());
    Ok(())
}




/// Creates the user stylesheet if missing, then reveals the folder.
#[tauri::command]
pub async fn custom_css_open(_window: tauri::Window, state: State<'_, AppState>) -> Result<String, AppError> {
    let path = state.dirs.user_data.join("custom.css");
    if !path.exists() {
        let starter = "/* Styles here are applied to every page. */\n\n/*\n.example {\n  outline: 1px solid #4c8dff;\n}\n*/\n";
        std::fs::write(&path, starter).map_err(|err| AppError::new("css", err.to_string()))?;
    }
    reveal_path(&path)?;
    Ok(path.display().to_string())
}
#[tauri::command]
pub async fn open_dir(_window: tauri::Window, state: State<'_, AppState>, which: String) -> Result<(), AppError> {
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
#[cfg(test)]
mod tab_order_tests {
    use super::*;

    fn tab(id: &str) -> Tab {
        Tab {
            id: id.to_string(),
            url: "https://a.test".into(),
            title: id.to_string(),
            zoom: 1.0,
            pinned: false,
            incognito: false,
            icon: None,
            loading: false,
            scroll: 0.0,
            alive: true,
            last_active: 0,
        }
    }

    fn ids(tabs: &[Tab]) -> Vec<&str> {
        tabs.iter().map(|tab| tab.id.as_str()).collect()
    }

    #[test]
    fn reordering_moves_a_tab() {
        let mut tabs = vec![tab("a"), tab("b"), tab("c")];
        assert!(move_tab(&mut tabs, "c", 0));
        assert_eq!(ids(&tabs), vec!["c", "a", "b"]);
    }

    #[test]
    fn moving_past_the_end_lands_last() {
        let mut tabs = vec![tab("a"), tab("b"), tab("c")];
        assert!(move_tab(&mut tabs, "a", 99));
        assert_eq!(ids(&tabs), vec!["b", "c", "a"]);
    }

    #[test]
    fn unknown_id_or_same_index_is_a_no_op() {
        let mut tabs = vec![tab("a"), tab("b")];
        assert!(!move_tab(&mut tabs, "zzz", 0));
        assert!(!move_tab(&mut tabs, "a", 0));
        assert_eq!(ids(&tabs), vec!["a", "b"]);
    }
}

#[cfg(test)]
mod recency_tests {
    use super::*;

    fn list(ids: &[&str]) -> Vec<String> {
        ids.iter().map(|id| id.to_string()).collect()
    }

    #[test]
    fn recency_jumps_to_the_last_used_tab() {
        let mru = list(&["c", "a", "b"]);
        assert_eq!(next_by_recency(&mru, 1), Some("a".to_string()));
        assert_eq!(next_by_recency(&mru, -1), Some("b".to_string()));
    }

    #[test]
    fn a_single_tab_has_nowhere_to_go() {
        assert_eq!(next_by_recency(&list(&["a"]), 1), None);
    }

    #[test]
    fn touching_moves_an_id_to_the_front() {
        let mut mru = list(&["a", "b", "c"]);
        touch_mru(&mut mru, "c");
        assert_eq!(mru, list(&["c", "a", "b"]));
        touch_mru(&mut mru, "d");
        assert_eq!(mru, list(&["d", "c", "a", "b"]));
    }
}
