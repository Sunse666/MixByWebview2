use std::collections::HashMap;
use std::time::Duration;

use tauri::{
    webview::WebviewBuilder, AppHandle, Emitter, LogicalPosition, LogicalSize, Manager, Webview,
    WebviewUrl, Window, WindowEvent,
};

use crate::ipc::AppState;
use crate::tabs;

pub const CHROME_H: f64 = 86.0;

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

/// Debug hooks, gated on MIX_DEBUG.
#[cfg(debug_assertions)]
fn debug_hooks() -> bool {
    std::env::var("MIX_DEBUG").is_ok_and(|value| value != "0")
}



/// Rebuilds the tab groups a session recorded.
fn apply_groups(app: &AppHandle, label: &str, groups: &[crate::session::SessionGroup]) {
    if groups.is_empty() {
        return;
    }
    let state = app.state::<AppState>();
    let mut core = state.write_core(label);
    for group in groups {
        let members: Vec<String> = group
            .members
            .iter()
            .filter_map(|index| core.tabs.get(*index).map(|tab| tab.id.clone()))
            .collect();
        if members.is_empty() {
            continue;
        }
        let id = format!("g{}", core.next_group_id + 1);
        core.next_group_id += 1;
        core.groups.push(crate::ipc::TabGroup {
            id,
            name: group.name.clone(),
            color: group.color.clone(),
            collapsed: group.collapsed,
            members,
        });
    }
}
/// What a popped-out window was told to open: the tab, its offset, and the geometry of
/// the window it came from.
struct InstanceRequest {
    url: String,
    scroll: f64,
    pinned: bool,
    geometry: Option<crate::session::WindowGeometry>,
}

fn arg_value(args: &[String], name: &str) -> Option<String> {
    args.iter()
        .position(|arg| arg == name)
        .and_then(|index| args.get(index + 1))
        .cloned()
}

fn instance_request() -> Option<InstanceRequest> {
    if !crate::paths::instance_mode() {
        return None;
    }
    let args: Vec<String> = std::env::args().collect();
    let url = args
        .iter()
        .find(|arg| arg.starts_with("http://") || arg.starts_with("https://"))?
        .clone();
    let scroll = arg_value(&args, "--scroll")
        .and_then(|value| value.parse::<f64>().ok())
        .unwrap_or(0.0);
    let pinned = args.iter().any(|arg| arg == "--pinned");
    let geometry = arg_value(&args, "--bounds").and_then(|value| {
        let parts: Vec<i64> = value
            .split(',')
            .filter_map(|part| part.trim().parse::<i64>().ok())
            .collect();
        if parts.len() != 4 {
            return None;
        }
        Some(crate::session::WindowGeometry {
            x: parts[0] as i32,
            y: parts[1] as i32,
            width: parts[2].max(400) as u32,
            height: parts[3].max(300) as u32,
            maximized: false,
        })
    });
    Some(InstanceRequest {
        url,
        scroll,
        pinned,
        geometry,
    })
}

/// The tray and the global hotkeys belong to one process: the first window that starts.
/// A window dragged out of it runs on its own and leaves these alone.
fn install_owner_ui(app: &tauri::AppHandle) -> tauri::Result<()> {
    if crate::paths::instance_mode() {
        return Ok(());
    }
    crate::snap::install_hotkeys(app);
    crate::tray::install(app)?;
    Ok(())
}

pub fn bootstrap(app: &mut tauri::App) -> tauri::Result<()> {
    let (start_url, restore, session_file, user_data) = {
        let state = app.state::<AppState>();
        let settings = state.settings.read();
        (
            settings.start_url.clone(),
            settings.restore_session,
            state.dirs.session_file.clone(),
            state.dirs.user_data.clone(),
        )
    };

    // A window that was dragged out of another one is told what to open, how big to be
    // and where to sit, so it takes over the tab the way it looked in the old window.
    if let Some(request) = instance_request() {
        open_window(
            app.handle(),
            PRIMARY,
            vec![(request.url, request.scroll, request.pinned)],
            0,
            request.geometry,
        )?;
        crate::downloads::start_progress_watch(app.handle().clone());
        crate::persist::spawn_history_flusher(app.handle().clone());
        crate::audio::watch(app.handle().clone());
        install_owner_ui(app.handle())?;
        return Ok(());
    }

    let shell_urls = initial_urls();
    if !shell_urls.is_empty() {
        let tabs: Vec<(String, f64, bool)> =
            shell_urls.into_iter().map(|url| (url, 0.0, false)).collect();
        open_window(app.handle(), PRIMARY, tabs, 0, None)?;
        crate::downloads::start_progress_watch(app.handle().clone());
        crate::persist::spawn_history_flusher(app.handle().clone());
        crate::audio::watch(app.handle().clone());
        install_owner_ui(app.handle())?;
        return Ok(());
    }

    // The window that closed last wins: every process leaves its final state behind,
    // and the newest of those is what the user was looking at when they quit.
    let restored = if restore {
        crate::session::latest_closed(&user_data).or_else(|| crate::session::Session::load(&session_file))
    } else {
        None
    };

    match &restored {
        Some(session) => crate::logging::info(&format!(
            "session restore: {} window(s), {} tab(s)",
            session.windows.len(),
            session.windows.iter().map(|window| window.tabs.len()).sum::<usize>()
        )),
        None => crate::logging::info("session restore: nothing to restore"),
    }

    // One window per launch: every other window is its own process now, so a restart
    // brings back the window it started from instead of rebuilding a wall of them.
    let primary = restored.filter(|session| !session.windows.is_empty());
    match primary {
        Some(session) => {
            if session.windows.len() > 1 {
                crate::logging::info(&format!(
                    "session restore: keeping the main window, dropping {}",
                    session.windows.len() - 1
                ));
            }
            let keep = &session.windows[0];
            let selected = keep.selected.min(keep.tabs.len().saturating_sub(1));
            let tabs: Vec<(String, f64, bool)> = keep
                .tabs
                .iter()
                .map(|tab| (tab.url().to_string(), tab.scroll(), tab.pinned()))
                .collect();
            open_window(app.handle(), PRIMARY, tabs, selected, keep.window.clone())?;
            apply_groups(app.handle(), PRIMARY, &keep.groups);
        }
        None => {
            open_window(app.handle(), PRIMARY, vec![(start_url, 0.0, false)], 0, None)?;
        }
    }

    crate::downloads::start_progress_watch(app.handle().clone());
    crate::persist::spawn_history_flusher(app.handle().clone());
    crate::audio::watch(app.handle().clone());
    install_owner_ui(app.handle())?;
    #[cfg(debug_assertions)]
    {
        debug_ext_toggle(app.handle());
        debug_gesture_test(app.handle());
    }
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

    #[cfg(debug_assertions)]
    if debug_hooks() && std::env::var("MIX_FULLSCREEN").is_ok() {
        let handle = app.handle().clone();
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(2500));
            let target = handle.clone();
            let _ = handle.run_on_main_thread(move || {
                let state = target.state::<AppState>();
                state.windows.fullscreen.write().insert(PRIMARY.to_string());
                let _ = relayout(&target, PRIMARY);
                crate::logging::info("debug: page marked fullscreen");
            });
        });
    }

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

    #[cfg(debug_assertions)]
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

    tauri::async_runtime::spawn(async {
        crate::translate::warm().await;
    });

    #[cfg(debug_assertions)]
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

    #[cfg(debug_assertions)]
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
                if target.get_window(PRIMARY).is_none() {
                    return;
                }
                let state = target.state::<AppState>();
                let patch = crate::ipc::SettingsPatch {
                    hidden_extensions: Some(vec![name]),
                    ..Default::default()
                };
                match crate::ipc::apply_settings_patch(&state, patch) {
                    Ok(_) => crate::logging::info("debug: settings patched"),
                    Err(err) => crate::logging::warn(&format!("debug patch failed: {}", err.message)),
                }
            });
        });
    }

    #[cfg(debug_assertions)]
    if debug_hooks() {
        if let Ok(url) = std::env::var("MIX_SAVE_URL") {
        let handle = app.handle().clone();
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(2500));
            let target = handle.clone();
            let _ = handle.run_on_main_thread(move || {
                let state = target.state::<AppState>();
                let call = crate::ipc::save_url(target.clone(), &state, url, None);
                match tauri::async_runtime::block_on(call) {
                    Ok(path) => crate::logging::info(&format!("debug save: {path}")),
                    Err(err) => crate::logging::warn(&format!("debug save failed: {}", err.message)),
                }
            });
        });
        }
    }

    #[cfg(debug_assertions)]
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

    #[cfg(debug_assertions)]
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

    #[cfg(debug_assertions)]
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

    {
        let handle = app.handle().clone();
        tauri::async_runtime::spawn_blocking(move || {
            std::thread::sleep(std::time::Duration::from_secs(6));
            crate::mem::report(&handle);
        });
    }
    Ok(())
}

/// Urls handed over on the command line.
pub fn initial_urls() -> Vec<String> {
    std::env::args()
        .skip(1)
        .filter(|arg| arg.starts_with("http://") || arg.starts_with("https://"))
        .collect()
}

/// Creates a window with its chrome strip and one webview per url.
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
    let window_color = crate::overlay::parse_color(&state.theme.read().colors.bg);
    drop(state);

    let win = Window::builder(app, label)
        .title("Mix")
        .background_color(window_color)
        .inner_size(1100.0, 760.0)
        .min_inner_size(500.0, 600.0)
        .decorations(false)
        .transparent(cfg!(any(target_os = "macos", target_os = "linux")))
        .visible(false)
        .build()?;

    crate::platform::install_resize_borders(&win);

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
                title: String::new(),
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
        core.mru = core.tabs.iter().map(|tab| tab.id.clone()).collect();
        if let Some(id) = core.selected.clone() {
            core.mru.retain(|entry| entry != &id);
            core.mru.insert(0, id);
        }
    }

    let chrome_builder = |dir: &Option<std::path::PathBuf>, with_extensions: bool| {
        let builder = WebviewBuilder::new(
            format!("{label}-chrome"),
            WebviewUrl::App("index.html".into()),
        )
        .devtools(true)
        .initialization_script(&format!("window.__mixWindow = \"{label}\";"));
        let builder = tabs::apply_profile(builder, dir);
        if with_extensions {
            tabs::apply_extensions(builder, &opts)
        } else {
            builder
        }
    };

    let fullscreen = app
        .try_state::<AppState>()
        .map(|state| state.windows.fullscreen.read().contains(label))
        .unwrap_or(false);
    let chrome_h = if fullscreen {
        0.0
    } else {
        crate::ipc::chrome_height(app)
    };
    let built = win.add_child(
        chrome_builder(&opts.ui_data_dir, true),
        LogicalPosition::new(0.0, 0.0),
        LogicalSize::new(width, chrome_h),
    );
    if let Err(err) = built {
        crate::logging::warn(&format!(
            "chrome webview refused the shared profile ({err}); using the ui profile"
        ));
        win.add_child(
            chrome_builder(&opts.ui_fallback_dir, false),
            LogicalPosition::new(0.0, 0.0),
            LogicalSize::new(width, chrome_h),
        )?;
    }

    if let Some(chrome) = app.get_webview(&format!("{label}-chrome")) {
        let scale = win.scale_factor().unwrap_or(1.0);
        if (scale - 1.0).abs() > f64::EPSILON {
            let _ = chrome.set_zoom(scale);
        }
    }



    for (index, (url, _scroll, _pinned)) in urls.iter().enumerate() {
        let id = crate::ipc::tab_label(label, index + 1);
        tabs::create(&win, &id, url, width, height - chrome_h, &opts, false)?;
        crate::tabs::apply_default_zoom(&app.clone(), &id);
    }

    crate::sidebar::ensure(app, label);
    if let Some(state) = app.try_state::<AppState>() {
        *state.windows.focused.write() = Some(label.to_string());
    }

    let handle = app.clone();
    let owner = label.to_string();
    win.on_window_event(move |event| match event {
        WindowEvent::Resized(_) | WindowEvent::ScaleFactorChanged { .. } => {
            if let Err(err) = relayout(&handle, &owner) {
                crate::logging::warn(&format!("relayout failed: {err}"));
            }
        }
        WindowEvent::Focused(true) => {
            if let Some(state) = handle.try_state::<AppState>() {
                *state.windows.focused.write() = Some(owner.clone());
            }
        }
        WindowEvent::CloseRequested { api, .. } => {
            crate::ipc::save_sessions(&handle);
            // A window that runs on its own has no tray to hide into: closing it ends
            // that process, the way closing a browser window does.
            if crate::paths::instance_mode() {
                return;
            }
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

    crate::logging::info(&format!("window {label} prepared"));
    win.show()?;
    crate::logging::info(&format!("window {label} shown"));
    win.set_focus()?;
    crate::logging::info(&format!("window {label} focused"));
    crate::ipc::save_sessions(app);
    crate::logging::info(&format!("window {label} ready"));
    Ok(win)
}

/// Keeps the address bar in step by polling the selected webviews.
pub fn start_url_watch(app: AppHandle, label: String) {
    std::thread::spawn(move || {
        let mut last: HashMap<String, String> = HashMap::new();
        loop {
            std::thread::sleep(Duration::from_millis(400));
            if app.get_window(&label).is_none() {
                return;
            }

            if let Some(state) = app.try_state::<AppState>() {
                if state.nav.read().app_base.is_empty() {
                    if let Some(chrome) = app.get_webview(&format!("{label}-chrome")) {
                        if let Ok(url) = chrome.url() {
                            let origin = url.origin().ascii_serialization();
                            if origin.starts_with("http") {
                                state.nav.write().app_base = format!("{origin}/");
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
                    if let Some(state) = app.try_state::<AppState>() {
                        let failed = state
                            .nav.read().pending
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
    let mut incognito = false;
    if let Some(state) = app.try_state::<AppState>() {
        let window = crate::ipc::window_of_tab(id);
        if let Some(mut core) = state.try_write_core(window) {
            if let Some(tab) = core.tabs.iter_mut().find(|tab| tab.id == id) {
                tab.url = url.to_string();
                incognito = tab.incognito;
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
    if !incognito {
        crate::ipc::record_visit(app, url, &title);
    }

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
    let fullscreen = app
        .try_state::<AppState>()
        .is_some_and(|state| state.windows.fullscreen.read().contains(label));
    let chrome_h = if fullscreen {
        0.0
    } else {
        crate::ipc::chrome_height(app)
    };
    let tab_height = (height - chrome_h).max(1.0);

    if let Some(chrome) = app.get_webview(&format!("{label}-chrome")) {
        if fullscreen {
            let _ = chrome.hide();
        } else {
            let _ = chrome.show();
        }
        set_bounds(&chrome, 0.0, 0.0, width, chrome_h)?;
    }

    let plan = crate::sidebar::plan(app, width, fullscreen);
    let (page_x, page_w) = plan.page(width);

    let ids: Vec<String> = match app.try_state::<AppState>() {
        Some(state) => match state.try_read_core(label) {
            Some(core) => core.tabs.iter().map(|tab| tab.id.clone()).collect(),
            None => Vec::new(),
        },
        None => Vec::new(),
    };
    for id in ids {
        if let Some(webview) = app.get_webview(&id) {
            set_bounds(&webview, page_x, chrome_h, page_w, tab_height)?;
        }
    }

    crate::sidebar::apply(app, label, &plan, width, chrome_h, tab_height)?;

    let _ = app.emit_to(label, "layout:changed", (width, height));
    Ok(())
}

/// Debug hook: toggles one extension and logs the outcome.
#[cfg(debug_assertions)]
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

/// Debug hook: runs a synthetic left gesture in the active tab.
#[cfg(debug_assertions)]
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





