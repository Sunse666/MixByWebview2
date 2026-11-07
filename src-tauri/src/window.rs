use std::collections::HashMap;
use std::time::Duration;

use tauri::{
    webview::WebviewBuilder, AppHandle, Emitter, LogicalPosition, LogicalSize, Manager, Webview,
    WebviewUrl, Window, WindowEvent,
};

use crate::ipc::AppState;
use crate::tabs;

pub const CHROME_H: f64 = 82.0;

pub fn logical_size(win: &Window) -> tauri::Result<(f64, f64)> {
    let size = win.inner_size()?;
    let scale = win.scale_factor()?;
    Ok((size.width as f64 / scale, size.height as f64 / scale))
}

pub fn set_bounds(webview: &Webview, x: f64, y: f64, w: f64, h: f64) -> tauri::Result<()> {
    webview.set_bounds(tauri::Rect {
        position: LogicalPosition::new(x, y).into(),
        size: LogicalSize::new(w.max(1.0), h.max(1.0)).into(),
    })
}

pub const PRIMARY: &str = "w1";

/// The MIX_* hooks exist for tests and manual probing. They are inert unless
/// this switch is set, so a release build cannot be driven by a stray variable.
fn debug_hooks() -> bool {
    std::env::var("MIX_DEBUG").is_ok_and(|value| value != "0")
}

pub fn bootstrap(app: &mut tauri::App) -> tauri::Result<()> {
    let (start_url, restore, session_file) = {
        let state = app.state::<AppState>();
        let settings = state.settings.read();
        (
            settings.start_url.clone(),
            settings.restore_session,
            state.dirs.session_file.clone(),
        )
    };

    // A url on the command line wins: that is what a default browser receives.
    let shell_urls = initial_urls();
    if !shell_urls.is_empty() {
        let tabs: Vec<(String, f64, bool)> =
            shell_urls.into_iter().map(|url| (url, 0.0, false)).collect();
        open_window(app.handle(), PRIMARY, tabs, 0, None)?;
        crate::downloads::start_progress_watch(app.handle().clone());
        crate::snap::install_hotkeys(app.handle());
        crate::tray::install(&app.handle().clone())?;
        return Ok(());
    }

    let restored = if restore {
        crate::session::Session::load(&session_file)
    } else {
        None
    };

    match restored {
        Some(session) => {
            for (index, window) in session.windows.iter().enumerate() {
                let label = if index == 0 {
                    PRIMARY.to_string()
                } else {
                    format!("w{}", index + 1)
                };
                let selected = window.selected.min(window.tabs.len().saturating_sub(1));
                let tabs: Vec<(String, f64, bool)> = window
                    .tabs
                    .iter()
                    .map(|tab| (tab.url().to_string(), tab.scroll(), tab.pinned()))
                    .collect();
                open_window(app.handle(), &label, tabs, selected, window.window.clone())?;
            }
        }
        None => {
            open_window(app.handle(), PRIMARY, vec![(start_url, 0.0, false)], 0, None)?;
        }
    }

    crate::downloads::start_progress_watch(app.handle().clone());
    crate::snap::install_hotkeys(app.handle());
    debug_ext_toggle(app.handle());
    debug_gesture_test(app.handle());
    crate::tray::install(&app.handle().clone())?;
    // Debug helper: MIX_OPEN_ON_START=settings opens that overlay shortly after
    // startup. Opening it inside setup deadlocks the page: the main thread is
    // still creating webviews, so its invoke never resolves.
    #[cfg(debug_assertions)]
    if debug_hooks() {
        if let Ok(kind) = std::env::var("MIX_OPEN_ON_START") {
        let handle = app.handle().clone();
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(1500));
            let target = handle.clone();
            let _ = handle.run_on_main_thread(move || {
                let Some(win) = target.get_window(PRIMARY) else {
                    return;
                };
                if let Err(err) = crate::overlay::open(&target, &win, &kind) {
                    crate::logging::warn(&format!("debug overlay failed: {err}"));
                }
            });
        });
        }
    }

    #[cfg(debug_assertions)]
    if debug_hooks() && std::env::var("MIX_DIAG_ON_START").is_ok() {
        let handle = app.handle().clone();
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(2500));
            match crate::diag::export(&handle) {
                Ok(path) => crate::logging::info(&format!("debug diag: {}", path.display())),
                Err(err) => crate::logging::warn(&format!("debug diag failed: {err}")),
            }
        });
    }

    // Debug helper: MIX_FULLSCREEN=1 reports a fullscreen page, so the strip hiding
    // can be checked without a video that asks for it.
    if debug_hooks() && std::env::var("MIX_FULLSCREEN").is_ok() {
        let handle = app.handle().clone();
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(2500));
            let target = handle.clone();
            let _ = handle.run_on_main_thread(move || {
                let state = target.state::<AppState>();
                state.fullscreen.write().insert(PRIMARY.to_string());
                let _ = relayout(&target, PRIMARY);
                crate::logging::info("debug: page marked fullscreen");
            });
        });
    }

    // Debug helper: MIX_NEW_WINDOW=1 opens a second window after startup so the
    // multi window path can be checked without a click.
    #[cfg(debug_assertions)]
    if debug_hooks() && std::env::var("MIX_NEW_WINDOW").is_ok() {
        let handle = app.handle().clone();
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(2000));
            let target = handle.clone();
            let _ = handle.run_on_main_thread(move || {
                let state = target.state::<AppState>();
                let url = state.settings.read().start_url.clone();
                drop(state);
                match crate::window::open_window(&target, "w2", vec![(url, 0.0, false)], 0, None) {
                    Ok(_) => crate::logging::info("debug: second window opened"),
                    Err(err) => crate::logging::warn(&format!("debug window failed: {err}")),
                }
            });
        });
    }

    // Support hook: MIX_TRANSLATE=1 runs the translator on the current page.
    if debug_hooks() && std::env::var("MIX_TRANSLATE").is_ok() {
        let handle = app.handle().clone();
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(3000));
            let repeats: u32 = std::env::var("MIX_TRANSLATE")
                .ok()
                .and_then(|value| value.parse().ok())
                .unwrap_or(1);
            for round in 0..repeats {
                std::thread::sleep(std::time::Duration::from_millis(if round == 0 { 0 } else { 5000 }));
                let target = handle.clone();
                let _ = handle.run_on_main_thread(move || {
                    let Some(win) = target.get_window(PRIMARY) else {
                        return;
                    };
                    let state = target.state::<AppState>();
                    let call = crate::ipc::translate_page_inplace(target.clone(), win, state);
                    match tauri::async_runtime::block_on(call) {
                        Ok(()) => crate::logging::info("debug translate: started"),
                        Err(err) => crate::logging::warn(&format!("debug translate failed: {}", err.message)),
                    }
                });
            }
        });
    }

    // The translator needs a token and a cookie; fetch them before they are needed.
    tauri::async_runtime::spawn(async {
        crate::translate::warm().await;
    });

    // Support hook: MIX_DOM_PUSH=1 appends a paragraph later, so the observer that
    // picks up dynamic content can be exercised without a real feed.
    if debug_hooks() && std::env::var("MIX_DOM_PUSH").is_ok() {
        let handle = app.handle().clone();
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(9000));
            let target = handle.clone();
            let _ = handle.run_on_main_thread(move || {
                let state = target.state::<AppState>();
                let Some(win) = target.get_window(PRIMARY) else {
                    return;
                };
                let label = crate::ipc::selected_label(&state, &win);
                if let Some(webview) = target.get_webview(&label) {
                    let _ = webview.eval(
                        "(function(){var p=document.createElement('p');p.textContent='A paragraph injected by the test hook.';document.body.appendChild(p);})()",
                    );
                }
            });
        });
    }

    // Support hook: MIX_TOGGLE_EXT=1 hides the first extension after startup, so
    // the live settings path can be checked without a click.
    if debug_hooks() && std::env::var("MIX_TOGGLE_EXT").is_ok() {
        let handle = app.handle().clone();
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(4000));
            let target = handle.clone();
            let _ = handle.run_on_main_thread(move || {
                let name = {
                    let state = target.state::<AppState>();
                    state
                        .extensions
                        .iter()
                        .find(|ext| ext.status == "loaded")
                        .map(|ext| ext.name.clone())
                };
                let Some(name) = name else {
                    return;
                };
                crate::logging::info(&format!("debug: hiding {name}"));
                let Some(win) = target.get_window(PRIMARY) else {
                    return;
                };
                let state = target.state::<AppState>();
                let patch = crate::ipc::SettingsPatch {
                    hidden_extensions: Some(vec![name]),
                    ..Default::default()
                };
                let call = crate::ipc::settings_set(win, state, patch);
                let _ = tauri::async_runtime::block_on(call);
            });
        });
    }

    // Support hook: MIX_SAVE_URL=<url> saves that file, so the download path
    // can be checked without a right click.
    if debug_hooks() {
        if let Ok(url) = std::env::var("MIX_SAVE_URL") {
        let handle = app.handle().clone();
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(2500));
            let target = handle.clone();
            let _ = handle.run_on_main_thread(move || {
                let state = target.state::<AppState>();
                let call = crate::ipc::download_url(target.clone(), state, url, None);
                match tauri::async_runtime::block_on(call) {
                    Ok(path) => crate::logging::info(&format!("debug save: {path}")),
                    Err(err) => crate::logging::warn(&format!("debug save failed: {}", err.message)),
                }
            });
        });
        }
    }

    // Support hook: MIX_NAV=1 navigates the first tab to a dead host, so the
    // "unreachable" path can be checked without typing.
    if debug_hooks() && std::env::var("MIX_NAV").is_ok() {
        let handle = app.handle().clone();
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(2500));
            let target = handle.clone();
            let _ = handle.run_on_main_thread(move || {
                let Some(win) = target.get_window(PRIMARY) else {
                    return;
                };
                let state = target.state::<AppState>();
                let call = crate::ipc::tab_navigate(
                    target.clone(),
                    win,
                    state,
                    "https://nonexistent-host-mix-test.invalid".to_string(),
                );
                let _ = tauri::async_runtime::block_on(call);
            });
        });
    }

    // Support hook: MIX_ERROR_PAGE=1 shows the error page so it can be reviewed.
    if debug_hooks() && std::env::var("MIX_ERROR_PAGE").is_ok() {
        let handle = app.handle().clone();
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(2500));
            let target = handle.clone();
            let _ = handle.run_on_main_thread(move || {
                if let Some(state) = target.try_state::<AppState>() {
                    let id = state
                        .try_read_core(PRIMARY)
                        .and_then(|core| core.selected.clone())
                        .unwrap_or_default();
                    crate::ipc::show_error_page(&target, &id, "hello world");
                }
            });
        });
    }

    // Support hook: MIX_DEVTOOLS=1 runs the devtools shortcut, so a broken F12
    // can be told apart from a broken key path.
    if debug_hooks() && std::env::var("MIX_DEVTOOLS").is_ok() {
        let handle = app.handle().clone();
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(2500));
            let target = handle.clone();
            let _ = handle.run_on_main_thread(move || {
                let Some(win) = target.get_window(PRIMARY) else {
                    return;
                };
                let state = target.state::<AppState>();
                let call = crate::ipc::shortcut(
                    target.clone(),
                    win,
                    state,
                    "devtools".to_string(),
                );
                let _ = tauri::async_runtime::block_on(call);
            });
        });
    }
    crate::logging::info(&format!(
        "main window ready, startup {} ms",
        crate::logging::elapsed_ms()
    ));
    Ok(())
}

/// Creates a window with its chrome strip and one webview per url.
/// A url handed over on the command line, e.g. when the app is the default
/// browser and the shell runs "mix.exe https://example.com".
pub fn initial_urls() -> Vec<String> {
    std::env::args()
        .skip(1)
        .filter(|arg| arg.starts_with("http://") || arg.starts_with("https://"))
        .collect()
}

pub fn open_window(
    app: &tauri::AppHandle,
    label: &str,
    urls: Vec<(String, f64, bool)>,
    selected: usize,
    geometry: Option<crate::session::WindowGeometry>,
) -> tauri::Result<Window> {
    let state = app.state::<AppState>();
    state.add_window(label);
    let opts = state.opts.clone();
    drop(state);

    let win = Window::builder(app, label)
        .title("Mix")
        .inner_size(1100.0, 760.0)
        .min_inner_size(500.0, 600.0)
        .decorations(false)
        // Rounded corners need a transparent window outside Windows 11.
        .transparent(cfg!(any(target_os = "macos", target_os = "linux")))
        .visible(false)
        .build()?;

    crate::platform::install_resize_borders(&win);

    // Geometry from the last run, kept in session.json so it travels with the folder.
    if let Some(geometry) = geometry {
        let _ = win.set_position(tauri::PhysicalPosition::new(geometry.x, geometry.y));
        let _ = win.set_size(tauri::PhysicalSize::new(
            geometry.width.max(400),
            geometry.height.max(300),
        ));
        if geometry.maximized {
            let _ = win.maximize();
        }
        crate::logging::info(&format!(
            "restored window {}x{} at {},{}",
            geometry.width, geometry.height, geometry.x, geometry.y
        ));
    }

    let (width, height) = logical_size(&win)?;

    {
        let state = app.state::<AppState>();
        let mut core = state.write_core(label);
        core.next_tab_id = urls.len();
        core.tick = urls.len() as u64;
        core.tabs = urls
            .iter()
            .enumerate()
            .map(|(index, (url, scroll, pinned))| tabs::Tab {
                id: crate::ipc::tab_label(label, index + 1),
                url: url.clone(),
                title: "New tab".into(),
                zoom: 1.0,
                pinned: *pinned,
                incognito: false,
                icon: None,
                loading: false,
                scroll: *scroll,
                alive: true,
                last_active: index as u64,
            })
            .collect();
        core.selected = Some(crate::ipc::tab_label(label, selected + 1));
        // Recency starts in strip order with the selected tab first.
        core.mru = core.tabs.iter().map(|tab| tab.id.clone()).collect();
        if let Some(id) = core.selected.clone() {
            core.mru.retain(|entry| entry != &id);
            core.mru.insert(0, id);
        }
    }

    let chrome_builder = tabs::apply_profile(
        WebviewBuilder::new(
            format!("{label}-chrome"),
            WebviewUrl::App("index.html".into()),
        )
        .devtools(true),
        &opts.ui_data_dir,
    );

    let fullscreen = app
        .try_state::<AppState>()
        .map(|state| state.fullscreen.read().contains(label))
        .unwrap_or(false);
    let chrome_h = if fullscreen {
        0.0
    } else {
        crate::ipc::chrome_height(app)
    };
    win.add_child(
        chrome_builder,
        LogicalPosition::new(0.0, 0.0),
        LogicalSize::new(width, chrome_h),
    )?;

    // The strip renders at 1:1 inside a DPI scaled box; zoom is what makes the
    // two agree, otherwise a band of empty panel colour is left at the bottom.
    if let Some(chrome) = app.get_webview(&format!("{label}-chrome")) {
        let scale = win.scale_factor().unwrap_or(1.0);
        if (scale - 1.0).abs() > f64::EPSILON {
            let _ = chrome.set_zoom(scale);
        }
    }



    for (index, (url, _scroll, _pinned)) in urls.iter().enumerate() {
        let id = crate::ipc::tab_label(label, index + 1);
        tabs::create(&win, &id, url, width, height - chrome_h, &opts, false)?;
    }

    let handle = app.clone();
    let owner = label.to_string();
    win.on_window_event(move |event| match event {
        WindowEvent::Resized(_) | WindowEvent::ScaleFactorChanged { .. } => {
            if let Err(err) = relayout(&handle, &owner) {
                crate::logging::warn(&format!("relayout failed: {err}"));
            }
        }
        // The primary window hides to the tray; extra windows just close.
        WindowEvent::CloseRequested { api, .. } => {
            crate::ipc::save_sessions(&handle);
            if owner == PRIMARY {
                api.prevent_close();
                if let Some(win) = handle.get_window(PRIMARY) {
                    let _ = win.hide();
                }
                crate::logging::info("window hidden to tray");
            } else {
                if let Some(state) = handle.try_state::<AppState>() {
                    state.remove_window(&owner);
                }
                crate::logging::info(&format!("window {owner} closed"));
            }
        }
        _ => {}
    });

    crate::logging::info(&format!("tabs ready: {} in {label}", urls.len()));
    crate::ipc::enforce_live_limit(app, label);
    crate::ipc::sync_visibility(app, label);
    crate::ipc::emit_tabs(app, label);
    start_url_watch(app.clone(), label.to_string());

    win.show()?;
    win.set_focus()?;
    // Persist right away so a window opened now survives a kill or a crash.
    crate::ipc::save_sessions(app);
    Ok(win)
}

/// Tauri exposes no navigation event, so the address bar is kept honest by
/// polling the selected webviews (only the ones that are attached).
pub fn start_url_watch(app: AppHandle, label: String) {
    std::thread::spawn(move || {
        let mut last: HashMap<String, String> = HashMap::new();
        loop {
            std::thread::sleep(Duration::from_millis(400));
            if app.get_window(&label).is_none() {
                return;
            }

            // The bundled pages live on whatever scheme this runtime picked.
            if let Some(state) = app.try_state::<AppState>() {
                if state.app_base.read().is_empty() {
                    if let Some(chrome) = app.get_webview(&format!("{label}-chrome")) {
                        if let Ok(url) = chrome.url() {
                            let origin = url.origin().ascii_serialization();
                            if origin.starts_with("http") {
                                *state.app_base.write() = format!("{origin}/");
                                crate::logging::info(&format!("app base {origin}/"));
                            }
                        }
                    }
                }
            }

            let ids: Vec<String> = match app.try_state::<AppState>() {
                Some(state) => match state.try_read_core(&label) {
                    Some(core) => core.tabs.iter().map(|tab| tab.id.clone()).collect(),
                    None => return,
                },
                None => return,
            };

            for id in ids {
                let Some(webview) = app.get_webview(&id) else {
                    continue;
                };
                let Ok(url) = webview.url() else {
                    continue;
                };
                let current = url.to_string();
                if current.starts_with("chrome-error://") {
                    // WebView2's own error page: replace it with ours.
                    if let Some(state) = app.try_state::<AppState>() {
                        let failed = state
                            .pending
                            .read()
                            .unwrap_or_else(|e| e.into_inner())
                            .get(&id)
                            .cloned()
                            .unwrap_or_default();
                        if !failed.is_empty() {
                            crate::ipc::show_error_page(&app, &id, &failed);
                        }
                    }
                    continue;
                }
                if last.get(&id).map(String::as_str) == Some(current.as_str()) {
                    continue;
                }
                last.insert(id.clone(), current.clone());
                publish_url(&app, &id, &current);
            }
        }
    });
}

fn publish_url(app: &AppHandle, id: &str, url: &str) {
    if let Some(state) = app.try_state::<AppState>() {
        let window = crate::ipc::window_of_tab(id);
        if let Some(mut core) = state.try_write_core(window) {
            if let Some(tab) = core.tabs.iter_mut().find(|tab| tab.id == id) {
                tab.url = url.to_string();
            }
        }
    }
    let title = {
        let state = app.state::<AppState>();
        let core = state.read_core(crate::ipc::window_of_tab(id));
        core.tabs
            .iter()
            .find(|tab| tab.id == id)
            .map(|tab| tab.title.clone())
            .unwrap_or_default()
    };
    crate::ipc::record_visit(app, url, &title);

    let _ = app.emit(
        "tab:url",
        crate::ipc::UrlPayload {
            id: id.to_string(),
            url: url.to_string(),
        },
    );
}

/// Bring the window back from the tray or from a second instance.
pub fn show_or_focus(app: &AppHandle) {
    if let Some(win) = app.get_window("w1") {
        let _ = win.unminimize();
        let _ = win.show();
        let _ = win.set_focus();
    }
}

pub fn relayout(app: &AppHandle, label: &str) -> Result<(), tauri::Error> {
    let Some(win) = app.get_window(label) else {
        return Ok(());
    };
    let (width, height) = logical_size(&win)?;
    // A fullscreen page owns the whole window, strip included.
    let fullscreen = app
        .try_state::<AppState>()
        .is_some_and(|state| state.fullscreen.read().contains(label));
    let chrome_h = if fullscreen {
        0.0
    } else {
        crate::ipc::chrome_height(app)
    };
    let tab_height = (height - chrome_h).max(1.0);

    if let Some(chrome) = app.get_webview(&format!("{label}-chrome")) {
        // A zero height webview keeps its last frame; hide it instead.
        if fullscreen {
            let _ = chrome.hide();
        } else {
            let _ = chrome.show();
        }
        set_bounds(&chrome, 0.0, 0.0, width, chrome_h)?;
    }

    let ids: Vec<String> = match app.try_state::<AppState>() {
        Some(state) => match state.try_read_core(label) {
            Some(core) => core.tabs.iter().map(|tab| tab.id.clone()).collect(),
            None => Vec::new(),
        },
        None => Vec::new(),
    };
    for id in ids {
        if let Some(webview) = app.get_webview(&id) {
            set_bounds(&webview, 0.0, chrome_h, width, tab_height)?;
        }
    }

    let _ = app.emit_to(label, "layout:changed", (width, height));
    Ok(())
}

/// Debug hook: toggles one extension and logs the outcome, so the COM path can
/// be checked without clicking. Inert unless MIX_DEBUG is set.
fn debug_ext_toggle(app: &tauri::AppHandle) {
    if !debug_hooks() {
        return;
    }
    let Ok(wanted) = std::env::var("MIX_EXT_TOGGLE") else {
        return;
    };
    let handle = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_secs(3));
        let Some(webview) = handle.get_webview("w1-tab-1") else {
            return;
        };
        let before = crate::ext_live::list(&webview);
        let Some(target) = before.iter().find(|item| item.name.contains(&wanted)) else {
            crate::logging::info(&format!("ext toggle: no match for {wanted}"));
            return;
        };
        let ok = crate::ext_live::set_enabled(&webview, &target.id, !target.enabled);
        crate::logging::info(&format!("ext toggle {} -> {}: {ok}", target.name, !target.enabled));
        let after = crate::ext_live::list(&webview);
        if let Some(seen) = after.iter().find(|item| item.id == target.id) {
            crate::logging::info(&format!("ext toggle after: enabled={}", seen.enabled));
        }
    });
}

/// Debug hook: runs a synthetic left gesture in the active tab and logs which
/// action reached the backend. Inert unless MIX_DEBUG is set.
fn debug_gesture_test(app: &tauri::AppHandle) {
    if !debug_hooks() {
        return;
    }
    if std::env::var("MIX_GESTURE_TEST").is_err() {
        return;
    }
    let handle = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_secs(4));
        let Some(webview) = handle.get_webview("w1-tab-1") else {
            return;
        };
        let script = r#"(function(){
          function fire(type, x, y) {
            var event = new MouseEvent(type, { bubbles: true, cancelable: true, button: 2, buttons: 2, clientX: x, clientY: y });
            window.dispatchEvent(event);
          }
          fire('mousedown', 600, 400);
          fire('mousemove', 520, 400);
          fire('mousemove', 440, 400);
          fire('mouseup', 380, 400);
          return 'gesture synthetic done';
        })();"#;
        match webview.eval(script) {
            Ok(()) => crate::logging::info("gesture test: synthetic left drag sent"),
            Err(err) => crate::logging::warn(&format!("gesture test failed: {err}")),
        }
    });
}








