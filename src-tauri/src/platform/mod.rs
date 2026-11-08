// Platform specific window behaviour that Tauri does not expose.

#[cfg(windows)]
mod windows;

pub fn install_resize_borders(window: &tauri::Window) {
    #[cfg(windows)]
    windows::install(window);

    // macOS and Linux keep the chrome strip handles; a borderless window there
    // would need an NSView / GtkWidget override (see DEV-PLAN 3.9).
    #[cfg(not(windows))]
    let _ = window;
}

/// Raises one child webview above its siblings without activating it.
///
/// The windows WebView2 creates are plain child HWNDs, so the stacking order is
/// theirs: a webview that is shown again does not come to the front on its own.
/// The live preview needs the opposite - a background tab shown *underneath* the
/// current one - which only works if the current one can be put back on top.
pub fn raise_webview(webview: &tauri::Webview) {
    #[cfg(windows)]
    windows::raise_webview(webview);

    #[cfg(not(windows))]
    let _ = webview;
}
