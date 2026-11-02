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
    let start_url = {
        let state = app.state::<AppState>();
        let core = state.core.read().unwrap_or_else(|e| e.into_inner());
        core.settings.start_url.clone()
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

    let (width, height) = logical_size(&win)?;

    win.add_child(
        WebviewBuilder::new("chrome", WebviewUrl::App("index.html".into())),
        LogicalPosition::new(0.0, 0.0),
        LogicalSize::new(width, CHROME_H),
    )?;

    let _tab = tabs::create(&win, "tab-1", &start_url, width, height - CHROME_H)?;

    {
        let state = app.state::<AppState>();
        let mut core = state.core.write().unwrap_or_else(|e| e.into_inner());
        core.next_tab_id = 1;
        core.tabs.push(tabs::Tab {
            id: "tab-1".into(),
            url: start_url,
            title: "New tab".into(),
        });
        core.selected = Some("tab-1".into());
    }

    let handle = app.handle().clone();
    win.on_window_event(move |event| match event {
        WindowEvent::Resized(_) | WindowEvent::ScaleFactorChanged { .. } => {
            if let Err(err) = relayout(&handle) {
                crate::logging::warn(&format!("relayout failed: {err}"));
            }
        }
        _ => {}
    });

    win.show()?;
    win.set_focus()?;

    crate::logging::info("main window ready");
    Ok(())
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
