use tauri::{webview::WebviewBuilder, AppHandle, Manager, WebviewUrl};

use crate::window::{logical_size, CHROME_H};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_ID: AtomicU64 = AtomicU64::new(1);

const SETTINGS: (f64, f64) = (520.0, 700.0);
const APP_MENU: (f64, f64) = (320.0, 566.0);
const DEFAULT: (f64, f64) = (420.0, 320.0);


/// Overlay chrome. Transparency is only requested on platforms where a
/// transparent child webview actually composites; on Windows it produces
/// ghosting and black boxes, so the panel paints an opaque background instead.
fn overlay_builder(app: &AppHandle, label: &str, page: &str) -> WebviewBuilder<tauri::Wry> {
    let color = {
        let state = app.state::<crate::ipc::AppState>();
        let theme = state.theme.read().clone();
        parse_color(&theme.colors.chrome)
    };

    let builder = WebviewBuilder::new(label, WebviewUrl::App(page.into()))
        .background_color(color)
        .devtools(true)
        .focused(true);

    #[cfg(any(target_os = "macos", target_os = "linux"))]
    let builder = builder.transparent(true);

    let _ = &builder;
    builder
}

fn parse_color(hex: &str) -> tauri::window::Color {
    let trimmed = hex.trim_start_matches('#');
    let value = u32::from_str_radix(trimmed, 16).unwrap_or(0x1F2020);
    let (r, g, b) = match trimmed.len() {
        3 => {
            let r = ((value >> 8) & 0xF) as u8 * 17;
            let g = ((value >> 4) & 0xF) as u8 * 17;
            let b = (value & 0xF) as u8 * 17;
            (r, g, b)
        }
        _ => (
            ((value >> 16) & 0xFF) as u8,
            ((value >> 8) & 0xFF) as u8,
            (value & 0xFF) as u8,
        ),
    };
    tauri::window::Color(r, g, b, 255)
}

/// The overlay is recreated on every open so it stays above tabs created later:
/// child webviews stack in creation order and there is no bring-to-front call.
pub fn open(app: &AppHandle, win: &tauri::Window, kind: &str) -> Result<(), String> {
    let (win_w, win_h) = logical_size(win).map_err(|e| e.to_string())?;

    // The page can ride along in the hash ("app_menu&page=extensions").
    let base = kind.split('&').next().unwrap_or(kind);
    // Settings panels size to the section they show, so Startup is not a
    // full height empty box.
    let section = kind
        .split('&')
        .skip(1)
        .find_map(|part| part.strip_prefix("section="))
        .unwrap_or("appearance");
    let settings_size = match section {
        "startup" => (520.0, 250.0),
        "theme" => (520.0, 380.0),
        _ => SETTINGS,
    };

    let (ow, oh) = match base {
        "settings" => settings_size,
        "app_menu" => APP_MENU,
        // Generic page menu: five rows and two dividers.
        "context_menu" => (250.0, 184.0),
        _ => DEFAULT,
    };
    // Never taller than the space under the chrome strip.
    let oh = oh.min((win_h - CHROME_H - 12.0).max(180.0));

    let (x, y) = if base == "app_menu" {
        // Right aligned under the menu button, like a desktop app menu.
        ((win_w - ow - 8.0).max(0.0), CHROME_H + 6.0)
    } else {
        (((win_w - ow) / 2.0).max(0.0), CHROME_H + 6.0)
    };

    crate::logging::info(&format!(
        "overlay geometry: window={win_w}x{win_h} scale={} panel={ow}x{oh} at {x},{y}",
        win.scale_factor().unwrap_or(0.0)
    ));

    open_at(app, win, kind, x, y, ow, oh)
}

/// Open a page with the caller-built hash, e.g. "permission&id=1&origin=...".
pub fn open_url(
    app: &AppHandle,
    win: &tauri::Window,
    page: &str,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
) -> Result<(), String> {
    close(app, win.label());

    let opts = {
        let state = app.state::<crate::ipc::AppState>();
        state.opts.clone()
    };

    let label = format!("overlay-{}", NEXT_ID.fetch_add(1, Ordering::Relaxed));
    let builder = overlay_builder(app, &label, page);
    let builder = crate::tabs::apply_profile(builder, &opts.ui_data_dir);

    let webview = place(win, builder, x, y, width, height)?;

    let _ = webview.set_focus();
    {
        let state = app.state::<crate::ipc::AppState>();
        let mut slots = state.overlay.write().unwrap_or_else(|e| e.into_inner());
        slots.insert(win.label().to_string(), label.clone());
    }
    crate::logging::info(&format!("overlay open {page}"));
    Ok(())
}

/// Positions are computed in logical units but the window reports physical
/// pixels; converting here once keeps the two from drifting under DPI scaling.
fn place(
    win: &tauri::Window,
    builder: WebviewBuilder<tauri::Wry>,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
) -> Result<tauri::Webview, String> {
    let scale = win.scale_factor().map_err(|e| e.to_string())?;
    let position = tauri::PhysicalPosition::new((x * scale).round() as i32, (y * scale).round() as i32);
    let size = tauri::PhysicalSize::new(
        (width * scale).round() as u32,
        (height * scale).round() as u32,
    );
    win.add_child(builder, position, size)
        .map_err(|e| e.to_string())
}

/// Place the overlay at an exact spot; used by the context menu.
pub fn open_at(
    app: &AppHandle,
    win: &tauri::Window,
    kind: &str,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
) -> Result<(), String> {
    close(app, win.label());

    let opts = {
        let state = app.state::<crate::ipc::AppState>();
        state.opts.clone()
    };

    // A fresh label each time: closing and reopening the same label races with
    // the runtime, which still owns the previous webview for a moment.
    let label = format!("overlay-{}", NEXT_ID.fetch_add(1, Ordering::Relaxed));
    let page = format!("overlay.html#{kind}");
    let builder = overlay_builder(app, &label, &page);
    let builder = crate::tabs::apply_profile(builder, &opts.ui_data_dir);

    let webview = place(win, builder, x, y, width, height)?;

    let _ = webview.set_focus();
    {
        let state = app.state::<crate::ipc::AppState>();
        let mut slots = state.overlay.write().unwrap_or_else(|e| e.into_inner());
        slots.insert(win.label().to_string(), label.clone());
    }
    crate::logging::info(&format!("overlay open {kind} at {x:.0},{y:.0}"));
    Ok(())
}

pub fn close(app: &AppHandle, window_label: &str) {
    let label = {
        let state = app.state::<crate::ipc::AppState>();
        let mut slots = state.overlay.write().unwrap_or_else(|e| e.into_inner());
        slots.remove(window_label)
    };

    if let Some(label) = label {
        if let Some(webview) = app.get_webview(&label) {
            let _ = webview.close();
            crate::logging::info(&format!("overlay closed ({label})"));
        }
    }
}