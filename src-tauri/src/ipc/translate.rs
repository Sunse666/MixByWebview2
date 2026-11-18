//! Page translation: in-place replacement and the proxy page fallback.

use super::*;

/// Starts the in-page translator.
#[tauri::command]
pub async fn translate_page_inplace(
    app: AppHandle,
    window: tauri::Window,
    state: State<'_, AppState>,
) -> Result<(), AppError> {
    let id = selected_label(&state, &window);
    if id.is_empty() {
        return Ok(());
    }
    start_tab(&app, &state, &id);
    Ok(())
}

/// Progress of the in-page translation, polled by the bubble.
#[tauri::command]
pub async fn translate_status(
    state: State<'_, AppState>,
) -> Result<(usize, Option<String>), AppError> {
    Ok(state.ui.read().translate_result.clone())
}

/// Starts the in-page translator for one tab.
fn start_tab(app: &AppHandle, state: &AppState, id: &str) {
    {
        let mut ui = state.ui.write();
        ui.translate_result = (0, None);
        ui.translate_skip.remove(id);
    }
    if let Some(webview) = app.get_webview(id) {
        let _ = webview.eval("window.__mixStartTranslate && window.__mixStartTranslate()");
    }
}

/// Puts the original text of one tab back, and remembers that it was asked for.
fn restore_tab(app: &AppHandle, state: &AppState, id: &str) {
    if let Some(webview) = app.get_webview(id) {
        let _ = webview.eval("window.__mixStopTranslate && window.__mixStopTranslate()");
    }
    let url = state
        .try_read_core(window_of_tab(id))
        .and_then(|core| {
            core.tabs
                .iter()
                .find(|tab| tab.id == id)
                .map(|tab| tab.url.clone())
        })
        .unwrap_or_default();
    {
        let mut ui = state.ui.write();
        ui.translated_tabs.remove(id);
        // Remember which page the user wants untranslated: on a site that is always
        // translated, the next load would otherwise put the translation straight back.
        if !url.is_empty() {
            ui.translate_skip.insert(id.to_string(), url);
        }
    }
}

/// Translates the page on screen, or puts its original text back when it already is.
///
/// The app menu needs this as one command: it is a webview of its own and has none of the
/// toolbar's state, and its row used to open a panel that does not exist - it fell through
/// to the settings panel, so "translate this page" showed the theme editor.
#[tauri::command]
pub async fn translate_toggle(
    app: AppHandle,
    window: tauri::Window,
    state: State<'_, AppState>,
) -> Result<bool, AppError> {
    let id = selected_label(&state, &window);
    if id.is_empty() {
        return Ok(false);
    }
    if state.ui.read().translated_tabs.contains(&id) {
        restore_tab(&app, &state, &id);
        return Ok(false);
    }
    start_tab(&app, &state, &id);
    Ok(true)
}

/// Puts the original text back.
#[tauri::command]
pub async fn translate_restore(
    app: AppHandle,
    window: tauri::Window,
    state: State<'_, AppState>,
) -> Result<(), AppError> {
    let id = selected_label(&state, &window);
    if !id.is_empty() {
        restore_tab(&app, &state, &id);
    }
    Ok(())
}

/// The site of a page, as the always-translate list stores it.
///
/// The host only, without `www.`, in lower case: a list holding whole urls would never match
/// twice, and the comparison at page load is exact.
fn site_of(url: &str) -> Option<String> {
    let parsed = tauri::Url::parse(url).ok()?;
    if !matches!(parsed.scheme(), "http" | "https") {
        return None;
    }
    let host = parsed.host_str()?.trim_start_matches("www.").to_lowercase();
    (!host.is_empty()).then_some(host)
}

fn site_of_tab(state: &AppState, tab: &str) -> Option<String> {
    let core = state.try_read_core(window_of_tab(tab))?;
    let url = core.tabs.iter().find(|entry| entry.id == tab)?.url.clone();
    site_of(&url)
}

/// Whether the page in this window is on a site that is translated on sight.
#[tauri::command]
pub async fn translate_site_state(
    window: tauri::Window,
    state: State<'_, AppState>,
) -> Result<bool, AppError> {
    let id = selected_label(&state, &window);
    let Some(host) = site_of_tab(&state, &id) else {
        return Ok(false);
    };
    Ok(state
        .settings
        .read()
        .translate_hosts
        .iter()
        .any(|known| known == &host))
}

/// Adds the current page's site to the always-translate list, or takes it out.
///
/// Turning it on also translates the page on screen: setting a preference and leaving that
/// page in its original language would look like nothing happened.
#[tauri::command]
pub async fn translate_site(
    app: AppHandle,
    window: tauri::Window,
    state: State<'_, AppState>,
    enable: bool,
) -> Result<bool, AppError> {
    let id = selected_label(&state, &window);
    let Some(host) = site_of_tab(&state, &id) else {
        return Err(AppError::new(
            "no_site",
            "this page has no site to remember",
        ));
    };
    let mut hosts = state.settings.read().translate_hosts.clone();
    hosts.retain(|known| known != &host);
    if enable {
        hosts.push(host.clone());
    }
    apply_settings_patch(&state, serde_json::json!({ "translate_hosts": hosts }))?;
    crate::logging::info(&format!(
        "translate {} for {host}",
        if enable { "always" } else { "never again" }
    ));
    if enable {
        state.ui.write().translated_tabs.remove(&id);
        start_tab(&app, &state, &id);
    }
    Ok(enable)
}

/// Translates a page that has just loaded, if its site is on the always-translate list.
///
/// A page the user asked to see in its original language is left alone, and so is one that
/// has already been translated.
pub(crate) fn auto_translate(app: &AppHandle, tab: &str, url: &str) {
    let Some(state) = app.try_state::<AppState>() else {
        return;
    };
    let Some(host) = site_of(url) else {
        return;
    };
    if !state
        .settings
        .read()
        .translate_hosts
        .iter()
        .any(|known| known == &host)
    {
        return;
    }
    {
        let ui = state.ui.write();
        if ui.translated_tabs.contains(tab)
            || ui
                .translate_skip
                .get(tab)
                .is_some_and(|skipped| skipped == url)
        {
            return;
        }
    }
    crate::logging::info(&format!("always translate: {host}"));
    start_tab(app, &state, tab);
}

#[cfg(test)]
mod tests {
    use super::site_of;

    #[test]
    fn a_site_is_a_bare_lowercase_host() {
        assert_eq!(
            site_of("https://www.Example.COM/a/b?c=1").as_deref(),
            Some("example.com")
        );
        assert_eq!(
            site_of("http://sub.example.com:8080/x").as_deref(),
            Some("sub.example.com")
        );
    }

    #[test]
    fn nothing_to_remember_on_a_bare_page() {
        assert_eq!(site_of("about:blank"), None);
        assert_eq!(site_of("data:text/html,hi"), None);
        assert_eq!(site_of("not a url"), None);
    }
}

/// Whether the selected tab currently shows translated text.
#[tauri::command]
pub async fn translate_state(
    window: tauri::Window,
    state: State<'_, AppState>,
) -> Result<bool, AppError> {
    let id = selected_label(&state, &window);
    Ok(state.ui.read().translated_tabs.contains(&id))
}

/// The page reports how many nodes it replaced, and Chrome shows a hint.
#[tauri::command]
pub async fn translate_done(
    app: AppHandle,
    webview: tauri::Webview,
    count: usize,
    error: Option<String>,
) -> Result<(), AppError> {
    let tab = webview.label().to_string();
    if let Some(state) = app.try_state::<AppState>() {
        let mut ui = state.ui.write();
        ui.translate_result = (count, error.clone());
        if error.is_none() && count > 0 {
            ui.translated_tabs.insert(tab.clone());
        } else if error.is_some() {
            ui.translated_tabs.remove(&tab);
        }
    }
    let owner = window_of_tab(webview.label()).to_string();
    if let Some(message) = &error {
        crate::logging::warn(&format!("translation failed: {message}"));
        if let Some(chrome) = app.get_webview(&format!("{owner}-chrome")) {
            let text = serde_json::to_string(message).unwrap_or_else(|_| "\"failed\"".into());
            let _ = chrome.eval(format!(
                "window.__mixTranslateFailed && window.__mixTranslateFailed({text})"
            ));
        }
    } else {
        crate::logging::info(&format!("translated {count} text nodes"));
    }
    let _ = app.emit_to(owner, "translate:done", (count, error));
    Ok(())
}
/// Translates a batch of page strings through the Microsoft engine.
#[tauri::command]
pub async fn translate_texts(
    _app: AppHandle,
    _window: tauri::Window,
    state: State<'_, AppState>,
    texts: Vec<String>,
    to: Option<String>,
) -> Result<Vec<String>, AppError> {
    let target = to.unwrap_or_else(|| {
        let saved = state.settings.read().translate_to.clone();
        match saved.as_str() {
            "" => "zh-Hans".to_string(),
            "zh" => "zh-Hans".to_string(),
            other => other.to_string(),
        }
    });

    let trimmed: Vec<String> = texts
        .into_iter()
        .map(|text| text.trim().to_string())
        .collect();
    let started = std::time::Instant::now();
    let out = crate::translate::translate(trimmed, &target).await;
    let done = out.iter().filter(|text| !text.is_empty()).count();
    crate::logging::info(&format!(
        "translated {done}/{} blocks to {target} in {} ms",
        out.len(),
        started.elapsed().as_millis()
    ));
    Ok(out)
}
/// Page translation through a web proxy.
#[tauri::command]
pub async fn translate_page(
    app: AppHandle,
    window: tauri::Window,
    state: State<'_, AppState>,
    target: Option<String>,
) -> Result<String, AppError> {
    let text = match target {
        Some(value) => value,
        None => {
            let id = selected_label(&state, &window);
            state
                .try_read_core(window.label())
                .and_then(|core| {
                    core.tabs
                        .iter()
                        .find(|tab| tab.id == id)
                        .map(|tab| tab.url.clone())
                })
                .unwrap_or_default()
        }
    };
    let text = text.trim().to_string();
    if text.is_empty() {
        return Err(AppError::new("bad_url", "nothing to translate"));
    }

    let encoded = crate::permissions::encode(&text);
    let proxy = if text.starts_with("http://") || text.starts_with("https://") {
        format!(
            "https://fanyi.baidu.com/transpage?query={encoded}&source=url&ie=utf8&from=auto&to=zh&render=1"
        )
    } else {
        format!("https://fanyi.baidu.com/mtpe-individual/multimodal?query={encoded}&lang=auto2zh")
    };

    let page = tab_new(app, window, state, Some(proxy.clone()), None, None).await?;
    crate::logging::info(&format!("translate {text} as {page}"));
    Ok(proxy)
}
