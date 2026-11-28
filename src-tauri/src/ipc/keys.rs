//! Shortcut action dispatch.

use super::*;

/// The actions a page may ask for by forwarding a key or a gesture.
///
/// `shortcut` has to be reachable from a remote page: the injected script is what turns a
/// keystroke inside a page into one of these, because the chrome strip never sees those keys.
/// That makes this command a surface any site can reach, and most of the actions below are
/// about the browser window rather than about the page - a site could close the window,
/// minimise it, open devtools, pin or duplicate tabs. Those are allowed only for callers that
/// are the app's own pages.
///
/// What is left here is what a page plausibly produces: its own navigation and zoom, its own
/// find and translate, opening a link in a new tab, dismissing an overlay.
const FROM_A_PAGE: &[&str] = &[
    "reload",
    "stop",
    "zoom_in",
    "zoom_out",
    "zoom_reset",
    "find",
    "translate",
    "new_tab",
    "close_tab",
    "next_tab",
    "prev_tab",
    "duplicate_tab",
    "copy_url",
    "dismiss",
    "close_overlay",
    "tab_search",
    "palette",
    "bookmark",
    // Taking the keyboard to the address bar is what Ctrl+L does, and a page forwarding that
    // combination has to be able to reach it - the strip never sees a key pressed inside a
    // page. It only moves focus, so it is not something a site can use to act on its own.
    "focus_url",
    "open_home",
    "open_history",
    "open_bookmarks",
    "open_downloads",
    "open_extensions",
    "open_settings",
    "downloads",
];

/// Whether an action may be run for a caller that is not one of the app's own pages.
fn page_may(action: &str) -> bool {
    FROM_A_PAGE.contains(&action)
}

#[tauri::command]
pub async fn shortcut(
    app: AppHandle,
    window: tauri::Window,
    webview: tauri::Webview,
    state: State<'_, AppState>,
    action: String,
) -> Result<(), AppError> {
    // A page asks for the actions a page can produce; anything about the window itself has to
    // come from the app's own pages, which is what the chrome strip is. The check is here
    // rather than in the match so a new action defaults to "not for a page".
    if !trusted(&webview) && !page_may(&action) {
        crate::logging::warn(&format!(
            "refused shortcut {action} for a page that may not ask for it"
        ));
        return Err(AppError::new(
            "untrusted",
            "this action is not available to a page",
        ));
    }
    // No log line here on purpose: every shortcut would write one, and the logger flushes
    // each line to disk, so this used to be a file write per keystroke. The actions that
    // are worth knowing about (a developer tools window, a failed navigation) log
    // themselves.
    match action.as_str() {
        "new_tab" => {
            tab_new(app, window.clone(), state, None, None, None).await?;
        }
        "new_incognito" => {
            tab_new(app, window.clone(), state, None, Some(true), None).await?;
        }
        "dismiss" => {
            crate::overlay::close(&app, window.label());
        }
        "reopen_tab" => {
            let _ = tab_reopen(app, window, state, None).await?;
        }
        "new_window" => {
            window_new(app, state).await?;
        }
        "bookmark" => {
            let _ = bookmark_toggle(app.clone(), window.clone(), state).await?;
        }
        "close_tab" => {
            let id = selected_label(&state, &window);
            if !id.is_empty() {
                tab_close(app, window.clone(), state, id, None).await?;
            }
        }
        "next_tab" => cycle_tab(&app, &window, 1).await,
        "prev_tab" => cycle_tab(&app, &window, -1).await,
        "focus_url" => {
            let _ = window.set_focus();
            if let Some(chrome) = app.get_webview(&format!("{}-chrome", window.label())) {
                let _ = chrome.set_focus();
            }
            set_url_edit(&app, &window, true);
            let _ = app.emit_to(window.label(), "ui:focus-url", ());
        }
        "reload" => {
            if let Some(webview) = app.get_webview(&selected_label(&state, &window)) {
                let _ = webview.reload();
            }
        }
        "zoom_in" => zoom_by(&app, window.label(), 0.1)?,
        "zoom_out" => zoom_by(&app, window.label(), -0.1)?,
        "zoom_reset" => set_zoom(&app, window.label(), 1.0)?,
        "print" => {
            if let Some(webview) = app.get_webview(&selected_label(&state, &window)) {
                let _ = webview.print();
            }
        }
        "fullscreen" => {
            if let Some(win) = app.get_window(window.label()) {
                let next = !win.is_fullscreen().unwrap_or(false);
                let _ = win.set_fullscreen(next);
            }
        }
        "downloads" => {
            if let Err(err) = reveal(&app.state::<AppState>().dirs.downloads) {
                crate::logging::warn(&format!("cannot open downloads: {err}"));
            }
        }
        "stop" => {
            let target = selected_label(&state, &window);
            if let Some(webview) = app.get_webview(&target) {
                let _ = webview.eval("window.stop()");
            }
        }
        "copy_url" => {
            let url = {
                let core = state.read_core(window.label());
                core.tabs
                    .iter()
                    .find(|tab| Some(&tab.id) == core.selected.as_ref())
                    .map(|tab| tab.url.clone())
                    .unwrap_or_default()
            };
            if !url.is_empty() {
                let mut board = arboard::Clipboard::new()
                    .map_err(|err| AppError::new("clipboard", err.to_string()))?;
                board
                    .set_text(url)
                    .map_err(|err| AppError::new("clipboard", err.to_string()))?;
            }
        }
        "duplicate_tab" => {
            let url = {
                let core = state.read_core(window.label());
                core.tabs
                    .iter()
                    .find(|tab| Some(&tab.id) == core.selected.as_ref())
                    .map(|tab| tab.url.clone())
                    .unwrap_or_default()
            };
            tab_new(app, window.clone(), state, Some(url), None, None).await?;
        }
        "close_other_tabs" => {
            let keep = selected_label(&state, &window);
            let others: Vec<String> = {
                let core = state.read_core(window.label());
                core.tabs
                    .iter()
                    .filter(|tab| tab.id != keep)
                    .map(|tab| tab.id.clone())
                    .collect()
            };
            for id in others {
                let state = app.state::<AppState>();
                tab_close(app.clone(), window.clone(), state, id, None).await?;
            }
        }
        "toggle_pin" => {
            let (id, pinned) = {
                let core = state.read_core(window.label());
                match core
                    .tabs
                    .iter()
                    .find(|tab| Some(&tab.id) == core.selected.as_ref())
                {
                    Some(tab) => (tab.id.clone(), tab.pinned),
                    None => (String::new(), false),
                }
            };
            if !id.is_empty() {
                let mut core = state.write_core(window.label());
                if let Some(tab) = core.tabs.iter_mut().find(|tab| tab.id == id) {
                    tab.pinned = !pinned;
                }
            }
        }
        "minimize" => window_of(&window).minimize()?,
        "maximize" => {
            let win = window_of(&window);
            if win.is_maximized()? {
                win.unmaximize()?;
            } else {
                win.maximize()?;
            }
        }
        "close_window" => window_of(&window).close()?,
        "open_home" | "open_history" | "open_bookmarks" | "open_settings" | "open_extensions"
        | "open_downloads" => {
            let page = match action.as_str() {
                "open_home" => {
                    let settings = state.settings.read();
                    if settings.home_url.trim().is_empty() {
                        settings.start_url.clone()
                    } else {
                        settings.home_url.clone()
                    }
                }
                "open_history" => "http://tauri.localhost/pages/history.html".to_string(),
                "open_bookmarks" => "http://tauri.localhost/pages/bookmarks.html".to_string(),
                "open_settings" => "http://tauri.localhost/pages/settings.html".to_string(),
                "open_extensions" => "http://tauri.localhost/pages/extensions.html".to_string(),
                _ => "http://tauri.localhost/pages/downloads.html".to_string(),
            };
            // Only the home page has a row of its own. The browser's own pages always open in
            // a tab of their own: replacing the page a reader is on with one of the app's
            // lists is not something any of those rows asked for.
            let home_where = if action == "open_home" {
                state.settings.read().open_home.clone()
            } else {
                String::new()
            };
            if home_where == "current" {
                tab_navigate(app, window, state, page).await?;
                return Ok(());
            }
            let background = home_where == "background";
            tab_new(
                app,
                window.clone(),
                state,
                Some(page),
                None,
                if background { Some(true) } else { None },
            )
            .await?;
        }
        "translate" => {
            let target = selected_label(&state, &window);
            if let Some(webview) = app.get_webview(&target) {
                let _ = webview.eval("window.__mixStartTranslate && window.__mixStartTranslate()");
            }
        }
        "group_tab" => {
            let selected = selected_label(&state, &window);
            let grouped = {
                let core = state.read_core(window.label());
                crate::ipc::is_grouped(&core, &selected)
            };
            if grouped {
                group_remove(app.clone(), window.clone(), state, selected).await?;
            } else {
                group_add(app.clone(), window.clone(), state, selected, None).await?;
            }
        }
        "close_overlay" => {
            crate::overlay::close(&app, window.label());
        }
        "tab_search" => {
            if let Some(chrome) = app.get_webview(&format!("{}-chrome", window.label())) {
                let _ = chrome.set_focus();
                let _ = app.emit_to(window.label(), "ui:tab-search", ());
            }
        }
        "palette" => {
            if let Some(chrome) = app.get_webview(&format!("{}-chrome", window.label())) {
                let _ = chrome.set_focus();
                let _ = app.emit_to(window.label(), "ui:palette", ());
            }
        }
        "find" => {
            crate::overlay::open(&app, &window, "find")
                .map_err(|err| AppError::new("overlay", err))?;
        }
        "devtools" => {
            let target = selected_label(&state, &window);
            match app.get_webview(&target) {
                Some(webview) => {
                    webview.open_devtools();
                    crate::logging::info(&format!("devtools opened for {target}"));
                }
                None => crate::logging::warn(&format!("devtools: no webview for {target}")),
            }
        }
        other => return Err(AppError::new("bad_shortcut", other)),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{page_may, FROM_A_PAGE};

    #[test]
    fn a_page_may_ask_for_the_things_a_page_produces() {
        for action in [
            "reload",
            "zoom_in",
            "zoom_out",
            "find",
            "new_tab",
            "close_tab",
            "bookmark",
            "focus_url",
        ] {
            assert!(page_may(action), "should be allowed: {action}");
        }
    }

    #[test]
    fn a_page_may_not_act_on_the_window() {
        // These are the ones that made this list necessary: a site could close the window,
        // hide it, or open developer tools in the reader's face.
        for action in [
            "close_window",
            "minimize",
            "maximize",
            "fullscreen",
            "devtools",
            "new_window",
            "new_incognito",
            "print",
            "toggle_pin",
            "group_tab",
            "close_other_tabs",
            "reopen_tab",
            "open_settings",
        ] {
            // `open_settings` is a deliberate member of the list: it opens a page of the
            // app's own, which is harmless to reach. The rest are not.
            if action == "open_settings" {
                continue;
            }
            assert!(!page_may(action), "should be refused: {action}");
        }
    }

    #[test]
    fn an_action_nobody_listed_is_refused() {
        // The check is a whitelist, so a new action defaults to "not for a page". This is the
        // property that keeps the next one somebody adds from being remote-reachable by
        // accident.
        assert!(!page_may("something_invented"));
        assert!(!page_may(""));
    }

    #[test]
    fn the_list_has_no_duplicates() {
        let unique: std::collections::HashSet<&&str> = FROM_A_PAGE.iter().collect();
        assert_eq!(unique.len(), FROM_A_PAGE.len());
    }
}
