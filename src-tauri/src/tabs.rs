use serde::Serialize;
use tauri::{
    webview::WebviewBuilder, Emitter, LogicalPosition, LogicalSize, Manager, Url, Webview,
    WebviewUrl, Window,
};

use crate::paths::AppDirs;

/// Runs in every page. Frames cannot see chrome keydown, so shortcuts are
/// forwarded from the page to Rust and dispatched there.
const SHORTCUT_FORWARDER: &str = r#"
  // Clicking anywhere in the page closes an open overlay (menu or context menu).
  let __mixDismiss = false;
  window.addEventListener("pointerdown", (event) => {
    if (event.button !== 0 || __mixDismiss) return;
    __mixDismiss = true;
    setTimeout(() => { __mixDismiss = false; }, 100);
    window.__TAURI_INTERNALS__.invoke("shortcut", { action: "dismiss" }).catch(() => {});
  }, true);

(function () {
  // Report the page favicon once the document is parsed.
  var sendIcon = function () {
    try {
      var link = document.querySelector('link[rel~="icon"]');
      var href = link && link.href ? link.href : "";
      if (!href && location.protocol.startsWith("http")) {
        href = location.origin + "/favicon.ico";
      }
      if (!href) return;
      window.__TAURI_INTERNALS__.invoke("tab_icon", { url: href }).catch(function () {});
    } catch (error) {
      /* ignore */
    }
  };
  document.addEventListener("DOMContentLoaded", sendIcon);
  setTimeout(sendIcon, 1500);
})();

(function () {
  if (window.__mixKeys) return;
  window.__mixKeys = true;

  var send = function (action) {
    if (!window.__TAURI_INTERNALS__ || !window.__TAURI_INTERNALS__.invoke) return;
    window.__TAURI_INTERNALS__.invoke("shortcut", { action: action }).catch(function () {});
  };

  var actionFor = function (event) {
    if (event.key === "F5") return "reload";
    if (event.key === "F11") return "fullscreen";
    if (event.key === "F12") return "devtools";
    var ctrl = event.ctrlKey || event.metaKey;
    if (!ctrl) return null;
    // Shift turns "t" into "T"; the switch is keyed on the lower-case letter.
    var key = event.key.length === 1 ? event.key.toLowerCase() : event.key;
    switch (key) {
      case "t": return event.shiftKey ? "reopen_tab" : "new_tab";
      case "w": return "close_tab";
      case "l": return "focus_url";
      case "r": return "reload";
      case "Tab": return event.shiftKey ? "prev_tab" : "next_tab";
      case "=":
      case "+": return "zoom_in";
      case "-": return "zoom_out";
      case "0": return "zoom_reset";
      case "p": return "print";
      case "j": return "downloads";
      case "d": return "bookmark";
      case "n": return event.shiftKey ? "new_incognito" : "new_window";
      default: return null;
    }
  };

  var report = function (payload) {
    if (!window.__TAURI_INTERNALS__ || !window.__TAURI_INTERNALS__.invoke) return;
    window.__TAURI_INTERNALS__.invoke("context_menu", { request: payload }).catch(function () {});
  };

  // Suppress the native menu and let the shell draw its own.
  window.addEventListener("contextmenu", function (event) {
    event.preventDefault();
    var link = event.target.closest ? event.target.closest("a") : null;
    var image = event.target.closest ? event.target.closest("img") : null;
    var selection = window.getSelection ? String(window.getSelection()) : "";
    menuShownAt = Date.now();
    report({
      dismiss: false,
      x: Math.round(event.clientX),
      y: Math.round(event.clientY),
      link: link ? link.href : null,
      image: image ? (image.currentSrc || image.src) : null,
      selection: selection ? selection.slice(0, 200) : null,
      editable: !!(event.target.isContentEditable || /^(input|textarea)$/i.test(event.target.tagName || ""))
    });
  }, true);

  // Any click in the page closes an open menu, but only for a short window
  // after one was requested, so ordinary clicks stay IPC free.
  var menuShownAt = 0;
  window.addEventListener("mousedown", function () {
    if (Date.now() - menuShownAt > 8000) return;
    menuShownAt = 0;
    report({ dismiss: true });
  }, true);

  // Sensitive APIs ask the shell first; the prompt is ours, not a browser dialog.
  var askPermission = function (kind) {
    if (!window.__TAURI_INTERNALS__ || !window.__TAURI_INTERNALS__.invoke) return Promise.resolve(false);
    return window.__TAURI_INTERNALS__.invoke("permission_request", {
      origin: location.origin,
      kind: kind
    }).then(function (allow) { return !!allow; }).catch(function () { return false; });
  };

  if (navigator.mediaDevices && navigator.mediaDevices.getUserMedia) {
    var originalMedia = navigator.mediaDevices.getUserMedia.bind(navigator.mediaDevices);
    navigator.mediaDevices.getUserMedia = function (constraints) {
      var kind = constraints && constraints.video ? "camera" : "microphone";
      return askPermission(kind).then(function (allow) {
        if (!allow) throw new DOMException("Permission denied", "NotAllowedError");
        return originalMedia(constraints);
      });
    };
  }

  if (navigator.geolocation) {
    ["getCurrentPosition", "watchPosition"].forEach(function (name) {
      var original = navigator.geolocation[name].bind(navigator.geolocation);
      navigator.geolocation[name] = function (success, failure, options) {
        askPermission("geolocation").then(function (allow) {
          if (allow) return original(success, failure, options);
          if (failure) failure({ code: 1, message: "Permission denied" });
        });
      };
    });
  }

  if (window.Notification && Notification.requestPermission) {
    var originalNotify = Notification.requestPermission.bind(Notification);
    Notification.requestPermission = function (callback) {
      return askPermission("notifications").then(function (allow) {
        if (allow) return originalNotify(callback);
        var denied = "denied";
        if (callback) callback(denied);
        return denied;
      });
    };
  }

  window.addEventListener("keydown", function (event) {
    if (event.altKey && event.shiftKey && event.key.indexOf("Arrow") === 0 && window.__TAURI_INTERNALS__) {
      event.preventDefault();
      window.__TAURI_INTERNALS__.invoke("snap", {
        direction: event.key.replace("Arrow", "").toLowerCase()
      }).catch(function () {});
      return;
    }
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
    /// Compiled userscript loader; empty when the drop folder has no scripts.
    pub userscripts: String,
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
            userscripts: String::new(),
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
    /// Incognito tabs never touch the shared profile and are not restored.
    pub incognito: bool,
    /// Favicon url reported by the page, if any.
    pub icon: Option<String>,
    pub loading: bool,
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

/// Returns None when the text cannot be turned into a web address, so the UI can
/// say so instead of silently loading a blank page.
pub fn parse_url(input: &str) -> Option<Url> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return "about:blank".parse().ok();
    }
    if let Ok(url) = trimmed.parse::<Url>() {
        return Some(url);
    }
    // A bare host like "example.com" is a common way to type an address.
    if !trimmed.contains(char::is_whitespace) && trimmed.contains('.') {
        if let Ok(url) = format!("https://{trimmed}").parse::<Url>() {
            return Some(url);
        }
    }
    None
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
    let owner = crate::ipc::window_of_tab(id).to_string();
    let target = parse_url(url).unwrap_or_else(|| "about:blank".parse().expect("static url"));
    let builder = WebviewBuilder::new(id, WebviewUrl::External(target))
        // WebView2 keeps ctrl+wheel zoom off unless asked (M0 E2).
        .zoom_hotkeys_enabled(true)
        // Without this wry leaves AreDevToolsEnabled off and open_devtools is a no-op.
        .devtools(true)
        // Form autofill stands in for a password vault: WebView2 does not expose one.
        .general_autofill_enabled(true)
        // Incognito tabs get their own store and never reach the shared profile.
        .incognito(incognito)
        .on_download(crate::downloads::handle)
        // target=_blank and window.open land here. Creating a webview on this
        // thread would deadlock, so the chrome ui opens the tab for us.
        .on_new_window(move |url, _features| {
            // Only the window that owns this tab should open the new one.
            let _ = app.emit_to(owner.clone(), "ui:open-url", url.to_string());
            tauri::webview::NewWindowResponse::Deny
        })
        .initialization_script(SHORTCUT_FORWARDER)
        // Page load events drive the tab spinner.
        .on_page_load(|webview, payload| {
            let app = webview.app_handle().clone();
            let id = webview.label().to_string();
            let loading = matches!(payload.event(), tauri::webview::PageLoadEvent::Started);
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
            if let Some(state) = app.try_state::<crate::ipc::AppState>() {
                let window = crate::ipc::window_of_tab(&payload.id);
                if let Some(mut core) = state.try_write_core(window) {
                    if let Some(tab) = core.tabs.iter_mut().find(|t| t.id == payload.id) {
                        tab.title = title;
                    }
                }
            }
            let owner = crate::ipc::window_of_tab(&payload.id).to_string();

            // The window title follows the selected tab, like any browser.
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

    win.add_child(
        builder,
        LogicalPosition::new(0.0, CHROME_H),
        LogicalSize::new(width.max(1.0), height.max(1.0)),
    )
}