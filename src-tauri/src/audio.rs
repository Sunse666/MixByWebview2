//! Per tab audio state and muting, read from WebView2.

use parking_lot::Mutex;
use std::time::Duration;
use tauri::{Emitter, Manager};

#[derive(Clone, Copy, PartialEq, Eq, Debug, serde::Serialize)]
pub struct Audio {
    pub playing: bool,
    pub muted: bool,
}

/// Last state read for each tab: the poller writes it, the tab strip reads it.
///
/// Keeping it means a strip redraw costs no COM calls, and the redraw happens far more
/// often than the state changes.
static CACHE: Mutex<Option<std::collections::HashMap<String, Audio>>> = Mutex::new(None);

/// What the last poll saw for one tab, if it has been polled.
pub fn cached(id: &str) -> Option<Audio> {
    CACHE.lock().as_ref().and_then(|map| map.get(id).copied())
}

/// Records a state and says whether it is news.
pub(crate) fn remember(id: &str, audio: Audio) -> bool {
    let mut guard = CACHE.lock();
    let map = guard.get_or_insert_with(std::collections::HashMap::new);
    let previous = map.insert(id.to_string(), audio);
    // A first, quiet reading is not news: every tab would otherwise be announced once at
    // startup, which is a strip redraw per tab for nothing.
    previous != Some(audio) && (previous.is_some() || audio.playing || audio.muted)
}

#[cfg(windows)]
fn read(webview: &tauri::Webview) -> Option<Audio> {
    use webview2_com::Microsoft::Web::WebView2::Win32::ICoreWebView2_8;
    use windows_core::Interface;

    crate::com::call(
        webview,
        Duration::from_millis(500),
        |platform, out| unsafe {
            let result = (|| -> windows_core::Result<Audio> {
                let controller = platform.controller();
                let core = controller.CoreWebView2()?;
                let core8 = core.cast::<ICoreWebView2_8>()?;
                let mut playing = windows_core::BOOL(0);
                let mut muted = windows_core::BOOL(0);
                core8.IsDocumentPlayingAudio(&mut playing)?;
                core8.IsMuted(&mut muted)?;
                Ok(Audio {
                    playing: playing.as_bool(),
                    muted: muted.as_bool(),
                })
            })();
            let _ = out.send(result.ok());
        },
    )
    .flatten()
}

#[cfg(not(windows))]
fn read(_webview: &tauri::Webview) -> Option<Audio> {
    None
}

/// Public read for the command layer.
pub fn state(webview: &tauri::Webview) -> Option<Audio> {
    read(webview)
}

/// Mutes or unmutes the whole webview, which is what a browser's tab mute does.
#[cfg(windows)]
pub fn set_muted(webview: &tauri::Webview, muted: bool) -> bool {
    use webview2_com::Microsoft::Web::WebView2::Win32::ICoreWebView2_8;
    use windows_core::Interface;

    crate::com::call(
        webview,
        Duration::from_millis(500),
        move |platform, out| unsafe {
            let result = (|| -> windows_core::Result<()> {
                let controller = platform.controller();
                let core = controller.CoreWebView2()?;
                let core8 = core.cast::<ICoreWebView2_8>()?;
                core8.SetIsMuted(muted)?;
                Ok(())
            })();
            let _ = out.send(result.is_ok());
        },
    )
    .unwrap_or(false)
}

#[cfg(not(windows))]
pub fn set_muted(_webview: &tauri::Webview, _muted: bool) -> bool {
    false
}

/// Polls every live tab of every window and emits when a state moves.
///
/// A background tab that starts playing music is exactly what the strip's speaker is for, so
/// this cannot only watch the tab on screen. It is one COM round trip per live tab, every
/// 1.2 s, and nothing is emitted unless a value actually changed.
pub fn watch(app: tauri::AppHandle) {
    std::thread::spawn(move || loop {
        std::thread::sleep(Duration::from_millis(1200));
        let Some(state) = app.try_state::<crate::ipc::AppState>() else {
            continue;
        };
        let targets: Vec<(String, String)> = state
            .cores
            .read()
            .iter()
            .flat_map(|(window, core)| {
                core.tabs
                    .iter()
                    .filter(|tab| tab.alive)
                    .map(|tab| (window.clone(), tab.id.clone()))
                    .collect::<Vec<_>>()
            })
            .collect();
        for (window, id) in targets {
            let Some(webview) = app.get_webview(&id) else {
                continue;
            };
            let Some(audio) = read(&webview) else {
                continue;
            };
            if remember(&id, audio) {
                let _ = app.emit_to(
                    window,
                    "tab:audio",
                    serde_json::json!({ "id": id, "playing": audio.playing, "muted": audio.muted }),
                );
            }
        }
        let mut last = CACHE.lock();
        if let Some(map) = last.as_mut() {
            if map.len() > 200 {
                map.retain(|id, _| app.get_webview(id).is_some());
            }
        }
    });
}
