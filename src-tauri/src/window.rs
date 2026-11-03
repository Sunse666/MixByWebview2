use std::collections::HashMap;
use std::time::Duration;

use tauri::{
    webview::WebviewBuilder, AppHandle, Emitter, LogicalPosition, LogicalSize, Manager, Webview,
    WebviewUrl, Window, WindowEvent,
};

use crate::ipc::AppState;
use crate::tabs;

pub const CHROME_H: f64 = 68.0;

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

pub fn bootstrap(app: &mut tauri::App) -> tauri::Result<()> {
    let (start_url, restore, session_file) = {
        let state = app.state::<AppState>();
        let core = state.core.read().unwrap_or_else(|e| e.into_inner());
        (
            core.settings.start_url.clone(),
            core.settings.restore_session,
            state.dirs.session_file.clone(),
        )
    };

    let restored = if restore {
        crate::session::Session::load(&session_file).filter(|session| !session.tabs.is_empty())
    } else {
        None
    };

    let win = Window::builder(app, "main")
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
    if let Some(geometry) = restored.as_ref().and_then(|session| session.window.clone()) {
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

    let opts = {
        let state = app.state::<AppState>();
        state.opts.clone()
    };

    let (urls, selected_index) = match restored {
        Some(session) => {
            let index = session.selected.min(session.tabs.len() - 1);
            (session.tabs, index)
        }
        None => (vec![start_url], 0),
    };

    {
        let state = app.state::<AppState>();
        let mut core = state.core.write().unwrap_or_else(|e| e.into_inner());
        core.next_tab_id = urls.len();
        core.tick = urls.len() as u64;
        core.tabs = urls
            .iter()
            .enumerate()
            .map(|(index, url)| tabs::Tab {
                id: format!("tab-{}", index + 1),
                url: url.clone(),
                title: "New tab".into(),
                zoom: 1.0,
                alive: true,
                last_active: index as u64,
            })
            .collect();
        core.selected = Some(format!("tab-{}", selected_index + 1));
    }

    let chrome_builder = tabs::apply_profile(
        WebviewBuilder::new("chrome", WebviewUrl::App("index.html".into())),
        &opts.ui_data_dir,
    );

    win.add_child(
        chrome_builder,
        LogicalPosition::new(0.0, 0.0),
        LogicalSize::new(width, CHROME_H),
    )?;


    for (index, url) in urls.iter().enumerate() {
        let id = format!("tab-{}", index + 1);
        tabs::create(&win, &id, url, width, height - CHROME_H, &opts)?;
    }

    let handle = app.handle().clone();
    win.on_window_event(move |event| match event {
        WindowEvent::Resized(_) | WindowEvent::ScaleFactorChanged { .. } => {
            if let Err(err) = relayout(&handle) {
                crate::logging::warn(&format!("relayout failed: {err}"));
            }
        }
        // Closing hides to the tray; quitting goes through the tray menu.
        WindowEvent::CloseRequested { api, .. } => {
            api.prevent_close();
            crate::ipc::save_session(&handle);
            if let Some(win) = handle.get_window("main") {
                let _ = win.hide();
            }
            crate::logging::info("window hidden to tray");
        }
        _ => {}
    });

    crate::logging::info(&format!("tabs ready: {}", urls.len()));
    crate::ipc::enforce_live_limit(app.handle());
    crate::ipc::sync_visibility(app.handle());
    crate::ipc::emit_tabs(app.handle());
    start_url_watch(app.handle().clone());

    win.show()?;
    win.set_focus()?;

    crate::snap::install_hotkeys(app.handle());
    crate::tray::install(&app.handle().clone())?;
    // Debug helper: MIX_OPEN_ON_START=settings opens that overlay shortly after
    // startup. Opening it inside setup deadlocks the page: the main thread is
    // still creating webviews, so its invoke never resolves.
    #[cfg(debug_assertions)]
    if let Ok(kind) = std::env::var("MIX_OPEN_ON_START") {
        let handle = app.handle().clone();
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(1500));
            let target = handle.clone();
            let _ = handle.run_on_main_thread(move || {
                if let Err(err) = crate::overlay::open(&target, &kind) {
                    crate::logging::warn(&format!("debug overlay failed: {err}"));
                }
            });
        });
    }

    #[cfg(debug_assertions)]
    if std::env::var("MIX_DIAG_ON_START").is_ok() {
        let handle = app.handle().clone();
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(2500));
            match crate::diag::export(&handle) {
                Ok(path) => crate::logging::info(&format!("debug diag: {}", path.display())),
                Err(err) => crate::logging::warn(&format!("debug diag failed: {err}")),
            }
        });
    }

    crate::logging::info("main window ready");
    Ok(())
}

/// Tauri exposes no navigation event, so the address bar is kept honest by
/// polling the selected webviews (only the ones that are attached).
pub fn start_url_watch(app: AppHandle) {
    std::thread::spawn(move || {
        let mut last: HashMap<String, String> = HashMap::new();
        loop {
            std::thread::sleep(Duration::from_millis(400));
            if app.get_window("main").is_none() {
                return;
            }

            let ids: Vec<String> = match app.try_state::<AppState>() {
                Some(state) => {
                    let core = state.core.read().unwrap_or_else(|e| e.into_inner());
                    core.tabs.iter().map(|tab| tab.id.clone()).collect()
                }
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
        let mut core = state.core.write().unwrap_or_else(|e| e.into_inner());
        if let Some(tab) = core.tabs.iter_mut().find(|tab| tab.id == id) {
            tab.url = url.to_string();
        }
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
    if let Some(win) = app.get_window("main") {
        let _ = win.unminimize();
        let _ = win.show();
        let _ = win.set_focus();
    }
}

pub fn relayout(app: &AppHandle) -> tauri::Result<()> {
    let Some(win) = app.get_window("main") else {
        return Ok(());
    };
    let (width, height) = logical_size(&win)?;
    let tab_height = (height - CHROME_H).max(1.0);

    if let Some(chrome) = app.get_webview("chrome") {
        set_bounds(&chrome, 0.0, 0.0, width, CHROME_H)?;
    }

    let labels: Vec<String> = {
        let state = app.state::<AppState>();
        let core = state.core.read().unwrap_or_else(|e| e.into_inner());
        core.tabs.iter().map(|t| t.id.clone()).collect()
    };
    for label in labels {
        if let Some(webview) = app.get_webview(&label) {
            set_bounds(&webview, 0.0, CHROME_H, width, tab_height)?;
        }
    }

    let _ = app.emit("layout:changed", (width, height));
    Ok(())
}