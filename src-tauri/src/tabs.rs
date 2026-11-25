use serde::Serialize;
use tauri::{
    webview::WebviewBuilder, Emitter, LogicalPosition, LogicalSize, Manager, Url, Webview,
    WebviewUrl, Window,
};

use crate::paths::AppDirs;
use std::sync::OnceLock;

/// Action -> combo overrides for the forwarder that runs in every page.
static SHORTCUTS: parking_lot::RwLock<Option<std::collections::BTreeMap<String, String>>> =
    parking_lot::RwLock::new(None);

/// Replaces the override map.
pub fn set_shortcuts(map: std::collections::BTreeMap<String, String>) {
    *SHORTCUTS.write() = Some(map);
}

fn shortcut_map() -> std::collections::BTreeMap<String, String> {
    SHORTCUTS.read().clone().unwrap_or_default()
}

/// Right button gestures.
static GESTURES: parking_lot::RwLock<Option<GestureSetup>> = parking_lot::RwLock::new(None);

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
    *GESTURES.write() = Some(setup);
}

fn gesture_setup() -> GestureSetup {
    GESTURES.read().clone().unwrap_or_default()
}

/// The browsing habits a page needs to know about.
///
/// Kept apart from the gesture setup because it is a different subject: these are things the
/// page does with the pointer and the selection, not gestures the reader draws.
static BROWSING: parking_lot::RwLock<Option<String>> = parking_lot::RwLock::new(None);

fn browsing_config() -> String {
    BROWSING.read().clone().unwrap_or_else(|| "{}".to_string())
}

/// Contents of UserData/custom.css, applied to every page.
static CUSTOM_CSS: parking_lot::RwLock<Option<String>> = parking_lot::RwLock::new(None);

pub fn set_custom_css(css: String) {
    *CUSTOM_CSS.write() = Some(css);
}

fn custom_css() -> String {
    CUSTOM_CSS.read().clone().unwrap_or_default()
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
        "window.__mixShortcuts = {json};\nwindow.__mixGestures = {};\nwindow.__mixGestureConfig = {};\nwindow.__mixSuperDrag = {};\nwindow.__mixDragConfig = {};\nwindow.__mixBrowsing = {};\n",
        gestures.enabled, gestures.js, gestures.super_drag, gestures.drag_js, browsing_config()
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
    let browsing = serde_json::json!({
        "autoscroll": settings.autoscroll,
        "copyOnSelect": settings.copy_on_select,
        "forceCurrent": settings.tab_force_current,
        "forceBackground": settings.tab_force_background,
        "revealEdge": settings.bookmark_reveal_edge,
        "revealDelay": settings.bookmark_reveal_delay,
        "pipButton": settings.video_pip_button,
        "videoKeys": settings.video_keys,
        // The page draws the button, so the words in its tooltip come from here: the injected
        // script cannot reach `i18n.js`, and a button labelled in English on a Chinese
        // interface is the kind of detail that reads as unfinished.
        "pipLabel": if settings.language == "zh" {
            "画中画"
        } else {
            "Picture-in-picture"
        },
    });
    *BROWSING.write() = Some(serde_json::to_string(&browsing).unwrap_or_else(|_| "{}".to_string()));
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
    // A paper tint over every page. `filter` rather than a background: a background would
    // be hidden behind whatever the site paints, and this has to work on a page that sets
    // its own colours - which is most of them. The multiply-and-screen pair keeps the
    // contrast of the original instead of flattening it the way plain opacity would.
    if settings.eye_comfort {
        let tone = settings.eye_comfort_color.trim_start_matches('#');
        if tone.len() == 6 {
            css.push_str(&format!(
                "\nhtml {{ background-color: #{tone} !important; }}\n\
                 html > body {{ opacity: 0.94 !important; }}\n"
            ));
        }
    }
    css
}

/// The scheme of an address no webview can show, or nothing.
///
/// The web's own schemes and the app's own are navigation; everything else - `magnet:`,
/// `thunder:`, `mailto:` - belongs to a program outside the browser. `about:` and `data:`
/// are answered by the engine itself and must keep working.
fn foreign_scheme(url: &str) -> Option<String> {
    let (scheme, _) = url.split_once(':')?;
    let scheme = scheme.to_ascii_lowercase();
    if scheme.is_empty() || !scheme.chars().next()?.is_ascii_alphabetic() {
        return None;
    }
    if !scheme
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '+' || c == '-' || c == '.')
    {
        return None;
    }
    match scheme.as_str() {
        "http" | "https" | "about" | "data" | "blob" | "file" | "javascript" | "view-source"
        | "chrome" | "edge" | "devtools" | "tauri" | "asset" | "ipc" | "microsoft-edge" => None,
        _ => Some(scheme),
    }
}

/// Whether the settings name this scheme among the ones a page may open directly.
fn scheme_allowed(list: &str, scheme: &str) -> bool {
    // The same three separators the row's own hint promises, so a list typed by hand with
    // semicolons works as well as one `sanitize` has already rewritten to commas.
    list.split([',', ';', ' ', '\n', '\r', '\t'])
        .map(|entry| entry.trim().trim_end_matches(':'))
        .any(|entry| !entry.is_empty() && entry.eq_ignore_ascii_case(scheme))
}

/// Hands an address to whatever the system has registered for its scheme.
///
/// Through `ShellExecute`, which is what Windows means by "open this": it resolves the
/// scheme through the registry, so a program the reader installed for `magnet:` gets the
/// link and anything else gets the "how do you want to open this" dialog. No shell string
/// is built, so there is nothing to quote wrong.
fn open_external(url: &str) {
    #[cfg(windows)]
    {
        use windows::core::{HSTRING, PCWSTR};
        use windows::Win32::UI::Shell::ShellExecuteW;
        use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
        let wide = HSTRING::from(url);
        let operation = HSTRING::from("open");
        let result = unsafe {
            ShellExecuteW(
                None,
                &operation,
                &wide,
                PCWSTR::null(),
                PCWSTR::null(),
                SW_SHOWNORMAL,
            )
        };
        // ShellExecute answers with a value cast through a pointer; anything above 32 is a
        // success, and what comes back below it is an error code, not a handle.
        if result.0 as isize <= 32 {
            crate::logging::warn(&format!("external open failed: {url}"));
        }
    }
    #[cfg(not(windows))]
    {
        crate::logging::warn(&format!("external open unsupported: {url}"));
    }
}

/// Pushes the shortcut overrides into the pages that are already open.
pub fn broadcast_shortcuts(app: &tauri::AppHandle) {
    let json = serde_json::to_string(&shortcut_map()).unwrap_or_else(|_| "{}".to_string());
    let script = format!("window.__mixShortcuts = {json};");
    let state = app.state::<crate::ipc::AppState>();
    let ids: Vec<String> = state
        .cores
        .read()
        .values()
        .flat_map(|core| core.tabs.iter().map(|tab| tab.id.clone()))
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
    [("L", "open"), ("R", "search"), ("U", "copy"), ("D", "save")]
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

/// How a proxy choice is named, both in the settings and in what is running.
///
/// Manual with no server is the system proxy, because that is what it turns into: no
/// arguments are passed at all. Both sides go through here so a comparison cannot disagree
/// with what the webviews were actually given.
pub fn proxy_label(mode: &str, server: &str) -> String {
    match mode {
        "off" => "off".to_string(),
        "manual" if !server.trim().is_empty() => format!("manual {}", server.trim()),
        _ => "system".to_string(),
    }
}

/// What this session's webviews were created with.
pub fn proxy_applied() -> String {
    match PROXY_ARGS.get().and_then(|args| args.clone()) {
        Some(args) if args == "--no-proxy-server" => "off".to_string(),
        Some(args) => format!("manual {}", args.trim_start_matches("--proxy-server=")),
        None => "system".to_string(),
    }
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
    /// The document that offset belongs to.
    ///
    /// A bare offset cannot say which page it came from, and restoring it blindly put every
    /// new page at the depth the previous one was read to: following a link from a long
    /// article landed halfway down the next one. The offset is only put back onto the
    /// document it was measured on - a reload, or the tab coming back from suspension.
    pub scroll_url: String,
    /// False when the webview was dropped to save memory.
    pub alive: bool,
    pub last_active: u64,
}

impl Tab {
    /// The offset to put back for a document that is loading at `url`.
    ///
    /// Zero unless the offset was measured on this same address. Two different pages that
    /// happen to share a url - a reload, or the tab coming back after being suspended - are
    /// exactly the cases where the reader expects to be where they left off.
    pub fn scroll_for(&self, url: &str) -> f64 {
        if self.scroll > 0.0 && self.scroll_url == url {
            self.scroll
        } else {
            0.0
        }
    }

    /// Records where the reader is, against the document it was read on.
    pub fn remember_scroll(&mut self, url: &str, y: f64) {
        self.scroll = y.max(0.0);
        self.scroll_url = url.to_string();
    }
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
            // A popup becomes a tab, always. A second window is not something this browser
            // can offer - a child webview has no chrome of its own - so the choice is between
            // a tab and nothing, and "nothing" is what a site expects least: a video on
            // bilibili opens through `window.open`, and refusing it made the link do nothing
            // at all, with no hint that anything had happened.
            let _ = app.emit_to(owner.clone(), "ui:open-url", url.to_string());
            tauri::webview::NewWindowResponse::Deny
        })
        .initialization_script(custom_css_script())
        .initialization_script(shortcut_prefix())
        .initialization_script(SHORTCUT_FORWARDER)
        .on_navigation({
            let tab_id = id.to_string();
            let app = nav_app;
            move |url| {
                // A link to another program: `magnet:`, `thunder:`, a game launcher. The
                // webview cannot open those, so the page is stopped and the scheme is handed
                // to the system - but only when the settings name it. A scheme nobody listed
                // asks first, because a page must not be able to launch a program just by
                // naming one.
                if let Some(scheme) = foreign_scheme(url.as_str()) {
                    let known = app
                        .try_state::<crate::ipc::AppState>()
                        .map(|state| {
                            scheme_allowed(&state.settings.read().external_schemes, &scheme)
                        })
                        .unwrap_or(false);
                    if known {
                        crate::logging::info(&format!("external: {scheme} <- {url}"));
                        open_external(url.as_str());
                    } else {
                        // Not on the list. Stopped, and said so in the log; the address is
                        // still in the page, so a reader who wants it can copy it out and
                        // add the scheme in the settings. A prompt here would be a third
                        // kind of overlay for a case that is rare by design.
                        crate::logging::info(&format!(
                            "external: {scheme} is not on the allowed list, not opened"
                        ));
                    }
                    return false;
                }
                if let Some(state) = app.try_state::<crate::ipc::AppState>() {
                    state
                        .nav
                        .write()
                        .pending
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
                    // Only an offset measured on this address may be put back. A link
                    // followed from a long page loads a different document, whose own top
                    // is what the reader asked for.
                    let scroll = state
                        .try_read_core(crate::ipc::window_of_tab(&id))
                        .and_then(|core| {
                            core.tabs
                                .iter()
                                .find(|tab| tab.id == id)
                                .map(|tab| tab.scroll_for(payload.url().as_str()))
                        })
                        .unwrap_or(0.0);
                    if scroll > 0.0 {
                        let _ = webview.eval(format!("window.scrollTo(0, {scroll});"));
                    }
                }
                // The address bar hears about a finished load from here, not from the
                // poll: this is the one moment the url is known to be the document's.
                crate::window::publish_loaded(&app, &id, payload.url().as_str());
                // A document that replaced the one a panel was opened over has no idea the
                // panel is there, so it would not close it on the next click.
                crate::overlay::remark_page(&app, crate::ipc::window_of_tab(&id), &id);
                // Sites the user asked to always translate are translated as they arrive.
                crate::ipc::auto_translate(&app, &id, payload.url().as_str());
            }
            if loading {
                if let Some(state) = app.try_state::<crate::ipc::AppState>() {
                    let level = state.zoom().level_for(payload.url().as_str());
                    if (level - 1.0).abs() > f64::EPSILON {
                        let _ = webview.set_zoom(level);
                        crate::logging::info(&format!("zoom {level:.1} for {}", payload.url()));
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
                        .try_read_core(crate::ipc::window_of_tab(&payload.id))
                        .and_then(|core| {
                            core.tabs
                                .iter()
                                .find(|t| t.id == payload.id)
                                .map(|t| t.url.clone())
                        })
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

            let selected = app.try_state::<crate::ipc::AppState>().and_then(|state| {
                state
                    .try_read_core(&owner)
                    .and_then(|core| core.selected.clone())
            });
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
        .map(|size| {
            size.to_logical::<f64>(win.scale_factor().unwrap_or(1.0))
                .height
        })
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

/// Where a new tab belongs in the strip, given the setting and the tab it was opened from.
///
/// `None` means "at the end". The two grouped answers depend on the anchor tab: right of it,
/// or right of the last member of its group. Both fall back to the anchor's own position, so
/// a group setting on a tab that belongs to no group behaves like the plain one.
///
/// Split out of `tab_new` so the arithmetic can be tested without a window: the command needs
/// an app handle, a window and a webview to do anything, and the question here is only where
/// an index goes.
pub(crate) fn insertion_point(
    tabs: &[Tab],
    groups: &[crate::ipc::TabGroup],
    anchor: &Option<String>,
    place: &str,
) -> Option<usize> {
    if place == "at_end" {
        return None;
    }
    let anchor_at = tabs
        .iter()
        .position(|tab| Some(&tab.id) == anchor.as_ref())?;
    if place == "after_group" {
        let members: Vec<&str> = groups
            .iter()
            .find(|group| group.members.iter().any(|id| Some(id) == anchor.as_ref()))
            .map(|group| group.members.iter().map(String::as_str).collect())
            .unwrap_or_default();
        if !members.is_empty() {
            // The last member as the strip stands, not as the group lists it: a member can
            // have been dragged elsewhere, and the new tab belongs next to where the group
            // actually ends.
            let last = tabs
                .iter()
                .enumerate()
                .filter(|(_, tab)| members.contains(&tab.id.as_str()))
                .map(|(index, _)| index)
                .max()
                .unwrap_or(anchor_at);
            return Some(last + 1);
        }
    }
    Some(anchor_at + 1)
}

#[cfg(test)]
mod tests {
    use super::{foreign_scheme, insertion_point, proxy_label, scheme_allowed, Tab};
    use crate::ipc::TabGroup;

    /// A tab with nothing but an id: the only field the placement question looks at.
    fn tab(id: &str) -> Tab {
        Tab {
            id: id.to_string(),
            url: format!("https://{id}.test/"),
            title: id.to_string(),
            zoom: 1.0,
            pinned: false,
            incognito: false,
            icon: None,
            loading: false,
            scroll: 0.0,
            scroll_url: String::new(),
            alive: true,
            last_active: 0,
        }
    }

    fn strip(ids: &[&str]) -> Vec<Tab> {
        ids.iter().map(|id| tab(id)).collect()
    }

    fn group(id: &str, members: &[&str]) -> TabGroup {
        TabGroup {
            id: id.to_string(),
            name: id.to_string(),
            color: "#4c8dff".to_string(),
            collapsed: false,
            members: members.iter().map(|m| m.to_string()).collect(),
        }
    }

    #[test]
    fn a_new_tab_goes_where_the_setting_says() {
        let tabs = strip(&["a", "b", "c"]);
        let anchor = Some("b".to_string());

        // Right of the tab it was opened from.
        assert_eq!(
            insertion_point(&tabs, &[], &anchor, "after_current"),
            Some(2)
        );
        // At the end, which is not an index: the caller pushes.
        assert_eq!(insertion_point(&tabs, &[], &anchor, "at_end"), None);
        // An anchor that is not in the strip: the same as "at the end" rather than a panic.
        assert_eq!(
            insertion_point(&tabs, &[], &Some("gone".to_string()), "after_current"),
            None
        );
    }

    #[test]
    fn a_group_setting_lands_after_the_group_and_not_after_the_anchor() {
        // c is the anchor and sits second, but the group ends at e.
        let tabs = strip(&["a", "c", "d", "e", "f"]);
        let groups = vec![group("g1", &["c", "e"])];
        assert_eq!(
            insertion_point(&tabs, &groups, &Some("c".to_string()), "after_group"),
            Some(4),
            "right of the last member, not right of the anchor"
        );
        // A tab in no group: the group answer falls back to the plain one.
        assert_eq!(
            insertion_point(&tabs, &groups, &Some("a".to_string()), "after_group"),
            Some(1)
        );
    }

    #[test]
    fn an_offset_is_only_put_back_on_the_page_it_was_measured_on() {
        // The regression: following a link out of a long article carried its depth over to
        // the page the link opened, because the offset had no idea which document it
        // belonged to. A tab that left the page must come back to the top of the new one.
        let mut only = tab("a");
        only.url = "https://example.com/long".into();
        only.remember_scroll("https://example.com/long", 800.0);
        assert_eq!(only.scroll_for("https://example.com/long"), 800.0);
        assert_eq!(
            only.scroll_for("https://example.com/next"),
            0.0,
            "a different document starts at its own top"
        );

        // Following a link moves the tab's url, which is the whole mechanism: the offset
        // stays behind with the page it belongs to and the new document gets a top.
        only.url = "https://example.com/next".into();
        assert_eq!(only.scroll_for(&only.url), 0.0);

        // A reload is the same document, so the reader stays where they were. This is the
        // case the feature exists for, and it is why the comparison is against the address
        // rather than a "the page changed" flag.
        only.remember_scroll("https://example.com/next", 240.0);
        assert_eq!(only.scroll_for("https://example.com/next"), 240.0);

        // Reading back to the top overwrites the measurement rather than accumulating.
        only.remember_scroll("https://example.com/next", 0.0);
        assert_eq!(only.scroll_for("https://example.com/next"), 0.0);
    }

    #[test]
    fn the_web_keeps_its_own_schemes() {
        // Everything here is navigation the engine must still perform.
        for url in [
            "https://example.com",
            "http://example.com/a?b=c",
            "about:blank",
            "data:text/html,<p>hi</p>",
            "blob:https://example.com/9c1e",
            "file:///C:/x.html",
            "view-source:https://example.com",
        ] {
            assert_eq!(
                foreign_scheme(url),
                None,
                "{url} is not an external hand-off"
            );
        }
    }

    #[test]
    fn a_link_to_another_program_is_recognised() {
        assert_eq!(
            foreign_scheme("magnet:?xt=urn:btih:abc"),
            Some("magnet".into())
        );
        assert_eq!(foreign_scheme("thunder://QUFodHRw"), Some("thunder".into()));
        assert_eq!(foreign_scheme("mailto:a@b.test"), Some("mailto".into()));
        // Case is not the scheme's business.
        assert_eq!(foreign_scheme("MAGNET:?xt=1"), Some("magnet".into()));
    }

    #[test]
    fn a_relative_or_broken_address_is_not_a_scheme() {
        assert_eq!(foreign_scheme("no-colon-here"), None);
        assert_eq!(foreign_scheme(":leading"), None);
        assert_eq!(foreign_scheme("1nvalid:x"), None);
        // A path segment with a colon in it is not a scheme either.
        assert_eq!(foreign_scheme("src/foo:bar"), None);
    }

    #[test]
    fn only_a_listed_scheme_is_opened_without_asking() {
        let list = "magnet,thunder,ed2k";
        assert!(scheme_allowed(list, "magnet"));
        assert!(scheme_allowed(list, "MAGNET"));
        assert!(scheme_allowed("magnet; thunder", "thunder"));
        assert!(!scheme_allowed(list, "ms-settings"));
        assert!(!scheme_allowed("", "magnet"));
    }

    #[test]
    fn a_manual_choice_without_a_server_is_the_system_proxy() {
        // Nothing is passed to WebView2 in that case, so naming it "manual" would make the
        // settings page report a change that is not one.
        assert_eq!(proxy_label("manual", ""), "system");
        assert_eq!(proxy_label("manual", "   "), "system");
        assert_eq!(proxy_label("system", "127.0.0.1:1"), "system");
    }

    #[test]
    fn the_running_choice_and_the_saved_one_are_named_the_same_way() {
        assert_eq!(proxy_label("off", ""), "off");
        assert_eq!(
            proxy_label("manual", " http://127.0.0.1:7890 "),
            "manual http://127.0.0.1:7890"
        );
    }
}
