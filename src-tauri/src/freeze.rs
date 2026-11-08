//! Freezing the tabs that are not on screen.
//!
//! A hidden webview keeps its renderer alive with every timer, animation and poll
//! still scheduled; that is most of what a background tab costs. Chromium has a
//! ready answer for it - the page lifecycle state - and it is the same one a real
//! browser uses: a frozen page stops running script until something asks for it.
//!
//! Two tabs are never frozen: the one on screen, and the one that is playing sound,
//! because a browser that silences your music when you switch tabs is broken. Both
//! rules mirror what the browsers this imitates do.

use std::collections::HashSet;
use std::sync::Mutex;

static FROZEN: Mutex<Option<HashSet<String>>> = Mutex::new(None);

fn store() -> std::sync::MutexGuard<'static, Option<HashSet<String>>> {
  FROZEN.lock().unwrap_or_else(|e| e.into_inner())
}

pub fn is_frozen(id: &str) -> bool {
  store().as_ref().map(|set| set.contains(id)).unwrap_or(false)
}

fn mark(id: &str, frozen: bool) {
  let mut guard = store();
  let set = guard.get_or_insert_with(HashSet::new);
  if frozen {
    set.insert(id.to_string());
  } else {
    set.remove(id);
  }
}

/// Stops the page: no timers, no animation frames, no polling. The renderer stays,
/// so the tab keeps its scroll position and its history, and thawing it is instant.
#[cfg(windows)]
pub fn freeze(webview: &tauri::Webview) -> bool {
  let id = webview.label().to_string();
  if is_frozen(&id) {
    return true;
  }
  // Collect first: garbage in the heap survives a freeze until something wakes the
  // page again, so this is the one moment it is cheap to reclaim.
  let _ = crate::screenshot::call_timeout(
    webview,
    "HeapProfiler.collectGarbage",
    "{}",
    std::time::Duration::from_millis(400),
  );
  let ok = crate::screenshot::call_timeout(
    webview,
    "Page.setWebLifecycleState",
    r#"{"state":"frozen"}"#,
    std::time::Duration::from_millis(400),
  )
  .is_some();
  if ok {
    mark(&id, true);
  }
  ok
}

#[cfg(not(windows))]
pub fn freeze(_webview: &tauri::Webview) -> bool {
  false
}

/// Gives the page its timers back.
#[cfg(windows)]
pub fn thaw(webview: &tauri::Webview) -> bool {
  let id = webview.label().to_string();
  if !is_frozen(&id) {
    return true;
  }
  let ok = crate::screenshot::call_timeout(
    webview,
    "Page.setWebLifecycleState",
    r#"{"state":"active"}"#,
    std::time::Duration::from_millis(600),
  )
  .is_some();
  if ok {
    mark(&id, false);
  }
  ok
}

#[cfg(not(windows))]
pub fn thaw(_webview: &tauri::Webview) -> bool {
  false
}

pub fn forget(id: &str) {
  mark(id, false);
}
