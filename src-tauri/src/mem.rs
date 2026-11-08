//! Memory numbers for the tab card.
//!
//! WebView2 does not say which process belongs to which tab: an environment reports
//! its processes (browser, renderer, gpu, utility) and their kinds, and nothing ties
//! a renderer back to a tab. So the card shows two honest numbers instead of one
//! made-up one: the whole browser's working set, read from the process list, and the
//! page's own javascript heap, which the page can report for itself.

/// Working set of the whole browser: every process the WebView2 environment owns.
#[cfg(windows)]
pub fn total(webview: &tauri::Webview) -> Option<u64> {
  use webview2_com::Microsoft::Web::WebView2::Win32::{
    ICoreWebView2Environment8, ICoreWebView2_2,
  };
  use windows_core::Interface;

  let (sender, receiver) = std::sync::mpsc::channel::<Vec<u32>>();
  let _ = webview.with_webview(move |platform| unsafe {
    let ids = (|| -> windows_core::Result<Vec<u32>> {
      let controller = platform.controller();
      let core = controller.CoreWebView2()?;
      let env = core.cast::<ICoreWebView2_2>()?.Environment()?;
      let env = env.cast::<ICoreWebView2Environment8>()?;
      let infos = env.GetProcessInfos()?;
      let mut count = 0u32;
      infos.Count(&mut count)?;
      let mut out = Vec::with_capacity(count as usize);
      for index in 0..count {
        let info = infos.GetValueAtIndex(index)?;
        let mut pid = 0i32;
        info.ProcessId(&mut pid)?;
        if pid > 0 {
          out.push(pid as u32);
        }
      }
      Ok(out)
    })()
    .unwrap_or_default();
    let _ = sender.send(ids);
  });
  let pids = receiver
    .recv_timeout(std::time::Duration::from_millis(600))
    .unwrap_or_default();
  if pids.is_empty() {
    return None;
  }
  let mut sum = 0u64;
  for pid in pids {
    sum += working_set(pid).unwrap_or(0);
  }
  if sum == 0 {
    None
  } else {
    Some(sum)
  }
}

#[cfg(not(windows))]
pub fn total(_webview: &tauri::Webview) -> Option<u64> {
  None
}

/// Resident memory of one process.
#[cfg(windows)]
fn working_set(pid: u32) -> Option<u64> {
  use windows::Win32::System::ProcessStatus::{K32GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS};
  use windows::Win32::System::Threading::{OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION};

  unsafe {
    let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
    let mut counters = PROCESS_MEMORY_COUNTERS::default();
    let size = std::mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32;
    let ok = K32GetProcessMemoryInfo(handle, &mut counters, size);
    let _ = windows::Win32::Foundation::CloseHandle(handle);
    if ok.as_bool() {
      Some(counters.WorkingSetSize as u64)
    } else {
      None
    }
  }
}

/// The page's own javascript heap. Read through the protocol, so it works for a tab
/// that is not on screen - nothing needs to be painted for this one.
#[cfg(windows)]
pub fn tab_heap(webview: &tauri::Webview) -> Option<u64> {
  const EXPR: &str = r#"JSON.stringify({heap: performance.memory ? performance.memory.usedJSHeapSize : 0})"#;
  let params = format!(
    r#"{{"expression":{},"returnByValue":true}}"#,
    serde_json::to_string(EXPR).ok()?
  );
  let json = crate::screenshot::call_timeout(
    webview,
    "Runtime.evaluate",
    &params,
    std::time::Duration::from_millis(600),
  )?;
  let value: serde_json::Value = serde_json::from_str(&json).ok()?;
  // The answer carries the json string twice: as a value and as its own text.
  let text = value
    .get("result")
    .and_then(|result| result.get("value"))
    .and_then(|value| value.as_str())?;
  let inner: serde_json::Value = serde_json::from_str(text).ok()?;
  let heap = inner.get("heap")?.as_f64()?;
  if heap <= 0.0 {
    None
  } else {
    Some(heap as u64)
  }
}

#[cfg(not(windows))]
pub fn tab_heap(_webview: &tauri::Webview) -> Option<u64> {
  None
}
