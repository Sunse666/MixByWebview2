//! Tab preview pictures for the hover card.
//!
//! The capture is the same DevTools protocol call the screenshot button uses. It
//! has to come from a webview that is actually on screen: a hidden one is not
//! composited and the protocol answers with whatever surface sits in its place.
//! So a picture is taken while the tab is visible - on hover when it is the
//! selected tab, and on the way out, before it is hidden, when it is switched
//! away from.
//!
//! The png is kept at full size and the card scales it in css: asking the protocol
//! for a clipped, scaled shot makes Chromium re-raster the page, which flashes.

use std::collections::HashMap;
use std::sync::Mutex;

/// A picture older than this is not worth showing any more.
const MAX_AGE_SECS: u64 = 900;

struct Thumb {
  png: Vec<u8>,
  at: u64,
}

static THUMBS: Mutex<Option<HashMap<String, Thumb>>> = Mutex::new(None);

fn store() -> std::sync::MutexGuard<'static, Option<HashMap<String, Thumb>>> {
  THUMBS.lock().unwrap_or_else(|e| e.into_inner())
}

/// Grabs what the webview is showing right now.
#[cfg(windows)]
pub fn capture(webview: &tauri::Webview) -> Option<Vec<u8>> {
  let png = capture_once(webview);
  match &png {
    Some(bytes) => crate::logging::info(&format!(
      "thumbs: captured {} ({} bytes)",
      webview.label(),
      bytes.len()
    )),
    None => crate::logging::warn(&format!("thumbs: capture failed for {}", webview.label())),
  }
  png
}

#[cfg(windows)]
fn capture_once(webview: &tauri::Webview) -> Option<Vec<u8>> {
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
fn capture_once(_webview: &tauri::Webview) -> Option<Vec<u8>> {
  None
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

/// Age of the stored picture, so the card can say how old the image is.
pub fn age(id: &str) -> Option<u64> {
  let guard = store();
  let map = guard.as_ref()?;
  let thumb = map.get(id)?;
  Some(crate::library::now().saturating_sub(thumb.at))
}

pub fn forget(id: &str) {
  if let Some(map) = store().as_mut() {
    map.remove(id);
  }
}

/// True when there is no picture, or only a stale one.
pub fn needs(id: &str, seconds: u64) -> bool {
  let guard = store();
  let Some(map) = guard.as_ref() else { return true };
  match map.get(id) {
    Some(thumb) => crate::library::now().saturating_sub(thumb.at) >= seconds,
    None => true,
  }
}
