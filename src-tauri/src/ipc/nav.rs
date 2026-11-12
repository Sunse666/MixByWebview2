//! Address bar, suggestions, find, zoom and the url edit bridge.

use super::*;

/// Opens a search in a new tab using the configured engine.
#[tauri::command]
pub async fn search_new_tab(
    app: AppHandle,
    window: tauri::Window,
    state: State<'_, AppState>,
    text: String,
) -> Result<(), AppError> {
    let query = text.trim().to_string();
    if query.is_empty() {
        return Ok(());
    }
    let url = {
        let settings = state.settings.read();
        let template = settings.custom_search.trim();
        if template.contains("%s") {
            template.replace("%s", &encode_query(&query))
        } else {
            let base = match settings.search_engine.as_str() {
                "bing" => "https://www.bing.com/search?q=",
                "google" => "https://www.google.com/search?q=",
                "ddg" => "https://duckduckgo.com/?q=",
                _ => "https://www.baidu.com/s?wd=",
            };
            format!("{base}{}", encode_query(&query))
        }
    };
    crate::logging::info(&format!("search: {query}"));
    tab_new(app, window, state, Some(url), None, None).await?;
    Ok(())
}
/// Shows what the drag engine sees under the pointer for a few seconds.
#[tauri::command]
pub async fn drag_probe(
    app: AppHandle,
    window: tauri::Window,
    state: State<'_, AppState>,
) -> Result<(), AppError> {
    let label = selected_label(&state, &window);
    let Some(webview) = app.get_webview(&label) else {
        return Ok(());
    };
    let _ = webview.eval(crate::tabs::DRAG_PROBE);
    crate::logging::info("drag probe injected");
    Ok(())
}
pub(crate) fn zoom_by(app: &AppHandle, label: &str, delta: f64) -> Result<(), AppError> {
    let current = {
        let state = app.state::<AppState>();
        let core = state.read_core(label);
        let label = core.selected.clone();
        label
            .and_then(|id| core.tabs.iter().find(|tab| tab.id == id).map(|tab| tab.zoom))
            .unwrap_or(1.0)
    };
    set_zoom(app, label, current + delta)
}

pub(crate) fn set_zoom(app: &AppHandle, label: &str, level: f64) -> Result<(), AppError> {
    let level = crate::zoom::normalize(level);
    let state = app.state::<AppState>();

    let (id, url, incognito) = {
        let core = state.read_core(label);
        match core
            .selected
            .as_ref()
            .and_then(|id| core.tabs.iter().find(|tab| &tab.id == id))
        {
            Some(tab) => (tab.id.clone(), tab.url.clone(), tab.incognito),
            None => return Ok(()),
        }
    };

    {
        let mut core = state.write_core(label);
        if let Some(tab) = core.tabs.iter_mut().find(|tab| tab.id == id) {
            tab.zoom = level;
        }
    }
    if let Some(webview) = app.get_webview(&id) {
        webview.set_zoom(level)?;
    }

    if incognito {
        return Ok(());
    }
    {
        let mut store = state.zoom_mut();
        if store.remember(&url, level) {
            if let Err(err) = store.save(&state.dirs.zoom_file) {
                crate::logging::warn(&format!("zoom save failed: {err}"));
            }
        }
    }

    crate::logging::info(&format!("zoom -> {level:.1} for {url}"));
    Ok(())
}

#[tauri::command]
pub async fn zoom_get(window: tauri::Window, state: State<'_, AppState>) -> Result<f64, AppError> {
    let core = state.read_core(window.label());
    let zoom = core
        .selected
        .as_ref()
        .and_then(|id| core.tabs.iter().find(|tab| &tab.id == id))
        .map(|tab| tab.zoom)
        .unwrap_or(1.0);
    Ok(zoom)
}

#[tauri::command]
pub async fn zoom_step(app: AppHandle, delta: f64, window: tauri::Window, _state: State<'_, AppState>) -> Result<f64, AppError> {
    let current = {
        let state = app.state::<AppState>();
        let core = state.read_core(window.label());
        core.selected
            .as_ref()
            .and_then(|id| core.tabs.iter().find(|tab| &tab.id == id))
            .map(|tab| tab.zoom)
            .unwrap_or(1.0)
    };
    set_zoom(&app, window.label(), current + delta)?;
    Ok(((current + delta) * 10.0).round() / 10.0)
}

#[derive(Serialize)]
pub struct Suggestion {
    pub url: String,
    pub title: String,
    pub kind: String,
}

/// History, then bookmarks, deduped by url and ranked by match quality.
fn collect_suggestions(state: &AppState, text: &str, limit: usize) -> Vec<Suggestion> {
    let needle = text.trim().to_lowercase();
    let mut found: Vec<Suggestion> = Vec::new();

    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    {
        let history = state.history();
        for visit in history.iter() {
            if seen.insert(visit.url.clone()) {
                found.push(Suggestion {
                    url: visit.url.clone(),
                    title: visit.title.clone(),
                    kind: "history".into(),
                });
            }
        }
    }
    {
        let bookmarks = state.bookmarks();
        for bookmark in bookmarks.iter() {
            if seen.insert(bookmark.url.clone()) {
                found.push(Suggestion {
                    url: bookmark.url.clone(),
                    title: bookmark.title.clone(),
                    kind: "bookmark".into(),
                });
            }
        }
    }

    if !needle.is_empty() {
        let mut ranked: Vec<(u8, Suggestion)> = found
            .into_iter()
            .filter_map(|item| {
                let url = item.url.to_lowercase();
                if url.contains(&needle) {
                    return Some((0, item));
                }
                if item.title.to_lowercase().contains(&needle) {
                    return Some((1, item));
                }
                None
            })
            .collect();
        ranked.sort_by_key(|(rank, _)| *rank);
        ranked.truncate(limit);
        return ranked.into_iter().map(|(_, item)| item).collect();
    }

    found.truncate(limit);
    found
}

/// Opens the dropdown under the address bar.
#[tauri::command]
pub async fn suggest_open(
    app: AppHandle,
    window: tauri::Window,
    x: f64,
    width: f64,
) -> Result<(), AppError> {
    if app
        .state::<AppState>()
        .windows.overlay
        .read()
        .contains_key(window.label())
    {
        return Ok(());
    }
    app.state::<AppState>().ui.write().suggest_width = width;
    crate::overlay::open_at(
        &app,
        &window,
        "suggest",
        x,
        chrome_height(&app) + 2.0,
        width,
        48.0,
    )
    .map_err(|e| AppError::new("overlay", e))
}

/// Recomputes the dropdown and pushes it into the overlay page.
#[tauri::command]
pub async fn suggest_feed(
    app: AppHandle,
    window: tauri::Window,
    state: State<'_, AppState>,
    text: String,
) -> Result<Vec<Suggestion>, AppError> {
    state
        .windows.suggest_text
        .write()
        .insert(window.label().to_string(), text.clone());
    Ok(push_suggestions(&app, window.label(), &state))
}

/// Highlights one row of the open dropdown; -1 clears it.
#[tauri::command]
pub async fn suggest_active(
    app: AppHandle,
    window: tauri::Window,
    state: State<'_, AppState>,
    index: i64,
) -> Result<(), AppError> {
    let label = state
        .windows.overlay
        .read()
        .get(window.label())
        .cloned();
    let Some(label) = label else {
        return Ok(());
    };
    if let Some(webview) = app.get_webview(&label) {
        let _ = webview.eval(&format!(
            "window.__mixSuggestActive && window.__mixSuggestActive({index})"
        ));
    }
    Ok(())
}

/// Re-sends the current suggestions to a page that just finished loading.
#[tauri::command]
pub async fn suggest_refresh(
    app: AppHandle,
    window: tauri::Window,
    state: State<'_, AppState>,
) -> Result<Vec<Suggestion>, AppError> {
    Ok(push_suggestions(&app, window.label(), &state))
}

fn push_suggestions(app: &AppHandle, window_label: &str, state: &AppState) -> Vec<Suggestion> {
    let text = state
        .windows.suggest_text
        .read()
        .get(window_label)
        .cloned()
        .unwrap_or_default();
    let items = collect_suggestions(state, &text, 8);
    let count = items.len();

    let label = app
        .state::<AppState>()
        .windows.overlay
        .read()
        .get(window_label)
        .cloned();

    let Some(label) = label else {
        return Vec::new();
    };
    let Some(webview) = app.get_webview(&label) else {
        return Vec::new();
    };

    let json = serde_json::to_string(&items).unwrap_or_else(|_| "[]".into());
    let rows = count as f64;
    let height = if rows == 0.0 { 40.0 } else { rows * 34.0 + 14.0 };
    let width = state.ui.read().suggest_width;
    let _ = webview.set_size(tauri::LogicalSize::new(width, height));
    let _ = webview.eval(&format!("window.__mixSuggest && window.__mixSuggest({json})"));
    items
}

/// Pages forward typed characters here while the address bar owns the keyboard.
#[tauri::command]
pub async fn url_type(
    app: AppHandle,
    window: tauri::Window,
    state: State<'_, AppState>,
    text: String,
) -> Result<(), AppError> {
    if !state
        .windows.url_edit
        .read()
        .contains_key(window.label())
    {
        return Ok(());
    }
    let _ = app.emit_to(window.label(), "ui:url-insert", text);
    Ok(())
}

#[tauri::command]
pub async fn url_key(
    app: AppHandle,
    window: tauri::Window,
    _state: State<'_, AppState>,
    key: String,
) -> Result<(), AppError> {
    match key.as_str() {
        "enter" => {
            exit_url_edit(&app, window.label());
            let _ = app.emit_to(window.label(), "url:commit", ());
        }
        "escape" => {
            exit_url_edit(&app, window.label());
            let _ = app.emit_to(window.label(), "url:cancel", ());
        }
        other => {
            let _ = app.emit_to(window.label(), "ui:url-key", other.to_string());
        }
    }
    Ok(())
}

#[tauri::command]
pub async fn url_blur(app: AppHandle, window: tauri::Window) -> Result<(), AppError> {
    exit_url_edit(&app, window.label());
    Ok(())
}

/// Turns the page-side key bridge on or off for every tab in a window.
pub(crate) fn set_url_edit(app: &AppHandle, window: &tauri::Window, on: bool) {
    let state = app.state::<AppState>();
    let tabs: Vec<String> = match state.try_read_core(window.label()) {
        Some(core) => core.tabs.iter().map(|tab| tab.id.clone()).collect(),
        None => Vec::new(),
    };
    let script = if on {
        "window.__mixUrlMode = true; window.dispatchEvent(new Event('mix-url-mode'))"
    } else {
        "window.__mixUrlMode = false; window.dispatchEvent(new Event('mix-url-mode'))"
    };
    let count = tabs.len();
    for id in tabs {
        if let Some(webview) = app.get_webview(&id) {
            let _ = webview.eval(script);
        }
    }
    crate::logging::info(&format!("url edit {on} for {} ({count} tabs)", window.label()));
    if on {
        state
            .windows.url_edit
            .write()
            .insert(window.label().to_string(), String::new());
    }
}

fn exit_url_edit(app: &AppHandle, window_label: &str) {
    if let Some(state) = app.try_state::<AppState>() {
        state
            .windows.url_edit
            .write()
            .remove(window_label);
    }
    if let Some(win) = app.get_window(window_label) {
        set_url_edit(app, &win, false);
    }
}

/// Drives the browser's own find through the page.
#[tauri::command]
pub async fn find_in_page(
    app: AppHandle,
    window: tauri::Window,
    state: State<'_, AppState>,
    text: String,
    forward: Option<bool>,
    restart: Option<bool>,
) -> Result<(), AppError> {
    let id = selected_label(&state, &window);
    let Some(webview) = app.get_webview(&id) else {
        return Ok(());
    };
    let query = serde_json::to_string(&text).unwrap_or_else(|_| "\"\"".into());
    let back = forward.map(|value| !value).unwrap_or(false);
    let again = restart.unwrap_or(false);
    let script = format!(
        "(function(){{var hit=window.find({query}, false, {back}, true, {again}, false, false);\
window.__TAURI_INTERNALS__.invoke('find_result', {{ hit: Boolean(hit) }}).catch(function(){{}});}})()"
    );
    let _ = webview.eval(&script);
    Ok(())
}

/// The page reports the outcome of a find, because eval cannot return a value.
#[tauri::command]
pub async fn find_result(state: State<'_, AppState>, hit: bool) -> Result<(), AppError> {
    state.ui.write().find_hit = hit;
    Ok(())
}

#[tauri::command]
pub async fn find_status(state: State<'_, AppState>) -> Result<bool, AppError> {
    Ok(state.ui.read().find_hit)
}


/// Runs a text edit in the chrome page, which owns the address field.
#[tauri::command]
pub async fn url_action(
    app: AppHandle,
    webview: tauri::Webview,
    action: String,
) -> Result<(), AppError> {
    require_trusted(&webview)?;
    if !matches!(action.as_str(), "cut" | "copy" | "paste" | "select-all") {
        return Err(AppError::new("bad_action", action));
    }
    let label = webview.window().label().to_string();
    if let Some(chrome) = app.get_webview(&format!("{label}-chrome")) {
        let encoded = serde_json::to_string(&action).unwrap_or_else(|_| "\"\"".to_string());
        let _ = chrome.eval(&format!("window.__mixUrlAction && window.__mixUrlAction({encoded})"));
    }
    Ok(())
}
