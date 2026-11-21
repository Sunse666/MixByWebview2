//! Overlay panels: menus, bubbles and the tab card, each one a webview of its own.
//!
//! A panel's height belongs to its content, and only the page knows what that is. So a
//! panel opens with the size this file keeps for its kind and then reports the height it
//! actually needs through `overlay_resize`, which moves the window's bounds to match.
//! That keeps one table here instead of one per panel across the ipc layer, and it means
//! a context menu with twelve items is not predicted from a row count any more.

use tauri::{webview::WebviewBuilder, AppHandle, Emitter, Manager, WebviewUrl};

use crate::window::logical_size;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_ID: AtomicU64 = AtomicU64::new(1);

/// Nothing smaller than this is a settled measurement; a webview that thin only flickers.
const MIN_W: f64 = 96.0;
const MIN_H: f64 = 32.0;
/// Gap kept between a panel and the window edge.
const MARGIN: f64 = 8.0;

/// Which point of a panel stays where it was when the content changes its size.
#[derive(Clone, Copy, Debug)]
pub enum Anchor {
    /// A menu hanging off a click: its top left corner.
    Left,
    /// A bubble over a button: the middle of its top edge.
    Center,
    /// The app menu: its top right corner, against the window edge.
    Right,
}

/// The geometry a panel was opened with. Only the height ever changes afterwards.
#[derive(Clone, Copy, Debug)]
pub struct Frame {
    pub anchor: Anchor,
    /// The anchor point: left edge, centre or right edge, depending on `anchor`.
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    /// How far the panel may move up when its content grows: the bottom of the chrome strip
    /// for a panel that hangs under the strip, and 4 px for one that hangs off a click or a
    /// control, whose own position is the thing the user is looking at.
    pub floor: f64,
}

impl Frame {
    /// Left edge in window coordinates.
    fn left(&self) -> f64 {
        match self.anchor {
            Anchor::Left => self.x,
            Anchor::Center => self.x - self.width / 2.0,
            Anchor::Right => self.x - self.width,
        }
    }
}

/// The size a panel opens with.
///
/// A panel that follows its content reports the height it needs one frame later (see
/// [`follows_content`]), so the number here only has to be close enough that the first
/// paint does not look wrong. A panel that does not follow its content - the tab card,
/// the command palette, the settings form - keeps this size for its whole life, because
/// there the size is the design rather than a measurement.
pub fn initial_size(kind: &str) -> (f64, f64) {
    match base(kind) {
        "app_menu" | "ext_info" => (320.0, 566.0),
        "context_menu" => (250.0, 184.0),
        "url_menu" => (190.0, 150.0),
        "tab_menu" => (200.0, 224.0),
        "chrome_menu" => (190.0, 110.0),
        "group_menu" => (214.0, 138.0),
        "downloads" => (360.0, 320.0),
        "zoom" => (136.0, 40.0),
        // The code plus its quiet zone, at a size a phone camera can read off a screen.
        "qr" => (300.0, 300.0),
        "find" => (380.0, 44.0),
        "translate" => (380.0, 46.0),
        "suggest" => (420.0, 48.0),
        "permission" => (380.0, 150.0),
        "thumb" => (372.0, 236.0),
        "palette" | "tab_search" => (560.0, 420.0),
        _ => (420.0, 320.0),
    }
}

/// Whether the page measures its own height for this kind of panel.
pub fn follows_content(kind: &str) -> bool {
    matches!(
        base(kind),
        "app_menu"
            | "ext_info"
            | "context_menu"
            | "url_menu"
            | "tab_menu"
            | "chrome_menu"
            | "group_menu"
            | "downloads"
            | "zoom"
            | "find"
            | "suggest"
            | "permission"
    )
}

/// The kind without its "&name=value" tail.
pub(crate) fn base(kind: &str) -> &str {
    kind.split('&').next().unwrap_or(kind)
}

/// A value carried in the hash, e.g. the settings section.
fn section_of(kind: &str) -> &str {
    kind.split('&')
        .skip(1)
        .find_map(|part| part.strip_prefix("section="))
        .unwrap_or("appearance")
}

/// Builds the overlay webview with its background and focus behaviour.
fn overlay_builder(app: &AppHandle, label: &str, page: &str) -> WebviewBuilder<tauri::Wry> {
    let opts = {
        let state = app.state::<crate::ipc::AppState>();
        state.opts.clone()
    };
    let color = {
        let state = app.state::<crate::ipc::AppState>();
        let theme = state.theme.read().clone();
        parse_color(&theme.colors.chrome)
    };

    let focused =
        !page.starts_with("overlay.html#suggest") && !page.starts_with("overlay.html#thumb");
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

/// Opens the panel the app itself positions: the app menu against the right edge, the
/// find bar and the zoom bubble there too, everything else centred under the chrome.
pub fn open(app: &AppHandle, win: &tauri::Window, kind: &str) -> Result<(), String> {
    let (win_w, win_h) = logical_size(win).map_err(|e| e.to_string())?;
    let chrome_h = crate::window::chrome_height_px(app);
    let room = (win_h - chrome_h - 12.0).max(MIN_H);

    // The settings form is not a bubble: its sections are different heights and it
    // scrolls inside whatever it is given.
    let (width, height) = if base(kind) == "settings" {
        let height = match section_of(kind) {
            "startup" => 250.0,
            "theme" => 380.0,
            _ => 700.0,
        };
        (520.0, height)
    } else {
        initial_size(kind)
    };

    let (anchor, x) = match base(kind) {
        "app_menu" | "find" | "zoom" => (Anchor::Right, win_w - MARGIN),
        _ => (Anchor::Center, win_w / 2.0),
    };

    place(
        app,
        win,
        kind,
        Frame {
            anchor,
            x,
            y: chrome_h + 6.0,
            width: width.clamp(MIN_W, win_w),
            height: height.min(room),
            floor: chrome_h + 2.0,
        },
    )
}

/// Opens a panel where a click happened, hanging from its top left corner.
///
/// The caller measures in the css pixels of its own page. The chrome strip is zoomed by
/// the window scale, which is exactly what makes one css pixel one logical pixel there,
/// so those numbers go to the window unchanged. ([`crate::platform`] places the webview
/// in physical pixels and converts once.)
pub fn open_at(
    app: &AppHandle,
    win: &tauri::Window,
    kind: &str,
    x: f64,
    y: f64,
) -> Result<(), String> {
    open_sized(app, win, kind, x, y, initial_size(kind))
}

/// Same, with a size the caller knows better than the table does: the tab card is a
/// picture frame, and the suggestion list is as wide as the address bar is.
pub fn open_sized(
    app: &AppHandle,
    win: &tauri::Window,
    kind: &str,
    x: f64,
    y: f64,
    size: (f64, f64),
) -> Result<(), String> {
    place(
        app,
        win,
        kind,
        Frame {
            anchor: Anchor::Left,
            x,
            y,
            width: size.0.max(MIN_W),
            height: size.1.max(MIN_H),
            floor: 4.0,
        },
    )
}

/// Opens a panel centred on a control, the way a browser shows a bubble under a button.
pub fn open_under(
    app: &AppHandle,
    win: &tauri::Window,
    kind: &str,
    center: f64,
    y: f64,
) -> Result<(), String> {
    let (width, height) = initial_size(kind);
    place(
        app,
        win,
        kind,
        Frame {
            anchor: Anchor::Center,
            x: center,
            y,
            width,
            height,
            floor: 4.0,
        },
    )
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
    let kind = page.split('#').nth(1).unwrap_or(page);
    place(
        app,
        win,
        kind,
        Frame {
            anchor: Anchor::Left,
            x,
            y,
            width: width.max(MIN_W),
            height: height.max(MIN_H),
            floor: 4.0,
        },
    )
}

/// Creates the webview for a panel, puts it there and records what it was given.
fn place(app: &AppHandle, win: &tauri::Window, kind: &str, frame: Frame) -> Result<(), String> {
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

    let webview = place_at(
        win,
        builder,
        frame.left(),
        frame.y,
        frame.width,
        frame.height,
    )?;

    if base(kind) != "suggest" && base(kind) != "thumb" {
        // Once, and no retry. Asking a second time a moment later was tried and it closes
        // panels instead of keeping them: the request lands on the window rather than the
        // child webview, so the panel's document sees a blur and dismisses itself - the find
        // bar appeared and vanished. A panel that really cannot take the keyboard needs a
        // measurement first, not another guess.
        let _ = webview.set_focus();
    }
    if base(kind) == "thumb" {
        let chrome_label = format!("{}-chrome", win.label());
        if let Some(chrome) = app.get_webview(&chrome_label) {
            let _ = chrome.set_focus();
        }
    }
    {
        let state = app.state::<crate::ipc::AppState>();
        let mut slots = state.windows.overlay.write();
        slots.insert(win.label().to_string(), label.clone());
        // What is open, so a toggle can tell "this panel" from "another panel".
        state
            .windows
            .overlay_kind
            .write()
            .insert(win.label().to_string(), base(kind).to_string());
        // Only a panel that measures itself gets a frame to resize against; a fixed one
        // keeps the size it was opened with for its whole life.
        if follows_content(kind) {
            let mut frames = state.windows.overlay_frame.write();
            frames.insert(win.label().to_string(), frame);
        }
    }
    crate::logging::info(&format!(
        "overlay {kind} at {:.0},{:.0} {:.0}x{:.0}",
        frame.left(),
        frame.y,
        frame.width,
        frame.height
    ));
    mark_page(app, win.label(), true);
    Ok(())
}

/// Where a panel goes once its content has been measured.
///
/// `frame.floor` is how far down the chrome strip reaches for a panel that hangs under it:
/// such a panel may move up to fit a tall list, but never over the strip. A panel that hangs
/// off a click or a control keeps the old 4 px floor, because its anchor is a point the user
/// chose - a group chip in the tab strip opens its menu above the strip's bottom, and pushing
/// that menu down to clear the strip would move it away from the chip.
fn fitted(frame: Frame, win_h: f64, height: f64) -> (f64, f64) {
    let floor = frame.floor.clamp(4.0, (win_h - MARGIN).max(4.0));
    let height = height.clamp(MIN_H, (win_h - floor - 4.0).max(MIN_H));
    let top = frame.y.min((win_h - height - 4.0).max(floor)).max(floor);
    (top, height)
}

/// Resizes the open panel of one window to the height its page measured.
///
/// The width stays as it was: a panel's width is a design decision, its height is what
/// the content needs. A panel hanging off the bottom edge grows upwards instead of
/// running past it, and anything taller than the window afterwards scrolls.
pub fn resize(
    app: &AppHandle,
    overlay_label: &str,
    height: f64,
    width: Option<f64>,
) -> Result<(), String> {
    let state = app.state::<crate::ipc::AppState>();
    let found = {
        let slots = state.windows.overlay.read();
        slots
            .iter()
            .find(|(_, label)| label.as_str() == overlay_label)
            .map(|(window, _)| window.clone())
    };
    let Some(window_label) = found else {
        return Err(format!("overlay {overlay_label} is not open"));
    };
    let frame = {
        let frames = state.windows.overlay_frame.read();
        frames.get(&window_label).copied()
    };
    let Some(frame) = frame else {
        return Err(format!("no geometry for window {window_label}"));
    };
    let Some(win) = app.get_window(&window_label) else {
        return Err(format!("window {window_label} is gone"));
    };
    let Some(webview) = app.get_webview(overlay_label) else {
        return Err(format!("overlay {overlay_label} is gone"));
    };

    let (win_w, win_h) = logical_size(&win).map_err(|err| err.to_string())?;
    let (top, height) = fitted(frame, win_h, height);

    // A panel that opens a submenu grows sideways. It grows to the left when it hangs off
    // the right edge, because that is the side a menu against the window edge has room on:
    // the anchor is the edge it stays against, and `left` follows from the new width.
    let width = width.unwrap_or(frame.width).clamp(MIN_W, win_w);
    let left = frame.left().clamp(4.0, (win_w - width - 4.0).max(4.0));

    #[cfg(debug_assertions)]
    crate::logging::info(&format!(
        "overlay {overlay_label} measured {height:.0}x{width:.0}"
    ));

    crate::window::set_bounds(&webview, left, top, width, height).map_err(|err| err.to_string())
}

/// Converts a logical position to physical pixels.
fn place_at(
    win: &tauri::Window,
    builder: WebviewBuilder<tauri::Wry>,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
) -> Result<tauri::Webview, String> {
    let scale = win.scale_factor().map_err(|e| e.to_string())?;
    let position =
        tauri::PhysicalPosition::new((x * scale).round() as i32, (y * scale).round() as i32);
    let size = tauri::PhysicalSize::new(
        (width * scale).round() as u32,
        (height * scale).round() as u32,
    );
    win.add_child(builder, position, size)
        .map_err(|e| e.to_string())
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
            let _ = webview.eval(format!("window.__mixOverlay = {open};"));
        }
    }
    // The sidebar is a webview of its own as well, and has to close it too.
    if let Some(side) = app.get_webview(&format!("{label}-side")) {
        let _ = side.eval(format!("window.__mixOverlay = {open};"));
    }
    // So is the strip. It had no such flag, so a click on the toolbar or the address bar
    // outside a panel closed nothing: the panel only went away if it happened to hold the
    // keyboard focus and could notice losing it. That is a race against a webview that was
    // just created, which is why it worked only sometimes.
    if let Some(chrome) = app.get_webview(&format!("{label}-chrome")) {
        let _ = chrome.eval(format!("window.__mixOverlay = {open};"));
    }
}

/// Puts the "a panel is open" flag back into a page that has just loaded a document.
///
/// The flag lives in the document, so a navigation throws it away: a page that redirects
/// while a panel is open would then ignore the next click, and the panel would sit there
/// covering it with nothing to dismiss it but the strip. The injected script itself is put
/// back by WebView2 on every document; this flag is the one thing that has to be repeated.
pub fn remark_page(app: &AppHandle, window_label: &str, webview_label: &str) {
    let Some(state) = app.try_state::<crate::ipc::AppState>() else {
        return;
    };
    if !state.windows.overlay.read().contains_key(window_label) {
        return;
    }
    if let Some(webview) = app.get_webview(webview_label) {
        let _ = webview.eval("window.__mixOverlay = true;");
    }
}

/// Where the keyboard goes when an overlay closes.
fn belongs_to_page(kind: &str) -> bool {
    matches!(base(kind), "context_menu" | "find" | "permission")
}

fn remember_owner(app: &AppHandle, window_label: &str, kind: &str) {
    let state = app.state::<crate::ipc::AppState>();
    let mut owners = state.windows.overlay_from_page.write();
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
            state.windows.overlay_frame.write().remove(window_label);
            state.windows.overlay_kind.write().remove(window_label);
            let flag = state.windows.overlay_from_page.write().remove(window_label);
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
        }
        // Whatever was changed behind the panel shows up now, unless the panel was the
        // tab card: a card opening and closing is not a settings change.
        let was_thumb = {
            let state = app.state::<crate::ipc::AppState>();
            let mut thumbs = state.windows.overlay_thumb.write();
            thumbs.remove(window_label)
        };
        if !was_thumb {
            let _ = app.emit_to(format!("{window_label}-chrome"), "settings:changed", ());
        }
    }
}

#[cfg(test)]
mod panel_geometry_tests {
    use super::*;

    #[test]
    fn a_hashed_kind_uses_the_size_of_its_panel() {
        assert_eq!(initial_size("thumb&id=w1-tab-2"), (372.0, 236.0));
        assert_eq!(initial_size("group_menu&id=g1"), (214.0, 138.0));
        assert_eq!(initial_size("zoom"), (136.0, 40.0));
    }

    #[test]
    fn a_menu_follows_its_content_but_a_card_does_not() {
        assert!(follows_content("context_menu"));
        assert!(follows_content("app_menu&page=settings"));
        assert!(!follows_content("thumb&id=t1"));
        assert!(!follows_content("palette"));
        assert!(!follows_content("settings&section=theme"));
    }

    #[test]
    fn a_long_menu_never_grows_over_the_chrome() {
        // The window is 917 tall and the strip reaches 71 down, so the app menu opens at
        // 77. Its content is longer than the room under the strip; it has to stop at the
        // strip and scroll, not jump to the top of the window.
        let menu = Frame {
            anchor: Anchor::Right,
            x: 900.0,
            y: 77.0,
            width: 320.0,
            height: 566.0,
            floor: 73.0,
        };
        let (top, height) = fitted(menu, 917.0, 900.0);
        assert!(top >= 73.0, "top {top} is above the chrome strip");
        assert!(top + height <= 917.0 - 3.0, "panel runs past the window");
        assert!(height > 700.0, "a tall menu should use the room it has");
    }

    #[test]
    fn a_menu_hanging_off_a_chip_stays_below_it() {
        // A group chip sits in the tab strip, above the strip's bottom edge. Its menu must
        // keep hanging from the chip: the strip's floor is not its floor.
        let menu = Frame {
            anchor: Anchor::Left,
            x: 120.0,
            y: 34.0,
            width: 214.0,
            height: 138.0,
            floor: 4.0,
        };
        let (top, _) = fitted(menu, 900.0, 320.0);
        assert_eq!(top, 34.0);
    }

    #[test]
    fn a_menu_near_the_bottom_grows_up_but_stays_below_the_strip() {
        let menu = Frame {
            anchor: Anchor::Left,
            x: 40.0,
            y: 800.0,
            width: 200.0,
            height: 100.0,
            floor: 4.0,
        };
        let (top, height) = fitted(menu, 1000.0, 450.0);
        assert_eq!(height, 450.0);
        assert!(top < menu.y, "a menu without room below should move up");
        assert!(top >= 4.0, "but not off the top of the window");
        assert!(top + height <= 1000.0 - 3.0);
    }

    #[test]
    fn a_window_with_no_room_still_gets_a_panel() {
        let menu = Frame {
            anchor: Anchor::Left,
            x: 4.0,
            y: 77.0,
            width: 200.0,
            height: 100.0,
            floor: 73.0,
        };
        let (top, height) = fitted(menu, 200.0, 400.0);
        assert_eq!(top, 73.0);
        assert_eq!(height, MIN_H.max(200.0 - 73.0 - 4.0));
    }

    #[test]
    fn an_anchor_keeps_its_edge_of_the_frame() {
        let menu = Frame {
            anchor: Anchor::Left,
            x: 40.0,
            y: 20.0,
            width: 200.0,
            height: 100.0,
            floor: 4.0,
        };
        assert_eq!(menu.left(), 40.0);

        let bubble = Frame {
            anchor: Anchor::Center,
            x: 300.0,
            ..menu
        };
        assert_eq!(bubble.left(), 200.0);

        let app_menu = Frame {
            anchor: Anchor::Right,
            x: 1024.0,
            ..menu
        };
        assert_eq!(app_menu.left(), 824.0);
    }
}
