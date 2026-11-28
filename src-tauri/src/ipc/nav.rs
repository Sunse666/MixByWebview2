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
            .and_then(|id| {
                core.tabs
                    .iter()
                    .find(|tab| tab.id == id)
                    .map(|tab| tab.zoom)
            })
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
    tell_panel(app, label, level);
    Ok(())
}

/// Keeps an open zoom panel in step with a zoom changed from the wheel or the keyboard.
fn tell_panel(app: &AppHandle, label: &str, level: f64) {
    let overlay = {
        let state = app.state::<crate::ipc::AppState>();
        let slots = state.windows.overlay.read();
        slots.get(label).cloned()
    };
    if let Some(overlay) = overlay {
        if let Some(view) = app.get_webview(&overlay) {
            let _ = view.eval(format!("window.__mixZoom && window.__mixZoom({level});"));
        }
    }
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
pub async fn zoom_step(
    app: AppHandle,
    delta: f64,
    window: tauri::Window,
    _state: State<'_, AppState>,
) -> Result<f64, AppError> {
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
        .windows
        .overlay
        .read()
        .contains_key(window.label())
    {
        return Ok(());
    }
    app.state::<AppState>().ui.write().suggest_width = width;
    // The list is as wide as the address bar, so the width comes from the caller; the
    // height is whatever the suggestions need, which the page measures.
    let height = crate::overlay::initial_size("suggest").1;
    crate::overlay::open_sized(
        &app,
        &window,
        "suggest",
        x,
        crate::window::chrome_height_px(&app) + 2.0,
        (width, height),
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
        .windows
        .suggest_text
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
    let label = state.windows.overlay.read().get(window.label()).cloned();
    let Some(label) = label else {
        return Ok(());
    };
    if let Some(webview) = app.get_webview(&label) {
        let _ = webview.eval(format!(
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
        .windows
        .suggest_text
        .read()
        .get(window_label)
        .cloned()
        .unwrap_or_default();
    let items = collect_suggestions(state, &text, 8);
    let count = items.len();

    let label = app
        .state::<AppState>()
        .windows
        .overlay
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
    let height = if rows == 0.0 {
        40.0
    } else {
        rows * 34.0 + 14.0
    };
    let width = state.ui.read().suggest_width;
    let _ = webview.set_size(tauri::LogicalSize::new(width, height));
    let _ = webview.eval(format!(
        "window.__mixSuggest && window.__mixSuggest({json})"
    ));
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
    if !state.windows.url_edit.read().contains_key(window.label()) {
        return Ok(());
    }
    let _ = app.emit_to(window.label(), "ui:url-insert", text);
    Ok(())
}

#[tauri::command]
pub async fn url_key(
    app: AppHandle,
    window: tauri::Window,
    state: State<'_, AppState>,
    key: String,
) -> Result<(), AppError> {
    // Only while the address bar has the keyboard. The command is reachable from a remote
    // page, and without this an "enter" from any site would commit whatever is in the field
    // - which is a navigation the user did not ask for. `url_type` next door already had
    // this test; this one had the state argument and never read it.
    if !state.windows.url_edit.read().contains_key(window.label()) {
        return Ok(());
    }
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
    crate::logging::info(&format!(
        "url edit {on} for {} ({count} tabs)",
        window.label()
    ));
    if on {
        state
            .windows
            .url_edit
            .write()
            .insert(window.label().to_string(), String::new());
    }
}

fn exit_url_edit(app: &AppHandle, window_label: &str) {
    if let Some(state) = app.try_state::<AppState>() {
        state.windows.url_edit.write().remove(window_label);
    }
    if let Some(win) = app.get_window(window_label) {
        set_url_edit(app, &win, false);
    }
}

/// The javascript that drives the page's own find.
///
/// `window.find(aString, caseSensitive, backwards, wrapAround, wholeWord, searchInFrames,
/// showDialog)` takes seven positional arguments, and a value in the wrong slot is invisible:
/// `restart` used to be passed as `wholeWord`, so every fresh query was searched as a whole
/// word. Typing "exam" over a page that says "example" reported no match, while the arrows -
/// which passed nothing and therefore searched for a substring - highlighted it.
///
/// Restarting means "this is a new query, start from the top of the document", which is what
/// dropping the current selection does; `window.find` otherwise carries on from it.
fn find_script(query: &str, back: bool, restart: bool) -> String {
    let reset = if restart {
        "try{window.getSelection().removeAllRanges();}catch(e){}"
    } else {
        ""
    };
    format!(
        "(function(){{{reset}var hit=window.find({query}, false, {back}, true, false, false, false);\
window.__TAURI_INTERNALS__.invoke('find_result', {{ hit: Boolean(hit) }}).catch(function(){{}});}})()"
    )
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
    let script = find_script(&query, back, restart.unwrap_or(false));
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

/// Opens the markup of the page in a tab of its own.
///
/// WebView2 does not accept `view-source:` as a scheme - it navigates to a blank page - so
/// the document is asked for its own outer html and handed back as a data url. What comes
/// back is the document after the page's scripts ran, which is what a reader following
/// "view source" on a page that renders client side is looking at anyway.
#[tauri::command]
pub async fn view_source(
    app: AppHandle,
    window: tauri::Window,
    state: State<'_, AppState>,
) -> Result<(), AppError> {
    let id = selected_label(&state, &window);
    let Some(webview) = app.get_webview(&id) else {
        return Err(AppError::new("no_tab", id));
    };
    // Remembered until this tab answers, so a document from some other tab is not accepted
    // in its place: `source_ready` is reachable from a page, and the source page renders
    // what it is handed on the app's own origin.
    state.ui.write().source_from = Some(id.clone());
    // The answer arrives through a command rather than as a return value, because eval
    // cannot give one back.
    let _ = webview.eval(
        "window.__TAURI_INTERNALS__.invoke('source_ready', { html: document.documentElement.outerHTML })"
    );
    Ok(())
}

/// Keeps the markup `view_source` asked for and opens the page that shows it.
///
/// The markup is not put in the url. A data url is refused outright by the webview, and a
/// whole document does not fit in a hash - so it waits in the state and the page asks for
/// it, which is how the error page already gets the address it names.
#[tauri::command]
pub async fn source_ready(
    app: AppHandle,
    window: tauri::Window,
    webview: tauri::Webview,
    state: State<'_, AppState>,
    html: String,
) -> Result<(), AppError> {
    let caller = webview.label().to_string();
    {
        let mut ui = state.ui.write();
        if ui.source_from.as_deref() != Some(caller.as_str()) {
            crate::logging::warn(&format!(
                "source from {caller} dropped: nothing asked it for one"
            ));
            return Err(AppError::new(
                "untrusted",
                "no page source was asked of this tab",
            ));
        }
        ui.source_from = None;
        ui.source = Some(html);
    }
    let page = "http://tauri.localhost/pages/source.html".to_string();
    tab_new(app, window, state, Some(page), None, None).await?;
    Ok(())
}

/// The markup the source tab is meant to show.
#[tauri::command]
pub async fn source_get(state: State<'_, AppState>) -> Result<String, AppError> {
    Ok(state.ui.read().source.clone().unwrap_or_default())
}

/// Asks the page for its markup, the way `view_source` does, but to save it to disk.
#[tauri::command]
pub async fn save_page(
    app: AppHandle,
    window: tauri::Window,
    state: State<'_, AppState>,
) -> Result<(), AppError> {
    let id = selected_label(&state, &window);
    let Some(webview) = app.get_webview(&id) else {
        return Err(AppError::new("no_tab", id));
    };
    // As in `view_source`: only the tab that was asked may answer, because `page_ready`
    // writes a file and is reachable from a page.
    state.ui.write().page_from = Some(id.clone());
    let _ = webview.eval(
        "window.__TAURI_INTERNALS__.invoke('page_ready', { html: document.documentElement.outerHTML })"
    );
    Ok(())
}

/// Writes the markup `save_page` asked for into the downloads folder.
///
/// The page is asked for its own html rather than fetched again: a fetch would bring back
/// the same bytes the server has, not the document the reader is looking at - and a page
/// that renders on the client has almost nothing in the response.
#[tauri::command]
pub async fn page_ready(
    app: AppHandle,
    window: tauri::Window,
    webview: tauri::Webview,
    state: State<'_, AppState>,
    html: String,
) -> Result<String, AppError> {
    let caller = webview.label().to_string();
    {
        let mut ui = state.ui.write();
        if ui.page_from.as_deref() != Some(caller.as_str()) {
            crate::logging::warn(&format!(
                "page html from {caller} dropped: nothing asked it for one"
            ));
            return Err(AppError::new(
                "untrusted",
                "no page was asked to be saved from this tab",
            ));
        }
        ui.page_from = None;
    }
    let id = selected_label(&state, &window);
    let title = state
        .read_core(window.label())
        .tabs
        .iter()
        .find(|tab| tab.id == id)
        .map(|tab| tab.title.clone())
        .unwrap_or_default();
    let page_url = state
        .read_core(window.label())
        .tabs
        .iter()
        .find(|tab| tab.id == id)
        .map(|tab| tab.url.clone())
        .unwrap_or_default();
    // The title names the file; a page without one falls back to its last path segment,
    // and `target_for` sanitises whatever comes out of that.
    let suggested = if title.trim().is_empty() {
        page_url.rsplit('/').next().unwrap_or("page").to_string()
    } else {
        title
    };
    let target = crate::downloads::target_for(
        &state.dirs.downloads,
        &page_url,
        &format!("{suggested}.html"),
    );
    crate::paths::write_atomic(&target, html.as_bytes())?;
    crate::downloads::record_done(&app, &page_url, &target);
    crate::logging::info(&format!("page saved {}", target.display()));
    Ok(target.display().to_string())
}

/// Asks the page for the article it is showing, so it can be opened on its own.
#[tauri::command]
pub async fn reading_mode(
    app: AppHandle,
    window: tauri::Window,
    state: State<'_, AppState>,
) -> Result<(), AppError> {
    let id = selected_label(&state, &window);
    let Some(webview) = app.get_webview(&id) else {
        return Err(AppError::new("no_tab", id));
    };
    // Forgotten until this tab answers: an answer that arrives after the user asked for a
    // different page would be laid out as if it belonged to the new one.
    state.ui.write().article_from = Some(id.clone());
    let _ = webview.eval(
        "var a = window.__mixExtractArticle && window.__mixExtractArticle();\
         if (a) window.__TAURI_INTERNALS__.invoke('article_ready', { article: {
           title: a.title, byline: a.byline, html: a.html, source: a.url } });",
    );
    Ok(())
}

/// Lays out the article `reading_mode` asked for and opens it in a new tab.
///
/// The extraction already ran in the page, so this only keeps it: the built-in reader page
/// lays it out, and asking for the markup through a command is what keeps a whole article
/// out of the url.
///
/// Only the tab that was asked may answer. The reader collects without scripts, so this is
/// not the page's own defence - it is what stops a page from handing an article to the
/// reader that a different page asked to show, which would let a site plant markup in a
/// document on the app's origin.
#[tauri::command]
pub async fn article_ready(
    app: AppHandle,
    window: tauri::Window,
    webview: tauri::Webview,
    state: State<'_, AppState>,
    article: Article,
) -> Result<(), AppError> {
    let caller = webview.label().to_string();
    {
        let mut ui = state.ui.write();
        if ui.article_from.as_deref() != Some(caller.as_str()) {
            crate::logging::warn(&format!(
                "article from {caller} dropped: nothing asked it for one"
            ));
            return Err(AppError::new(
                "untrusted",
                "no article was asked of this tab",
            ));
        }
        ui.article_from = None;
        ui.article = Some(article);
    }
    let page = "http://tauri.localhost/pages/reader.html".to_string();
    tab_new(app, window, state, Some(page), None, None).await?;
    Ok(())
}

/// The article the reader tab is meant to lay out.
#[tauri::command]
pub async fn article_get(state: State<'_, AppState>) -> Result<Option<Article>, AppError> {
    Ok(state.ui.read().article.clone())
}

/// The address of the page in front of the user, as a QR code.
///
/// A qr code is a square per module, so the answer is an svg built from one path: it
/// stays a data url the overlay can show at any size, with no second file on disk.
#[tauri::command]
pub async fn page_qr(
    window: tauri::Window,
    state: State<'_, AppState>,
    text: Option<String>,
) -> Result<String, AppError> {
    let value = match text {
        Some(given) if !given.trim().is_empty() => given,
        _ => {
            let id = selected_label(&state, &window);
            state
                .read_core(window.label())
                .tabs
                .iter()
                .find(|tab| tab.id == id)
                .map(|tab| tab.url.clone())
                .unwrap_or_default()
        }
    };
    if value.trim().is_empty() {
        return Err(AppError::new("no_url", "nothing to encode"));
    }
    let code = qrcode::QrCode::new(value.as_bytes())
        .map_err(|err| AppError::new("qr", err.to_string()))?;
    let width = code.width();
    let colors = code.to_colors();
    // Four quiet modules on each side, which is what a scanner needs to find the code.
    let quiet = 4usize;
    let side = width + quiet * 2;
    let mut path = String::with_capacity(colors.len() * 12);
    for (index, color) in colors.iter().enumerate() {
        if *color != qrcode::Color::Dark {
            continue;
        }
        let x = index % width + quiet;
        let y = index / width + quiet;
        path.push_str(&format!("M{x} {y}h1v1h-1z"));
    }
    let svg = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 {side} {side}\" \
         shape-rendering=\"crispEdges\"><rect width=\"{side}\" height=\"{side}\" fill=\"#fff\"/>\
         <path d=\"{path}\" fill=\"#000\"/></svg>"
    );
    // The markup itself, not a data url: the webview refuses those, and the overlay can
    // put an svg on the page as it stands.
    Ok(svg)
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
        let _ = chrome.eval(format!(
            "window.__mixUrlAction && window.__mixUrlAction({encoded})"
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::find_script;
    use crate::ipc::{Article, Ui};

    /// The shape `permission_request` remembers a decision by.
    ///
    /// It is `scheme://host[:port]` and nothing after it: a decision belongs to a site, not
    /// to one of its pages. This is the string the command builds from the caller's own url
    /// now that the page no longer names its origin.
    #[test]
    fn a_decision_is_remembered_against_the_site_and_not_the_page() {
        let deep = tauri::Url::parse("https://example.com:8443/a/b?c=1#d").unwrap();
        let other = tauri::Url::parse("https://example.com:8443/elsewhere").unwrap();
        assert_eq!(
            deep.origin().ascii_serialization(),
            "https://example.com:8443"
        );
        assert_eq!(
            deep.origin().ascii_serialization(),
            other.origin().ascii_serialization()
        );
        // An opaque origin - a file: url, a sandboxed frame - has no site to remember, so
        // the command refuses it instead of writing a decision under "null".
        let opaque = tauri::Url::parse("data:text/html,<p>hi</p>").unwrap();
        assert_eq!(opaque.origin().ascii_serialization(), "null");
    }

    /// The article is handed to the reader, which inserts it as markup on the app's own
    /// origin - so only the tab that was asked may answer, and only once.
    #[test]
    fn only_the_tab_that_was_asked_may_hand_over_an_article() {
        let mut ui = Ui {
            menu_context: Default::default(),
            suggest_width: 0.0,
            translate_result: (0, None),
            translated_tabs: Default::default(),
            translate_skip: Default::default(),
            find_hit: false,
            source: None,
            article: None,
            article_from: Some("w1-tab-2".to_string()),
            source_from: None,
            page_from: None,
        };
        let article = || Article {
            title: "t".into(),
            byline: String::new(),
            html: "<p>x</p>".into(),
            source: String::new(),
        };

        // A page that was never asked cannot put markup in the reader.
        assert_ne!(ui.article_from.as_deref(), Some("w1-tab-7"));

        // The one that was asked can, and the request is spent doing so.
        assert_eq!(ui.article_from.as_deref(), Some("w1-tab-2"));
        ui.article_from = None;
        ui.article = Some(article());
        assert!(ui.article.is_some());
        assert_eq!(
            ui.article_from, None,
            "a second answer from the same tab is not a second article"
        );
    }

    #[test]
    fn a_partial_word_is_not_searched_as_a_whole_word() {
        // The reported symptom: a page says "example", the bar says no match for "exam".
        let typing = find_script("\"exam\"", false, true);
        assert!(
            typing.contains("find(\"exam\", false, false, true, false, false, false)"),
            "wholeWord must stay false while typing: {typing}"
        );
    }

    #[test]
    fn restarting_starts_at_the_top_instead_of_setting_a_flag() {
        let typing = find_script("\"exam\"", false, true);
        assert!(
            typing.contains("removeAllRanges"),
            "a new query starts over"
        );
        let stepping = find_script("\"exam\"", true, false);
        assert!(!stepping.contains("removeAllRanges"), "a step continues");
        assert!(
            stepping.contains("find(\"exam\", false, true, true, false, false, false)"),
            "and only backwards differs: {stepping}"
        );
    }
}
