//! Tab preview pictures for the hover card.

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

/// Cheap fingerprint over the first 64 KB of a capture.
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
pub fn capture_later(app: tauri::AppHandle, id: String, window_label: String) {
  tauri::async_runtime::spawn_blocking(move || {
    let Some(state) = app.try_state::<crate::ipc::AppState>() else {
      return;
    };
    if !state.settings.read().live_thumbs {
      return;
    }
    const WAKE_AFTER_SECS: u64 = 30;
    let shown = state
      .windows.shown_at
      .read()
      
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
    crate::freeze::thaw(&webview);
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
      if let Some(state) = app.try_state::<crate::ipc::AppState>() {
        let overlay = {
          let guard = state.windows.overlay.read();
          guard.get(&window_label).cloned()
        };
        if let Some(overlay) = overlay {
          if state
            .windows.overlay_thumb
            .read()
            
            .contains(&window_label)
          {
            use tauri::Emitter;
            let payload = serde_json::json!({
              "id": id,
              "image": format!("data:image/jpeg;base64,{}", crate::extensions::base64(&png)),
            });
            let _ = app.emit_to(overlay, "thumbs:ready", payload);
          }
        }
      }
    }
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
/// Age after which a live preview is refreshed.
const LIVE_REFRESH_SECS: u64 = 4;

/// How long the wake-up is given to paint before the photograph is taken.
const LIVE_PAINT_WAIT: std::time::Duration = std::time::Duration::from_millis(160);

/// Takes a picture of a tab that is not the selected one.
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
  if let Some(selected) = selected {
    crate::platform::place_below(target, selected);
  }
  std::thread::sleep(LIVE_PAINT_WAIT);
  if !still_hidden() {
    crate::logging::info(&format!("thumbs: {id} became visible, left on screen"));
    return None;
  }
  let png = capture(target);
  let _ = target.hide();
  crate::logging::info(&format!(
    "thumbs: live {id} -> {}",
    match &png {
      Some(bytes) => format!("{} bytes fp={:x}", bytes.len(), fingerprint(bytes)),
      None => "nothing".to_string(),
    }
  ));
  png
}