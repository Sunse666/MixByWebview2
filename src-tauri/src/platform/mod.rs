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
