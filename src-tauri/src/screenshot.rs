//! Page screenshots.
//!
//! ICoreWebView2::CapturePreview wants an IStream and a completion handler, and
//! the bytes then have to be read back out of the stream. The DevTools protocol
//! the webview already exposes does the same job with one callback: Page
//! .captureScreenshot answers with a base64 png.

/// Grabs the visible page as png bytes.
#[cfg(windows)]
pub fn capture(webview: &tauri::Webview) -> Option<Vec<u8>> {
  use webview2_com::Microsoft::Web::WebView2::Win32::ICoreWebView2;

  let (sender, receiver) = std::sync::mpsc::channel();
  let done = sender.clone();
  let request = webview.with_webview(move |platform| unsafe {
    let result = (|| -> windows_core::Result<()> {
      let controller = platform.controller();
      let core = controller.CoreWebView2()?;
      let core: &ICoreWebView2 = &core;
      let sender_for_handler = std::sync::mpsc::Sender::clone(&done);
      let handler = webview2_com::CallDevToolsProtocolMethodCompletedHandler::create(Box::new(
        move |status, json| {
          let answer = if status.is_ok() {
            Some(json.to_string())
          } else {
            None
          };
          let _ = sender_for_handler.send(answer);
          Ok(())
        },
      ));
      let method = windows_core::HSTRING::from("Page.captureScreenshot");
      let params = windows_core::HSTRING::from(r#"{"format":"png"}"#);
      core.CallDevToolsProtocolMethod(&method, &params, &handler)
    })();
    if result.is_err() {
      let _ = std::sync::mpsc::Sender::send(&done, None);
    }
  });
  if request.is_err() {
    return None;
  }
  let json = receiver
    .recv_timeout(std::time::Duration::from_secs(8))
    .ok()
    .flatten()?;
  let value: serde_json::Value = serde_json::from_str(&json).ok()?;
  let data = value.get("data")?.as_str()?;
  decode_base64(data)
}

#[cfg(not(windows))]
pub fn capture(_webview: &tauri::Webview) -> Option<Vec<u8>> {
  None
}

/// Decodes standard base64, ignoring whitespace. Enough for a data field, so the
/// project does not need another dependency for twenty lines.
fn decode_base64(input: &str) -> Option<Vec<u8>> {
  let mut out = Vec::with_capacity(input.len() * 3 / 4);
  let mut buffer: u32 = 0;
  let mut bits: u32 = 0;
  for byte in input.bytes() {
    let value = match byte {
      b'A'..=b'Z' => u32::from(byte - b'A'),
      b'a'..=b'z' => u32::from(byte - b'a') + 26,
      b'0'..=b'9' => u32::from(byte - b'0') + 52,
      b'+' => 62,
      b'/' => 63,
      b'=' => break,
      _ => continue,
    };
    buffer = (buffer << 6) | value;
    bits += 6;
    if bits >= 8 {
      bits -= 8;
      out.push((buffer >> bits) as u8);
    }
  }
  if out.is_empty() {
    None
  } else {
    Some(out)
  }
}

#[cfg(test)]
mod tests {
  use super::decode_base64;

  #[test]
  fn decodes_a_round_trip() {
    let encoded = crate::extensions::base64(b"hello world");
    assert_eq!(decode_base64(&encoded).as_deref(), Some(&b"hello world"[..]));
  }

  #[test]
  fn ignores_padding_and_noise() {
    // "hi" with padding, plus a stray newline as a data field could carry.
    assert_eq!(decode_base64("aGk=\n").as_deref(), Some(&b"hi"[..]));
    assert_eq!(decode_base64(""), None);
  }
}
