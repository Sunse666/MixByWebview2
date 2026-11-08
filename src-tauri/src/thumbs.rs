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
      "thumbs: captured {} ({} bytes fp={:x})",
      webview.label(),
      bytes.len(),
      fingerprint(bytes)
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

/// Cheap fingerprint over the first 64 KB: two captures of the same pixels hash
/// the same, so the log shows whether a forced repaint produced new content.
fn fingerprint(png: &[u8]) -> u64 {
  let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
  for byte in &png[..png.len().min(64 * 1024)] {
    hash ^= *byte as u64;
    hash = hash.wrapping_mul(0x100_0000_01b3);
  }
  hash
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
/// A live preview is refreshed when the stored picture is older than this. Short
/// enough that a video tab looks alive while hovering it, long enough that moving
/// the pointer across the strip does not wake a page over and over.
const LIVE_REFRESH_SECS: u64 = 4;

/// How long the wake-up is given to paint before the photograph is taken.
///
/// Showing a webview does not paint it on the spot: the compositor produces the
/// next frame a little later, so an immediate capture hands back the frame from
/// before the wake-up - which is exactly why the card seemed frozen.
const LIVE_PAINT_WAIT: std::time::Duration = std::time::Duration::from_millis(160);

/// Takes a picture of a tab that is not the selected one.
///
/// A hidden webview does not paint, so the only way to get a fresh frame is to
/// show it for a moment - underneath the tab the user is looking at, so nothing
/// becomes visible - let it paint, photograph it, and hide it again.
///
/// The page does see a visibility change while this happens, which is why it is a
/// setting of its own.
pub fn capture_live(selected: Option<&tauri::Webview>, target: &tauri::Webview) -> Option<Vec<u8>> {
  let id = target.label().to_string();
  if let Some(selected) = selected {
    if selected.label() == target.label() {
      return None;
    }
  }
  if !needs(&id, LIVE_REFRESH_SECS) {
    return None;
  }

  target.show().ok()?;
  // Underneath the tab on screen, so the photograph cannot catch itself.
  if let Some(selected) = selected {
    crate::platform::place_below(target, selected);
  }
  std::thread::sleep(LIVE_PAINT_WAIT);
  let png = capture(target);
  let _ = target.hide();
  crate::logging::info(&format!(
    "thumbs: live {id} -> {}",
    match &png {
      // The fingerprint says whether the wake-up really produced a new frame or
      // handed back the one the card was already showing.
      Some(bytes) => format!("{} bytes fp={:x}", bytes.len(), fingerprint(bytes)),
      None => "nothing".to_string(),
    }
  ));
  png
}
