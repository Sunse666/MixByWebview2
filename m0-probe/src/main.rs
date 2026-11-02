//! M0 probe: one binary, one experiment per run.
//! Usage: cargo run -- --exp e1|e2|e3|e4|e5|e6|e7|e9
//! Every experiment opens the same 68px chrome strip so the layout matches the real app.

use std::path::PathBuf;
use tauri::{LogicalPosition, LogicalSize, Webview, WebviewBuilder, WebviewUrl, Window};

const W: f64 = 1100.0;
const H: f64 = 760.0;
const CHROME_H: f64 = 68.0;

fn arg_value(flag: &str) -> Option<String> {
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        if a == flag {
            return args.next();
        }
    }
    None
}

fn exe_dir() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(PathBuf::from))
        .unwrap_or_else(|| PathBuf::from("."))
}

fn main() {
    let exp = arg_value("--exp").unwrap_or_else(|| "e2".into());
    println!("[probe] experiment = {exp}");

    tauri::Builder::default()
        .setup(move |app| {
            let win = Window::builder(app, "probe")
                .title(format!("M0 {exp}"))
                .inner_size(W, H)
                .decorations(false)
                .build()?;

            // Chrome strip always exists; tab webviews sit below it.
            let chrome = win.add_child(
                WebviewBuilder::new("chrome", WebviewUrl::App("chrome.html".into())),
                LogicalPosition::new(0.0, 0.0),
                LogicalSize::new(W, CHROME_H),
            )?;

            match exp.as_str() {
                "e1" => e1_extensions(&win, &chrome)?,
                "e2" => e2_tabs(&win)?,
                "e3" => e3_inject(&win)?,
                "e4" => e4_overlay(&win)?,
                "e5" => e5_download(&win)?,
                "e6" => e6_ime(&win)?,
                "e7" => e7_dpi(&win)?,
                "e9" => e9_platform(&win)?,
                other => println!("[probe] unknown experiment: {other}"),
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("probe failed to start");
}

fn tab_at(win: &Window, label: &str, page: &str, x: f64, w: f64) -> tauri::Result<Webview> {
    win.add_child(
        WebviewBuilder::new(label, WebviewUrl::App(page.into())),
        LogicalPosition::new(x, CHROME_H),
        LogicalSize::new(w, H - CHROME_H),
    )
}

/// E1: WebView2 loads an unpacked MV3 extension. Put it under <exe dir>/extensions.
fn e1_extensions(win: &Window, _chrome: &Webview) -> tauri::Result<()> {
    let ext_dir = exe_dir().join("extensions");
    println!("[e1] extensions dir: {}", ext_dir.display());
    if !ext_dir.is_dir() {
        println!("[e1] MISSING: create it and drop an unpacked MV3 extension inside");
    }

    let builder = WebviewBuilder::new("tab0", WebviewUrl::App("tab.html?e1".into()));
    #[cfg(windows)]
    let builder = builder
        .browser_extensions_enabled(true)
        .extensions_path(ext_dir);

    let wv = win.add_child(
        builder.on_page_load(|_wv, payload| println!("[e1] loaded {}", payload.url())),
        LogicalPosition::new(0.0, CHROME_H),
        LogicalSize::new(W, H - CHROME_H),
    )?;

    // Pass criteria: content script output shows up and chrome.runtime.id is defined.
    let _ = wv.eval("console.log('[e1] runtime.id =', window.chrome && chrome.runtime && chrome.runtime.id)");
    Ok(())
}

/// E2: two independent webviews, side by side. Type, scroll and resize in both.
fn e2_tabs(win: &Window) -> tauri::Result<()> {
    tab_at(win, "tab0", "tab.html?left", 0.0, W / 2.0)?
        .eval("console.log('[e2] left ready')")
        .ok();
    tab_at(win, "tab1", "tab.html?right", W / 2.0, W / 2.0)?
        .eval("console.log('[e2] right ready')")
        .ok();
    Ok(())
}

/// E3: init script must run in every frame before page scripts.
fn e3_inject(win: &Window) -> tauri::Result<()> {
    let init = r#"
      window.__PROBE = {
        isTop: window.top === window,
        ready: document.readyState,
        at: performance.now(),
        href: location.href
      };
    "#;

    let wv = win.add_child(
        WebviewBuilder::new("tab0", WebviewUrl::App("frames.html".into()))
            .initialization_script_for_all_frames(init)
            .on_page_load(|wv, _p| {
                let _ = wv.eval("console.log('[e3]', JSON.stringify(window.__PROBE))");
            }),
        LogicalPosition::new(0.0, CHROME_H),
        LogicalSize::new(W, H - CHROME_H),
    )?;
    drop(wv);
    Ok(())
}

/// E4: third webview floating above a tab webview. Check paint order and focus.
fn e4_overlay(win: &Window) -> tauri::Result<()> {
    tab_at(win, "tab0", "tab.html?e4", 0.0, W)?;
    win.add_child(
        WebviewBuilder::new("overlay", WebviewUrl::App("overlay.html".into()))
            .transparent(true)
            .focused(true)
            .on_page_load(|wv, _p| {
                let _ = wv.eval("console.log('[e4] overlay loaded')");
            }),
        LogicalPosition::new(140.0, CHROME_H + 60.0),
        LogicalSize::new(420.0, 260.0),
    )?;
    Ok(())
}

/// E5: download handler plus a file input. Click the link and pick a file.
fn e5_download(win: &Window) -> tauri::Result<()> {
    win.add_child(
        WebviewBuilder::new("tab0", WebviewUrl::App("tab.html?e5".into())).on_download(
            |_wv, event| {
                let requested = matches!(event, tauri::webview::DownloadEvent::Requested { .. });
                println!("[e5] download event, requested = {requested}");
                true
            },
        ),
        LogicalPosition::new(0.0, CHROME_H),
        LogicalSize::new(W, H - CHROME_H),
    )?;
    Ok(())
}

/// E6: IME. Two editable panes: switch focus, type Chinese, check the candidate window.
fn e6_ime(win: &Window) -> tauri::Result<()> {
    tab_at(win, "tab0", "tab.html?ime-a", 0.0, W / 2.0)?;
    tab_at(win, "tab1", "tab.html?ime-b", W / 2.0, W / 2.0)?;
    Ok(())
}

/// E7: DPI. Drag the window across monitors with different scale factors.
fn e7_dpi(win: &Window) -> tauri::Result<()> {
    println!("[e7] scale_factor = {}", win.scale_factor()?);
    println!("[e7] inner_size = {:?}", win.inner_size()?);
    for m in win.available_monitors()? {
        println!(
            "[e7] monitor {:?} size={:?} scale={}",
            m.name(),
            m.size(),
            m.scale_factor()
        );
    }
    tab_at(win, "tab0", "tab.html?e7", 0.0, W)?;
    Ok(())
}

/// E9: platform. Tray and global hotkeys live in the real app; here we only prove
/// that the window can position itself (Wayland refuses) and report the monitors.
fn e9_platform(win: &Window) -> tauri::Result<()> {
    println!("[e9] os = {}", std::env::consts::OS);
    let before = win.outer_position()?;
    win.set_position(tauri::PhysicalPosition::new(before.x + 120, before.y + 80))?;
    let after = win.outer_position()?;
    println!("[e9] move round-trip {before:?} -> {after:?}");
    tab_at(win, "tab0", "tab.html?e9", 0.0, W)?;
    Ok(())
}
