//! Window chrome, overlays, context menus, permissions and fullscreen.

use super::*;

#[tauri::command]
pub async fn window_control(
    _app: AppHandle,
    window: tauri::Window,
    action: String,
) -> Result<(), AppError> {
    let win = window_of(&window);
    match action.as_str() {
        "min" => win.minimize()?,
        "max" => {
            if win.is_maximized()? {
                win.unmaximize()?;
            } else {
                win.maximize()?;
            }
        }
        "close" => win.close()?,
        other => {
            return Err(AppError::new(
                "bad_action",
                format!("unknown action {other}"),
            ))
        }
    }
    Ok(())
}

#[tauri::command]
pub async fn window_drag(_app: AppHandle, window: tauri::Window) -> Result<(), AppError> {
    window.start_dragging()?;
    Ok(())
}

#[tauri::command]
pub async fn window_resize(
    _app: AppHandle,
    direction: String,
    window: tauri::Window,
) -> Result<(), AppError> {
    let dir = match direction.as_str() {
        "n" => ResizeDirection::North,
        "s" => ResizeDirection::South,
        "e" => ResizeDirection::East,
        "w" => ResizeDirection::West,
        "ne" => ResizeDirection::NorthEast,
        "nw" => ResizeDirection::NorthWest,
        "se" => ResizeDirection::SouthEast,
        "sw" => ResizeDirection::SouthWest,
        other => return Err(AppError::new("bad_direction", other)),
    };
    window.start_resize_dragging(dir)?;
    Ok(())
}

/// Opens the toolbar menu over the chrome strip.
#[tauri::command]
pub async fn chrome_menu(
    app: AppHandle,
    window: tauri::Window,
    kind: String,
    x: f64,
    y: f64,
) -> Result<(), AppError> {
    let label = match kind.as_str() {
        "url" => "url_menu",
        "tab" => "tab_menu",
        _ => "chrome_menu",
    };
    let _ = app.emit_to(window.label(), "chrome-menu", ());
    let _ = crate::overlay::open_at(&app, &window, label, x, y);
    Ok(())
}

#[tauri::command]
pub async fn context_menu(
    app: AppHandle,
    window: tauri::Window,
    state: State<'_, AppState>,
    request: MenuRequest,
) -> Result<(), AppError> {
    if request.dismiss.unwrap_or(false) {
        crate::overlay::close(&app, window.label());
        return Ok(());
    }

    let context = MenuContext {
        link: request.link.filter(|value| !value.is_empty()),
        image: request.image.filter(|value| !value.is_empty()),
        selection: request
            .selection
            .filter(|value| !value.trim().is_empty())
            .map(|value| value.chars().take(200).collect()),
        editable: request.editable.unwrap_or(false),
        tab: None,
    };
    state.ui.write().menu_context = context.clone();

    let win = window_of(&window);
    let (width, height) = crate::window::logical_size(&win)?;
    // Which rows the menu will have is the page's business: this only places the corner
    // it hangs from, with the size the menu opens at.
    let (menu_w, menu_h) = crate::overlay::initial_size("context_menu");
    let x = (request.x.unwrap_or(0.0) + 80.0).clamp(4.0, (width - menu_w - 4.0).max(4.0));
    let y = (crate::window::chrome_height_px(&app) + request.y.unwrap_or(0.0))
        .clamp(4.0, (height - menu_h - 4.0).max(4.0));

    crate::logging::info(&format!(
        "context menu at {x:.0},{y:.0} link={} image={} sel={}",
        context.link.is_some(),
        context.image.is_some(),
        context.selection.is_some()
    ));
    if let Err(err) = crate::overlay::open_at(&app, &window, "context_menu", x, y) {
        crate::logging::warn(&format!("context menu overlay failed: {err}"));
        return Err(AppError::new("overlay", err));
    }
    Ok(())
}

/// Opens the menu for one tab, remembering which tab it was opened for.
#[tauri::command]
pub async fn tab_menu(
    app: AppHandle,
    window: tauri::Window,
    state: State<'_, AppState>,
    id: String,
    x: f64,
    y: f64,
) -> Result<(), AppError> {
    state.ui.write().menu_context.tab = Some(id);
    let _ = app.emit_to(window.label(), "chrome-menu", ());
    let _ = crate::overlay::open_at(&app, &window, "tab_menu", x, y);
    Ok(())
}

#[tauri::command]
pub async fn menu_context_get(
    _window: tauri::Window,
    state: State<'_, AppState>,
) -> Result<MenuContext, AppError> {
    Ok(state.ui.read().menu_context.clone())
}

/// What the page gets back: an answer, or a prompt id to poll.
#[derive(Serialize)]
pub struct PermissionAsk {
    /// Set when this origin and kind were already decided for the session.
    allow: Option<bool>,
    /// Set when the user has to be asked; collect it with permission_poll.
    id: Option<u64>,
}

/// Asked from the page before a sensitive API is used.
///
/// The origin is read from the webview, not from the argument. The injected script sends
/// its own `location.origin`, and that is right for the script - but the command is also
/// reachable by anything the page itself runs, and a page that can name its own origin can
/// ask for permission as somebody else. What the user is shown and what is written to
/// `permissions.json` has to be the page that really asked, so the caller's url is the only
/// trustworthy source. A caller that is not a page at all (the strip) has no origin, and is
/// refused rather than prompted under a blank one.
#[tauri::command]
pub async fn permission_request(
    app: AppHandle,
    window: tauri::Window,
    webview: tauri::Webview,
    kind: String,
) -> Result<PermissionAsk, AppError> {
    let Some(caller) = webview.url().ok() else {
        return Err(AppError::new("untrusted", "no origin for this caller"));
    };
    // `origin` is scheme + host + port and nothing after it, which is what a decision is
    // remembered against: a path would make every page of a site its own question.
    let origin = caller.origin().ascii_serialization();
    if origin == "null" {
        return Err(AppError::new("untrusted", "no origin for this caller"));
    }
    let key = crate::permissions::key(&origin, &kind);
    if let Some(known) = crate::permissions::remembered(&app, &key) {
        return Ok(PermissionAsk {
            allow: Some(known),
            id: None,
        });
    }

    let id = crate::permissions::open_prompt(&app, window.label(), &key);
    let win = window_of(&window);
    let (width, _) = crate::window::logical_size(&win)?;
    let (prompt_w, prompt_h) = crate::overlay::initial_size("permission");
    let x = ((width - prompt_w) / 2.0).max(0.0);
    let hash = format!(
        "permission&id={id}&origin={}&kind={}",
        crate::permissions::encode(&origin),
        crate::permissions::encode(&kind)
    );
    let url = format!("overlay.html#{hash}");

    crate::overlay::open_url(
        &app,
        &window,
        &url,
        x,
        crate::window::chrome_height_px(&app) + 48.0,
        prompt_w,
        prompt_h,
    )
    .map_err(|err| AppError::new("overlay", err))?;

    crate::logging::info(&format!("permission prompt {key}"));
    Ok(PermissionAsk {
        allow: None,
        id: Some(id),
    })
}

#[tauri::command]
pub async fn permission_reply(
    app: AppHandle,
    window: tauri::Window,
    id: u64,
    allow: bool,
) -> Result<(), AppError> {
    if !crate::permissions::answer(&app, id, allow) {
        crate::logging::warn(&format!("permission reply for unknown prompt {id}"));
    }
    crate::overlay::close(&app, window.label());
    Ok(())
}

/// The page asks again until the prompt is answered; null means "not yet".
#[tauri::command]
pub async fn permission_poll(app: AppHandle, id: u64) -> Result<Option<bool>, AppError> {
    Ok(crate::permissions::poll(&app, id))
}

/// What this browser has been told to allow, so it can be looked at and undone.
#[tauri::command]
pub async fn permission_list(
    webview: tauri::Webview,
    app: AppHandle,
) -> Result<Vec<serde_json::Value>, AppError> {
    require_trusted(&webview)?;
    Ok(crate::permissions::decisions(&app))
}

/// Forgets one site's answer, or every answer when no key is given.
#[tauri::command]
pub async fn permission_forget(
    webview: tauri::Webview,
    app: AppHandle,
    key: Option<String>,
) -> Result<(), AppError> {
    require_trusted(&webview)?;
    crate::permissions::forget(&app, key.as_deref());
    Ok(())
}

#[tauri::command]
pub async fn snap(app: AppHandle, direction: String) -> Result<(), AppError> {
    let dir = match direction.as_str() {
        "left" => crate::snap::Direction::Left,
        "right" => crate::snap::Direction::Right,
        "up" => crate::snap::Direction::Up,
        "down" => crate::snap::Direction::Down,
        other => return Err(AppError::new("bad_direction", other)),
    };
    crate::snap::cycle(&app, dir).map_err(|err| AppError::new("snap", err))
}

#[tauri::command]
pub async fn window_fullscreen(
    _app: AppHandle,
    window: tauri::Window,
    enabled: Option<bool>,
) -> Result<bool, AppError> {
    let win = window_of(&window);
    let next = enabled.unwrap_or(!win.is_fullscreen()?);
    win.set_fullscreen(next)?;
    Ok(next)
}

/// Opens an overlay at a point the caller measured, used by the menu bar.
#[tauri::command]
pub async fn overlay_open_at(
    app: AppHandle,
    window: tauri::Window,
    kind: String,
    x: f64,
    y: f64,
) -> Result<(), AppError> {
    let _ = crate::overlay::open_at(&app, &window, &kind, x, y);
    Ok(())
}

/// Hides the strip while a page is in fullscreen.
#[tauri::command]
pub async fn page_fullscreen(
    app: AppHandle,
    webview: tauri::Webview,
    state: State<'_, AppState>,
    on: bool,
) -> Result<(), AppError> {
    let owner = window_of_tab(webview.label()).to_string();
    {
        let mut set = state.windows.fullscreen.write();
        if on {
            set.insert(owner.clone());
        } else {
            set.remove(&owner);
        }
    }
    // The strip is hidden while a page is fullscreen, so the only thing that can tell a
    // reader how to get out is a word from the browser itself. Sent only when the setting
    // asks for it: a hint that appears every time is noise.
    if on && state.settings.read().fullscreen_hint {
        let _ = app.emit_to(&owner, "page:fullscreen", true);
    }
    let _ = crate::window::relayout(&app, &owner);
    Ok(())
}

/// Reports the height the chrome strip actually rendered at, which the layout then follows.
///
/// The strip is html: how tall it is belongs to a stylesheet, and the constant in
/// `window.rs` can only ever be a guess for the first paint. This is the number that
/// settles it. It used to be accepted and then ignored - the layout kept the constant and
/// only a warning was logged, so the page area started 16 px too low on every window.
///
/// A report outside the plausible range is refused rather than stored: a page that is still
/// laying itself out says 0, and a height of 0 would collapse the window's layout.
#[tauri::command]
pub async fn chrome_fit(
    app: AppHandle,
    window: tauri::Window,
    state: State<'_, AppState>,
    height: f64,
) -> Result<(), AppError> {
    if !(MIN_CHROME_H..=MAX_CHROME_H).contains(&height) {
        crate::logging::warn(&format!("chrome strip reported {height}; ignored"));
        return Ok(());
    }
    // Half a pixel is the cutoff, not a whole one. With a whole one a strip that measured
    // 70.4 and later 71.2 counted as unchanged and kept the value it first stored, so the
    // number that survived was history rather than the current measurement.
    let changed = {
        let mut settings = state.settings.write();
        let changed = (settings.chrome_height - height).abs() >= 0.5;
        settings.chrome_height = height;
        changed
    };
    if changed {
        crate::logging::info(&format!("chrome height {height}"));
        let _ = crate::window::relayout(&app, window.label());
    }
    Ok(())
}
/// Gives the keyboard to the page once the address bar is done.
#[tauri::command]
pub async fn focus_page(
    app: AppHandle,
    window: tauri::Window,
    state: State<'_, AppState>,
) -> Result<(), AppError> {
    crate::overlay::close(&app, window.label());
    let id = selected_label(&state, &window);
    if let Some(webview) = app.get_webview(&id) {
        let _ = webview.set_focus();
    }
    Ok(())
}

#[tauri::command]
pub async fn page_failed(
    app: AppHandle,
    webview: tauri::Webview,
    state: State<'_, AppState>,
    code: String,
) -> Result<(), AppError> {
    let id = webview.label().to_string();
    let failed = state
        .nav
        .read()
        .pending
        .get(&id)
        .cloned()
        .unwrap_or_default();
    if failed.is_empty() {
        return Ok(());
    }
    crate::logging::info(&format!("page failed: {failed} ({code})"));
    show_error_page(&app, &id, &failed);
    Ok(())
}

/// First window label that is not in use.
fn free_window_label(state: &AppState) -> String {
    let taken: Vec<String> = state.cores.read().keys().cloned().collect();
    let mut index = 2;
    loop {
        let candidate = format!("w{index}");
        if !taken.contains(&candidate) {
            return candidate;
        }
        index += 1;
    }
}

/// Opens another window with its own tab strip.
///
/// It runs in this process, the way a second browser window does. A tab dragged out of a
/// window is a different thing and starts its own process; the two share `UserData`, so
/// the difference a user can see is that closing this one leaves the app running.
#[tauri::command]
pub async fn window_new(app: AppHandle, state: State<'_, AppState>) -> Result<String, AppError> {
    let label = free_window_label(&state);
    let url = state.settings.read().start_url.clone();
    crate::window::open_window(&app, &label, vec![(url, 0.0, false)], 0, None)?;
    crate::logging::info(&format!("window {label} opened"));
    Ok(label)
}

/// Starts another copy of the app on this url, carrying the tab's offset and the size
/// and place of the window it came from.
fn spawn_instance(
    url: &str,
    scroll: f64,
    pinned: bool,
    geometry: &crate::session::WindowGeometry,
) -> bool {
    let Ok(exe) = std::env::current_exe() else {
        return false;
    };
    let mut command = std::process::Command::new(exe);
    command
        .arg("--instance")
        .arg(url)
        .arg("--scroll")
        .arg(scroll.to_string())
        .arg("--bounds")
        .arg(format!(
            "{},{},{},{}",
            geometry.x, geometry.y, geometry.width, geometry.height
        ));
    if pinned {
        command.arg("--pinned");
    }
    match command.spawn() {
        Ok(_) => true,
        Err(err) => {
            crate::logging::warn(&format!("cannot start a second process: {err}"));
            false
        }
    }
}

/// Moves a tab into a window of its own, dropped where the pointer let go.
#[tauri::command]
pub async fn tab_pop_out(
    app: AppHandle,
    window: tauri::Window,
    state: State<'_, AppState>,
    id: String,
    x: f64,
    y: f64,
) -> Result<(), AppError> {
    let (url, scroll, pinned, only_tab) = {
        let core = state.read_core(window.label());
        match core.tabs.iter().find(|tab| tab.id == id) {
            Some(tab) => (
                tab.url.clone(),
                tab.scroll,
                tab.pinned,
                core.tabs.len() == 1,
            ),
            None => return Ok(()),
        }
    };
    let scale = window.scale_factor().unwrap_or(1.0);
    let size = window
        .inner_size()
        .unwrap_or(tauri::PhysicalSize::new(1100, 760));
    let geometry = crate::session::WindowGeometry {
        x: (x * scale) as i32 - 80,
        y: (y * scale) as i32 - 16,
        width: size.width,
        height: size.height,
        maximized: false,
    };
    // The dragged tab becomes a window of its own, run as its own process: the same
    // thing as starting mix.exe again, which is what "drag it out" means to a user.
    if spawn_instance(&url, scroll, pinned, &geometry) {
        crate::logging::info(&format!("tab {id} opened in its own process"));
    } else {
        let label = free_window_label(&state);
        crate::window::open_window(&app, &label, vec![(url, scroll, pinned)], 0, Some(geometry))?;
        crate::logging::info(&format!("tab {id} popped out into {label}"));
    }

    // A window with a single tab is not a place to drag from: taking it away would leave
    // nothing behind, so the gesture is refused instead of moving the last tab out.
    if only_tab {
        crate::logging::info(&format!("tab {id} stays: it is the only one in its window"));
        return Ok(());
    }
    tab_close(app.clone(), window, app.state::<AppState>(), id, None).await?;
    Ok(())
}

#[tauri::command]
pub async fn overlay_open(
    app: AppHandle,
    window: tauri::Window,
    kind: String,
) -> Result<(), AppError> {
    crate::overlay::open(&app, &window, &kind).map_err(|e| AppError::new("overlay", e))
}

/// Opens the overlay, or closes it when it is already open for this window.
#[tauri::command]
pub async fn overlay_toggle(
    app: AppHandle,
    window: tauri::Window,
    kind: String,
    x: Option<f64>,
    y: Option<f64>,
) -> Result<bool, AppError> {
    // The same panel closes; a different one is shown instead. Closing whatever happened to
    // be open and leaving nothing - which is what this did - made a click on one button while
    // another panel was showing look like it had done nothing at all.
    let current = app
        .state::<AppState>()
        .windows
        .overlay_kind
        .read()
        .get(window.label())
        .cloned();
    if current.as_deref() == Some(crate::overlay::base(&kind)) {
        crate::overlay::close(&app, window.label());
        return Ok(false);
    }
    match (x, y) {
        // The caller knows where its button is, so the panel opens under it.
        (Some(x), Some(y)) => crate::overlay::open_under(&app, &window, &kind, x, y)
            .map_err(|e| AppError::new("overlay", e))?,
        _ => crate::overlay::open(&app, &window, &kind).map_err(|e| AppError::new("overlay", e))?,
    }
    Ok(true)
}

#[tauri::command]
pub async fn overlay_close(app: AppHandle, window: tauri::Window) -> Result<(), AppError> {
    crate::overlay::close(&app, window.label());
    Ok(())
}

/// The panel measured its own content; put the window's bounds there.
///
/// A panel's height belongs to its content and only the page can see it, so the page is
/// the one that says how tall it is.
///
/// `width` is optional and only a panel that opens a submenu asks for one: the list and the
/// menu it came from have to sit side by side, and a child webview cannot be placed beside
/// its parent, so the parent grows and draws both.
#[tauri::command]
pub async fn overlay_resize(
    app: AppHandle,
    webview: tauri::Webview,
    height: f64,
    width: Option<f64>,
) -> Result<(), AppError> {
    require_trusted(&webview)?;
    let label = webview.label().to_string();
    if !label.starts_with("overlay-") {
        return Err(AppError::new(
            "untrusted",
            "only an overlay panel may size itself",
        ));
    }
    crate::overlay::resize(&app, &label, height, width).map_err(|err| AppError::new("overlay", err))
}
