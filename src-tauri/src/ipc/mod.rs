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
    /// Window label -> whether its overlay belongs to the page rather than the
    /// chrome strip, so closing it can hand the keyboard back to the right one.
    pub overlay_from_page: RwLock<std::collections::HashMap<String, bool>>,
    pub permissions: crate::permissions::PermissionStore,
    pub snap: std::sync::Mutex<crate::snap::SnapState>,
    pub opts: crate::tabs::WebviewOpts,
    pub extensions: Vec<ExtInfo>,
    pub history: RwLock<Vec<Visit>>,
    /// host -> favicon url, shared by history, bookmarks and the new tab page.
    pub favicons: RwLock<crate::favicons::Favicons>,
    /// Window label -> what is typed in that window's address bar; two windows
    /// must not share a suggestion query.
    pub suggest_text: parking_lot::RwLock<std::collections::HashMap<String, String>>,
    pub suggest_width: parking_lot::RwLock<f64>,
    /// Recently closed tabs per window, newest last, so Ctrl+Shift+T brings
    /// back one of this window's tabs and never one closed in another.
    pub closed_tabs: RwLock<std::collections::HashMap<String, Vec<(String, String)>>>,
    pub zoom: parking_lot::RwLock<crate::zoom::ZoomStore>,
    /// Windows whose address bar currently owns the keyboard; the entry carries
    /// an empty string and exists only as a flag.
    pub url_edit: RwLock<std::collections::HashMap<String, String>>,
    /// Tab id -> the url it is currently trying to load.
    /// Origin of the bundled pages, e.g. http://tauri.localhost/
    pub app_base: parking_lot::RwLock<String>,
    pub pending: RwLock<std::collections::HashMap<String, String>>,
    /// Cached "can we reach this host" answers, so navigation is not slowed down.
    /// Shared through an Arc so the probe can run on a blocking thread.
    pub reachability: std::sync::Arc<crate::net::Reachability>,
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
            suggest_text: parking_lot::RwLock::new(std::collections::HashMap::new()),
            suggest_width: parking_lot::RwLock::new(420.0),
            closed_tabs: RwLock::new(std::collections::HashMap::new()),
            zoom: parking_lot::RwLock::new(crate::zoom::ZoomStore::load(&dirs.zoom_file)),
            url_edit: RwLock::new(std::collections::HashMap::new()),
            app_base: parking_lot::RwLock::new(String::new()),
            pending: RwLock::new(std::collections::HashMap::new()),
            reachability: std::sync::Arc::new(crate::net::Reachability::default()),
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
            overlay_from_page: RwLock::new(std::collections::HashMap::new()),
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
    pub translate_to: Option<String>,
    pub new_tab_page: Option<bool>,
    pub search_engine: Option<String>,
    pub shortcuts: Option<std::collections::BTreeMap<String, String>>,
    pub custom_search: Option<String>,
    pub gestures: Option<bool>,
    pub super_drag: Option<bool>,
    pub memory_saver: Option<bool>,
    pub keep_alive_hosts: Option<String>,
    pub mark_suspended: Option<bool>,
    pub drag_hint: Option<bool>,
    pub drag_threshold: Option<u32>,
    pub drag_map: Option<std::collections::BTreeMap<String, String>>,
    pub gesture_trail: Option<bool>,
    pub gesture_color: Option<String>,
    pub gesture_width: Option<u32>,
    pub gesture_threshold: Option<u32>,
    pub gesture_map: Option<std::collections::BTreeMap<String, String>>,
    pub gesture_blocked: Option<String>,
    pub theme_mode: Option<String>,
    pub font_family: Option<String>,
    pub font_mono: Option<String>,
    pub font_size: Option<u32>,
    pub font_min_size: Option<u32>,
    pub home_url: Option<String>,
    pub default_zoom: Option<f64>,
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

/// Pages the app serves itself: the chrome strip, the overlay panels and the
/// bundled html pages. Remote sites never match, which is what keeps privileged
/// commands away from page content even if a capability is widened by mistake.
pub(crate) fn trusted_url(url: &str) -> bool {
    let trimmed = url.trim();
    if trimmed.starts_with("about:blank") || trimmed.starts_with("about:srcdoc") {
        return true;
    }
    let Some((scheme, rest)) = trimmed.split_once("://") else {
        return false;
    };
    // macOS and Linux serve the bundle through a custom scheme; Windows uses
    // http://tauri.localhost through the asset protocol.
    let scheme = scheme.to_ascii_lowercase();
    if matches!(scheme.as_str(), "tauri" | "asset" | "ipc") {
        return true;
    }
    let authority = rest.split(['/', '?', '#']).next().unwrap_or_default();
    let authority = authority
        .rsplit_once('@')
        .map(|(_, tail)| tail)
        .unwrap_or(authority);
    let host = authority.split(':').next().unwrap_or_default().to_ascii_lowercase();
    matches!(host.as_str(), "tauri.localhost" | "ipc.localhost")
}

/// The caller's own address decides whether it may run a privileged command.
pub(crate) fn trusted(webview: &tauri::Webview) -> bool {
    webview
        .url()
        .map(|url| trusted_url(url.as_str()))
        .unwrap_or(false)
}

fn untrusted() -> AppError {
    AppError::new("untrusted", "only app pages may run this command")
}

/// Rejects a caller that is not an app page. Used by every command that can
/// destroy data, spawn a webview or reach the clipboard.
pub(crate) fn require_trusted(webview: &tauri::Webview) -> Result<(), AppError> {
    if trusted(webview) {
        Ok(())
    } else {
        crate::logging::warn(&format!(
            "denied privileged command for {}",
            webview.label()
        ));
        Err(untrusted())
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

/// Height of the chrome strip: the base plus the optional bookmarks row.
pub fn chrome_height(app: &AppHandle) -> f64 {
    match app.try_state::<AppState>() {
        Some(state) => {
            let show_bookmarks = state.settings.read().show_bookmarks;
            // The strip is laid out from the constant, not from what the page
            // measures: the address row flexes to fill, so a measurement can only
            // echo back whatever height it was given and would lock in any wrong
            // value instead of correcting it.
            crate::window::CHROME_H + if show_bookmarks { 30.0 } else { 0.0 }
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
    if !app
        .try_state::<AppState>()
        .map(|state| state.settings.read().memory_saver)
        .unwrap_or(false)
    {
        return;
    }
    let keep_alive: Vec<String> = app
        .try_state::<AppState>()
        .map(|state| {
            state
                .settings
                .read()
                .keep_alive_hosts
                .split([' ', ',', ';'])
                .map(|entry| entry.trim().trim_start_matches("www.").to_ascii_lowercase())
                .filter(|entry| !entry.is_empty())
                .collect()
        })
        .unwrap_or_default();

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
                    .filter(|tab| {
                        if keep_alive.is_empty() {
                            return true;
                        }
                        let host = tab
                            .url
                            .split("://")
                            .nth(1)
                            .unwrap_or("")
                            .split(['/', '?', '#'])
                            .next()
                            .unwrap_or("")
                            .trim_start_matches("www.")
                            .to_ascii_lowercase();
                        !keep_alive
                            .iter()
                            .any(|entry| host == *entry || host.ends_with(&format!(".{entry}")))
                    })
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
    crate::tabs::apply_default_zoom(&app, &id);

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
    app: AppHandle,
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
        // Pinned tabs sit on the left, like every other browser; the strip is
        // drawn from this order, so it has to be applied here.
        let mut pinned_tabs: Vec<Tab> = Vec::new();
        let mut loose: Vec<Tab> = Vec::new();
        for tab in core.tabs.drain(..) {
            if tab.pinned {
                pinned_tabs.push(tab);
            } else {
                loose.push(tab);
            }
        }
        core.tabs.extend(pinned_tabs);
        core.tabs.extend(loose);
    }
    emit_tabs(&app, window.label());
    save_sessions(&app);
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
                let list = history.entry(window.label().to_string()).or_default();
                list.push((closed.url, closed.title));
                if list.len() > 20 {
                    list.remove(0);
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

/// Saves a png of the visible page into the downloads folder.
#[tauri::command]
pub async fn tab_screenshot(
    app: AppHandle,
    window: tauri::Window,
    state: State<'_, AppState>,
) -> Result<String, AppError> {
    let id = selected_label(&state, &window);
    let Some(webview) = app.get_webview(&id) else {
        return Err(AppError::new("no_tab", "there is no tab to capture"));
    };
    let Some(bytes) = crate::screenshot::capture(&webview) else {
        return Err(AppError::new("capture", "the page did not answer with an image"));
    };
    let name = format!("Mix-shot-{}.png", crate::library::now());
    let target = crate::downloads::target_for(&state.dirs.downloads, "mix://screenshot", &name);
    crate::paths::write_atomic(&target, &bytes)?;
    crate::downloads::record_done(&app, "mix://screenshot", &target);
    crate::logging::info(&format!("screenshot {}", target.display()));
    Ok(target.display().to_string())
}

/// Mutes or unmutes the selected tab; with no argument it flips. The mute
/// button in the toolbar drives this.
#[tauri::command]
pub async fn tab_mute(
    app: AppHandle,
    window: tauri::Window,
    state: State<'_, AppState>,
    muted: Option<bool>,
) -> Result<bool, AppError> {
    let id = selected_label(&state, &window);
    if id.is_empty() {
        return Ok(false);
    }
    let Some(webview) = app.get_webview(&id) else {
        return Ok(false);
    };
    let current = crate::audio::state(&webview).unwrap_or(crate::audio::Audio {
        playing: false,
        muted: false,
    });
    let target = muted.unwrap_or(!current.muted);
    let ok = crate::audio::set_muted(&webview, target);
    let now = crate::audio::state(&webview).unwrap_or(crate::audio::Audio {
        playing: current.playing,
        muted: target,
    });
    crate::logging::info(&format!("tab {id} muted={} ({ok})", now.muted));
    let _ = app.emit_to(
        window.label(),
        "tab:audio",
        serde_json::json!({ "id": id, "playing": now.playing, "muted": now.muted }),
    );
    Ok(now.muted)
}

#[tauri::command]
pub async fn tab_navigate(app: AppHandle, window: tauri::Window, state: State<'_, AppState>, url: String) -> Result<(), AppError> {
    // A dead host would land on WebView2's own error page, which we cannot
    // replace, so answer it ourselves before navigating. The probe resolves dns
    // and therefore runs on a blocking thread, never on this command's future.
    if url.starts_with("http") && !trusted_url(&url) {
        let reach = state.reachability.clone();
        let probe_url = url.clone();
        let reachable = tauri::async_runtime::spawn_blocking(move || reach.check(&probe_url))
            .await
            .unwrap_or(true);
        if !reachable {
            let id = selected_label(&state, &window);
            if !id.is_empty() {
                crate::logging::info(&format!("unreachable: {url}"));
                show_error_page(&app, &id, url.trim());
            }
            return Ok(());
        }
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
                "open_home" => {
                    let settings = state.settings.read();
                    if settings.home_url.trim().is_empty() {
                        settings.start_url.clone()
                    } else {
                        settings.home_url.clone()
                    }
                }
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
        "palette" => {
            // The palette lives in the chrome page, so it needs the keyboard.
            if let Some(chrome) = app.get_webview(&format!("{}-chrome", window.label())) {
                let _ = chrome.set_focus();
                let _ = app.emit_to(window.label(), "ui:palette", ());
            }
        }
        "find" => {
            crate::overlay::open(&app, &window, "find")
                .map_err(|err| AppError::new("overlay", err))?;
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

/// What the page gets back: an answer, or a prompt id to poll.
#[derive(Serialize)]
pub struct PermissionAsk {
    /// Set when this origin and kind were already decided for the session.
    allow: Option<bool>,
    /// Set when the user has to be asked; collect it with permission_poll.
    id: Option<u64>,
}

/// Asked from the page before a sensitive API is used.
#[tauri::command]
pub async fn permission_request(
    app: AppHandle,
    window: tauri::Window,
    origin: String,
    kind: String,
) -> Result<PermissionAsk, AppError> {
    let key = crate::permissions::key(&origin, &kind);
    if let Some(known) = crate::permissions::remembered(&app, &key) {
        return Ok(PermissionAsk {
            allow: Some(known),
            id: None,
        });
    }

    let id = crate::permissions::open_prompt(&app, window.label(), &key);
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
    // Nothing waits here: the page polls, so this command returns at once and no
    // runtime thread is held while the user makes up their mind.
    Ok(PermissionAsk {
        allow: None,
        id: Some(id),
    })
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

/// The page asks again until the prompt is answered; null means "not yet".
#[tauri::command]
pub async fn permission_poll(app: AppHandle, id: u64) -> Result<Option<bool>, AppError> {
    Ok(crate::permissions::poll(&app, id))
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
pub async fn chrome_fit(
    app: AppHandle,
    _window: tauri::Window,
    state: State<'_, AppState>,
    height: f64,
) -> Result<(), AppError> {
    // Kept as a diagnostic only. The address row has flex: 1, so the page can
    // never report more than the height it was laid out with: adopting it (and
    // relaying out, which fires another resize) shrinks the strip a little on
    // every pass, which is exactly what happens once a stale value gets in.
    let reported = height.clamp(0.0, 400.0);
    state.settings.write().chrome_height = reported;
    let expected = chrome_height(&app);
    if (reported - expected).abs() > 8.0 {
        crate::logging::warn(&format!(
            "chrome strip reported {reported} but is laid out at {expected}; layout kept"
        ));
    }
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

    let (page_url, incognito) = state
        .try_read_core(&window_of_tab(&id))
        .and_then(|core| {
            core.tabs
                .iter()
                .find(|tab| tab.id == id)
                .map(|tab| (tab.url.clone(), tab.incognito))
        })
        .unwrap_or_default();
    // A private tab leaves no trace in history or in the shared icon cache, and
    // the app's own pages have no site icon to cache: their fallback url is
    // /favicon.ico on tauri.localhost, which only ever 404s.
    if !page_url.is_empty() && !incognito && !trusted_url(&page_url) {
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
        .get_mut(window.label())
        .and_then(|list| list.pop())
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




#[cfg(test)]
mod trusted_caller_tests {
    use super::*;

    #[test]
    fn app_pages_are_trusted() {
        // Windows serves the bundle through the asset protocol, macOS and Linux
        // through a custom scheme.
        assert!(trusted_url("http://tauri.localhost/index.html"));
        assert!(trusted_url("http://tauri.localhost/pages/settings.html"));
        assert!(trusted_url("https://tauri.localhost/overlay.html#app_menu"));
        assert!(trusted_url("tauri://localhost/index.html"));
        assert!(trusted_url("about:blank"));
    }

    #[test]
    fn remote_pages_are_not_trusted() {
        assert!(!trusted_url("https://example.com/"));
        assert!(!trusted_url("http://example.com/tauri.localhost"));
        assert!(!trusted_url("https://tauri.localhost.evil.test/"));
        assert!(!trusted_url("https://user@evil.test/?x=http://tauri.localhost"));
        assert!(!trusted_url("data:text/html,<p>hi</p>"));
        assert!(!trusted_url("file:///C:/Windows/System32/drivers/etc/hosts"));
        assert!(!trusted_url(""));
    }
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

mod tools;
pub use tools::*;
mod translate;
pub use translate::*;

mod library;
pub use library::*;
mod nav;
pub use nav::*;

mod settings;
pub use settings::*;
