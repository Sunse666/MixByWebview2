//! Shared state and helpers for the ipc layer. The commands themselves live in
//! one module per area and are re-exported here, so crate::ipc::<command> keeps
//! working and main.rs does not have to know the layout.

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
