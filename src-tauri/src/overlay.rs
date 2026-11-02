use tauri::{
    webview::WebviewBuilder, AppHandle, LogicalPosition, LogicalSize, Manager, WebviewUrl,
};

use crate::window::{logical_size, CHROME_H};

const SETTINGS: (f64, f64) = (460.0, 540.0);
const DEFAULT: (f64, f64) = (420.0, 320.0);

/// The overlay is recreated on every open so it stays above tabs created later:
/// child webviews stack in creation order and there is no bring-to-front call.
pub fn open(app: &AppHandle, kind: &str) -> Result<(), String> {
    let win = app
        .get_window("main")
        .ok_or_else(|| "main window missing".to_string())?;
    close(app);

    let (win_w, _) = logical_size(&win).map_err(|e| e.to_string())?;
    let (ow, oh) = match kind {
        "settings" => SETTINGS,
        _ => DEFAULT,
    };
    let x = ((win_w - ow) / 2.0).max(0.0);
    let y = CHROME_H + 6.0;

    let url = WebviewUrl::App(format!("overlay.html#{kind}").into());
    let webview = win
        .add_child(
            WebviewBuilder::new("overlay", url)
                .transparent(true)
                .focused(true),
            LogicalPosition::new(x, y),
            LogicalSize::new(ow, oh),
        )
        .map_err(|e| e.to_string())?;

    let _ = webview.set_focus();
    Ok(())
}

pub fn close(app: &AppHandle) {
    if let Some(webview) = app.get_webview("overlay") {
        let _ = webview.close();
    }
}
