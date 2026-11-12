//! History, bookmarks and the favicon cache.

use super::*;

/// The favicon cache, stored as one file per host.
pub(crate) fn cache_icon(app: &AppHandle, host: &str, url: &str) {
    if host.is_empty() || !url.starts_with("http") {
        return;
    }
    let Some(state) = app.try_state::<AppState>() else {
        return;
    };
    let dir = state.dirs.favicons_dir.clone();
    if crate::favicons::cache_path(&dir, host).is_some() {
        return;
    }
    crate::logging::info(&format!("favicon download: {host} <- {url}"));
    let handle = app.clone();
    let host = host.to_string();
    let url = url.to_string();
    tauri::async_runtime::spawn(async move {
        if crate::favicons::cache_path(&dir, &host).is_some() {
            return;
        }
        let Ok(client) = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .build()
        else {
            return;
        };
        let response = match client.get(&url).send().await {
            Ok(response) => response,
            Err(err) => {
                crate::logging::warn(&format!("favicon fetch failed: {host}: {err}"));
                return;
            }
        };
        let content_type = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .unwrap_or_default()
            .to_string();
        let bytes = match response.bytes().await {
            Ok(bytes) => bytes,
            Err(err) => {
                crate::logging::warn(&format!("favicon body failed: {host}: {err}"));
                return;
            }
        };
        match crate::favicons::store_icon(&dir, &host, &bytes, &content_type) {
            Some(path) => crate::logging::info(&format!("favicon cached: {}", path.display())),
            None => crate::logging::warn(&format!(
                "favicon not stored: {host} ({} bytes, {content_type})",
                bytes.len()
            )),
        }
        let _ = handle;
    });
}

/// Fills in the title that arrives after the visit was recorded.
pub(crate) fn attach_title(app: &AppHandle, url: &str, title: &str) {
    if title.trim().is_empty() {
        return;
    }
    let Some(state) = app.try_state::<AppState>() else {
        return;
    };
    let changed = {
        let mut visits = state.library.history.write();
        match visits.iter_mut().find(|visit| visit.url == url) {
            Some(visit) if visit.title != title => {
                visit.title = title.to_string();
                true
            }
            _ => false,
        }
    };
    if changed {
        crate::persist::touch_history();
    }
}

/// Patches the newest visit for a url with its icon.
pub(crate) fn attach_icon(app: &AppHandle, url: &str, icon: &str) {
    let Some(state) = app.try_state::<AppState>() else {
        return;
    };
    let changed = {
        let mut visits = state.library.history.write();
        match visits
            .iter_mut()
            .find(|visit| visit.url == url && visit.icon.as_deref() != Some(icon))
        {
            Some(visit) => {
                visit.icon = Some(icon.to_string());
                true
            }
            None => false,
        }
    };
    if changed {
        crate::persist::touch_history();
    }
}

pub(crate) fn record_visit(app: &AppHandle, url: &str, title: &str) {
    if !crate::library::worth_recording(url) || trusted_url(url) {
        return;
    }
    let Some(state) = app.try_state::<AppState>() else {
        return;
    };
    {
        let mut visits = state.library.history.write();
        crate::library::push_visit(&mut visits, url, title);
    }
    crate::persist::touch_history();
}

/// Writes history once, for the paths that only touch memory otherwise.
pub(crate) fn flush_history(app: &AppHandle) {
    let Some(state) = app.try_state::<AppState>() else {
        return;
    };
    let snapshot = state.library.history.read().clone();
    if let Err(err) = crate::library::save_visits(&state.dirs.history_file, &snapshot) {
        crate::logging::warn(&format!("history flush failed: {err}"));
    }
}

/// host -> icon url, used by the bookmarks and new tab pages.
#[tauri::command]
pub async fn favicon_map(state: State<'_, AppState>) -> Result<std::collections::BTreeMap<String, String>, AppError> {
    let icons = state.library.favicons.read();
    let mut out = std::collections::BTreeMap::new();
    for (host, remote) in icons.hosts.iter() {
        let value = crate::favicons::cache_path(&state.dirs.favicons_dir, host)
            .and_then(|path| crate::favicons::data_url(&path))
            .unwrap_or_else(|| remote.clone());
        out.insert(host.clone(), value);
    }
    Ok(out)
}

#[tauri::command]
pub async fn history_list(_window: tauri::Window, state: State<'_, AppState>, limit: Option<usize>) -> Result<Vec<Visit>, AppError> {
    let visits = state.library.history.read();
    let icons = state.library.favicons.read();
    let take = limit.unwrap_or(500).min(visits.len());
    Ok(visits
        .iter()
        .take(take)
        .map(|visit| {
            let mut copy = visit.clone();
            if copy.icon.is_none() {
                let host = crate::favicons::host_of(&visit.url);
                copy.icon = icons.get(&host).map(str::to_string);
            }
            copy
        })
        .collect())
}

/// Removes the rows the history page has selected.
#[tauri::command]
pub async fn history_remove(
    webview: tauri::Webview,
    state: State<'_, AppState>,
    entries: Vec<(String, u64)>,
) -> Result<usize, AppError> {
    require_trusted(&webview)?;
    let removed = {
        let mut visits = state.library.history.write();
        let before = visits.len();
        visits.retain(|visit| {
            !entries
                .iter()
                .any(|(url, at)| visit.url == *url && visit.at == *at)
        });
        if visits.len() == before {
            visits.retain(|visit| !entries.iter().any(|(url, _)| visit.url == *url));
        }
        before - visits.len()
    };
    let snapshot = state.library.history.read().clone();
    crate::library::save_visits(&state.dirs.history_file, &snapshot)?;
    crate::logging::info(&format!("history: removed {removed} entries"));
    Ok(removed)
}

/// Clears the selected kinds of local data and reports what was cleared.
#[tauri::command]
pub async fn clear_data(
    app: AppHandle,
    webview: tauri::Webview,
    state: State<'_, AppState>,
    kinds: Vec<String>,
) -> Result<Vec<String>, AppError> {
    require_trusted(&webview)?;
    let mut done = Vec::new();

    if kinds.iter().any(|kind| kind == "history") {
        state.library.history.write().clear();
        crate::library::save_visits(&state.dirs.history_file, &[])?;
        done.push("history".to_string());
    }

    if kinds.iter().any(|kind| kind == "bookmarks") {
        state.library.bookmarks.write().clear();
        crate::library::save_bookmarks(&state.dirs.bookmarks_file, &[])?;
        let _ = app.emit("bookmarks:changed", false);
        done.push("bookmarks".to_string());
    }

    if kinds.iter().any(|kind| kind == "favicons") {
        state
            .library.favicons
            .write()
            
            .clear();
        let _ = std::fs::remove_dir_all(&state.dirs.favicons_dir);
        let empty = state.library.favicons.read().clone();
        empty.save(&state.dirs.favicons_file);
        done.push("favicons".to_string());
    }

    if kinds.iter().any(|kind| kind == "session") {
        let _ = std::fs::remove_file(&state.dirs.session_file);
        done.push("session".to_string());
    }

    if kinds.iter().any(|kind| kind == "zoom") {
        let mut zoom = state.library.zoom.write();
        zoom.forget_all();
        let _ = zoom.save(&state.dirs.zoom_file);
        done.push("zoom".to_string());
    }

    crate::logging::info(&format!("cleared: {}", done.join(", ")));
    Ok(done)
}

#[tauri::command]
pub async fn history_clear(webview: tauri::Webview, state: State<'_, AppState>) -> Result<(), AppError> {
    require_trusted(&webview)?;
    {
        let mut visits = state.library.history.write();
        visits.clear();
    }
    crate::library::save_visits(&state.dirs.history_file, &[])?;
    crate::logging::info("history cleared");
    Ok(())
}

#[tauri::command]
pub async fn bookmark_list(_window: tauri::Window, state: State<'_, AppState>) -> Result<Vec<Bookmark>, AppError> {
    Ok(state.library.bookmarks.read().clone())
}

/// Toggles the selected tab and reports whether it is bookmarked now.
#[tauri::command]
pub async fn bookmark_toggle(app: AppHandle, window: tauri::Window, state: State<'_, AppState>) -> Result<bool, AppError> {
    let (url, title) = {
        let core = state.read_core(window.label());
        match core
            .selected
            .as_ref()
            .and_then(|id| core.tabs.iter().find(|tab| &tab.id == id))
        {
            Some(tab) => (tab.url.clone(), tab.title.clone()),
            None => return Ok(false),
        }
    };

    let (added, snapshot) = {
        let mut bookmarks = state.library.bookmarks.write();
        let added = crate::library::toggle_bookmark(&mut bookmarks, &url, &title);
        (added, bookmarks.clone())
    };
    crate::library::save_bookmarks(&state.dirs.bookmarks_file, &snapshot)?;
    crate::logging::info(&format!("bookmark {url} -> {added}"));
    let _ = app.emit("bookmarks:changed", added);
    Ok(added)
}

/// Adds a bookmark for an explicit url; used when something is dropped on the bar.
#[tauri::command]
pub async fn bookmark_add(
    app: AppHandle,
    webview: tauri::Webview,
    state: State<'_, AppState>,
    url: String,
    title: String,
) -> Result<(), AppError> {
    require_trusted(&webview)?;
    if !url.starts_with("http://") && !url.starts_with("https://") {
        return Ok(());
    }
    let snapshot = {
        let mut bookmarks = state.library.bookmarks.write();
        if !bookmarks.iter().any(|bookmark| bookmark.url == url) {
            crate::library::toggle_bookmark(&mut bookmarks, &url, &title);
        }
        bookmarks.clone()
    };
    crate::library::save_bookmarks(&state.dirs.bookmarks_file, &snapshot)?;
    let _ = app.emit("bookmarks:changed", true);
    Ok(())
}

#[tauri::command]
pub async fn bookmark_move(
    app: AppHandle,
    webview: tauri::Webview,
    state: State<'_, AppState>,
    from: String,
    to: usize,
) -> Result<(), AppError> {
    require_trusted(&webview)?;
    let snapshot = {
        let mut bookmarks = state.library.bookmarks.write();
        move_bookmark(&mut bookmarks, &from, to);
        bookmarks.clone()
    };
    crate::library::save_bookmarks(&state.dirs.bookmarks_file, &snapshot)?;
    let _ = app.emit("bookmarks:changed", false);
    Ok(())
}

fn move_bookmark(bookmarks: &mut Vec<Bookmark>, from: &str, to: usize) -> bool {
    let Some(index) = bookmarks.iter().position(|bookmark| bookmark.url == from) else {
        return false;
    };
    let target = to.min(bookmarks.len().saturating_sub(1));
    if index == target {
        return false;
    }
    let item = bookmarks.remove(index);
    bookmarks.insert(target, item);
    true
}

/// Moves a bookmark into a folder; an empty name means the top level.
#[tauri::command]
pub async fn bookmark_set_folder(
    app: AppHandle,
    webview: tauri::Webview,
    state: State<'_, AppState>,
    url: String,
    folder: String,
) -> Result<bool, AppError> {
    require_trusted(&webview)?;
    let changed = {
        let mut list = state.library.bookmarks.write();
        let Some(item) = list.iter_mut().find(|item| item.url == url) else {
            return Err(AppError::new("not_found", "no such bookmark"));
        };
        let next = folder.trim().to_string();
        if item.folder == next {
            false
        } else {
            item.folder = next;
            true
        }
    };
    if changed {
        let snapshot = state.library.bookmarks.read().clone();
        crate::library::save_bookmarks(&state.dirs.bookmarks_file, &snapshot)?;
        let _ = app.emit("bookmarks:changed", false);
    }
    Ok(changed)
}
/// Renames a bookmark, and optionally re-points it at another url.
#[tauri::command]
pub async fn bookmark_update(
    app: AppHandle,
    webview: tauri::Webview,
    state: State<'_, AppState>,
    url: String,
    title: String,
    new_url: Option<String>,
) -> Result<bool, AppError> {
    require_trusted(&webview)?;
    let changed = {
        let mut list = state.library.bookmarks.write();
        let Some(item) = list.iter_mut().find(|item| item.url == url) else {
            return Err(AppError::new("not_found", "no such bookmark"));
        };
        let trimmed = title.trim().to_string();
        let mut changed = false;
        if !trimmed.is_empty() && item.title != trimmed {
            item.title = trimmed;
            changed = true;
        }
        if let Some(next) = new_url {
            let next = next.trim().to_string();
            if !next.is_empty() && item.url != next {
                item.url = next;
                changed = true;
            }
        }
        changed
    };

    if changed {
        let snapshot = state.library.bookmarks.read().clone();
        crate::library::save_bookmarks(&state.dirs.bookmarks_file, &snapshot)?;
        let _ = app.emit("bookmarks:changed", false);
    }
    Ok(changed)
}
#[tauri::command]
pub async fn bookmark_remove(
    app: AppHandle,
    webview: tauri::Webview, state: State<'_, AppState>,
    url: String,
) -> Result<(), AppError> {
    require_trusted(&webview)?;
    let snapshot = {
        let mut bookmarks = state.library.bookmarks.write();
        bookmarks.retain(|bookmark| bookmark.url != url);
        bookmarks.clone()
    };
    crate::library::save_bookmarks(&state.dirs.bookmarks_file, &snapshot)?;
    let _ = app.emit("bookmarks:changed", false);
    Ok(())
}
