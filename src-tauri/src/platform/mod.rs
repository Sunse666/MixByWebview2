//! Platform specific window behaviour that Tauri does not expose.

#[cfg(windows)]
mod windows;

pub fn install_resize_borders(window: &tauri::Window) {
    #[cfg(windows)]
    windows::install(window);

    #[cfg(not(windows))]
    let _ = window;
}

/// Puts one child webview directly underneath another.
pub fn place_below(webview: &tauri::Webview, above: &tauri::Webview) {
    #[cfg(windows)]
    windows::place_below(webview, above);

    #[cfg(not(windows))]
    {
        let _ = webview;
        let _ = above;
    }
}

/// Takes the picture-in-picture window out of the taskbar.
pub fn hide_pip_from_taskbar() {
    #[cfg(windows)]
    windows::hide_pip_from_taskbar();
}
