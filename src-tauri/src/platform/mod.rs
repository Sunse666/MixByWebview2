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

/// Puts one child webview directly underneath another, without activating either.
///
/// The windows WebView2 creates are plain child HWNDs, so the stacking order is
/// theirs, and a webview that is shown again comes to the front by itself. The live
/// preview needs it the other way round: a background tab shown underneath the
/// current one, so that nothing of it becomes visible while it is photographed.
pub fn place_below(webview: &tauri::Webview, above: &tauri::Webview) {
    #[cfg(windows)]
    windows::place_below(webview, above);

    #[cfg(not(windows))]
    {
        let _ = webview;
        let _ = above;
    }
}
