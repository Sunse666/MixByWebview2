//! Freezing the tabs that are not on screen.

use std::collections::HashSet;
use parking_lot::Mutex;

static FROZEN: Mutex<Option<HashSet<String>>> = Mutex::new(None);

fn store() -> parking_lot::MutexGuard<'static, Option<HashSet<String>>> {
  FROZEN.lock()
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

/// Stops the page: no timers, no animation frames, no polling.
#[cfg(windows)]
pub fn freeze(webview: &tauri::Webview) -> bool {
  let id = webview.label().to_string();
  if is_frozen(&id) {
    return true;
  }
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
