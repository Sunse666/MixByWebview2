use serde::Serialize;
use tauri::{
    webview::WebviewBuilder, Emitter, LogicalPosition, LogicalSize, Manager, Url, Webview,
    WebviewUrl, Window,
};

use crate::paths::AppDirs;
use std::sync::OnceLock;

/// Action -> combo overrides for the forwarder that runs in every page.
static SHORTCUTS: std::sync::RwLock<Option<std::collections::BTreeMap<String, String>>> =
    std::sync::RwLock::new(None);

/// Replaces the override map.
pub fn set_shortcuts(map: std::collections::BTreeMap<String, String>) {
    *SHORTCUTS.write().unwrap_or_else(|e| e.into_inner()) = Some(map);
}

fn shortcut_map() -> std::collections::BTreeMap<String, String> {
    SHORTCUTS
        .read()
        .unwrap_or_else(|e| e.into_inner())
        .clone()
        .unwrap_or_default()
}

/// Right button gestures.
static GESTURES: std::sync::RwLock<Option<GestureSetup>> = std::sync::RwLock::new(None);

/// Everything the injected engine needs, pre-serialised for the page.
#[derive(Default, Clone)]
pub struct GestureSetup {
    pub enabled: bool,
    pub super_drag: bool,
    pub js: String,
    /// Serialised drag hint, threshold and direction map.
    pub drag_js: String,
}

pub fn set_gestures(setup: GestureSetup) {
    *GESTURES.write().unwrap_or_else(|e| e.into_inner()) = Some(setup);
}

fn gesture_setup() -> GestureSetup {
    GESTURES
        .read()
        .unwrap_or_else(|e| e.into_inner())
        .clone()
        .unwrap_or_default()
}

/// Contents of UserData/custom.css, applied to every page.
static CUSTOM_CSS: std::sync::RwLock<Option<String>> = std::sync::RwLock::new(None);

pub fn set_custom_css(css: String) {
    *CUSTOM_CSS.write().unwrap_or_else(|e| e.into_inner()) = Some(css);
}

fn custom_css() -> String {
    CUSTOM_CSS
        .read()
        .unwrap_or_else(|e| e.into_inner())
        .clone()
        .unwrap_or_default()
}

/// Wraps the user stylesheet so pages pick it up without a round trip.
fn custom_css_script() -> String {
    let css = custom_css();
    crate::logging::info(&format!("custom css bytes: {}", css.len()));
    if css.trim().is_empty() {
        return String::new();
    }
    let json = serde_json::to_string(&css).unwrap_or_else(|_| "\"\"".to_string());
    format!(
        "(function(){{var css={json};function put(){{var s=document.createElement('style');s.textContent=css;(document.head||document.documentElement).appendChild(s);}}if(document.head){{put();}}else{{document.addEventListener('DOMContentLoaded',put);}}}})();\n"
    )
}

/// Small script that hands the overrides to the forwarder below.
fn shortcut_prefix() -> String {
    let json = serde_json::to_string(&shortcut_map()).unwrap_or_else(|_| "{}".to_string());
    let gestures = gesture_setup();
    let out = format!(
        "window.__mixShortcuts = {json};\nwindow.__mixGestures = {};\nwindow.__mixGestureConfig = {};\nwindow.__mixSuperDrag = {};\nwindow.__mixDragConfig = {};\n",
        gestures.enabled, gestures.js, gestures.super_drag, gestures.drag_js
    );
    out
}
/// Reports what a press lands on, for the diagnose command.
pub const DRAG_PROBE: &str = include_str!("injected/drag_probe.js");
/// Pushes everything the page init scripts copy at creation time.
pub fn apply_settings(dirs: &AppDirs, settings: &crate::state::Settings) {
    let config = serde_json::json!({
        "trail": settings.gesture_trail,
        "color": settings.gesture_color,
        "width": settings.gesture_width,
        "threshold": settings.gesture_threshold,
        "map": if settings.gesture_map.is_empty() {
            default_gestures()
        } else {
            settings.gesture_map.clone()
        },
        "blocked": settings.gesture_blocked,
    });
    let drag = serde_json::json!({
        "hint": settings.drag_hint,
        "threshold": settings.drag_threshold,
        "map": if settings.drag_map.is_empty() {
            default_drag()
        } else {
            settings.drag_map.clone()
        },
    });
    set_gestures(GestureSetup {
        enabled: settings.gestures,
        super_drag: settings.super_drag,
        drag_js: serde_json::to_string(&drag).unwrap_or_else(|_| "{}".to_string()),
        js: serde_json::to_string(&config).unwrap_or_else(|_| "{}".to_string()),
    });
    set_shortcuts(settings.shortcuts.clone());
    set_custom_css(build_custom_css(dirs, settings));
}

/// UserData/custom.css plus the font overrides from the settings.
fn build_custom_css(dirs: &AppDirs, settings: &crate::state::Settings) -> String {
    let mut css = std::fs::read_to_string(dirs.user_data.join("custom.css"))
        .unwrap_or_default()
        .trim_start_matches('\u{feff}')
        .to_string();
    if !settings.font_family.trim().is_empty() {
        css.push_str(&format!(
            "\nbody, p, li, td, dd, blockquote {{ font-family: \"{}\" !important; }}\n",
            settings.font_family.replace('"', "")
        ));
    }
    if !settings.font_mono.trim().is_empty() {
        css.push_str(&format!(
            "\ncode, pre, kbd, samp, tt, textarea {{ font-family: \"{}\" !important; }}\n",
            settings.font_mono.replace('"', "")
        ));
    }
    if settings.font_size > 0 {
        css.push_str(&format!(
            "\nbody {{ font-size: {}px !important; }}\n",
            settings.font_size
        ));
    }
    if settings.font_min_size > 0 {
        css.push_str(&format!(
            "\n:where(p, li, td, th, dd, dt, span, div, a, label, button, input, select, textarea, blockquote, figcaption) {{ font-size: max(1em, {}px) !important; }}\n",
            settings.font_min_size
        ));
    }
    css
}

/// Pushes the shortcut overrides into the pages that are already open.
pub fn broadcast_shortcuts(app: &tauri::AppHandle) {
    let json = serde_json::to_string(&shortcut_map()).unwrap_or_else(|_| "{}".to_string());
    let script = format!("window.__mixShortcuts = {json};");
    let state = app.state::<crate::ipc::AppState>();
    let ids: Vec<String> = state
        .cores
        .read()
        .iter()
        .flat_map(|(_, core)| core.tabs.iter().map(|tab| tab.id.clone()))
        .collect();
    for id in ids {
        if let Some(webview) = app.get_webview(&id) {
            let _ = webview.eval(&script);
        }
    }
}

/// The mapping used until the user edits it.
pub fn default_gestures() -> std::collections::BTreeMap<String, String> {
    [
        ("L", "back"),
        ("R", "forward"),
        ("U", "scroll_top"),
        ("D", "scroll_bottom"),
        ("UD", "reload"),
        ("DU", "reopen_tab"),
        ("LR", "next_tab"),
        ("RL", "prev_tab"),
        ("UL", "bookmark"),
        ("DR", "downloads"),
        ("UR", "new_tab"),
        ("DL", "close_tab"),
    ]
    .iter()
    .map(|(pattern, action)| (pattern.to_string(), action.to_string()))
    .collect()
}
/// Default drag directions: left opens, up copies, right searches.
pub fn default_drag() -> std::collections::BTreeMap<String, String> {
    [
        ("L", "open"),
        ("R", "search"),
        ("U", "copy"),
        ("D", "save"),
    ]
    .iter()
    .map(|(dir, action)| (dir.to_string(), action.to_string()))
    .collect()
}

const SHORTCUT_FORWARDER: &str = include_str!("injected/forwarder.js");


/// Where the webview keeps its profile and, on Windows, its extensions.
#[derive(Clone, Debug, Default)]
pub struct WebviewOpts {
    pub tab_data_dir: Option<std::path::PathBuf>,
    /// Where the strip and the overlays keep their profile.
    pub ui_data_dir: Option<std::path::PathBuf>,
    /// Used when sharing that profile is refused.
    pub ui_fallback_dir: Option<std::path::PathBuf>,
    pub extensions_dir: Option<std::path::PathBuf>,
    /// Compiled userscript loader; empty when the drop folder has no scripts.
    pub userscripts: String,
}

impl WebviewOpts {
    pub fn for_dirs(dirs: &AppDirs, extensions: &[crate::extensions::ExtInfo]) -> Self {
        let any_loaded = extensions.iter().any(|ext| ext.status == "loaded");
        Self {
            tab_data_dir: Some(dirs.profile.clone()),
            ui_data_dir: Some(dirs.profile.clone()),
            ui_fallback_dir: Some(dirs.profile_ui.clone()),
            #[cfg(windows)]
            extensions_dir: if any_loaded {
                Some(dirs.extensions.clone())
            } else {
                None
            },
            #[cfg(not(windows))]
            extensions_dir: None,
            userscripts: String::new(),
        }
    }
}

pub(crate) fn apply_profile(
    builder: WebviewBuilder<tauri::Wry>,
    dir: &Option<std::path::PathBuf>,
) -> WebviewBuilder<tauri::Wry> {
    let builder = proxy_args(builder);
    #[cfg(any(windows, target_os = "linux"))]
    if let Some(dir) = dir {
        return builder.data_directory(dir.clone());
    }
    let _ = dir;
    builder
}

/// The browser arguments for this session's proxy choice.
static PROXY_ARGS: OnceLock<Option<String>> = OnceLock::new();

/// Sets the arguments for the rest of the run; the first call wins.
pub fn set_proxy_args(args: Option<String>) {
    let _ = PROXY_ARGS.set(args);
}

fn proxy_args(builder: WebviewBuilder<tauri::Wry>) -> WebviewBuilder<tauri::Wry> {
    match PROXY_ARGS.get().and_then(|args| args.clone()) {
        Some(args) => builder.additional_browser_args(&args),
        None => builder,
    }
}

pub(crate) fn apply_extensions(
    builder: WebviewBuilder<tauri::Wry>,
    opts: &WebviewOpts,
) -> WebviewBuilder<tauri::Wry> {
    #[cfg(windows)]
    if let Some(dir) = &opts.extensions_dir {
        return builder
            .browser_extensions_enabled(true)
            .extensions_path(dir.clone());
    }
    let _ = opts;
    builder
}

#[derive(Debug, Clone, Serialize)]
pub struct Tab {
    pub id: String,
    pub url: String,
    pub title: String,
    pub zoom: f64,
    /// Pinned tabs stay at the left and keep a narrow strip.
    pub pinned: bool,
    /// Incognito tabs never touch the shared profile and are not restored.
    pub incognito: bool,
    /// Favicon url reported by the page, if any.
    pub icon: Option<String>,
    pub loading: bool,
    /// Last scroll offset, restored when the tab becomes visible again.
    pub scroll: f64,
    /// False when the webview was dropped to save memory.
    pub alive: bool,
    pub last_active: u64,
}

#[derive(Clone, Serialize)]
pub struct TitlePayload {
    pub id: String,
    pub title: String,
}

/// Returns None when the text cannot be turned into a web address.
pub fn parse_url(input: &str) -> Option<Url> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return "about:blank".parse().ok();
    }
    if let Ok(url) = trimmed.parse::<Url>() {
        return Some(url);
    }
    if !trimmed.contains(char::is_whitespace) && trimmed.contains('.') {
        if let Ok(url) = format!("https://{trimmed}").parse::<Url>() {
            return Some(url);
        }
    }
    None
}

/// Applies the default page zoom to a freshly created tab.
pub fn apply_default_zoom(app: &tauri::AppHandle, id: &str) {
    let zoom = app
        .try_state::<crate::ipc::AppState>()
        .map(|state| state.settings.read().default_zoom)
        .unwrap_or(1.0);
    if (zoom - 1.0).abs() < f64::EPSILON {
        return;
    }
    if let Some(webview) = app.get_webview(id) {
        let _ = webview.set_zoom(zoom);
    }
}
pub fn create(
    win: &Window,
    id: &str,
    url: &str,
    width: f64,
    height: f64,
    opts: &WebviewOpts,
    incognito: bool,
) -> tauri::Result<Webview> {
    let app = win.app_handle().clone();
    let nav_app = app.clone();
    let owner = crate::ipc::window_of_tab(id).to_string();
    let target = parse_url(url).unwrap_or_else(|| "about:blank".parse().expect("static url"));
    let background = win
        .app_handle()
        .try_state::<crate::ipc::AppState>()
        .map(|state| crate::overlay::parse_color(&state.theme.read().colors.bg))
        .unwrap_or(tauri::window::Color(0x1F, 0x20, 0x20, 0xFF));
    let builder = WebviewBuilder::new(id, WebviewUrl::External(target))
        .background_color(background)
        .zoom_hotkeys_enabled(true)
        .devtools(true)
        .general_autofill_enabled(true)
        .incognito(incognito)
        .on_download(crate::downloads::handle)
        .on_new_window(move |url, _features| {
            let _ = app.emit_to(owner.clone(), "ui:open-url", url.to_string());
            tauri::webview::NewWindowResponse::Deny
        })
        .initialization_script(&custom_css_script())
        .initialization_script(&shortcut_prefix())
        .initialization_script(SHORTCUT_FORWARDER)
        .on_navigation({
            let tab_id = id.to_string();
            let app = nav_app;
            move |url| {
                if let Some(state) = app.try_state::<crate::ipc::AppState>() {
                    state
                        .pending
                        .write()
                        
                        .insert(tab_id.clone(), url.to_string());
                }
                true
            }
        })
        .on_page_load(|webview, payload| {
            let app = webview.app_handle().clone();
            let id = webview.label().to_string();
            let loading = matches!(payload.event(), tauri::webview::PageLoadEvent::Started);

            if !loading {
                if let Some(state) = app.try_state::<crate::ipc::AppState>() {
                    let scroll = state
                        .try_read_core(crate::ipc::window_of_tab(&id))
                        .and_then(|core| core.tabs.iter().find(|tab| tab.id == id).map(|tab| tab.scroll))
                        .unwrap_or(0.0);
                    if scroll > 0.0 {
                        let _ = webview.eval(&format!("window.scrollTo(0, {scroll});"));
                    }
                }
            }
            if loading {
                if let Some(state) = app.try_state::<crate::ipc::AppState>() {
                    let level = state.zoom.read().level_for(payload.url().as_str());
                    if (level - 1.0).abs() > f64::EPSILON {
                        let _ = webview.set_zoom(level);
                        crate::logging::info(&format!(
                            "zoom {level:.1} for {}",
                            payload.url()
                        ));
                        if let Ok(mut core) = state
                            .try_write_core(crate::ipc::window_of_tab(&id))
                            .ok_or(())
                            .map_err(|_| ())
                        {
                            if let Some(tab) = core.tabs.iter_mut().find(|tab| tab.id == id) {
                                tab.zoom = level;
                            }
                        }
                    }
                }
            }
            let owner = crate::ipc::window_of_tab(&id).to_string();
            let _ = app.emit_to(
                owner,
                "tab:loading",
                crate::ipc::LoadingPayload { id, loading },
            );
        })
        .on_document_title_changed(|webview, title| {
            #[cfg(debug_assertions)]
            crate::logging::info(&format!("tab title: {title}"));
            let app = webview.app_handle().clone();
            let payload = TitlePayload {
                id: webview.label().to_string(),
                title: title.clone(),
            };
            let tab_url = app
                .try_state::<crate::ipc::AppState>()
                .and_then(|state| {
                    state
                        .try_read_core(&crate::ipc::window_of_tab(&payload.id))
                        .and_then(|core| core.tabs.iter().find(|t| t.id == payload.id).map(|t| t.url.clone()))
                })
                .unwrap_or_default();
            if !tab_url.is_empty() {
                crate::ipc::attach_title(&app, &tab_url, &title);
            }
            if let Some(state) = app.try_state::<crate::ipc::AppState>() {
                let window = crate::ipc::window_of_tab(&payload.id);
                if let Some(mut core) = state.try_write_core(window) {
                    if let Some(tab) = core.tabs.iter_mut().find(|t| t.id == payload.id) {
                        tab.title = title;
                    }
                }
            }
            let owner = crate::ipc::window_of_tab(&payload.id).to_string();

            let selected = app
                .try_state::<crate::ipc::AppState>()
                .and_then(|state| state.try_read_core(&owner).and_then(|core| core.selected.clone()));
            if selected.as_deref() == Some(payload.id.as_str()) {
                if let Some(win) = app.get_window(&owner) {
                    let _ = win.set_title(&format!("{} - Mix", payload.title));
                }
            }

            let _ = app.emit_to(owner, "tab:title", payload);
        });

    let builder = if opts.userscripts.is_empty() {
        builder
    } else {
        builder.initialization_script_for_all_frames(&opts.userscripts)
    };
    let builder = apply_profile(builder, &opts.tab_data_dir);
    let builder = apply_extensions(builder, opts);

    let top = win
        .inner_size()
        .map(|size| size.to_logical::<f64>(win.scale_factor().unwrap_or(1.0)).height)
        .map(|full| (full - height).max(0.0))
        .unwrap_or(0.0);
    let webview = win.add_child(
        builder,
        LogicalPosition::new(0.0, top),
        LogicalSize::new(width.max(1.0), height.max(1.0)),
    )?;
    crate::downloads::ops::watch(&webview);
    Ok(webview)
}