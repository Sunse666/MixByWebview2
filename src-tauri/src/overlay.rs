use tauri::{webview::WebviewBuilder, AppHandle, Emitter, Manager, WebviewUrl};

use crate::window::logical_size;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_ID: AtomicU64 = AtomicU64::new(1);

const SETTINGS: (f64, f64) = (520.0, 700.0);
const APP_MENU: (f64, f64) = (320.0, 566.0);
const DEFAULT: (f64, f64) = (420.0, 320.0);


/// Builds the overlay webview with its background and focus behaviour.
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

    let focused = !page.starts_with("overlay.html#suggest") && !page.starts_with("overlay.html#thumb");
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

/// Size of a panel, in logical pixels. Each page brings its own content, so the size
/// belongs to the kind rather than to the page.
fn panel_size(base: &str, settings_size: (f64, f64)) -> (f64, f64) {
    match base {
        "settings" => settings_size,
        "translate" => (380.0, 46.0),
        "app_menu" => APP_MENU,
        "context_menu" => (250.0, 184.0),
        "url_menu" => (190.0, 150.0),
        "chrome_menu" => (190.0, 110.0),
        "find" => (380.0, 44.0),
        // The row is 26 + 6 + 54 + 6 + 26 wide and 26 tall, inside 6x4 padding and the
        // menu's 2px padding and 1px border.
        "zoom" => (136.0, 40.0),
        "thumb" => (372.0, 232.0),
        _ => DEFAULT,
    }
}

/// Opens the overlay panel, recreating it each time.
pub fn open(app: &AppHandle, win: &tauri::Window, kind: &str) -> Result<(), String> {
    let (win_w, win_h) = logical_size(win).map_err(|e| e.to_string())?;

    // The page can ride along in the hash ("app_menu&page=extensions").
    let base = kind.split('&').next().unwrap_or(kind);
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

    let (ow, oh) = panel_size(base, settings_size);
    let chrome_h = crate::ipc::chrome_height(app);
    let room = (win_h - chrome_h - 12.0).max(oh.min(180.0));
    let oh = oh.min(room);

    let (x, y) = if base == "app_menu" || base == "find" || base == "zoom" {
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

/// A css pixel from a page is smaller than a logical pixel of the window by exactly the
/// window scale, so positions measured in a page have to be multiplied by it.
pub fn page_scale(win: &tauri::Window) -> f64 {
    win.scale_factor().unwrap_or(1.0)
}

/// Opens a panel centered on a control, the way a browser shows a bubble under a button.
pub fn open_under(
    app: &AppHandle,
    win: &tauri::Window,
    kind: &str,
    center: f64,
    y: f64,
) -> Result<(), String> {
    let (win_w, _) = logical_size(win).map_err(|e| e.to_string())?;
    let base = kind.split('&').next().unwrap_or(kind);
    let scale = page_scale(win);
    // The size is a page size too, so it is converted the same way as the position.
    let (ow, oh) = panel_size(base, SETTINGS);
    let (ow, oh) = (ow * scale, oh * scale);
    let center = center * scale;
    let y = y * scale;
    let x = (center - ow / 2.0)
        .min((win_w - ow - 8.0).max(0.0))
        .max(8.0);
    let chrome = app.get_webview(&format!("{}-chrome", win.label()));
    let chrome_rect = chrome.and_then(|view| {
        let pos = view.position().ok()?;
        let size = view.size().ok()?;
        Some(format!(
            "{:.0},{:.0} {:.0}x{:.0}",
            pos.x as f64 / scale,
            pos.y as f64 / scale,
            size.width as f64 / scale,
            size.height as f64 / scale
        ))
    });
    let inner = win.inner_size().map(|s| (s.width, s.height)).unwrap_or((0, 0));
    let outer = win.outer_size().map(|s| (s.width, s.height)).unwrap_or((0, 0));
    crate::logging::info(&format!(
        "overlay under: anchor={center:.0} y={y:.0} panel={ow:.0}x{oh:.0} at {x:.0} window={win_w:.0} inner={:?} outer={:?} scale={:.2} chrome={:?}",
        inner,
        outer,
        win.scale_factor().unwrap_or(0.0),
        chrome_rect
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
        let mut slots = state.windows.overlay.write();
        slots.insert(win.label().to_string(), label.clone());
    }
    crate::logging::info(&format!("overlay open {page}"));
    mark_page(app, win.label(), true);
    Ok(())
}

/// Tells the page it is covered, so a click there closes the overlay. The overlay is a
/// webview of its own: clicks in the page never reach it, and the page cannot see it.
fn mark_page(app: &AppHandle, label: &str, open: bool) {
    let Some(state) = app.try_state::<crate::ipc::AppState>() else {
        return;
    };
    let selected = state
        .try_read_core(label)
        .and_then(|core| core.selected.clone());
    if let Some(tab) = selected {
        if let Some(webview) = app.get_webview(&tab) {
            let _ = webview.eval(&format!("window.__mixOverlay = {open};"));
        }
    }
    // The sidebar is a webview of its own as well, and has to close it too.
    if let Some(side) = app.get_webview(&format!("{label}-side")) {
        let _ = side.eval(&format!("window.__mixOverlay = {open};"));
    }
}

/// Converts a logical position to physical pixels.
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

    let label = format!("overlay-{}", NEXT_ID.fetch_add(1, Ordering::Relaxed));
    let page = format!("overlay.html#{kind}");
    let builder = overlay_builder(app, &label, &page);
    let builder = crate::tabs::apply_profile(builder, &opts.ui_data_dir);

    let webview = place(win, builder, x, y, width, height)?;

    if !kind.starts_with("suggest") && !kind.starts_with("thumb") {
        let _ = webview.set_focus();
    }
    if kind.starts_with("thumb") {
        let chrome_label = format!("{}-chrome", win.label());
        if let Some(chrome) = app.get_webview(&chrome_label) {
            let _ = chrome.set_focus();
        }
    }
    {
        let state = app.state::<crate::ipc::AppState>();
        let mut slots = state.windows.overlay.write();
        slots.insert(win.label().to_string(), label.clone());
    }
    crate::logging::info(&format!("overlay open {kind} at {x:.0},{y:.0}"));
    mark_page(app, win.label(), true);
    Ok(())
}

/// Where the keyboard goes when an overlay closes.
fn belongs_to_page(kind: &str) -> bool {
    let base = kind.split('&').next().unwrap_or(kind);
    matches!(base, "context_menu" | "find" | "permission")
}

fn remember_owner(app: &AppHandle, window_label: &str, kind: &str) {
    let state = app.state::<crate::ipc::AppState>();
    let mut owners = state
        .windows.overlay_from_page
        .write()
        ;
    owners.insert(window_label.to_string(), belongs_to_page(kind));
}

pub fn close(app: &AppHandle, window_label: &str) {
    let label = {
        let state = app.state::<crate::ipc::AppState>();
        let mut slots = state.windows.overlay.write();
        slots.remove(window_label)
    };

    if let Some(label) = label {
        if let Some(webview) = app.get_webview(&label) {
            let _ = webview.close();
            if let Some(leftover) = app.get_webview(&label) {
                let _ = leftover.hide();
                crate::logging::info(&format!("overlay hid ({label}) after close was refused"));
            }
            crate::logging::info(&format!("overlay closed ({label})"));
        }
        let from_page = {
            let state = app.state::<crate::ipc::AppState>();
            let flag = state
                .windows.overlay_from_page
                .write()
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
        mark_page(app, window_label, false);
        let cancelled = crate::permissions::cancel_window(app, window_label);
        if cancelled > 0 {
            crate::logging::info(&format!("{cancelled} permission prompt(s) dismissed"));
        }        // Whatever was changed behind the panel shows up now - unless the panel
        let was_thumb = {
            let state = app.state::<crate::ipc::AppState>();
            let mut thumbs = state
                .windows.overlay_thumb
                .write()
                ;
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