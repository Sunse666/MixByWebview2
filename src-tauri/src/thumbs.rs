//! Tab preview thumbnails for the hover card.
//!
//! Captured through the same DevTools protocol call as the screenshot button. The
//! png comes back at full size and the card scales it in css: a clip with a scale
//! factor would make Chromium re-raster the page, which flashes white on WebView2.
//!
//! A hidden webview does not render, so a thumbnail is taken while the tab is
//! still visible - when it becomes the selected tab - rather than on hover.

use std::collections::HashMap;
use std::sync::Mutex;
use tauri::Manager;

/// A thumbnail older than this is thrown away rather than shown stale forever.
const MAX_AGE_SECS: u64 = 600;

#[derive(Clone)]
struct Thumb {
  png: Vec<u8>,
  at: u64,
}

static THUMBS: Mutex<Option<HashMap<String, Thumb>>> = Mutex::new(None);

fn store() -> std::sync::MutexGuard<'static, Option<HashMap<String, Thumb>>> {
  THUMBS.lock().unwrap_or_else(|e| e.into_inner())
}

/// Takes a preview of whatever the webview is showing right now.
///
/// No clip and no scale: asking the protocol to re-raster the page at a smaller
/// scale makes Chromium rebuild the whole surface, and on WebView2 that shows as
/// several white frames. The full-size png is scaled down by the card instead,
/// which costs a little memory and no flicker.
#[cfg(windows)]
pub fn capture(webview: &tauri::Webview) -> Option<Vec<u8>> {
  let json = crate::screenshot::call(
    webview,
    "Page.captureScreenshot",
    r#"{"format":"png","captureBeyondViewport":false,"optimizeForSpeed":true}"#,
  )?;
  let value: serde_json::Value = serde_json::from_str(&json).ok()?;
  let data = value.get("data")?.as_str()?;
  let png = crate::screenshot::decode_base64(data)?;
  if png.is_empty() {
    None
  } else {
    Some(png)
  }
}

#[cfg(not(windows))]
pub fn capture(_webview: &tauri::Webview) -> Option<Vec<u8>> {
  None
}

pub fn remember(id: &str, png: Vec<u8>) {
  let mut guard = store();
  let map = guard.get_or_insert_with(HashMap::new);
  map.insert(
    id.to_string(),
    Thumb {
      png,
      at: crate::library::now(),
    },
  );
  // Tab ids are per window counters and never reused, so old entries would pile
  // up; drop the ones that aged out whenever a new preview arrives.
  let now = crate::library::now();
  map.retain(|_, thumb| now.saturating_sub(thumb.at) < MAX_AGE_SECS);
}

pub fn get(id: &str) -> Option<Vec<u8>> {
  let guard = store();
  let map = guard.as_ref()?;
  let thumb = map.get(id)?;
  if crate::library::now().saturating_sub(thumb.at) >= MAX_AGE_SECS {
    return None;
  }
  Some(thumb.png.clone())
}

pub fn forget(id: &str) {
  if let Some(map) = store().as_mut() {
    map.remove(id);
  }
}

/// Captures on a worker thread: the CDP round trip must not run on the thread
/// that is switching tabs.
pub fn remember_later(app: tauri::AppHandle, id: String) {
  let Some(webview) = app.get_webview(&id) else {
    return;
  };
  tauri::async_runtime::spawn_blocking(move || {
    // The compositor needs a frame or two after the tab is shown, otherwise the
    // capture catches the page that was there before the switch.
    std::thread::sleep(std::time::Duration::from_millis(350));
    // The setting can be turned off while the worker is asleep.
    {
      let state = app.state::<crate::ipc::AppState>();
      if !state.settings.read().tab_hover_preview {
        return;
      }
    }
    if is_fresh(&id, 5) {
      return;
    }
    if let Some(png) = capture(&webview) {
      remember(&id, png);
    }
  });
}

fn is_fresh(id: &str, seconds: u64) -> bool {
  let guard = store();
  let Some(map) = guard.as_ref() else { return false };
  match map.get(id) {
    Some(thumb) => crate::library::now().saturating_sub(thumb.at) < seconds,
    None => false,
  }
}