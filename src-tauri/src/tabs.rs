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

// In-place page translation: the page owns the DOM, Rust owns the network.
(function () {
  var nodes = [];
  // Nodes already handled, so later passes only pick up new text.
  var done = new WeakSet();
  var observer = null;
  var running = false;
  var timer = null;

  // Same origin frames share the document tree with us; cross origin ones throw.
  var documents = function () {
    var list = [document];
    var frames = document.querySelectorAll("iframe, frame");
    for (var i = 0; i < frames.length; i++) {
      try {
        var doc = frames[i].contentDocument;
        if (doc && doc.body) list.push(doc);
      } catch (error) {
        /* cross origin */
      }
    }
    return list;
  };

  var collect = function () {
    nodes = [];
    var seen = new Set();
    var total = 0;
    var docs = documents();

    for (var d = 0; d < docs.length; d++) {
      var walker = docs[d].createTreeWalker(docs[d].body, NodeFilter.SHOW_TEXT, {
        acceptNode: function (node) {
          var text = node.nodeValue || "";
          if (text.trim().length < 2) return NodeFilter.FILTER_REJECT;
          var parent = node.parentElement;
          if (!parent) return NodeFilter.FILTER_REJECT;
          if (/^(SCRIPT|STYLE|NOSCRIPT|TEXTAREA|CODE|PRE|KBD|SAMP)$/.test(parent.tagName)) {
            return NodeFilter.FILTER_REJECT;
          }
          return NodeFilter.FILTER_ACCEPT;
        },
      });

      while (walker.nextNode()) {
        var node = walker.currentNode;
        var text = node.nodeValue;
        if (done.has(node)) continue;
        if (seen.has(text)) continue;
        if (total + text.length > 6000) break;
        seen.add(text);
        total += text.length;
        nodes.push({ node: node, original: text });
        if (nodes.length >= 400) break;
      }
    }

    return nodes.map(function (item) {
      return item.original;
    });
  };

  var apply = function (translated) {
    var applied = 0;
    for (var i = 0; i < nodes.length; i++) {
      var replacement = translated[i];
      if (!replacement) continue;
      if (replacement === nodes[i].original) continue;
      nodes[i].node.nodeValue = replacement;
      done.add(nodes[i].node);
      applied++;
    }
    return applied;
  };

  var restore = function () {
    for (var i = 0; i < nodes.length; i++) {
      nodes[i].node.nodeValue = nodes[i].original;
    }
    nodes = [];
    done = new WeakSet();
  };

  var rounds = function (total) {
    var texts = collect();
    if (!texts.length) {
      window.__TAURI_INTERNALS__.invoke("translate_done", {
        count: total,
        error: total === 0 ? "no translatable text" : null,
      });
      running = false;
      return;
    }

    window.__TAURI_INTERNALS__
      .invoke("translate_texts", { texts: texts })
      .then(function (translated) {
        var applied = apply(translated || []);
        total += applied;
        if (applied === 0) {
          window.__TAURI_INTERNALS__.invoke("translate_done", {
            count: total,
            error: total === 0 ? "translation returned nothing" : null,
          });
          running = false;
          return;
        }
        setTimeout(function () {
          rounds(total);
        }, 50);
      })
      .catch(function (err) {
        window.__TAURI_INTERNALS__.invoke("translate_done", { count: total, error: String(err) });
        running = false;
      });
  };

  // New content (infinite scroll, SPA updates) is picked up after it settles.
  var watch = function () {
    if (observer) return;
    var notify = function () {
      if (timer) clearTimeout(timer);
      timer = setTimeout(function () {
        if (!running) window.__mixStartTranslate();
      }, 900);
    };
    observer = new MutationObserver(notify);
    var docs = documents();
    for (var i = 0; i < docs.length; i++) {
      observer.observe(docs[i].body, { childList: true, subtree: true });
    }
  };

  window.__mixStartTranslate = function () {
    if (running) return;
    running = true;
    rounds(0);
    watch();
  };

  window.__mixStopTranslate = function () {
    if (observer) {
      observer.disconnect();
      observer = null;
    }
    if (timer) clearTimeout(timer);
    running = false;
    restore();
    window.__TAURI_INTERNALS__.invoke("translate_done", { count: 0 });
  };

  window.__mixTranslate = { collect: collect, apply: apply, restore: restore };
})();

// Video players ask the document for fullscreen; the strip has to follow.
(function () {
  var report = function (on) {
    window.__TAURI_INTERNALS__.invoke("page_fullscreen", { on: on }).catch(function () {});
  };

  document.addEventListener("fullscreenchange", function () {
    report(Boolean(document.fullscreenElement));
  });

  // Safari style prefix, still used by some players.
  document.addEventListener("webkitfullscreenchange", function () {
    report(Boolean(document.webkitFullscreenElement));
  });
})();

// WebView2 renders its own error page; detect it so we can show ours instead.
(function () {
  var check = function () {
    try {
      var text = document.body ? document.body.innerText || "" : "";
      var match = /ERR_[A-Z_]+/.exec(text);
      if (!match) return;
      window.__TAURI_INTERNALS__.invoke("page_failed", { code: match[0] }).catch(function () {});
    } catch (error) {
      /* ignore */
    }
  };
  document.addEventListener("DOMContentLoaded", function () {
    setTimeout(check, 600);
  });
})();

// Scroll position is remembered per tab so switching back does not jump to top.
(function () {
  var timer = null;
  window.addEventListener("scroll", function () {
    if (timer) return;
    timer = setTimeout(function () {
      timer = null;
      window.__TAURI_INTERNALS__.invoke("tab_scroll", { y: window.scrollY }).catch(function () {});
    }, 400);
  }, { passive: true });
})();

// While the address bar owns the keyboard, keys are forwarded to it. This does
// not depend on which webview has OS focus.
(function () {
  var forward = function (kind, value) {
    var payload = kind === "type" ? { text: value } : { key: value };
    var command = kind === "type" ? "url_type" : "url_key";
    window.__TAURI_INTERNALS__.invoke(command, payload).catch(function () {});
  };

  window.addEventListener("keydown", function (event) {
    if (!window.__mixUrlMode) return;
    if (event.ctrlKey || event.altKey || event.metaKey) return;
    if (event.key.length === 1) {
      forward("type", event.key);
      event.preventDefault();
      return;
    }
    if (event.key === "Backspace") {
      forward("type", "\u0008");
      event.preventDefault();
      return;
    }
    if (event.key === "Enter") {
      forward("key", "enter");
      event.preventDefault();
      return;
    }
    if (event.key === "Escape") {
      forward("key", "escape");
      event.preventDefault();
    }
  }, true);
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
    /// Pinned tabs stay at the left and keep a narrow strip.
    pub pinned: bool,
    /// Incognito tabs never touch the shared profile and are not restored.
    pub incognito: bool,
    /// Favicon url reported by the page, if any.
    pub icon: Option<String>,
    pub loading: bool,
    /// Last scroll offset, restored when the tab becomes visible again.
    pub scroll: f64,
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
    let nav_app = app.clone();
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
        // Remember what we are loading, so a failed load can say which url died.
        .on_navigation({
            let tab_id = id.to_string();
            let app = nav_app;
            move |url| {
                if let Some(state) = app.try_state::<crate::ipc::AppState>() {
                    state
                        .pending
                        .write()
                        .unwrap_or_else(|e| e.into_inner())
                        .insert(tab_id.clone(), url.to_string());
                }
                true
            }
        })
        // Page load events drive the tab spinner.
        .on_page_load(|webview, payload| {
            let app = webview.app_handle().clone();
            let id = webview.label().to_string();
            let loading = matches!(payload.event(), tauri::webview::PageLoadEvent::Started);

            // Zoom is remembered per site, so it has to be reapplied on every load.
            if !loading {
                // Restoring the offset has to wait until the document exists.
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

    // The caller already subtracted the chrome height; place below it.
    let top = win
        .inner_size()
        .map(|size| size.to_logical::<f64>(win.scale_factor().unwrap_or(1.0)).height)
        .map(|full| (full - height).max(0.0))
        .unwrap_or(0.0);
    win.add_child(
        builder,
        LogicalPosition::new(0.0, top),
        LogicalSize::new(width.max(1.0), height.max(1.0)),
    )
}