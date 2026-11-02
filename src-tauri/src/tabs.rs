use serde::Serialize;
use tauri::{
    webview::WebviewBuilder, Emitter, LogicalPosition, LogicalSize, Manager, Url, Webview,
    WebviewUrl, Window,
};

use crate::window::CHROME_H;

#[derive(Debug, Clone, Serialize)]
pub struct Tab {
    pub id: String,
    pub url: String,
    pub title: String,
    /// False means the webview was dropped to save memory; selecting the tab
    /// recreates it from url.
    pub alive: bool,
    pub last_active: u64,
}

#[derive(Clone, Serialize)]
pub struct TitlePayload {
    pub id: String,
    pub title: String,
}

pub fn parse_url(input: &str) -> Url {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return "about:blank".parse().expect("static url");
    }
    if let Ok(url) = trimmed.parse::<Url>() {
        return url;
    }
    match format!("https://{trimmed}").parse::<Url>() {
        Ok(url) => url,
        Err(_) => "about:blank".parse().expect("static url"),
    }
}

pub fn create(win: &Window, id: &str, url: &str, width: f64, height: f64) -> tauri::Result<Webview> {
    let builder = WebviewBuilder::new(id, WebviewUrl::External(parse_url(url)))
        // WebView2 keeps ctrl+wheel zoom off unless asked (M0 E2).
        .zoom_hotkeys_enabled(true)
        .on_document_title_changed(|webview, title| {
            let app = webview.app_handle().clone();
            let payload = TitlePayload {
                id: webview.label().to_string(),
                title: title.clone(),
            };
            if let Some(state) = app.try_state::<crate::ipc::AppState>() {
                if let Ok(mut core) = state.core.write() {
                    if let Some(tab) = core.tabs.iter_mut().find(|t| t.id == payload.id) {
                        tab.title = title;
                    }
                }
            }
            let _ = app.emit("tab:title", payload);
        });

    win.add_child(
        builder,
        LogicalPosition::new(0.0, CHROME_H),
        LogicalSize::new(width.max(1.0), height.max(1.0)),
    )
}