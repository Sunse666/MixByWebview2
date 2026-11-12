//! Per tab audio state and muting, read from WebView2.

use std::sync::Mutex;
use std::time::Duration;
use tauri::{Emitter, Manager};

#[derive(Clone, Copy, PartialEq, Eq, Debug, serde::Serialize)]
pub struct Audio {
  pub playing: bool,
  pub muted: bool,
}

/// Last state pushed to the ui, so an unchanged second costs no event.
static LAST: Mutex<Option<std::collections::HashMap<String, Audio>>> = Mutex::new(None);

#[cfg(windows)]
fn read(webview: &tauri::Webview) -> Option<Audio> {
  use webview2_com::Microsoft::Web::WebView2::Win32::ICoreWebView2_8;
  use windows_core::Interface;

  let (sender, receiver) = std::sync::mpsc::channel();
  let done = sender.clone();
  let request = webview.with_webview(move |platform| unsafe {
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
    let _ = std::sync::mpsc::Sender::send(&done, result.ok());
  });
  if request.is_err() {
    return None;
  }
  receiver
    .recv_timeout(Duration::from_millis(500))
    .ok()
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

  let (sender, receiver) = std::sync::mpsc::channel();
  let done = sender.clone();
  let request = webview.with_webview(move |platform| unsafe {
    let result = (|| -> windows_core::Result<()> {
      let controller = platform.controller();
      let core = controller.CoreWebView2()?;
      let core8 = core.cast::<ICoreWebView2_8>()?;
      core8.SetIsMuted(muted)?;
      Ok(())
    })();
    let _ = std::sync::mpsc::Sender::send(&done, result.is_ok());
  });
  if request.is_err() {
    return false;
  }
  receiver
    .recv_timeout(Duration::from_millis(500))
    .unwrap_or(false)
}

#[cfg(not(windows))]
pub fn set_muted(_webview: &tauri::Webview, _muted: bool) -> bool {
  false
}

/// Polls the selected tab of every window and emits when the state moves.
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
      .filter_map(|(window, core)| core.selected.clone().map(|id| (window.clone(), id)))
      .collect();
    for (window, id) in targets {
      let Some(webview) = app.get_webview(&id) else {
        continue;
      };
      let Some(audio) = read(&webview) else {
        continue;
      };
      let changed = {
        let mut last = LAST.lock().unwrap_or_else(|e| e.into_inner());
        let map = last.get_or_insert_with(std::collections::HashMap::new);
        map.insert(id.clone(), audio) != Some(audio)
      };
      if changed {
        let _ = app.emit_to(
          window,
          "tab:audio",
          serde_json::json!({ "id": id, "playing": audio.playing, "muted": audio.muted }),
        );
      }
    }
    let mut last = LAST.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(map) = last.as_mut() {
      if map.len() > 200 {
        map.retain(|id, _| app.get_webview(id).is_some());
      }
    }
  });
}
