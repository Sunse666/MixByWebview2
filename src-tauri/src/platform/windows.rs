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

/// Answers WM_NCHITTEST so a borderless window can be resized.
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

    let raw = crate::com::call(
        webview,
        std::time::Duration::from_millis(500),
        |platform, out| unsafe {
            let controller = platform.controller();
            let controller: &ICoreWebView2Controller = &controller;
            let mut hwnd = HWND::default();
            if controller.ParentWindow(&mut hwnd).is_ok() && !hwnd.0.is_null() {
                let _ = out.send(hwnd.0 as isize);
            }
        },
    )
    .unwrap_or(0);
    if raw == 0 {
        None
    } else {
        Some(HWND(raw as *mut std::ffi::c_void))
    }
}

/// Places one webview directly under another.
pub fn place_below(webview: &tauri::Webview, above: &tauri::Webview) {
    let Some(above) = child_hwnd(above) else {
        return;
    };
    let Some(hwnd) = child_hwnd(webview) else {
        return;
    };
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

/// Takes the picture-in-picture window out of the taskbar.
///
/// The small window is a real top-level window that the engine creates inside this process,
/// with `Chrome_WidgetWin_1` as its class and no `WS_EX_TOOLWINDOW` - and the absence of that
/// one bit is the whole reason it gets a taskbar button. Measured on this build: the window
/// carried `0x200108` (`NOREDIRECTIONBITMAP | WINDOWEDGE | TOPMOST`) and nothing else, and
/// setting the bit makes the button go away without touching how the window draws.
///
/// Only windows belonging to this process are visited, so a browser running beside this one
/// cannot be affected: the enumeration starts from our own windows and checks the owner of
/// every candidate.
///
/// Called when a page says it has gone into picture-in-picture, which is a moment after the
/// window exists - hence the retry, and the short one: the engine creates the window while
/// the page is still finishing its own transition.
pub fn hide_pip_from_taskbar() {
    use std::thread;
    use std::time::Duration;

    thread::spawn(|| {
        for _ in 0..20 {
            if mark_pip_windows() > 0 {
                return;
            }
            thread::sleep(Duration::from_millis(150));
        }
        crate::logging::warn("no picture-in-picture window found to hide from the taskbar");
    });
}

/// Marks every picture-in-picture window of this process. Returns how many were found.
fn mark_pip_windows() -> usize {
    use windows::Win32::Foundation::{HWND, LPARAM};
    use windows::Win32::UI::WindowsAndMessaging::{
        EnumWindows, GetClassNameW, GetWindowLongW, GetWindowThreadProcessId, SetWindowLongW,
        SetWindowPos, GWL_EXSTYLE, HWND_TOP, SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_NOMOVE,
        SWP_NOSIZE, SWP_NOZORDER, WS_EX_TOOLWINDOW,
    };

    struct Found(usize);
    let mut found = Found(0);

    unsafe extern "system" fn visit(hwnd: HWND, data: LPARAM) -> windows::core::BOOL {
        use windows::Win32::UI::WindowsAndMessaging::{GetWindowTextW, GWL_STYLE, WS_CHILD};

        let found = unsafe { &mut *(data.0 as *mut Found) };
        let mut pid = 0u32;
        unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
        if pid != std::process::id() {
            return true.into();
        }
        // A child window never gets a taskbar button of its own.
        let style = unsafe { GetWindowLongW(hwnd, GWL_STYLE) } as u32;
        if style & WS_CHILD.0 != 0 {
            return true.into();
        }
        let mut name = [0u16; 64];
        let len = unsafe { GetClassNameW(hwnd, &mut name) };
        let class = String::from_utf16_lossy(&name[..len.max(0) as usize]);
        if class != "Chrome_WidgetWin_1" {
            return true.into();
        }
        // The engine's own window for the player, identified by its title. A top-level
        // Chrome_WidgetWin_1 in this process that is not the pip one belongs to a tab webview
        // and is a child; one that is a child was already skipped above.
        let mut title = [0u16; 128];
        let text_len = unsafe { GetWindowTextW(hwnd, &mut title) };
        let text = String::from_utf16_lossy(&title[..text_len.max(0) as usize]);
        if !looks_like_pip(&text) {
            return true.into();
        }
        let ex = unsafe { GetWindowLongW(hwnd, GWL_EXSTYLE) } as u32;
        if ex & WS_EX_TOOLWINDOW.0 == 0 {
            unsafe { SetWindowLongW(hwnd, GWL_EXSTYLE, (ex | WS_EX_TOOLWINDOW.0) as i32) };
            // The style is only honoured after the frame is recomputed.
            unsafe {
                let _ = SetWindowPos(
                    hwnd,
                    Some(HWND_TOP),
                    0,
                    0,
                    0,
                    0,
                    SWP_FRAMECHANGED | SWP_NOACTIVATE | SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER,
                );
            }
            crate::logging::info(&format!(
                "picture-in-picture window hidden from the taskbar: {text}"
            ));
        }
        found.0 += 1;
        true.into()
    }

    unsafe {
        let _ = EnumWindows(Some(visit), LPARAM(&mut found as *mut Found as isize));
    }
    found.0
}

/// Whether a window title is the picture-in-picture one.
///
/// Matched loosely on the words the engine uses, because the title is localised: the Chinese
/// build says 画中画, the English one "Picture-in-Picture", and a future one may say something
/// else again. The window is already known to be a top-level `Chrome_WidgetWin_1` in this
/// process, so the bar for a match does not have to be high.
fn looks_like_pip(title: &str) -> bool {
    let lower = title.to_lowercase();
    lower.contains("picture") || lower.contains("pip") || title.contains("画中画")
}
