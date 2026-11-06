//! Reads and toggles the extensions WebView2 itself loaded. The manifest data we
//! parse is only a mirror: this is the authoritative list.

#[cfg(windows)]
use webview2_com::Microsoft::Web::WebView2::Win32::{
    ICoreWebView2BrowserExtension, ICoreWebView2Profile7, ICoreWebView2_13,
};
#[cfg(windows)]
use webview2_com::ProfileGetBrowserExtensionsCompletedHandler;
#[cfg(windows)]
use windows_core::Interface;

/// One extension as WebView2 sees it.
#[derive(Debug, Clone, serde::Serialize)]
pub struct LiveExtension {
    pub id: String,
    pub name: String,
    pub enabled: bool,
}

#[cfg(windows)]
unsafe fn wide_to_string(value: windows_core::PWSTR) -> String {
    if value.is_null() {
        return String::new();
    }
    let text = value.to_string().unwrap_or_default();
    windows_core::PWSTR::from_raw(value.as_ptr());
    text
}

#[cfg(windows)]
unsafe fn read_extension(extension: &ICoreWebView2BrowserExtension) -> LiveExtension {
    let mut id = windows_core::PWSTR::null();
    let mut name = windows_core::PWSTR::null();
    let mut enabled = windows_core::BOOL(0);
    let _ = extension.Id(&mut id);
    let _ = extension.Name(&mut name);
    let _ = extension.IsEnabled(&mut enabled);
    LiveExtension {
        id: wide_to_string(id),
        name: wide_to_string(name),
        enabled: enabled.as_bool(),
    }
}

/// Lists the extensions the running browser actually loaded.
#[cfg(windows)]
pub fn list(webview: &tauri::Webview) -> Vec<LiveExtension> {
    let (sender, receiver) = std::sync::mpsc::channel();
    let fallback = sender.clone();
    let request = webview.with_webview(move |platform| unsafe {
        let result = (|| -> windows_core::Result<()> {
            let controller = platform.controller();
            let core = controller.CoreWebView2()?;
            let profile = core.cast::<ICoreWebView2_13>()?.Profile()?;
            let profile7 = profile.cast::<ICoreWebView2Profile7>()?;
            let handler = ProfileGetBrowserExtensionsCompletedHandler::create(Box::new(
                move |error, list| {
                    let mut found = Vec::new();
                    if error.is_ok() {
                        if let Some(list) = list {
                            let mut count = 0u32;
                            if list.Count(&mut count).is_ok() {
                                for index in 0..count {
                                    if let Ok(extension) = list.GetValueAtIndex(index) {
                                        found.push(read_extension(&extension));
                                    }
                                }
                            }
                        }
                    }
                    let _ = sender.send(found);
                    Ok(())
                },
            ));
            profile7.GetBrowserExtensions(&handler)
        })();
        if result.is_err() {
            let _ = std::sync::mpsc::Sender::send(&fallback, Vec::new());
        }
    });

    if request.is_err() {
        return Vec::new();
    }
    receiver
        .recv_timeout(std::time::Duration::from_secs(4))
        .unwrap_or_default()
}

/// Turns one extension on or off without a restart.
#[cfg(windows)]
pub fn set_enabled(webview: &tauri::Webview, id: &str, enabled: bool) -> bool {
    let target = id.to_string();
    let (sender, receiver) = std::sync::mpsc::channel();
    let request = webview.with_webview(move |platform| unsafe {
        let done = std::sync::mpsc::Sender::clone(&sender);
        let result = (|| -> windows_core::Result<()> {
            let controller = platform.controller();
            let core = controller.CoreWebView2()?;
            let profile = core.cast::<ICoreWebView2_13>()?.Profile()?;
            let profile7 = profile.cast::<ICoreWebView2Profile7>()?;
            let sender_for_list = std::sync::mpsc::Sender::clone(&done);
            let wanted = target.clone();
            let handler = ProfileGetBrowserExtensionsCompletedHandler::create(Box::new(
                move |error, list| {
                    if error.is_err() {
                        let _ = sender_for_list.send(false);
                        return Ok(());
                    }
                    let Some(list) = list else {
                        let _ = sender_for_list.send(false);
                        return Ok(());
                    };
                    let mut count = 0u32;
                    if list.Count(&mut count).is_err() {
                        let _ = sender_for_list.send(false);
                        return Ok(());
                    }
                    for index in 0..count {
                        let Ok(extension) = list.GetValueAtIndex(index) else {
                            continue;
                        };
                        let current = read_extension(&extension);
                        if current.id != wanted {
                            continue;
                        }
                        let sender_for_enable = std::sync::mpsc::Sender::clone(&sender_for_list);
                        let enable = webview2_com::BrowserExtensionEnableCompletedHandler::create(
                            Box::new(move |status| {
                                let _ = sender_for_enable.send(status.is_ok());
                                Ok(())
                            }),
                        );
                        if extension.Enable(enabled, &enable).is_err() {
                            let _ = sender_for_list.send(false);
                        }
                        return Ok(());
                    }
                    let _ = sender_for_list.send(false);
                    Ok(())
                },
            ));
            profile7.GetBrowserExtensions(&handler)
        })();
        if result.is_err() {
            let _ = std::sync::mpsc::Sender::send(&done, false);
        }
    });

    if request.is_err() {
        return false;
    }
    receiver
        .recv_timeout(std::time::Duration::from_secs(4))
        .unwrap_or(false)
}

#[cfg(not(windows))]
pub fn list(_webview: &tauri::Webview) -> Vec<LiveExtension> {
    Vec::new()
}

#[cfg(not(windows))]
pub fn set_enabled(_webview: &tauri::Webview, _id: &str, _enabled: bool) -> bool {
    false
}
