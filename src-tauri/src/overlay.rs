use tauri::{webview::WebviewBuilder, AppHandle, Emitter, Manager, WebviewUrl};

use crate::window::logical_size;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_ID: AtomicU64 = AtomicU64::new(1);

const SETTINGS: (f64, f64) = (520.0, 700.0);
const APP_MENU: (f64, f64) = (320.0, 566.0);
const DEFAULT: (f64, f64) = (420.0, 320.0);


/// Overlay chrome. Transparency is only requested on platforms where a
/// transparent child webview actually composites; on Windows it produces
/// ghosting and black boxes, so the panel paints an opaque background instead.
fn overlay_builder(app: &AppHandle, label: &str, page: &str) -> WebviewBuilder<tauri::Wry> {
    let _ = page;
    let opts = {
        let state = app.state::<crate::ipc::AppState>();
        state.opts.clone()
    };
    let color = {
        let state = app.state::<crate::ipc::AppState>();
        let theme = state.theme.read().clone();
        parse_color(&theme.colors.chrome)
    };

    // The dropdown must never take focus away from the address bar, and the tab
    // hover card is opened by a mouse move, not by a click.
    let focused = !page.starts_with("overlay.html#suggest") && !page.starts_with("overlay.html#thumb");
    // Same extension setting as the pages: every controller in an environment has
    // to agree on it, or WebView2 refuses to create this one.
    let builder = WebviewBuilder::new(label, WebviewUrl::App(page.into()))
        .background_color(color)
        .devtools(true)
        .focused(focused);
    let builder = crate::tabs::apply_extensions(builder, &opts);

    #[cfg(any(target_os = "macos", target_os = "linux"))]
    let builder = builder.transparent(true);

    let _ = &builder;
    builder
}

pub(crate) fn parse_color(hex: &str) -> tauri::window::Color {
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
        // Chrome's translate bubble is a thin strip under the address bar.
        "translate" => (380.0, 46.0),
        "app_menu" => APP_MENU,
        // Generic page menu: five rows and two dividers.
        "context_menu" => (250.0, 184.0),
        "url_menu" => (190.0, 150.0),
        // Menu bar dropdown; the exact height comes from the caller.
        "menubar_menu" => (232.0, 300.0),
        "chrome_menu" => (190.0, 110.0),
        // The find bar rides under the address row, right aligned.
        "find" => (380.0, 44.0),
        // Tab hover card: the preview plus a caption line.
        "thumb" => (372.0, 232.0),
        _ => DEFAULT,
    };
    // Never taller than the space under the chrome strip, but a small panel
    // keeps its own height even in a short window.
    let chrome_h = crate::ipc::chrome_height(app);
    let room = (win_h - chrome_h - 12.0).max(oh.min(180.0));
    let oh = oh.min(room);

    let (x, y) = if base == "app_menu" || base == "find" {
        // Right aligned under the menu button, like a desktop app menu.
        ((win_w - ow - 8.0).max(0.0), chrome_h + 6.0)
    } else {
        (((win_w - ow) / 2.0).max(0.0), chrome_h + 6.0)
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
    remember_owner(app, win.label(), page);

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
    remember_owner(app, win.label(), kind);

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

    if !kind.starts_with("suggest") && !kind.starts_with("thumb") {
        let _ = webview.set_focus();
    }
    // A card is looked at, not typed into. Creating a child webview hands it the
    // keyboard - WebView2 focuses a new controller - which took focus away from the
    // strip and from the group menu's name field. The strip gets it back, now and once
    // more after the controller has settled.
    if kind.starts_with("thumb") {
        let chrome_label = format!("{}-chrome", win.label());
        if let Some(chrome) = app.get_webview(&chrome_label) {
            let _ = chrome.set_focus();
        }
        let app = app.clone();
        tauri::async_runtime::spawn_blocking(move || {
            std::thread::sleep(std::time::Duration::from_millis(80));
            if let Some(chrome) = app.get_webview(&chrome_label) {
                let _ = chrome.set_focus();
            }
        });
    }
    {
        let state = app.state::<crate::ipc::AppState>();
        let mut slots = state.overlay.write().unwrap_or_else(|e| e.into_inner());
        slots.insert(win.label().to_string(), label.clone());
    }
    crate::logging::info(&format!("overlay open {kind} at {x:.0},{y:.0}"));
    Ok(())
}

/// Where the keyboard goes when an overlay closes. A webview exposes no focus
/// getter, so the kind decides: menus, settings and the translate bubble belong
/// to the chrome strip, while the context menu, the find bar and the permission
/// prompt belong to the page that was being used.
fn belongs_to_page(kind: &str) -> bool {
    let base = kind.split('&').next().unwrap_or(kind);
    matches!(base, "context_menu" | "find" | "permission")
}

fn remember_owner(app: &AppHandle, window_label: &str, kind: &str) {
    let state = app.state::<crate::ipc::AppState>();
    let mut owners = state
        .overlay_from_page
        .write()
        .unwrap_or_else(|e| e.into_inner());
    owners.insert(window_label.to_string(), belongs_to_page(kind));
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
            // A child webview close can be refused while it is navigating or
            // busy; hiding it is the fallback that keeps it off the screen
            // instead of leaving a panel frozen over the page.
            if let Some(leftover) = app.get_webview(&label) {
                let _ = leftover.hide();
                crate::logging::info(&format!("overlay hid ({label}) after close was refused"));
            }
            crate::logging::info(&format!("overlay closed ({label})"));
        }
        // Put the keyboard back where it was before the overlay opened. A page
        // context menu returns to the page, the app menu to the chrome strip,
        // and the suggest dropdown to whatever already had focus.
        let from_page = {
            let state = app.state::<crate::ipc::AppState>();
            let flag = state
                .overlay_from_page
                .write()
                .unwrap_or_else(|e| e.into_inner())
                .remove(window_label);
            flag.unwrap_or(false)
        };
        let page = if from_page {
            app.try_state::<crate::ipc::AppState>().and_then(|state| {
                state
                    .try_read_core(window_label)
                    .and_then(|core| core.selected.clone())
            })
        } else {
            None
        };
        let target = page.unwrap_or_else(|| format!("{window_label}-chrome"));
        if let Some(webview) = app.get_webview(&target) {
            let _ = webview.set_focus();
        }
        // A prompt that is closed without an answer must not leave the page
        // polling: count it as denied.
        let cancelled = crate::permissions::cancel_window(app, window_label);
        if cancelled > 0 {
            crate::logging::info(&format!("{cancelled} permission prompt(s) dismissed"));
        }        // Whatever was changed behind the panel shows up now - unless the panel
        // was the tab hover card, which is opened and closed on every mouse move
        // across the strip and has nothing to do with settings.
        let was_thumb = {
            let state = app.state::<crate::ipc::AppState>();
            let mut thumbs = state
                .overlay_thumb
                .write()
                .unwrap_or_else(|e| e.into_inner());
            thumbs.remove(window_label)
        };
        if !was_thumb {
            let _ = app.emit_to(
                format!("{window_label}-chrome"),
                "settings:changed",
                (),
            );
        }
    }
}