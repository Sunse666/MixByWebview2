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
        other => return Err(AppError::new("bad_action", format!("unknown action {other}"))),
    }
    Ok(())
}

#[tauri::command]
pub async fn window_drag(_app: AppHandle, window: tauri::Window) -> Result<(), AppError> {
    window.start_dragging()?;
    Ok(())
}

#[tauri::command]
pub async fn window_resize(_app: AppHandle, direction: String, window: tauri::Window) -> Result<(), AppError> {
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
    let (menu_w, menu_h) = match label {
        "url_menu" => (190.0, 150.0),
        "tab_menu" => (200.0, 224.0),
        "downloads" => (360.0, 320.0),
        _ => (190.0, 110.0),
    };
    let _ = app.emit_to(window.label(), "chrome-menu", ());
    let _ = crate::overlay::open_at(&app, &window, label, x, y, menu_w, menu_h);
    Ok(())
}

#[tauri::command]
pub async fn context_menu(
    app: AppHandle,
    window: tauri::Window, state: State<'_, AppState>,
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
    let mut rows = 5.0; // back, forward, reload, print, inspect
    let mut groups = 2.0;
    if context.link.is_some() { rows += 2.0; groups += 1.0; }
    if context.image.is_some() { rows += 2.0; groups += 1.0; }
    if context.selection.is_some() { rows += 1.0; groups += 1.0; }
    if context.editable { rows += 3.0; groups += 1.0; }

    let menu_w = 250.0;
    let menu_h = rows * 30.0 + groups * 11.0 + 12.0;
    let x = (request.x.unwrap_or(0.0) + 80.0).clamp(4.0, (width - menu_w - 4.0).max(4.0));
    let y = (chrome_height(&app) + request.y.unwrap_or(0.0))
        .clamp(4.0, (height - menu_h - 4.0).max(4.0));

    crate::logging::info(&format!(
        "context menu at {x:.0},{y:.0} link={} image={} sel={}",
        context.link.is_some(),
        context.image.is_some(),
        context.selection.is_some()
    ));
    if let Err(err) = crate::overlay::open_at(&app, &window, "context_menu", x, y, menu_w, menu_h) {
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
    let _ = crate::overlay::open_at(&app, &window, "tab_menu", x, y, 200.0, 224.0);
    Ok(())
}

#[tauri::command]
pub async fn menu_context_get(_window: tauri::Window, state: State<'_, AppState>) -> Result<MenuContext, AppError> {
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
#[tauri::command]
pub async fn permission_request(
    app: AppHandle,
    window: tauri::Window,
    origin: String,
    kind: String,
) -> Result<PermissionAsk, AppError> {
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
    let prompt_w = 380.0;
    let prompt_h = 150.0;
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
        chrome_height(&app) + 48.0,
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
    let menu_h = match kind.split("group=").nth(1).unwrap_or("file") {
        "file" => 322.0,
        "edit" => 128.0,
        "view" => 150.0,
        "history" => 96.0,
        "bookmarks" => 96.0,
        "tools" => 254.0,
        _ => 96.0,
    };
    let palette = kind == "palette" || kind == "tab_search";
    let downloads = kind.starts_with("downloads");
    let group_menu = kind.starts_with("group_menu");
    let (width, height) = if palette {
        (560.0, 420.0)
    } else if group_menu {
        (214.0, 138.0)
    } else if downloads {
        (360.0, 320.0)
    } else {
        (232.0, menu_h)
    };
    let _ = crate::overlay::open_at(&app, &window, &kind, x, y, width, height);
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
    let _ = crate::window::relayout(&app, &owner);
    Ok(())
}

/// Reports the height the chrome strip actually rendered at.
#[tauri::command]
pub async fn chrome_fit(
    app: AppHandle,
    _window: tauri::Window,
    state: State<'_, AppState>,
    height: f64,
) -> Result<(), AppError> {
    let reported = height.clamp(0.0, 400.0);
    state.settings.write().chrome_height = reported;
    let expected = chrome_height(&app);
    if (reported - expected).abs() > 8.0 {
        crate::logging::warn(&format!(
            "chrome strip reported {reported} but is laid out at {expected}; layout kept"
        ));
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
        .nav.pending
        .read()
        
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

/// Opens another window with its own tab strip.
#[tauri::command]
pub async fn window_new(app: AppHandle, state: State<'_, AppState>) -> Result<String, AppError> {
    let taken: Vec<String> = state.cores.read().keys().cloned().collect();
    let mut index = 2;
    let label = loop {
        let candidate = format!("w{index}");
        if !taken.contains(&candidate) {
            break candidate;
        }
        index += 1;
    };

    let url = state.settings.read().start_url.clone();
    crate::window::open_window(&app, &label, vec![(url, 0.0, false)], 0, None)?;
    crate::logging::info(&format!("window {label} opened"));
    Ok(label)
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
) -> Result<bool, AppError> {
    let open = app
        .state::<AppState>()
        .windows.overlay
        .read()
        
        .contains_key(window.label());
    if open {
        crate::overlay::close(&app, window.label());
        return Ok(false);
    }
    crate::overlay::open(&app, &window, &kind).map_err(|e| AppError::new("overlay", e))?;
    Ok(true)
}

#[tauri::command]
pub async fn overlay_close(app: AppHandle, window: tauri::Window) -> Result<(), AppError> {
    crate::overlay::close(&app, window.label());
    Ok(())
}



