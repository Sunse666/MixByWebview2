//! Page translation: in-place replacement and the proxy page fallback.

use super::*;

/// Starts the in-page translator.
#[tauri::command]
pub async fn translate_page_inplace(
    app: AppHandle,
    window: tauri::Window,
    state: State<'_, AppState>,
) -> Result<(), AppError> {
    state.ui.write().translate_result = (0, None);
    let id = selected_label(&state, &window);
    let Some(webview) = app.get_webview(&id) else {
        return Ok(());
    };
    let _ = webview.eval("window.__mixStartTranslate && window.__mixStartTranslate()");
    Ok(())
}

/// Progress of the in-page translation, polled by the bubble.
#[tauri::command]
pub async fn translate_status(
    state: State<'_, AppState>,
) -> Result<(usize, Option<String>), AppError> {
    Ok(state.ui.read().translate_result.clone())
}

/// Puts the original text back.
#[tauri::command]
pub async fn translate_restore(
    app: AppHandle,
    window: tauri::Window,
    state: State<'_, AppState>,
) -> Result<(), AppError> {
    let id = selected_label(&state, &window);
    if let Some(webview) = app.get_webview(&id) {
        let _ = webview.eval("window.__mixStopTranslate && window.__mixStopTranslate()");
    }
    state.ui.write().translated_tabs.remove(&id);
    Ok(())
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
            let _ = chrome.eval(&format!(
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
                .and_then(|core| core.tabs.iter().find(|tab| tab.id == id).map(|tab| tab.url.clone()))
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