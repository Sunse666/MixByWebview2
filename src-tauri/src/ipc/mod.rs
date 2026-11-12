//! Shared state for the ipc layer; the commands live in one module per area.

mod chrome;
mod keys;
mod library;
mod nav;
mod settings;
mod tabs;
mod tools;
mod translate;

pub use chrome::*;
pub use keys::*;
pub use library::*;
pub use nav::*;
pub use settings::*;
pub use tabs::*;
pub use tools::*;
pub use translate::*;

use crate::extensions::ExtInfo;
use crate::library::{Bookmark, Visit};
use crate::paths::AppDirs;
use crate::state::Settings;
use crate::tabs::Tab;
use crate::theme::Theme;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_runtime::ResizeDirection;

/// Tabs shown as one group, collapsed to a single slot in the strip.
#[derive(Clone, Debug, Serialize)]
pub struct TabGroup {
    pub id: String,
    pub name: String,
    /// Six digit hex, used for the chip and the stripe on its tabs.
    pub color: String,
    pub collapsed: bool,
    /// Member tab ids, in strip order within the group.
    pub members: Vec<String>,
}

#[derive(Default)]
pub struct Core {
    pub tabs: Vec<Tab>,
    pub selected: Option<String>,
    pub next_tab_id: usize,
    /// Tab ids by recency, most recent first; Ctrl+Tab walks this.
    pub mru: Vec<String>,
    pub groups: Vec<TabGroup>,
    pub next_group_id: usize,
    pub tick: u64,
}

/// Everything the app tracks per window: overlay panel, suggestion query, closed tabs
/// and the rest of the address bar and hover state.
#[derive(Default)]
pub struct WindowMaps {
    /// Overlay webview label per window, so each window has its own menu.
    pub overlay: parking_lot::RwLock<std::collections::HashMap<String, String>>,
    /// Whether each window's overlay came from a page rather than the chrome strip.
    pub overlay_from_page: parking_lot::RwLock<std::collections::HashMap<String, bool>>,
    /// Windows whose overlay is the tab hover card.
    pub overlay_thumb: parking_lot::RwLock<std::collections::HashSet<String>>,
    /// Last time each tab was on screen, in seconds.
    pub shown_at: parking_lot::RwLock<std::collections::HashMap<String, u64>>,
    /// What is typed in each window's address bar, by window label.
    pub suggest_text: parking_lot::RwLock<std::collections::HashMap<String, String>>,
    /// Recently closed tabs per window, oldest first.
    pub closed_tabs: parking_lot::RwLock<std::collections::HashMap<String, Vec<(String, String)>>>,
    /// Windows whose address bar currently has the keyboard.
    pub url_edit: parking_lot::RwLock<std::collections::HashMap<String, String>>,
    /// Windows showing a fullscreen page.
    pub fullscreen: parking_lot::RwLock<std::collections::HashSet<String>>,
    /// The window the user touched last. The session keeps that one, so reopening the
    /// app brings back the window that was in use rather than the one that started.
    pub focused: parking_lot::RwLock<Option<String>>,
}

/// Browsing data that outlives the window: history, bookmarks, site icons and zoom.
pub struct Library {
    pub history: Vec<Visit>,
    /// host -> favicon url, shared by history, bookmarks and the new tab page.
    pub favicons: crate::favicons::Favicons,
    pub bookmarks: Vec<Bookmark>,
    pub zoom: crate::zoom::ZoomStore,
}

impl Default for Library {
    fn default() -> Self {
        Self {
            history: Vec::new(),
            favicons: crate::favicons::Favicons::default(),
            bookmarks: Vec::new(),
            zoom: crate::zoom::ZoomStore::default(),
        }
    }
}

/// What the chrome is showing right now: menu, suggestions, find and translation.
pub struct Ui {
    pub menu_context: MenuContext,
    pub suggest_width: f64,
    /// Result of the last in-page translation.
    pub translate_result: (usize, Option<String>),
    /// Tabs whose text the translator replaced.
    pub translated_tabs: std::collections::HashSet<String>,
    /// Whether the last in-page search matched anything.
    pub find_hit: bool,
}

/// Navigation bookkeeping: what each tab is loading, where the bundled pages live and
/// which hosts answered a reachability probe.
pub struct Nav {
    /// Origin of the bundled pages.
    pub app_base: String,
    /// Tab id -> the url it is currently loading.
    pub pending: std::collections::HashMap<String, String>,
    /// Tab id -> the url whose error page is already showing.
    pub error_shown: std::collections::HashMap<String, String>,
    /// Reachability cache shared with the probe thread.
    pub reachability: std::sync::Arc<crate::net::Reachability>,
}

pub struct AppState {
    pub dirs: AppDirs,
    /// One tab set per window, keyed by the window label.
    pub cores: parking_lot::RwLock<std::collections::HashMap<String, Core>>,
    pub settings: parking_lot::RwLock<Settings>,
    pub theme: parking_lot::RwLock<Theme>,
    pub baseline: parking_lot::RwLock<Theme>,
    pub downloads: parking_lot::RwLock<Vec<crate::downloads::DownloadTask>>,
    /// Overlay, suggestion and session state, one entry per window.
    pub windows: WindowMaps,
    pub library: parking_lot::RwLock<Library>,
    pub ui: parking_lot::RwLock<Ui>,
    pub permissions: crate::permissions::PermissionStore,
    pub snap: parking_lot::Mutex<crate::snap::SnapState>,
    pub opts: crate::tabs::WebviewOpts,
    pub extensions: Vec<ExtInfo>,
    pub nav: parking_lot::RwLock<Nav>,
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
        opts.userscripts = scripts_js.clone();
        let downloads = crate::downloads::load_history(&dirs);
        let history = crate::library::load_visits(&dirs.history_file);
        let bookmarks = crate::library::load_bookmarks(&dirs.bookmarks_file);
        let mut cores = std::collections::HashMap::new();
        cores.insert("w1".to_string(), Core::default());

        Self {
            opts,
            extensions,
            library: parking_lot::RwLock::new(Library {
                history,
                favicons: crate::favicons::Favicons::load(&dirs.favicons_file),
                bookmarks,
                zoom: crate::zoom::ZoomStore::load(&dirs.zoom_file),
            }),
            ui: parking_lot::RwLock::new(Ui {
                menu_context: MenuContext::default(),
                suggest_width: 420.0,
                translate_result: (0, None),
                translated_tabs: std::collections::HashSet::new(),
                find_hit: true,
            }),
            nav: parking_lot::RwLock::new(Nav {
                app_base: String::new(),
                pending: std::collections::HashMap::new(),
                error_shown: std::collections::HashMap::new(),
                reachability: std::sync::Arc::new(crate::net::Reachability::default()),
            }),
            scripts,
            dirs,
            cores: parking_lot::RwLock::new(cores),
            settings: parking_lot::RwLock::new(settings),
            baseline: parking_lot::RwLock::new(theme.clone()),
            theme: parking_lot::RwLock::new(theme),
            downloads: parking_lot::RwLock::new(downloads),
            windows: WindowMaps::default(),
            permissions: crate::permissions::PermissionStore::default(),
            snap: parking_lot::Mutex::new(crate::snap::SnapState::default()),
        }
    }

    pub fn history(&self) -> parking_lot::MappedRwLockReadGuard<'_, Vec<Visit>> {
        parking_lot::RwLockReadGuard::map(self.library.read(), |library| &library.history)
    }

    pub fn history_mut(&self) -> parking_lot::MappedRwLockWriteGuard<'_, Vec<Visit>> {
        parking_lot::RwLockWriteGuard::map(self.library.write(), |library| &mut library.history)
    }

    pub fn bookmarks(&self) -> parking_lot::MappedRwLockReadGuard<'_, Vec<Bookmark>> {
        parking_lot::RwLockReadGuard::map(self.library.read(), |library| &library.bookmarks)
    }

    pub fn bookmarks_mut(&self) -> parking_lot::MappedRwLockWriteGuard<'_, Vec<Bookmark>> {
        parking_lot::RwLockWriteGuard::map(self.library.write(), |library| &mut library.bookmarks)
    }

    pub fn favicons(&self) -> parking_lot::MappedRwLockReadGuard<'_, crate::favicons::Favicons> {
        parking_lot::RwLockReadGuard::map(self.library.read(), |library| &library.favicons)
    }

    pub fn favicons_mut(&self) -> parking_lot::MappedRwLockWriteGuard<'_, crate::favicons::Favicons> {
        parking_lot::RwLockWriteGuard::map(self.library.write(), |library| &mut library.favicons)
    }

    pub fn zoom(&self) -> parking_lot::MappedRwLockReadGuard<'_, crate::zoom::ZoomStore> {
        parking_lot::RwLockReadGuard::map(self.library.read(), |library| &library.zoom)
    }

    pub fn zoom_mut(&self) -> parking_lot::MappedRwLockWriteGuard<'_, crate::zoom::ZoomStore> {
        parking_lot::RwLockWriteGuard::map(self.library.write(), |library| &mut library.zoom)
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

    /// Tab state of one window.
    pub fn try_read_core(&self, label: &str) -> Option<parking_lot::MappedRwLockReadGuard<'_, Core>> {
        parking_lot::RwLockReadGuard::try_map(self.cores.read(), |map| map.get(label)).ok()
    }

    pub fn try_write_core(&self, label: &str) -> Option<parking_lot::MappedRwLockWriteGuard<'_, Core>> {
        parking_lot::RwLockWriteGuard::try_map(self.cores.write(), |map| map.get_mut(label)).ok()
    }

    /// Tab state of one window. A window whose state is gone — a webview callback that
    /// outlived its window — gets an empty one instead of taking the process down.
    pub fn read_core(&self, label: &str) -> parking_lot::MappedRwLockReadGuard<'_, Core> {
        if let Some(core) = self.try_read_core(label) {
            return core;
        }
        let mut guard = self.cores.write();
        guard.entry(label.to_string()).or_insert_with(Core::default);
        crate::logging::warn(&format!("no tab state for window {label}; using an empty one"));
        let read = parking_lot::RwLockWriteGuard::downgrade(guard);
        // The entry cannot vanish here: the lock was never released between the
        // insert and the downgrade.
        parking_lot::RwLockReadGuard::map(read, |map| {
            map.get(label).expect("core inserted under the same lock")
        })
    }

    pub fn write_core(&self, label: &str) -> parking_lot::MappedRwLockWriteGuard<'_, Core> {
        let mut guard = self.cores.write();
        if !guard.contains_key(label) {
            crate::logging::warn(&format!("no tab state for window {label}; using an empty one"));
            guard.insert(label.to_string(), Core::default());
        }
        parking_lot::RwLockWriteGuard::map(guard, |map| {
            map.get_mut(label).expect("core inserted under the same lock")
        })
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct SettingsPatch {
    pub restore_session: Option<bool>,
    pub show_bookmarks: Option<bool>,
    pub proxy_mode: Option<String>,
    pub proxy_server: Option<String>,
    pub sidebar: Option<String>,
    pub sidebar_width: Option<f64>,
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
    pub tab_hover_preview: Option<bool>,
    pub freeze_background: Option<bool>,
    pub live_thumbs: Option<bool>,
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
    /// The tab a menu was opened for.
    pub tab: Option<String>,
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

/// Percent-encodes a search query.
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

/// Stores key combos in one spelling: "ctrl+shift+t".
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

/// Pages the app serves itself; remote sites never match.
pub(crate) fn trusted_url(url: &str) -> bool {
    let trimmed = url.trim();
    if trimmed.starts_with("about:blank") || trimmed.starts_with("about:srcdoc") {
        return true;
    }
    let Some((scheme, rest)) = trimmed.split_once("://") else {
        return false;
    };
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

/// Whether the caller's own address may run a privileged command.
pub(crate) fn trusted(webview: &tauri::Webview) -> bool {
    webview
        .url()
        .map(|url| trusted_url(url.as_str()))
        .unwrap_or(false)
}

fn untrusted() -> AppError {
    AppError::new("untrusted", "only app pages may run this command")
}

/// Rejects a caller that is not an app page.
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


/// Height of the chrome strip: the base plus the optional bookmarks row.
pub fn chrome_height(app: &AppHandle) -> f64 {
    match app.try_state::<AppState>() {
        Some(state) => {
            let show_bookmarks = state.settings.read().show_bookmarks;
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
    /// The window this list belongs to. Events reach every window, so the page has to
    /// check this before drawing a list that is not its own.
    window: String,
    tabs: Vec<TabMeta>,
    selected: Option<String>,
    groups: Vec<TabGroup>,
}

fn snapshot(core: &Core, label: &str) -> TabList {
    TabList {
        window: label.to_string(),
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
        groups: core.groups.clone(),
    }
}

/// Tell the chrome UI to rebuild the tab strip.
pub(crate) fn emit_tabs(app: &AppHandle, label: &str) {
    let state = app.state::<AppState>();
    let list = {
        let core = state.read_core(label);
        snapshot(&core, label)
    };
    let json = serde_json::to_string(&list).unwrap_or_else(|_| "{}".into());
    crate::logging::info(&format!("tabs:changed -> {label} n={}", list.tabs.len()));
    let _ = app.emit_to(label, "tabs:changed", list);
    if let Some(side) = app.get_webview(&format!("{label}-side")) {
        let _ = side.eval(&format!(
            "window.__mixSidebarTabs && window.__mixSidebarTabs({json})"
        ));
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

/// Writes every window to session.json.
pub(crate) fn save_sessions(app: &AppHandle) {
    // A window that runs as its own process must not rewrite the session of the window
    // that spawned it: two writers would keep replacing each other's file.
    if crate::paths::instance_mode() {
        return;
    }
    let state = app.state::<AppState>();

    // The geometry of a window is asked for on the main thread, so the tab state has to
    // be copied out first: waiting for the event loop while holding this lock is what
    // deadlocks against relayout.
    let mut snapshots: Vec<(String, Vec<crate::session::SessionTab>, usize, Vec<crate::session::SessionGroup>)> =
        Vec::new();
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
        let kept: Vec<&Tab> = core.tabs.iter().filter(|tab| !tab.incognito).collect();
        let groups: Vec<crate::session::SessionGroup> = core
            .groups
            .iter()
            .filter_map(|group| {
                let members: Vec<usize> = group
                    .members
                    .iter()
                    .filter_map(|member| kept.iter().position(|tab| &tab.id == member))
                    .collect();
                if members.is_empty() {
                    return None;
                }
                Some(crate::session::SessionGroup {
                    name: group.name.clone(),
                    color: group.color.clone(),
                    collapsed: group.collapsed,
                    members,
                })
            })
            .collect();
        snapshots.push((label.clone(), tabs, selected, groups));
    }
    // A hash map has no order. The window the user was last in goes first, so the one
    // that gets restored is the one they were working in.
    let focused = state.windows.focused.read().clone();
    snapshots.sort_by_key(|(label, ..)| match &focused {
        Some(active) if active == label => (0, label.clone()),
        _ if label == crate::window::PRIMARY => (1, label.clone()),
        _ => (2, label.clone()),
    });

    let windows: Vec<crate::session::WindowSession> = snapshots
        .into_iter()
        .map(|(label, tabs, selected, groups)| crate::session::WindowSession {
            tabs,
            selected,
            window: window_geometry(app, &label),
            groups,
        })
        .collect();

    let session = crate::session::Session {
        schema_version: crate::session::SCHEMA,
        windows,
    };
    if let Err(err) = session.save(&state.dirs.session_file) {
        crate::logging::warn(&format!("session save failed: {err}"));
    }
}

pub(crate) fn selected_label(state: &AppState, window: &tauri::Window) -> String {
    state
        .read_core(window.label())
        .selected
        .clone()
        .unwrap_or_default()
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

/// Replaces WebView2's error page with ours; returns the url it replaced.
pub fn show_error_page(app: &AppHandle, id: &str, failed_url: &str) -> Option<String> {
    let state = app.try_state::<AppState>()?;
    {
        let mut nav = state.nav.write();
        if nav.error_shown.get(id).map(String::as_str) == Some(failed_url) {
            return None;
        }
        nav.error_shown.insert(id.to_string(), failed_url.to_string());
    }

    let base = state.nav.read().app_base.clone();
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
#[cfg(test)]
mod trusted_caller_tests {
    use super::*;

    #[test]
    fn app_pages_are_trusted() {
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
