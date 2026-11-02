use tauri::{
    webview::WebviewBuilder, AppHandle, LogicalPosition, LogicalSize, Manager, WebviewUrl,
};

use crate::window::{logical_size, CHROME_H};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_ID: AtomicU64 = AtomicU64::new(1);

const SETTINGS: (f64, f64) = (460.0, 540.0);
const DEFAULT: (f64, f64) = (420.0, 320.0);

/// The overlay is recreated on every open so it stays above tabs created later:
/// child webviews stack in creation order and there is no bring-to-front call.
pub fn open(app: &AppHandle, kind: &str) -> Result<(), String> {
    let win = app
        .get_window("main")
        .ok_or_else(|| "main window missing".to_string())?;
    let (win_w, _) = logical_size(&win).map_err(|e| e.to_string())?;
    let (ow, oh) = match kind {
        "settings" => SETTINGS,
        _ => DEFAULT,
    };
    let x = ((win_w - ow) / 2.0).max(0.0);
    open_at(app, kind, x, CHROME_H + 6.0, ow, oh)
}

/// Place the overlay at an exact spot; used by the context menu.
pub fn open_at(
    app: &AppHandle,
    kind: &str,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
) -> Result<(), String> {
    let win = app
        .get_window("main")
        .ok_or_else(|| "main window missing".to_string())?;
    close(app);

    let opts = {
        let state = app.state::<crate::ipc::AppState>();
        state.opts.clone()
    };

    // A fresh label each time: closing and reopening the same label races with
    // the runtime, which still owns the previous webview for a moment.
    let label = format!("overlay-{}", NEXT_ID.fetch_add(1, Ordering::Relaxed));
    let url = WebviewUrl::App(format!("overlay.html#{kind}").into());
    let builder = crate::tabs::apply_profile(
        WebviewBuilder::new(&label, url).transparent(true).focused(true),
        &opts.ui_data_dir,
    );

    let webview = win
        .add_child(
            builder,
            LogicalPosition::new(x, y),
            LogicalSize::new(width, height),
        )
        .map_err(|e| e.to_string())?;

    let _ = webview.set_focus();
    {
        let state = app.state::<crate::ipc::AppState>();
        let mut slot = state.overlay.write().unwrap_or_else(|e| e.into_inner());
        *slot = Some(label.clone());
    }
    crate::logging::info(&format!("overlay open {kind} at {x:.0},{y:.0}"));
    Ok(())
}

pub fn close(app: &AppHandle) {
    let label = {
        let state = app.state::<crate::ipc::AppState>();
        let mut slot = state.overlay.write().unwrap_or_else(|e| e.into_inner());
        slot.take()
    };

    if let Some(label) = label {
        if let Some(webview) = app.get_webview(&label) {
            let _ = webview.close();
            crate::logging::info(&format!("overlay closed ({label})"));
        }
    }
}