use serde::Serialize;
use tauri::{
    webview::WebviewBuilder, Emitter, LogicalPosition, LogicalSize, Manager, Url, Webview,
    WebviewUrl, Window,
};

use crate::paths::AppDirs;

/// Runs in every page. Frames cannot see chrome keydown, so shortcuts are
/// forwarded from the page to Rust and dispatched there.
const SHORTCUT_FORWARDER: &str = r#"
(function () {
  if (window.__mixKeys) return;
  window.__mixKeys = true;

  var send = function (action) {
    if (!window.__TAURI_INTERNALS__ || !window.__TAURI_INTERNALS__.invoke) return;
    window.__TAURI_INTERNALS__.invoke("shortcut", { action: action }).catch(function () {});
  };

  var actionFor = function (event) {
    if (event.key === "F5") return "reload";
    if (event.key === "F12") return "devtools";
    var ctrl = event.ctrlKey || event.metaKey;
    if (!ctrl) return null;
    switch (event.key) {
      case "t": return event.shiftKey ? null : "new_tab";
      case "w": return "close_tab";
      case "l": return "focus_url";
      case "r": return "reload";
      case "Tab": return event.shiftKey ? "prev_tab" : "next_tab";
      case "=":
      case "+": return "zoom_in";
      case "-": return "zoom_out";
      case "0": return "zoom_reset";
      default: return null;
    }
  };

  window.addEventListener("keydown", function (event) {
    var action = actionFor(event);
    if (!action) return;
    event.preventDefault();
    event.stopPropagation();
    send(action);
  }, true);
})();
"#;
use crate::window::CHROME_H;

/// Where the webview keeps its profile and, on Windows, which folders hold extensions.
#[derive(Clone, Debug, Default)]
pub struct WebviewOpts {
    pub tab_data_dir: Option<std::path::PathBuf>,
    pub ui_data_dir: Option<std::path::PathBuf>,
    pub extensions_dir: Option<std::path::PathBuf>,
}

impl WebviewOpts {
    pub fn for_dirs(dirs: &AppDirs, extensions: &[crate::extensions::ExtInfo]) -> Self {
        let any_loaded = extensions.iter().any(|ext| ext.status == "loaded");
        Self {
            tab_data_dir: Some(dirs.profile.clone()),
            ui_data_dir: Some(dirs.profile_ui.clone()),
            #[cfg(windows)]
            extensions_dir: if any_loaded {
                Some(dirs.extensions.clone())
            } else {
                None
            },
            #[cfg(not(windows))]
            extensions_dir: None,
        }
    }
}

pub fn apply_profile(builder: WebviewBuilder<tauri::Wry>, dir: &Option<std::path::PathBuf>) -> WebviewBuilder<tauri::Wry> {
    #[cfg(any(windows, target_os = "linux"))]
    if let Some(dir) = dir {
        return builder.data_directory(dir.clone());
    }
    let _ = dir;
    builder
}

fn apply_extensions(builder: WebviewBuilder<tauri::Wry>, opts: &WebviewOpts) -> WebviewBuilder<tauri::Wry> {
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
    /// False means the webview was dropped to save memory; selecting the tab
    /// recreates it from url.
    pub alive: bool,
    pub last_active: u64,
}

#[derive(Clone, Serialize)]
pub struct TitlePayload {
    pub id: String,
    pub title: String,
}

pub fn parse_url(input: &str) -> Url {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return "about:blank".parse().expect("static url");
    }
    if let Ok(url) = trimmed.parse::<Url>() {
        return url;
    }
    match format!("https://{trimmed}").parse::<Url>() {
        Ok(url) => url,
        Err(_) => "about:blank".parse().expect("static url"),
    }
}

pub fn create(
    win: &Window,
    id: &str,
    url: &str,
    width: f64,
    height: f64,
    opts: &WebviewOpts,
) -> tauri::Result<Webview> {
    let builder = WebviewBuilder::new(id, WebviewUrl::External(parse_url(url)))
        // WebView2 keeps ctrl+wheel zoom off unless asked (M0 E2).
        .zoom_hotkeys_enabled(true)
        .on_download(crate::downloads::handle)
        .initialization_script(SHORTCUT_FORWARDER)
        .on_document_title_changed(|webview, title| {
            let app = webview.app_handle().clone();
            let payload = TitlePayload {
                id: webview.label().to_string(),
                title: title.clone(),
            };
            if let Some(state) = app.try_state::<crate::ipc::AppState>() {
                if let Ok(mut core) = state.core.write() {
                    if let Some(tab) = core.tabs.iter_mut().find(|t| t.id == payload.id) {
                        tab.title = title;
                    }
                }
            }
            let _ = app.emit("tab:title", payload);
        });

    let builder = apply_profile(builder, &opts.tab_data_dir);
    let builder = apply_extensions(builder, opts);

    win.add_child(
        builder,
        LogicalPosition::new(0.0, CHROME_H),
        LogicalSize::new(width.max(1.0), height.max(1.0)),
    )
}