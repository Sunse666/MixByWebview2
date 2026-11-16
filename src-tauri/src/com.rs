//! One way to ask a webview something.
//!
//! A COM call has to run on the thread that owns the webview, and the answer usually
//! arrives later, from a callback. Every call site used to spell out that channel and
//! its own deadline, so "what happens when a webview does not answer" had as many
//! answers as there were callers. It is not optional: a background tab, a page that is
//! gone, a controller that is being torn down - all of them leave a caller waiting
//! forever, and the caller is often the thread that draws the window.

/// Runs one call against a webview and waits for the answer it sends.
///
/// `run` gets the platform webview and the sender to answer on. A call whose answer
/// comes from a COM callback keeps the sender alive inside that callback, which is why
/// the sender is handed over rather than returned.
///
/// None means the webview refused the call or did not answer in time. A call that has
/// nothing to say - "it worked", "it failed" - should say so, rather than leave the
/// caller waiting out the deadline.
#[cfg(windows)]
pub fn call<T: Send + 'static>(
    webview: &tauri::Webview,
    wait: std::time::Duration,
    run: impl FnOnce(&tauri::webview::PlatformWebview, std::sync::mpsc::Sender<T>) + Send + 'static,
) -> Option<T> {
    let (sender, receiver) = std::sync::mpsc::channel();
    webview
        .with_webview(move |platform| run(&platform, sender))
        .ok()?;
    receiver.recv_timeout(wait).ok()
}
