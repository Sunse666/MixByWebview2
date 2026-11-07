//! The keyboard dispatcher: one place where a shortcut action name turns into
//! something happening.

use super::*;

#[tauri::command]
pub async fn shortcut(app: AppHandle, window: tauri::Window, state: State<'_, AppState>, action: String) -> Result<(), AppError> {
    if action != "dismiss" {
        crate::logging::info(&format!("shortcut {action}"));
    }
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
            let _ = tab_reopen(app, window, state).await?;
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
                tab_close(app, window.clone(), state, id).await?;
            }
        }
        "next_tab" => cycle_tab(&app, &window, 1).await,
        "prev_tab" => cycle_tab(&app, &window, -1).await,
        "focus_url" => {
            // Focus the chrome webview, and arm the page side key bridge so typing
            // works even when WebView2 keeps the keyboard on the page.
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
        // Not gated on debug: F12 must work in the packaged build too.
        // Gesture and palette actions that reuse existing commands.
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
                let mut board =
                    arboard::Clipboard::new().map_err(|err| AppError::new("clipboard", err.to_string()))?;
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
                tab_close(app.clone(), window.clone(), state, id).await?;
            }
        }
        "toggle_pin" => {
            let (id, pinned) = {
                let core = state.read_core(window.label());
                match core.tabs.iter().find(|tab| Some(&tab.id) == core.selected.as_ref()) {
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
        "open_home" | "open_history" | "open_bookmarks" | "open_settings" | "open_extensions" | "open_downloads" => {
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
            tab_new(app, window.clone(), state, Some(page), None, None).await?;
        }
        "translate" => {
            let target = selected_label(&state, &window);
            if let Some(webview) = app.get_webview(&target) {
                let _ = webview.eval("window.__mixStartTranslate && window.__mixStartTranslate()");
            }
        }
        "palette" => {
            // The palette lives in the chrome page, so it needs the keyboard.
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
