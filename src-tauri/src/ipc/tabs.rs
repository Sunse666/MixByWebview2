//! Tab lifecycle: creating, closing, selecting, ordering and the per-tab state the strip reads.

use super::*;

fn spawn_tab(
    app: &AppHandle,
    label: &str,
    id: &str,
    url: &str,
    incognito: bool,
) -> Result<tauri::Webview, AppError> {
    let win = app
        .get_window(label)
        .ok_or_else(|| AppError::new("no_window", "window is missing"))?;
    let opts = app.state::<AppState>().opts.clone();
    let (width, height) = crate::window::logical_size(&win)?;
    crate::tabs::create(
        &win,
        id,
        url,
        width,
        (height - chrome_height(app)).max(1.0),
        &opts,
        incognito,
    )
    .map_err(AppError::from)
}





#[tauri::command]
pub async fn tab_list(window: tauri::Window, state: State<'_, AppState>) -> Result<TabList, AppError> {
    let core = state.read_core(window.label());
    Ok(snapshot(&core))
}

#[tauri::command]
pub async fn tab_select(app: AppHandle, window: tauri::Window, state: State<'_, AppState>, id: String) -> Result<(), AppError> {
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

    sync_visibility(&app, window.label());
    if let Some(webview) = app.get_webview(&id) {
        let _ = webview.set_focus();
        let scroll = state
            .try_read_core(window.label())
            .and_then(|core| core.tabs.iter().find(|tab| tab.id == id).map(|tab| tab.scroll))
            .unwrap_or(0.0);
        if scroll > 0.0 {
            let _ = webview.eval(&format!("window.scrollTo(0, {scroll});"));
        }
    }
    emit_tabs(&app, window.label());
    Ok(())
}

#[tauri::command]
pub async fn tab_new(
    app: AppHandle,
    window: tauri::Window, state: State<'_, AppState>,
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
                return Err(AppError::new("bad_url", format!("not a web address: {input}")));
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
            title: if incognito.unwrap_or(false) {
                "Incognito".into()
            } else {
                "New tab".into()
            },
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

    let created = spawn_tab(&app, window.label(), &id, &target, incognito.unwrap_or(false))?;
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
#[tauri::command]
pub async fn tab_close(app: AppHandle, window: tauri::Window, state: State<'_, AppState>, id: String) -> Result<(), AppError> {
    if let Some(webview) = app.get_webview(&id) {
        let _ = webview.close();
    }
    crate::thumbs::forget(&id);
    crate::freeze::forget(&id);

    let (remaining, next_url) = {
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
            let go_left = state.settings.read().tab_close_goes_left;
            core.selected = match closed_at {
                Some(index) if go_left => core
                    .tabs
                    .get(index.saturating_sub(1))
                    .map(|tab| tab.id.clone()),
                Some(index) => core.tabs.get(index).map(|tab| tab.id.clone()),
                None => None,
            }
            .or_else(|| core.tabs.last().map(|tab| tab.id.clone()));
        }
        (core.tabs.len(), state.settings.read().start_url.clone())
    };

    if remaining == 0 {
        let _ = tab_new(app, window.clone(), state, Some(next_url), None, None).await;
        return Ok(());
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
        return Err(AppError::new("capture", "the page did not answer with an image"));
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
    let (width, height) = (372.0, 236.0);
    let left = (x - width / 2.0).clamp(6.0, (win_w - width - 6.0).max(6.0));
    let top = chrome_height(&app) + 6.0;
    let kind = format!("thumb&id={}", crate::permissions::encode(&id));
    crate::overlay::open_at(&app, &window, &kind, left, top, width, height)
        .map_err(|err| AppError::new("overlay", err))?;
    app.state::<AppState>()
        .windows.overlay_thumb
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
        .try_read_core(&window_of_tab(&id))
        .map(|core| core.tabs.iter().filter(|tab| tab.alive).count().max(1) as u64)
        .unwrap_or(1);
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
    let metrics = app.get_webview(&id).and_then(|webview| crate::mem::tab_metrics(&webview));
    Ok(ThumbStats {
        memory_average: stats.memory_total.map(|total| total / live.max(1)),
        nodes: metrics.map(|m| m.nodes).unwrap_or(0),
        listeners: metrics.map(|m| m.listeners).unwrap_or(0),
        documents: metrics.map(|m| m.documents).unwrap_or(0),
        ..stats
    })
}

/// Mutes or unmutes the selected tab; with no argument it flips.
#[tauri::command]
pub async fn tab_mute(
    app: AppHandle,
    window: tauri::Window,
    state: State<'_, AppState>,
    muted: Option<bool>,
) -> Result<bool, AppError> {
    let id = selected_label(&state, &window);
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
    crate::logging::info(&format!("tab {id} muted={} ({ok})", now.muted));
    let _ = app.emit_to(
        window.label(),
        "tab:audio",
        serde_json::json!({ "id": id, "playing": now.playing, "muted": now.muted }),
    );
    Ok(now.muted)
}

#[tauri::command]
pub async fn tab_navigate(app: AppHandle, window: tauri::Window, state: State<'_, AppState>, url: String) -> Result<(), AppError> {
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
        return Err(AppError::new("bad_url", format!("not a web address: {url}")));
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
pub(crate) fn move_tab(tabs: &mut Vec<Tab>, id: &str, to: usize) -> bool {
    let Some(from) = tabs.iter().position(|tab| tab.id == id) else {
        return false;
    };
    let target = to.min(tabs.len().saturating_sub(1));
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
        .try_read_core(&window_of_tab(&id))
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
                Some(state) => state
                    .library.favicons
                    .write()
                    
                    .set(&host, &icon),
                None => false,
            }
        };
        if changed {
            if let Some(state) = app.try_state::<AppState>() {
                state
                    .library.favicons
                    .read()
                    
                    .save(&state.dirs.favicons_file);
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
    let _ = app.emit_to(owner, "tab:icon", IconPayload { id, icon: Some(icon) });
    Ok(())
}
/// Reopens the most recently closed tab of this window.
#[tauri::command]
pub async fn tab_reopen(
    app: AppHandle,
    window: tauri::Window,
    state: State<'_, AppState>,
) -> Result<bool, AppError> {
    let Some((url, _title)) = state
        .windows.closed_tabs
        .write()
        
        .get_mut(window.label())
        .and_then(|list| list.pop())
    else {
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
    Ok(target)
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
                        .windows.shown_at
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