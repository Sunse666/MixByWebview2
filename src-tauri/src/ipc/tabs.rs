//! Tab lifecycle: creating, closing, selecting, ordering and the per-tab state the strip reads.

use super::*;

fn spawn_tab(
    app: &AppHandle,
    label: &str,
    id: &str,
    url: &str,
    incognito: bool,
    visible: bool,
) -> Result<tauri::Webview, AppError> {
    let win = app
        .get_window(label)
        .ok_or_else(|| AppError::new("no_window", "window is missing"))?;
    let opts = app.state::<AppState>().opts.clone();
    let (width, height) = crate::window::logical_size(&win)?;
    let tab_height = (height - crate::window::chrome_height_px(app)).max(1.0);
    // A webview that is not the one on screen is created as small as a webview gets and
    // hidden before anything resizes it: WebviewBuilder has no "start hidden", and at full
    // size it would paint over the page the user is looking at for a frame.
    let (created_w, created_h) = if visible {
        (width, tab_height)
    } else {
        (1.0, 1.0)
    };
    let webview = crate::tabs::create(&win, id, url, created_w, created_h, &opts, incognito)
        .map_err(AppError::from)?;
    if !visible {
        let _ = webview.hide();
    }
    // A new tab is created at the full window width, so it would cover the sidebar
    // until something resizes the window. Put everything back where the plan says.
    let _ = crate::window::relayout(app, label);
    Ok(webview)
}

/// Brings a tab's webview back after it was dropped to save memory.
///
/// Suspending a tab closes its webview and keeps the url; nothing used to create it again,
/// so a tab parked by `max_live_tabs` came back as an empty page - and an address typed
/// into it updated the strip and went nowhere. Returns whether the tab has a webview
/// afterwards.
pub(crate) fn ensure_live(app: &AppHandle, label: &str, id: &str) -> bool {
    if app.get_webview(id).is_some() {
        return true;
    }
    let (url, incognito, visible) = {
        let state = app.state::<AppState>();
        let core = state.read_core(label);
        match core.tabs.iter().find(|tab| tab.id == id) {
            Some(tab) if !tab.url.is_empty() => (
                tab.url.clone(),
                tab.incognito,
                core.selected.as_deref() == Some(id),
            ),
            _ => return false,
        }
    };
    match spawn_tab(app, label, id, &url, incognito, visible) {
        Ok(_) => {
            let state = app.state::<AppState>();
            if let Some(mut core) = state.try_write_core(label) {
                if let Some(tab) = core.tabs.iter_mut().find(|tab| tab.id == id) {
                    tab.alive = true;
                }
            }
            crate::logging::info(&format!("tab {id} revived"));
            true
        }
        Err(err) => {
            crate::logging::warn(&format!("tab {id} not revived: {}", err.message));
            false
        }
    }
}

#[tauri::command]
pub async fn tab_list(
    window: tauri::Window,
    state: State<'_, AppState>,
) -> Result<TabList, AppError> {
    let core = state.read_core(window.label());
    Ok(snapshot(&core, window.label()))
}

#[tauri::command]
pub async fn tab_select(
    app: AppHandle,
    window: tauri::Window,
    state: State<'_, AppState>,
    id: String,
) -> Result<(), AppError> {
    let exists = {
        let core = state.read_core(window.label());
        core.tabs.iter().any(|tab| tab.id == id)
    };
    if !exists {
        return Err(AppError::new("no_tab", id));
    }
    crate::overlay::close(&app, window.label());
    if state.settings.read().tab_hover_preview {
        let previous = state.read_core(window.label()).selected.clone();
        if let Some(previous) = previous.filter(|label| label != &id) {
            if crate::thumbs::needs(&previous, 5) {
                if let Some(webview) = app.get_webview(&previous) {
                    if let Some(png) = crate::thumbs::capture(&webview) {
                        crate::thumbs::remember(&previous, png);
                    }
                }
            }
        }
    }
    {
        let mut core = state.write_core(window.label());
        core.selected = Some(id.clone());
        touch_mru(&mut core.mru, &id);
    }

    // A tab can have no webview: it was suspended to save memory, or this window has not
    // finished restoring it. Selecting it has to bring it back, otherwise the page area
    // stays empty and nothing says why.
    ensure_live(&app, window.label(), &id);
    sync_visibility(&app, window.label());
    if let Some(webview) = app.get_webview(&id) {
        let _ = webview.set_focus();
        let scroll = state
            .try_read_core(window.label())
            .and_then(|core| {
                core.tabs
                    .iter()
                    .find(|tab| tab.id == id)
                    .map(|tab| tab.scroll)
            })
            .unwrap_or(0.0);
        if scroll > 0.0 {
            let _ = webview.eval(format!("window.scrollTo(0, {scroll});"));
        }
    }
    emit_tabs(&app, window.label());
    Ok(())
}

#[tauri::command]
pub async fn tab_new(
    app: AppHandle,
    window: tauri::Window,
    state: State<'_, AppState>,
    url: Option<String>,
    incognito: Option<bool>,
    background: Option<bool>,
) -> Result<String, AppError> {
    let (id, target, selection_before) = {
        let mut core = state.write_core(window.label());
        core.next_tab_id += 1;
        let id = tab_label(window.label(), core.next_tab_id);
        let target = match url {
            Some(input) => {
                if crate::tabs::parse_url(&input).is_none() {
                    return Err(AppError::new(
                        "bad_url",
                        format!("not a web address: {input}"),
                    ));
                }
                input
            }
            None => {
                let settings = state.settings.read();
                if !settings.newtab_url.is_empty() {
                    settings.newtab_url.clone()
                } else if settings.new_tab_page {
                    "http://tauri.localhost/pages/newtab.html".to_string()
                } else {
                    settings.start_url.clone()
                }
            }
        };
        core.tick += 1;
        let tick = core.tick;
        let previous = core.selected.clone();
        core.tabs.push(Tab {
            id: id.clone(),
            url: target.clone(),
            // Left empty on purpose: the ui names a tab that has not loaded yet, in
            // the language it is running in.
            title: String::new(),
            zoom: 1.0,
            pinned: false,
            incognito: incognito.unwrap_or(false),
            icon: None,
            loading: false,
            scroll: 0.0,
            alive: true,
            last_active: tick,
        });
        core.selected = Some(id.clone());
        touch_mru(&mut core.mru, &id);
        (id, target, previous)
    };

    let created = spawn_tab(
        &app,
        window.label(),
        &id,
        &target,
        incognito.unwrap_or(false),
        // A tab opening in the background is hidden from the start, so it never paints
        // over the page the user is on.
        !background.unwrap_or(false),
    )?;
    crate::tabs::apply_default_zoom(&app, &id);

    if background.unwrap_or(false) {
        {
            let mut core = state.write_core(window.label());
            core.selected = selection_before;
        }
        sync_visibility(&app, window.label());
        emit_tabs(&app, window.label());
        save_sessions(&app);
        crate::logging::info(&format!("tab {id} opened in the background"));
        return Ok(id);
    }

    let _ = created.set_focus();
    crate::logging::info(&format!("tab created {id} -> {target}"));
    sync_visibility(&app, window.label());
    enforce_live_limit(&app, window.label());
    emit_tabs(&app, window.label());
    save_sessions(&app);
    Ok(id)
}
/// Pins a tab to the strip, or releases it.
#[tauri::command]
pub async fn tab_pin(
    app: AppHandle,
    window: tauri::Window,
    state: State<'_, AppState>,
    id: String,
    pinned: bool,
) -> Result<(), AppError> {
    {
        let mut core = state.write_core(window.label());
        if let Some(tab) = core.tabs.iter_mut().find(|tab| tab.id == id) {
            tab.pinned = pinned;
        }
        let mut pinned_tabs: Vec<Tab> = Vec::new();
        let mut loose: Vec<Tab> = Vec::new();
        for tab in core.tabs.drain(..) {
            if tab.pinned {
                pinned_tabs.push(tab);
            } else {
                loose.push(tab);
            }
        }
        core.tabs.extend(pinned_tabs);
        core.tabs.extend(loose);
    }
    emit_tabs(&app, window.label());
    save_sessions(&app);
    crate::logging::info(&format!("tab {id} pinned={pinned}"));
    Ok(())
}
/// Closes one tab.
///
/// `from_menu` is true when the caller is the tab's own menu: the one route a pinned tab
/// allows when the setting says it may only be closed from there. Every other caller leaves
/// it out and the field defaults to the stricter answer.
#[tauri::command]
pub async fn tab_close(
    app: AppHandle,
    window: tauri::Window,
    state: State<'_, AppState>,
    id: String,
    from_menu: Option<bool>,
) -> Result<(), AppError> {
    if window_of_tab(&id) != window.label() {
        crate::logging::warn(&format!(
            "refused to close {id} from window {}",
            window.label()
        ));
        return Ok(());
    }
    {
        let settings = state.settings.read();
        let pinned = state
            .try_read_core(window.label())
            .and_then(|core| {
                core.tabs
                    .iter()
                    .find(|tab| tab.id == id)
                    .map(|tab| tab.pinned)
            })
            .unwrap_or(false);
        if pinned && settings.tab_pinned_menu_only && from_menu != Some(true) {
            crate::logging::info(&format!("{id} is pinned; only its menu may close it"));
            return Ok(());
        }
    }
    if let Some(webview) = app.get_webview(&id) {
        let _ = webview.close();
    }
    crate::thumbs::forget(&id);
    crate::freeze::forget(&id);
    // Per-tab bookkeeping that nothing would ever read again once the tab is gone.
    state.windows.shown_at.write().remove(&id);
    state.windows.url_published.write().remove(&id);
    {
        let mut ui = state.ui.write();
        ui.translated_tabs.remove(&id);
        ui.translate_skip.remove(&id);
    }

    // Where the selection lands, and what an empty strip means. `tab_close_to` is the
    // setting the row offers; `tab_close_goes_left` is the older boolean and is still what a
    // file written before it carries, so "auto" falls back to it.
    let (close_to, go_left, next_url, last_close) = {
        let settings = state.settings.read();
        (
            settings.tab_close_to.clone(),
            settings.tab_close_goes_left,
            settings.start_url.clone(),
            settings.tab_last_close.clone(),
        )
    };
    // The tab that had been used before this one, taken before the id is dropped from `mru`.
    let previous = {
        let core = state.read_core(window.label());
        core.mru
            .iter()
            .find(|entry| entry.as_str() != id.as_str())
            .cloned()
    };
    let remaining = {
        let mut core = state.write_core(window.label());
        if let Some(index) = core.tabs.iter().position(|tab| tab.id == id) {
            let closed = core.tabs[index].clone();
            if !closed.incognito {
                let mut history = state.windows.closed_tabs.write();
                let list = history.entry(window.label().to_string()).or_default();
                list.push((closed.url, closed.title));
                if list.len() > 20 {
                    list.remove(0);
                }
            }
        }
        let closed_at = core.tabs.iter().position(|tab| tab.id == id);
        core.tabs.retain(|tab| tab.id != id);
        core.mru.retain(|entry| entry != &id);
        for group in core.groups.iter_mut() {
            group.members.retain(|member| member != &id);
        }
        core.groups.retain(|group| !group.members.is_empty());
        if core.selected.as_deref() == Some(id.as_str()) {
            let pick = |index: usize| core.tabs.get(index).map(|tab| tab.id.clone());
            core.selected = match closed_at {
                Some(index) => match close_to.as_str() {
                    "left" => index.checked_sub(1).and_then(pick).or_else(|| pick(0)),
                    "right" => pick(index).or_else(|| index.checked_sub(1).and_then(pick)),
                    "last_used" => previous
                        .clone()
                        .filter(|entry| core.tabs.iter().any(|tab| &tab.id == entry))
                        .or_else(|| pick(index))
                        .or_else(|| index.checked_sub(1).and_then(pick)),
                    // "auto" is the browser's own rule, and the older boolean picks a side
                    // for the case where it matters.
                    _ if go_left => index.checked_sub(1).and_then(pick).or_else(|| pick(0)),
                    _ => pick(index).or_else(|| index.checked_sub(1).and_then(pick)),
                },
                None => None,
            }
            .or_else(|| core.tabs.last().map(|tab| tab.id.clone()));
        }
        core.tabs.len()
    };

    if remaining == 0 {
        // The row offers three answers; the default opens a fresh tab rather than ending the
        // browser, which is what a window with no tabs left has historically done here.
        match last_close.as_str() {
            "close_window" => {
                if let Some(win) = app.get_window(window.label()) {
                    let _ = win.close();
                }
                return Ok(());
            }
            "nothing" => {
                emit_tabs(&app, window.label());
                save_sessions(&app);
                return Ok(());
            }
            _ => {
                let _ = tab_new(app, window.clone(), state, Some(next_url), None, None).await;
                return Ok(());
            }
        }
    }

    sync_visibility(&app, window.label());
    emit_tabs(&app, window.label());
    save_sessions(&app);
    Ok(())
}

/// Saves a png of the visible page into the downloads folder.
#[tauri::command]
pub async fn tab_screenshot(
    app: AppHandle,
    window: tauri::Window,
    state: State<'_, AppState>,
    full: Option<bool>,
) -> Result<String, AppError> {
    let id = selected_label(&state, &window);
    let Some(webview) = app.get_webview(&id) else {
        return Err(AppError::new("no_tab", "there is no tab to capture"));
    };
    let shot = if full.unwrap_or(false) {
        crate::screenshot::capture_full(&webview)
    } else {
        crate::screenshot::capture(&webview)
    };
    let Some(bytes) = shot else {
        return Err(AppError::new(
            "capture",
            "the page did not answer with an image",
        ));
    };
    let name = format!("Mix-shot-{}.png", crate::library::now());
    let target = crate::downloads::target_for(&state.dirs.downloads, "mix://screenshot", &name);
    crate::paths::write_atomic(&target, &bytes)?;
    crate::downloads::record_done(&app, "mix://screenshot", &target);
    crate::logging::info(&format!("screenshot {}", target.display()));
    Ok(target.display().to_string())
}

/// Payload for the tab hover card.
#[derive(Serialize)]
pub struct ThumbCard {
    title: String,
    url: String,
    icon: Option<String>,
    /// Full-size png as a data url.
    image: Option<String>,
    /// Age of the picture, in seconds.
    image_age_secs: Option<u64>,
    loading: bool,
    playing: bool,
    muted: bool,
    suspended: bool,
    pinned: bool,
    incognito: bool,
    /// Working set of every process the browser owns, in bytes.
    memory_total: Option<u64>,
    /// Working set divided by the live tabs of this window.
    memory_average: Option<u64>,
    /// The page's own javascript heap, in bytes.
    memory_tab: Option<u64>,
    /// DOM counters for this tab.
    nodes: u64,
    listeners: u64,
    documents: u64,
    /// Live tabs of this window.
    live_tabs: u64,
}

#[tauri::command]
pub async fn thumb_show(
    app: AppHandle,
    window: tauri::Window,
    _state: State<'_, AppState>,
    id: String,
    x: f64,
) -> Result<bool, AppError> {
    let visible = _state.read_core(window.label()).selected.as_deref() == Some(id.as_str());
    if visible && crate::thumbs::needs(&id, 1) {
        if let Some(webview) = app.get_webview(&id) {
            if let Some(png) = crate::thumbs::capture(&webview) {
                crate::thumbs::remember(&id, png);
            }
        }
    }
    if !visible && _state.settings.read().live_thumbs {
        crate::thumbs::capture_later(app.clone(), id.clone(), window.label().to_string());
    }
    let (win_w, _) = crate::window::logical_size(&window)?;
    // The card is a picture frame: its size is the design, not a measurement.
    let (width, height) = crate::overlay::initial_size("thumb");
    let left = (x - width / 2.0).clamp(6.0, (win_w - width - 6.0).max(6.0));
    let top = crate::window::chrome_height_px(&app) + 6.0;
    let kind = format!("thumb&id={}", crate::permissions::encode(&id));
    crate::overlay::open_sized(&app, &window, &kind, left, top, (width, height))
        .map_err(|err| AppError::new("overlay", err))?;
    app.state::<AppState>()
        .windows
        .overlay_thumb
        .write()
        .insert(window.label().to_string());
    Ok(true)
}

/// What the hover card draws.
#[tauri::command]
pub async fn thumb_card(
    _app: AppHandle,
    state: State<'_, AppState>,
    id: String,
) -> Result<ThumbCard, AppError> {
    let core = state.read_core(window_of_tab(&id));
    let tab = core
        .tabs
        .iter()
        .find(|tab| tab.id == id)
        .ok_or_else(|| AppError::new("no_tab", id.clone()))?;
    let audio = _app
        .get_webview(&id)
        .and_then(|webview| crate::audio::state(&webview));
    let live = core.tabs.iter().filter(|tab| tab.alive).count().max(1) as u64;
    Ok(ThumbCard {
        title: tab.title.clone(),
        url: tab.url.clone(),
        icon: tab.icon.clone(),
        image: crate::thumbs::get(&id)
            .map(|png| format!("data:image/jpeg;base64,{}", crate::extensions::base64(&png))),
        image_age_secs: crate::thumbs::age(&id),
        loading: tab.loading,
        playing: audio.map(|state| state.playing).unwrap_or(false),
        muted: audio.map(|state| state.muted).unwrap_or(false),
        suspended: !tab.alive,
        pinned: tab.pinned,
        incognito: tab.incognito,
        memory_total: None,
        memory_average: None,
        memory_tab: None,
        nodes: 0,
        listeners: 0,
        documents: 0,
        live_tabs: live,
    })
}

#[derive(serde::Serialize)]
pub struct ThumbStats {
    memory_total: Option<u64>,
    memory_average: Option<u64>,
    memory_tab: Option<u64>,
    nodes: u64,
    listeners: u64,
    documents: u64,
    live_tabs: u64,
}

/// The numbers on the hover card, read separately from the card itself.
#[tauri::command]
pub async fn thumb_stats(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
) -> Result<ThumbStats, AppError> {
    let live = state
        .try_read_core(window_of_tab(&id))
        .map(|core| core.tabs.iter().filter(|tab| tab.alive).count().max(1) as u64)
        .unwrap_or(1);
    // The counts come from a process list and a DevTools call, both of which are the
    // expensive part of the card. With the numbers switched off there is nothing to report
    // and no reason to ask: the row is not drawn, so this returns what an empty card shows.
    if !state.settings.read().thumb_show_memory {
        return Ok(ThumbStats {
            memory_total: None,
            memory_average: None,
            memory_tab: None,
            nodes: 0,
            listeners: 0,
            documents: 0,
            live_tabs: live,
        });
    }
    let handle = app.clone();
    let worker_id = id.clone();
    let stats = tauri::async_runtime::spawn_blocking(move || ThumbStats {
        memory_total: handle
            .get_webview(&worker_id)
            .and_then(|webview| crate::mem::total(&webview)),
        memory_average: None,
        memory_tab: handle
            .get_webview(&worker_id)
            .and_then(|webview| crate::mem::tab_heap(&webview)),
        nodes: 0,
        listeners: 0,
        documents: 0,
        live_tabs: live,
    })
    .await
    .map_err(|err| AppError::new("stats", err.to_string()))?;
    let metrics = app
        .get_webview(&id)
        .and_then(|webview| crate::mem::tab_metrics(&webview));
    Ok(ThumbStats {
        memory_average: stats.memory_total.map(|total| total / live.max(1)),
        nodes: metrics.map(|m| m.nodes).unwrap_or(0),
        listeners: metrics.map(|m| m.listeners).unwrap_or(0),
        documents: metrics.map(|m| m.documents).unwrap_or(0),
        ..stats
    })
}

/// Mutes or unmutes a tab; the selected one unless an id is given, and with no `muted` it
/// flips whatever the state is.
#[tauri::command]
pub async fn tab_mute(
    app: AppHandle,
    window: tauri::Window,
    state: State<'_, AppState>,
    id: Option<String>,
    muted: Option<bool>,
) -> Result<bool, AppError> {
    let id = id.unwrap_or_else(|| selected_label(&state, &window));
    if id.is_empty() {
        return Ok(false);
    }
    let Some(webview) = app.get_webview(&id) else {
        return Ok(false);
    };
    let current = crate::audio::state(&webview).unwrap_or(crate::audio::Audio {
        playing: false,
        muted: false,
    });
    let target = muted.unwrap_or(!current.muted);
    let ok = crate::audio::set_muted(&webview, target);
    let now = crate::audio::state(&webview).unwrap_or(crate::audio::Audio {
        playing: current.playing,
        muted: target,
    });
    // The strip reads the cache, so it has to know before the next poll comes round.
    crate::audio::remember(&id, now);
    crate::logging::info(&format!("tab {id} muted={} ({ok})", now.muted));
    let _ = app.emit_to(
        window.label(),
        "tab:audio",
        serde_json::json!({ "id": id, "playing": now.playing, "muted": now.muted }),
    );
    Ok(now.muted)
}

#[tauri::command]
pub async fn tab_navigate(
    app: AppHandle,
    window: tauri::Window,
    state: State<'_, AppState>,
    url: String,
) -> Result<(), AppError> {
    if url.starts_with("http") && !trusted_url(&url) {
        let reach = state.nav.read().reachability.clone();
        let probe_url = url.clone();
        let reachable = tauri::async_runtime::spawn_blocking(move || reach.check(&probe_url))
            .await
            .unwrap_or(true);
        if !reachable {
            let id = selected_label(&state, &window);
            if !id.is_empty() {
                crate::logging::info(&format!("unreachable: {url}"));
                show_error_page(&app, &id, url.trim());
            }
            return Ok(());
        }
    }

    let Some(target) = crate::tabs::parse_url(&url) else {
        crate::logging::warn(&format!("rejected address: {url}"));
        let id = selected_label(&state, &window);
        if !id.is_empty() {
            show_error_page(&app, &id, url.trim());
        }
        return Err(AppError::new(
            "bad_url",
            format!("not a web address: {url}"),
        ));
    };
    let label = selected_label(&state, &window);
    if label.is_empty() {
        return Ok(());
    }
    {
        let mut core = state.write_core(window.label());
        if let Some(tab) = core.tabs.iter_mut().find(|tab| tab.id == label) {
            tab.url = target.to_string();
        }
    }
    // Same as selecting it: a suspended tab has to exist again before it can go somewhere.
    ensure_live(&app, window.label(), &label);
    if let Some(webview) = app.get_webview(&label) {
        webview.navigate(target)?;
        let _ = webview.set_focus();
    }
    Ok(())
}

#[tauri::command]
pub async fn tab_nav(
    app: AppHandle,
    window: tauri::Window,
    state: State<'_, AppState>,
    action: String,
) -> Result<(), AppError> {
    let Some(webview) = app.get_webview(&selected_label(&state, &window)) else {
        return Ok(());
    };
    match action.as_str() {
        "back" => webview.eval("history.back()")?,
        "forward" => webview.eval("history.forward()")?,
        "reload" => webview.reload()?,
        "stop" => webview.eval("window.stop()")?,
        other => return Err(AppError::new("bad_nav", other)),
    }
    Ok(())
}

#[tauri::command]
pub async fn tab_print(
    app: AppHandle,
    window: tauri::Window,
    state: State<'_, AppState>,
) -> Result<(), AppError> {
    if let Some(webview) = app.get_webview(&selected_label(&state, &window)) {
        webview.print()?;
    }
    Ok(())
}

/// Moves a tab to a new index; the strip calls this after a drag.
///
/// `to` is where the tab should sit once it has been lifted out of the list, which is what
/// the strip measures: it finds the slot under the pointer, and a slot after the dragged
/// tab has already moved one to the left by then.
///
/// Pinned tabs own the front of the strip - `tab_pin` sorts them there - so the target is
/// kept on its own side of that line: a drag reorders within a section, never across it.
pub(crate) fn move_tab(tabs: &mut Vec<Tab>, id: &str, to: usize) -> bool {
    let Some(from) = tabs.iter().position(|tab| tab.id == id) else {
        return false;
    };
    let last = tabs.len().saturating_sub(1);
    let pinned = tabs.iter().filter(|tab| tab.pinned).count();
    let mut target = to.min(last);
    target = if tabs[from].pinned {
        target.min(pinned.saturating_sub(1))
    } else {
        target.max(pinned.min(last))
    };
    if from == target {
        return false;
    }
    let tab = tabs.remove(from);
    tabs.insert(target, tab);
    true
}

/// Pages report their scroll offset here so it survives a tab switch.
#[tauri::command]
pub async fn tab_scroll(
    _app: AppHandle,
    webview: tauri::Webview,
    state: State<'_, AppState>,
    y: f64,
) -> Result<(), AppError> {
    let id = webview.label().to_string();

    let owner = window_of_tab(&id).to_string();
    if let Some(mut core) = state.try_write_core(&owner) {
        if let Some(tab) = core.tabs.iter_mut().find(|tab| tab.id == id) {
            tab.scroll = y.max(0.0);
        }
    }
    Ok(())
}

#[tauri::command]
pub async fn tab_move(
    app: AppHandle,
    window: tauri::Window,
    state: State<'_, AppState>,
    id: String,
    to: usize,
) -> Result<(), AppError> {
    {
        let mut core = state.write_core(window.label());
        move_tab(&mut core.tabs, &id, to);
    }
    emit_tabs(&app, window.label());
    save_sessions(&app);
    Ok(())
}

/// Pages report their favicon here so the tab strip can show it.
#[tauri::command]
pub async fn tab_icon(
    app: AppHandle,
    webview: tauri::Webview,
    state: State<'_, AppState>,
    url: String,
) -> Result<(), AppError> {
    let id = webview.label().to_string();
    let icon = url.trim().to_string();
    if icon.is_empty() || !icon.starts_with("http") {
        return Ok(());
    }

    let (page_url, incognito) = state
        .try_read_core(window_of_tab(&id))
        .and_then(|core| {
            core.tabs
                .iter()
                .find(|tab| tab.id == id)
                .map(|tab| (tab.url.clone(), tab.incognito))
        })
        .unwrap_or_default();
    if trusted_url(&page_url) {
        return Ok(());
    }
    if !page_url.is_empty() && !incognito && !trusted_url(&page_url) {
        attach_icon(&app, &page_url, &icon);
        let host = crate::favicons::host_of(&page_url);
        let changed = {
            let state = app.try_state::<AppState>();
            match state {
                Some(state) => state.favicons_mut().set(&host, &icon),
                None => false,
            }
        };
        if changed {
            if let Some(state) = app.try_state::<AppState>() {
                state.favicons().save(&state.dirs.favicons_file);
            }
        }
        cache_icon(&app, &host, &icon);
    }

    let owner = window_of_tab(&id).to_string();
    if let Some(mut core) = state.try_write_core(&owner) {
        if let Some(tab) = core.tabs.iter_mut().find(|tab| tab.id == id) {
            tab.icon = Some(icon.clone());
        }
    }
    let _ = app.emit_to(
        owner,
        "tab:icon",
        IconPayload {
            id,
            icon: Some(icon),
        },
    );
    Ok(())
}
/// Takes the entry the menu asked for out of the stack.
///
/// The stack is oldest first, so the entry at menu position 0 - the top row - is the last
/// one in it. `checked_sub` rather than an index calculation so a stale menu cannot panic
/// the process: the list shrinks while it is open (the shortcut reopens one too).
fn take_closed(list: &mut Vec<(String, String)>, index: Option<usize>) -> Option<(String, String)> {
    match index {
        Some(index) => {
            let at = list.len().checked_sub(1 + index)?;
            Some(list.remove(at))
        }
        None => list.pop(),
    }
}

/// What this window closed lately, newest first, so the menu can offer the list.
///
/// Newest first because that is the order the entries are wanted in: the top row is the
/// one `tab_reopen` without an index would bring back.
#[tauri::command]
pub async fn tab_closed_list(
    window: tauri::Window,
    state: State<'_, AppState>,
) -> Result<Vec<serde_json::Value>, AppError> {
    let entries = state
        .windows
        .closed_tabs
        .read()
        .get(window.label())
        .map(|list| list.iter().rev().cloned().collect::<Vec<_>>())
        .unwrap_or_default();
    Ok(entries
        .into_iter()
        .map(|(url, title)| serde_json::json!({ "url": url, "title": title }))
        .collect())
}

/// Reopens a closed tab: the newest one, or the one at `index` in the list the menu shows.
#[tauri::command]
pub async fn tab_reopen(
    app: AppHandle,
    window: tauri::Window,
    state: State<'_, AppState>,
    index: Option<usize>,
) -> Result<bool, AppError> {
    let entry = {
        let mut history = state.windows.closed_tabs.write();
        match history.get_mut(window.label()) {
            Some(list) => take_closed(list, index),
            None => None,
        }
    };
    let Some((url, _title)) = entry else {
        return Ok(false);
    };
    tab_new(app, window, state, Some(url), None, None).await?;
    Ok(true)
}

/// Freezes a tab once it has been off screen for a while.
///
/// The wait is what keeps the freeze out of the way: the memory is reclaimed for tabs
/// that are actually parked, and a switch back happens long before the timer fires.
fn schedule_freeze(app: AppHandle, id: String) {
    const DELAY: std::time::Duration = std::time::Duration::from_secs(8);
    tauri::async_runtime::spawn_blocking(move || {
        std::thread::sleep(DELAY);
        let Some(state) = app.try_state::<AppState>() else {
            return;
        };
        if !state.settings.read().freeze_background {
            return;
        }
        let owner = window_of_tab(&id);
        let still_hidden = state
            .try_read_core(owner)
            .map(|core| core.selected.as_deref() != Some(id.as_str()))
            .unwrap_or(false);
        if !still_hidden {
            return;
        }
        let Some(webview) = app.get_webview(&id) else {
            return;
        };
        let playing = crate::audio::state(&webview)
            .map(|audio| audio.playing)
            .unwrap_or(false);
        if !playing {
            crate::freeze::freeze(&webview);
        }
    });
}

/// Puts a tab in a group of its own, or in an existing group.
#[tauri::command]
pub async fn group_add(
    app: AppHandle,
    window: tauri::Window,
    state: State<'_, AppState>,
    id: String,
    group_id: Option<String>,
) -> Result<String, AppError> {
    let target = {
        let mut core = state.write_core(window.label());
        if !core.tabs.iter().any(|tab| tab.id == id) {
            return Err(AppError::new("no_tab", id));
        }
        let target = match group_id {
            Some(wanted) => core
                .groups
                .iter()
                .find(|group| group.id == wanted)
                .map(|group| group.id.clone())
                .ok_or_else(|| AppError::new("no_group", wanted))?,
            None => {
                let fresh = format!("g{}", core.next_group_id + 1);
                core.next_group_id += 1;
                core.groups.push(TabGroup {
                    id: fresh.clone(),
                    name: "Group".to_string(),
                    color: "#4c8dff".to_string(),
                    collapsed: false,
                    members: Vec::new(),
                });
                fresh
            }
        };
        for group in core.groups.iter_mut() {
            group.members.retain(|member| member != &id);
        }
        if let Some(group) = core.groups.iter_mut().find(|group| group.id == target) {
            group.members.push(id.clone());
        }
        let members: Vec<String> = core
            .groups
            .iter()
            .find(|group| group.id == target)
            .map(|group| group.members.clone())
            .unwrap_or_default();
        let mut block: Vec<Tab> = Vec::new();
        let mut rest: Vec<Tab> = Vec::new();
        let mut insert_at: Option<usize> = None;
        for tab in core.tabs.drain(..) {
            if members.contains(&tab.id) {
                insert_at.get_or_insert(rest.len());
                block.push(tab);
            } else {
                rest.push(tab);
            }
        }
        let at = insert_at.unwrap_or(rest.len()).min(rest.len());
        for (offset, tab) in block.into_iter().enumerate() {
            rest.insert(at + offset, tab);
        }
        core.tabs = rest;
        target
    };
    emit_tabs(&app, window.label());
    save_sessions(&app);
    pin_group_to_bookmarks(&app, window.label(), &target);
    Ok(target)
}

/// Writes a group's tabs into the bookmarks as a folder, when the setting asks for it.
///
/// A group is not a bookmark and the bookmarks bar draws bookmarks. Rather than teach the
/// bar about a second kind of thing, the group's name becomes a folder: its members are
/// written into it, which is what a reader would have done by hand to keep a set of pages
/// together. A page that is already bookmarked is left where it is - moving it would silently
/// re-file something the user put somewhere on purpose.
pub(crate) fn pin_group_to_bookmarks(app: &AppHandle, label: &str, group_id: &str) {
    let state = app.state::<AppState>();
    if !state.settings.read().group_auto_pin {
        return;
    }
    let Some(core) = state.try_read_core(label) else {
        return;
    };
    let Some(group) = core.groups.iter().find(|group| group.id == group_id) else {
        return;
    };
    let folder = group.name.trim().to_string();
    let members: Vec<String> = group.members.clone();
    let pages: Vec<(String, String)> = core
        .tabs
        .iter()
        .filter(|tab| members.contains(&tab.id))
        .map(|tab| (tab.url.clone(), tab.title.clone()))
        .collect();
    drop(core);

    if folder.is_empty() || pages.is_empty() {
        return;
    }
    {
        let mut marks = state.bookmarks_mut();
        for (url, title) in pages {
            if url.is_empty() || marks.iter().any(|mark| mark.url == url) {
                continue;
            }
            marks.push(crate::library::Bookmark {
                url: url.clone(),
                title: if title.is_empty() { url } else { title },
                at: crate::library::now(),
                folder: folder.clone(),
            });
        }
    }
    let snapshot = state.bookmarks().clone();
    if let Err(err) = crate::library::save_bookmarks(&state.dirs.bookmarks_file, &snapshot) {
        crate::logging::warn(&format!("cannot save the pinned group: {err}"));
    }
    let _ = app.emit("bookmarks:changed", ());
}

/// Opens or closes a tab group.
#[tauri::command]
pub async fn group_toggle(
    app: AppHandle,
    window: tauri::Window,
    state: State<'_, AppState>,
    id: String,
) -> Result<bool, AppError> {
    let collapsed = {
        let mut core = state.write_core(window.label());
        let group = core
            .groups
            .iter_mut()
            .find(|group| group.id == id)
            .ok_or_else(|| AppError::new("no_group", id.clone()))?;
        group.collapsed = !group.collapsed;
        group.collapsed
    };
    if collapsed {
        let hide_selected = {
            let core = state.read_core(window.label());
            match core.selected.as_deref() {
                Some(selected) => core
                    .groups
                    .iter()
                    .find(|group| group.id == id)
                    .map(|group| group.members.iter().any(|member| member == selected))
                    .unwrap_or(false),
                None => false,
            }
        };
        if hide_selected {
            let next = {
                let core = state.read_core(window.label());
                core.tabs
                    .iter()
                    .find(|tab| !is_grouped(&core, &tab.id))
                    .map(|tab| tab.id.clone())
            };
            if let Some(next) = next {
                state.write_core(window.label()).selected = Some(next);
                sync_visibility(&app, window.label());
            }
        }
    }
    emit_tabs(&app, window.label());
    save_sessions(&app);
    Ok(collapsed)
}

/// Renames and recolours in one call; either field may be left out.
#[tauri::command]
pub async fn group_style(
    app: AppHandle,
    window: tauri::Window,
    state: State<'_, AppState>,
    id: String,
    name: Option<String>,
    color: Option<String>,
) -> Result<(), AppError> {
    {
        let mut core = state.write_core(window.label());
        let group = core
            .groups
            .iter_mut()
            .find(|group| group.id == id)
            .ok_or_else(|| AppError::new("no_group", id.clone()))?;
        if let Some(name) = name.filter(|value| !value.trim().is_empty()) {
            group.name = name.trim().to_string();
        }
        if let Some(color) = color.filter(|value| value.starts_with('#')) {
            group.color = color;
        }
    }
    emit_tabs(&app, window.label());
    save_sessions(&app);
    Ok(())
}

/// Takes one tab out of whatever group it is in; an empty group disappears.
#[tauri::command]
pub async fn group_remove(
    app: AppHandle,
    window: tauri::Window,
    state: State<'_, AppState>,
    id: String,
) -> Result<(), AppError> {
    {
        let mut core = state.write_core(window.label());
        for group in core.groups.iter_mut() {
            group.members.retain(|member| member != &id);
        }
        core.groups.retain(|group| !group.members.is_empty());
    }
    emit_tabs(&app, window.label());
    save_sessions(&app);
    Ok(())
}

/// Takes a group apart; its tabs stay open, just ungrouped.
#[tauri::command]
pub async fn group_dissolve(
    app: AppHandle,
    window: tauri::Window,
    state: State<'_, AppState>,
    id: String,
) -> Result<(), AppError> {
    {
        let mut core = state.write_core(window.label());
        core.groups.retain(|group| group.id != id);
    }
    emit_tabs(&app, window.label());
    save_sessions(&app);
    Ok(())
}

/// True when a tab belongs to a group, closed or not.
pub(crate) fn is_grouped(core: &Core, id: &str) -> bool {
    core.groups
        .iter()
        .any(|group| group.members.iter().any(|member| member == id))
}

/// Every tab in every window, for the tab search.
#[tauri::command]
pub async fn tab_search_all(
    state: State<'_, AppState>,
) -> Result<Vec<serde_json::Value>, AppError> {
    let mut out = Vec::new();
    let cores = state.cores.read();
    for (label, core) in cores.iter() {
        for tab in core.tabs.iter() {
            out.push(serde_json::json!({
                "window": label,
                "id": tab.id,
                "title": tab.title,
                "url": tab.url,
                "pinned": tab.pinned,
                "selected": core.selected.as_deref() == Some(tab.id.as_str()),
            }));
        }
    }
    Ok(out)
}

/// Selects a tab that may belong to another window, and brings that window forward.
#[tauri::command]
pub async fn tab_focus_any(
    app: AppHandle,
    state: State<'_, AppState>,
    window: String,
    id: String,
) -> Result<(), AppError> {
    {
        let mut core = state.write_core(&window);
        if !core.tabs.iter().any(|tab| tab.id == id) {
            return Err(AppError::new("no_tab", id));
        }
        core.selected = Some(id.clone());
        core.mru.retain(|entry| entry != &id);
        core.mru.insert(0, id.clone());
    }
    sync_visibility(&app, &window);
    if let Some(win) = app.get_window(&window) {
        let _ = win.unminimize();
        let _ = win.set_focus();
    }
    emit_tabs(&app, &window);
    save_sessions(&app);
    Ok(())
}

/// Only the selected tab is visible; the rest keep running underneath.
pub(crate) fn sync_visibility(app: &AppHandle, label: &str) {
    let pairs: Vec<(String, Option<String>)> = {
        let state = app.state::<AppState>();
        let core = state.read_core(label);
        core.tabs
            .iter()
            .map(|tab| (tab.id.clone(), core.selected.clone()))
            .collect()
    };
    let freeze_background = app
        .try_state::<AppState>()
        .map(|state| state.settings.read().freeze_background)
        .unwrap_or(false);
    for (id, selected) in pairs {
        if let Some(webview) = app.get_webview(&id) {
            if selected.as_deref() == Some(id.as_str()) {
                crate::freeze::thaw(&webview);
                let _ = webview.show();
                if let Some(state) = app.try_state::<AppState>() {
                    state
                        .windows
                        .shown_at
                        .write()
                        .insert(id.clone(), crate::library::now());
                }
            } else {
                let _ = webview.hide();
                if freeze_background {
                    schedule_freeze(app.clone(), id.clone());
                }
            }
        }
    }
}

/// Drop the least recently used background tab once too many are alive.
pub(crate) fn enforce_live_limit(app: &AppHandle, label: &str) {
    if !app
        .try_state::<AppState>()
        .map(|state| state.settings.read().memory_saver)
        .unwrap_or(false)
    {
        return;
    }
    let keep_alive: Vec<String> = app
        .try_state::<AppState>()
        .map(|state| {
            state
                .settings
                .read()
                .keep_alive_hosts
                .split([' ', ',', ';'])
                .map(|entry| entry.trim().trim_start_matches("www.").to_ascii_lowercase())
                .filter(|entry| !entry.is_empty())
                .collect()
        })
        .unwrap_or_default();

    loop {
        let victim = {
            let state = app.state::<AppState>();
            let mut core = state.write_core(label);
            let alive = core.tabs.iter().filter(|tab| tab.alive).count();
            let limit = state.settings.read().max_live_tabs.max(1);
            if alive <= limit {
                None
            } else {
                let selected = core.selected.clone();
                let candidate = core
                    .tabs
                    .iter()
                    .filter(|tab| tab.alive && Some(&tab.id) != selected.as_ref())
                    .filter(|tab| {
                        if keep_alive.is_empty() {
                            return true;
                        }
                        let host = tab
                            .url
                            .split("://")
                            .nth(1)
                            .unwrap_or("")
                            .split(['/', '?', '#'])
                            .next()
                            .unwrap_or("")
                            .trim_start_matches("www.")
                            .to_ascii_lowercase();
                        !keep_alive
                            .iter()
                            .any(|entry| host == *entry || host.ends_with(&format!(".{entry}")))
                    })
                    .min_by_key(|tab| tab.last_active)
                    .map(|tab| tab.id.clone());
                if let Some(id) = &candidate {
                    if let Some(tab) = core.tabs.iter_mut().find(|tab| &tab.id == id) {
                        tab.alive = false;
                    }
                }
                candidate
            }
        };

        match victim {
            Some(id) => {
                if let Some(webview) = app.get_webview(&id) {
                    let _ = webview.close();
                }
                crate::logging::info(&format!("suspended {id}"));
                let _ = app.emit_to(crate::ipc::window_of_tab(&id), "tab:suspended", &id);
            }
            None => return,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::take_closed;

    fn stack() -> Vec<(String, String)> {
        ["a", "b", "c"]
            .iter()
            .map(|name| (name.to_string(), name.to_uppercase()))
            .collect()
    }

    #[test]
    fn the_menu_counts_from_the_newest() {
        let mut list = stack();
        assert_eq!(
            take_closed(&mut list, Some(0)).map(|entry| entry.0),
            Some("c".to_string())
        );
        // One entry is gone, so the second row is now the oldest.
        assert_eq!(
            take_closed(&mut list, Some(1)).map(|entry| entry.0),
            Some("a".to_string())
        );
        assert_eq!(
            take_closed(&mut list, None).map(|entry| entry.0),
            Some("b".to_string())
        );
    }

    #[test]
    fn a_stale_position_is_ignored_instead_of_panicking() {
        let mut list = stack();
        assert_eq!(take_closed(&mut list, Some(7)), None);
        assert_eq!(list.len(), 3);
        list.clear();
        assert_eq!(take_closed(&mut list, Some(0)), None);
        assert_eq!(take_closed(&mut list, None), None);
    }
}
