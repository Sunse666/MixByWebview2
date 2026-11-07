//! Tab preview thumbnails for the hover card.
//!
//! Captured through the same DevTools protocol call as the screenshot button, but
//! with a clip and a scale factor, so the png that comes back is already the size
//! of the card: no image crate, no resize step, and nothing big enough to matter
//! in memory. A hidden webview does not render, so the thumbnail is taken when the
//! tab is still visible - on the way out when it is switched away from, and once
//! the page finishes loading.

use std::collections::HashMap;
use std::sync::Mutex;
use tauri::Manager;

/// Preview width in css pixels; the card scales it up to its own size.
const WIDTH: f64 = 360.0;
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
#[cfg(windows)]
pub fn capture(webview: &tauri::Webview) -> Option<Vec<u8>> {
  let metrics = crate::screenshot::call(webview, "Page.getLayoutMetrics", "{}")?;
  let value: serde_json::Value = serde_json::from_str(&metrics).ok()?;
  let viewport = value
    .get("cssVisualViewport")
    .or_else(|| value.get("visualViewport"))?;
  let width = viewport.get("clientWidth")?.as_f64()?;
  let height = viewport.get("clientHeight")?.as_f64()?;
  if width < 1.0 || height < 1.0 {
    return None;
  }
  let scale = (WIDTH / width).clamp(0.05, 1.0);
  // The clip is in css pixels and the scale shrinks the raster, so the result is
  // WIDTH x (height * scale) without any post-processing.
  let params = format!(
    r#"{{"format":"png","captureBeyondViewport":false,"clip":{{"x":0,"y":0,"width":{width},"height":{height},"scale":{scale}}}}}"#
  );
  let json = crate::screenshot::call(webview, "Page.captureScreenshot", &params)?;
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
    if is_fresh(&id, 2) {
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
