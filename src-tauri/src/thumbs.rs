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
use tauri::Manager;

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

/// Reads the width and height out of a png header.
fn png_size(png: &[u8]) -> Option<(u32, u32)> {
  if png.len() < 24 || &png[1..4] != b"PNG" {
    return None;
  }
  let width = u32::from_be_bytes([png[16], png[17], png[18], png[19]]);
  let height = u32::from_be_bytes([png[20], png[21], png[22], png[23]]);
  Some((width, height))
}

/// One screenshot attempt, with or without the compositor surface.
/// One screenshot attempt, with or without the compositor surface.
#[cfg(windows)]
fn capture_try(webview: &tauri::Webview, params: &str) -> Option<Vec<u8>> {
  let json = crate::screenshot::call_timeout(
    webview,
    "Page.captureScreenshot",
    params,
    std::time::Duration::from_millis(1500),
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

/// When a tab's hidden capture was last probed, successful or not. Without this a
/// failure repeats on every mouse move and the card ends up waiting on it.
static PROBED: Mutex<Option<HashMap<String, u64>>> = Mutex::new(None);

fn probe_due(id: &str, seconds: u64) -> bool {
  let mut guard = PROBED.lock().unwrap_or_else(|e| e.into_inner());
  let map = guard.get_or_insert_with(HashMap::new);
  let now = crate::library::now();
  match map.get(id) {
    Some(at) if now.saturating_sub(*at) < seconds => false,
    _ => {
      map.insert(id.to_string(), now);
      true
    }
  }
}

/// Experiment: can a webview that is hidden still be photographed?
///
/// Two attempts, because they answer different questions. "fromSurface": false
/// asks the renderer for a frame it may still be holding; the plain call is the
/// control, and if that one answers with a real picture then the surface is still
/// there as well. A blank page compresses to a few kilobytes, so anything above
/// MIN_PNG is treated as a real frame. Both outcomes are logged with the size and
/// the png dimensions, which is what says which one worked.
#[cfg(windows)]
pub fn probe_hidden(webview: &tauri::Webview) -> Option<Vec<u8>> {
  const MIN_PNG: usize = 12 * 1024;
  const VIEW: &str =
    r#"{"format":"png","fromSurface":false,"captureBeyondViewport":false,"optimizeForSpeed":true}"#;
  const SURFACE: &str =
    r#"{"format":"png","captureBeyondViewport":false,"optimizeForSpeed":true}"#;

  let label = webview.label();

  // A hidden page does not paint, so the plain attempts below can only ever hand
  // back the frame from when the tab was last on screen. captureBeyondViewport
  // forces a fresh raster of the document as it is now, which is the only way to
  // learn whether a hidden tab can be shown live at all.
  let forced = forced_repaint(webview);
  match &forced {
    Some(png) => {
      let size = png_size(png).unwrap_or((0, 0));
      crate::logging::info(&format!(
        "thumbs: hidden {label} repaint -> {} bytes {}x{} fp={:x}",
        png.len(),
        size.0,
        size.1,
        fingerprint(png)
      ));
    }
    None => crate::logging::warn(&format!("thumbs: hidden {label} repaint -> no answer")),
  }

  let mut best: Option<Vec<u8>> = forced.filter(|png| png.len() >= MIN_PNG);
  for (name, params) in [("view", VIEW), ("surface", SURFACE)] {
    if best.is_some() {
      break;
    }
    match capture_try(webview, params) {
      Some(png) => {
        let size = png_size(&png).unwrap_or((0, 0));
        crate::logging::info(&format!(
          "thumbs: hidden {label} {name} -> {} bytes {}x{}",
          png.len(),
          size.0,
          size.1
        ));
        if png.len() >= MIN_PNG {
          best = Some(png);
        }
      }
      None => crate::logging::warn(&format!("thumbs: hidden {label} {name} -> no answer")),
    }
  }
  best
}

/// Asks for the document as it is now, at its own scale, clipped to the viewport.
/// No scale factor: that is what used to make the page re-raster visibly.
#[cfg(windows)]
fn forced_repaint(webview: &tauri::Webview) -> Option<Vec<u8>> {
  let metrics = crate::screenshot::call_timeout(
    webview,
    "Page.getLayoutMetrics",
    "{}",
    std::time::Duration::from_millis(1200),
  )?;
  let value: serde_json::Value = serde_json::from_str(&metrics).ok()?;
  let viewport = value
    .get("cssVisualViewport")
    .or_else(|| value.get("visualViewport"))?;
  let width = viewport.get("clientWidth")?.as_f64()?;
  let height = viewport.get("clientHeight")?.as_f64()?;
  if width < 8.0 || height < 8.0 {
    return None;
  }
  let params = format!(
    r#"{{"format":"png","captureBeyondViewport":true,"optimizeForSpeed":true,"clip":{{"x":0,"y":0,"width":{width},"height":{height},"scale":1}}}}"#
  );
  capture_try(webview, &params)
}

/// Runs the probe off the command thread and stores whatever it gets, so the card
/// never waits for a webview that may not answer at all.
pub fn probe_later(app: tauri::AppHandle, id: String) {
  if !probe_due(&id, 30) {
    return;
  }
  let Some(webview) = app.get_webview(&id) else {
    return;
  };
  tauri::async_runtime::spawn_blocking(move || {
    if let Some(png) = probe_hidden(&webview) {
      let printed = fingerprint(&png);
      remember(&id, png);
      crate::logging::info(&format!(
        "thumbs: hidden {id} used as the preview (fp={printed:x})"
      ));
    }
  });
}

#[cfg(not(windows))]
pub fn probe_hidden(_webview: &tauri::Webview) -> Option<Vec<u8>> {
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