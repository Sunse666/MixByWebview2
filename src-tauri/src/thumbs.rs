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
    // Jpeg rather than png: a full-window png runs to about 900 KB for a page with
    // pictures in it, and that is a 1.2 MB data url through the ipc for every card - the
    // ones that big were the ones that arrived broken. At quality 70 the same frame is a
    // fraction of the size and the card is 372 px wide anyway.
    r#"{"format":"jpeg","quality":70,"captureBeyondViewport":false,"optimizeForSpeed":true}"#,
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


/// Photographs a background tab off the command thread.
///
/// The wake-up and the wait for a frame are deliberately slow; doing them inline made
/// the card feel glued to the pointer.
pub fn capture_later(app: tauri::AppHandle, id: String, window_label: String) {
  tauri::async_runtime::spawn_blocking(move || {
    let Some(state) = app.try_state::<crate::ipc::AppState>() else {
      return;
    };
    if !state.settings.read().live_thumbs {
      return;
    }
    // A tab that was on screen moments ago already has a fresh picture, and waking one is
    // the operation that can hide the page someone is looking at. Only tabs that have been
    // parked for a while are worth the wake-up.
    const WAKE_AFTER_SECS: u64 = 30;
    let shown = state
      .shown_at
      .read()
      .unwrap_or_else(|e| e.into_inner())
      .get(&id)
      .copied()
      .unwrap_or(0);
    if crate::library::now().saturating_sub(shown) < WAKE_AFTER_SECS {
      return;
    }
    let selected = state
      .read_core(&window_label)
      .selected
      .clone()
      .and_then(|label| app.get_webview(&label));
    let Some(webview) = app.get_webview(&id) else {
      return;
    };
    // A frozen page cannot paint, so wake it for the photograph.
    crate::freeze::thaw(&webview);
    // One retry: a tab that has just been woken sometimes answers the devtools call
    // before it has a surface, and that is exactly the case that left the frame empty.
    // Asked fresh every time: the answer can change while the capture is working.
    let still_hidden = || {
      app
        .try_state::<crate::ipc::AppState>()
        .map(|state| state.read_core(&window_label).selected.as_deref() != Some(id.as_str()))
        .unwrap_or(false)
    };
    let mut shot = capture_live(selected.as_ref(), &webview, &still_hidden);
    if shot.is_none() && still_hidden() {
      std::thread::sleep(std::time::Duration::from_millis(150));
      shot = capture_live(selected.as_ref(), &webview, &still_hidden);
    }
    if shot.is_none() {
      crate::logging::warn(&format!("thumbs: no picture for {id} after two tries"));
    }
    if let Some(png) = shot {
      remember(&id, png.clone());
      // The card may already be open with nothing in it; hand it the picture rather
      // than making the pointer wait for the next hover.
      if let Some(state) = app.try_state::<crate::ipc::AppState>() {
        // The card is an overlay webview, one per window, tracked by the state.
        let overlay = {
          let guard = state.overlay.read().unwrap_or_else(|e| e.into_inner());
          guard.get(&window_label).cloned()
        };
        if let Some(overlay) = overlay {
          if state
            .overlay_thumb
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .contains(&window_label)
          {
            use tauri::Emitter;
            // A data url, not the raw bytes: the page puts this straight into an img
            // src, and a byte array there is just a broken picture.
            let payload = serde_json::json!({
              "id": id,
              "image": format!("data:image/jpeg;base64,{}", crate::extensions::base64(&png)),
            });
            let _ = app.emit_to(overlay, "thumbs:ready", payload);
          }
        }
      }
    }
    // Back to sleep: the wake-up was for the picture only, and only while the tab is
    // still in the background - freezing the page someone is looking at blanks it.
    if still_hidden()
      && app
        .try_state::<crate::ipc::AppState>()
        .map(|state| state.settings.read().freeze_background)
        .unwrap_or(false)
    {
      crate::freeze::freeze(&webview);
    }
  });
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
pub fn capture_live(
  selected: Option<&tauri::Webview>,
  target: &tauri::Webview,
  still_hidden: &dyn Fn() -> bool,
) -> Option<Vec<u8>> {
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
  // The tab may have been selected while we waited for it to paint, and it is the page on
  // screen now: hiding it at this point is what leaves an empty window behind.
  if !still_hidden() {
    crate::logging::info(&format!("thumbs: {id} became visible, left on screen"));
    return None;
  }
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