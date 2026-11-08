use std::ffi::c_void;

use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use tauri::Window;
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::UI::HiDpi::GetDpiForWindow;
use windows::Win32::UI::Shell::{DefSubclassProc, SetWindowSubclass};
use windows::Win32::UI::WindowsAndMessaging::{
    GetWindowRect, IsZoomed, SetWindowPos, HTBOTTOM, HTBOTTOMLEFT, HTBOTTOMRIGHT, HTLEFT, HTRIGHT,
    HTTOP, HTTOPLEFT, HTTOPRIGHT, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, WM_NCHITTEST,
};

const BASE_EDGE: f32 = 5.0;

/// A borderless window has no resize frame, so answer WM_NCHITTEST ourselves and
/// let Windows do the resizing. Works for all eight directions and for the
/// bottom/left/right edges that the chrome strip cannot reach.
pub fn install(window: &Window) {
    let Ok(handle) = window.window_handle() else {
        return;
    };
    let RawWindowHandle::Win32(raw) = handle.as_raw() else {
        return;
    };
    let hwnd = HWND(raw.hwnd.get() as *mut c_void);

    let installed = unsafe { SetWindowSubclass(hwnd, Some(hit_test), 0, 0) };
    if installed.as_bool() {
        crate::logging::info("native resize borders installed");
    } else {
        crate::logging::warn("failed to install native resize borders");
    }
}

unsafe extern "system" fn hit_test(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    _id: usize,
    _data: usize,
) -> LRESULT {
    if msg == WM_NCHITTEST {
        if let Some(code) = edge_code(hwnd, lparam) {
            return LRESULT(code as isize);
        }
    }
    DefSubclassProc(hwnd, msg, wparam, lparam)
}

unsafe fn edge_code(hwnd: HWND, lparam: LPARAM) -> Option<u32> {
    if IsZoomed(hwnd).as_bool() {
        return None;
    }

    let mut rect = RECT::default();
    if GetWindowRect(hwnd, &mut rect).is_err() {
        return None;
    }

    let scale = GetDpiForWindow(hwnd) as f32 / 96.0;
    let edge = (BASE_EDGE * scale).round() as i32;
    let corner = edge * 3;

    let x = (lparam.0 & 0xffff) as i16 as i32;
    let y = ((lparam.0 >> 16) & 0xffff) as i16 as i32;

    let on_left = x < rect.left + edge;
    let on_right = x >= rect.right - edge;
    let on_top = y < rect.top + edge;
    let on_bottom = y >= rect.bottom - edge;

    let corner_left = x < rect.left + corner;
    let corner_right = x >= rect.right - corner;
    let corner_top = y < rect.top + corner;
    let corner_bottom = y >= rect.bottom - corner;

    let code = if corner_top && corner_left {
        HTTOPLEFT
    } else if corner_top && corner_right {
        HTTOPRIGHT
    } else if corner_bottom && corner_left {
        HTBOTTOMLEFT
    } else if corner_bottom && corner_right {
        HTBOTTOMRIGHT
    } else if on_left {
        HTLEFT
    } else if on_right {
        HTRIGHT
    } else if on_top {
        HTTOP
    } else if on_bottom {
        HTBOTTOM
    } else {
        return None;
    };

    Some(code)
}
/// The child window WebView2 creates for one webview.
fn child_hwnd(webview: &tauri::Webview) -> Option<HWND> {
    use webview2_com::Microsoft::Web::WebView2::Win32::ICoreWebView2Controller;

    // with_webview wants a 'static Send closure, so the handle travels back through
    // a channel as a plain number.
    let (sender, receiver) = std::sync::mpsc::channel::<isize>();
    let _ = webview.with_webview(move |platform| unsafe {
        let controller = platform.controller();
        let controller: &ICoreWebView2Controller = &controller;
        let mut hwnd = HWND::default();
        if controller.ParentWindow(&mut hwnd).is_ok() && !hwnd.0.is_null() {
            let _ = sender.send(hwnd.0 as isize);
        }
    });
    let raw = receiver.recv_timeout(std::time::Duration::from_millis(500)).unwrap_or(0);
    if raw == 0 {
        None
    } else {
        Some(HWND(raw as *mut std::ffi::c_void))
    }
}

/// Places one webview directly under another: SetWindowPos puts a window below the
/// one passed as hWndInsertAfter, which is exactly the order the preview wants.
pub fn place_below(webview: &tauri::Webview, above: &tauri::Webview) {
    let Some(above) = child_hwnd(above) else { return };
    let Some(hwnd) = child_hwnd(webview) else { return };
    unsafe {
        let _ = SetWindowPos(
            hwnd,
            Some(above),
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
        );
    }
}
