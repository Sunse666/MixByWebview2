//! Debug hooks: the things only a debug build can be asked to do.
//!
//! Every one of them is armed by an environment variable and starts a thread that waits
//! a second or two. They exist because this machine cannot be driven from a script: the
//! only way to reach a panel, a gesture or an extension without a click is to ask for it
//! before the window opens. `install` is called once the main window is up, so nothing
//! here has to be reachable from release code.

use tauri::Manager;

use crate::ipc::AppState;
use crate::window::{relayout, PRIMARY};

/// Set `MIX_DEBUG=1` to arm any of them; `MIX_DEBUG=0` disarms the lot.
fn armed() -> bool {
    std::env::var("MIX_DEBUG").is_ok_and(|value| value != "0")
}

/// Runs whatever the environment asked for.
pub fn install(app: &mut tauri::App) {
    ext_toggle(app.handle());
    gesture_test(app.handle());
    if armed() {
        if let Ok(kind) = std::env::var("MIX_OPEN_ON_START") {
        let handle = app.handle().clone();
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(1500));
            let target = handle.clone();
            let _ = handle.run_on_main_thread(move || {
                let Some(win) = target.get_window(PRIMARY) else {
                    return;
                };
                if let Err(err) = crate::overlay::open(&target, &win, &kind) {
                    crate::logging::warn(&format!("debug overlay failed: {err}"));
                }
            });
        });
        }
    }

    if armed() && std::env::var("MIX_DIAG_ON_START").is_ok() {
        let handle = app.handle().clone();
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(2500));
            match crate::diag::export(&handle) {
                Ok(path) => crate::logging::info(&format!("debug diag: {}", path.display())),
                Err(err) => crate::logging::warn(&format!("debug diag failed: {err}")),
            }
        });
    }

    if armed() && std::env::var("MIX_FULLSCREEN").is_ok() {
        let handle = app.handle().clone();
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(2500));
            let target = handle.clone();
            let _ = handle.run_on_main_thread(move || {
                let state = target.state::<AppState>();
                state.windows.fullscreen.write().insert(PRIMARY.to_string());
                let _ = relayout(&target, PRIMARY);
                crate::logging::info("debug: page marked fullscreen");
            });
        });
    }

    if armed() && std::env::var("MIX_NEW_WINDOW").is_ok() {
        let handle = app.handle().clone();
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(2000));
            let target = handle.clone();
            let _ = handle.run_on_main_thread(move || {
                let state = target.state::<AppState>();
                let url = state.settings.read().start_url.clone();
                drop(state);
                match crate::window::open_window(&target, "w2", vec![(url, 0.0, false)], 0, None) {
                    Ok(_) => crate::logging::info("debug: second window opened"),
                    Err(err) => crate::logging::warn(&format!("debug window failed: {err}")),
                }
            });
        });
    }

    if armed() && std::env::var("MIX_TRANSLATE").is_ok() {
        let handle = app.handle().clone();
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(3000));
            let repeats: u32 = std::env::var("MIX_TRANSLATE")
                .ok()
                .and_then(|value| value.parse().ok())
                .unwrap_or(1);
            for round in 0..repeats {
                std::thread::sleep(std::time::Duration::from_millis(if round == 0 { 0 } else { 5000 }));
                let target = handle.clone();
                let _ = handle.run_on_main_thread(move || {
                    let Some(win) = target.get_window(PRIMARY) else {
                        return;
                    };
                    let state = target.state::<AppState>();
                    let call = crate::ipc::translate_page_inplace(target.clone(), win, state);
                    match tauri::async_runtime::block_on(call) {
                        Ok(()) => crate::logging::info("debug translate: started"),
                        Err(err) => crate::logging::warn(&format!("debug translate failed: {}", err.message)),
                    }
                });
            }
        });
    }

    if armed() && std::env::var("MIX_DOM_PUSH").is_ok() {
        let handle = app.handle().clone();
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(9000));
            let target = handle.clone();
            let _ = handle.run_on_main_thread(move || {
                let state = target.state::<AppState>();
                let Some(win) = target.get_window(PRIMARY) else {
                    return;
                };
                let label = crate::ipc::selected_label(&state, &win);
                if let Some(webview) = target.get_webview(&label) {
                    let _ = webview.eval(
                        "(function(){var p=document.createElement('p');p.textContent='A paragraph injected by the test hook.';document.body.appendChild(p);})()",
                    );
                }
            });
        });
    }

    if armed() && std::env::var("MIX_TOGGLE_EXT").is_ok() {
        let handle = app.handle().clone();
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(4000));
            let target = handle.clone();
            let _ = handle.run_on_main_thread(move || {
                let name = {
                    let state = target.state::<AppState>();
                    state
                        .extensions
                        .iter()
                        .find(|ext| ext.status == "loaded")
                        .map(|ext| ext.name.clone())
                };
                let Some(name) = name else {
                    return;
                };
                crate::logging::info(&format!("debug: hiding {name}"));
                if target.get_window(PRIMARY).is_none() {
                    return;
                }
                let state = target.state::<AppState>();
                let patch = serde_json::json!({ "hidden_extensions": [name] });
                match crate::ipc::apply_settings_patch(&state, patch) {
                    Ok(_) => crate::logging::info("debug: settings patched"),
                    Err(err) => crate::logging::warn(&format!("debug patch failed: {}", err.message)),
                }
            });
        });
    }

    if armed() {
        if let Ok(url) = std::env::var("MIX_SAVE_URL") {
        let handle = app.handle().clone();
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(2500));
            let target = handle.clone();
            let _ = handle.run_on_main_thread(move || {
                let state = target.state::<AppState>();
                let call = crate::ipc::save_url(target.clone(), &state, url, None);
                match tauri::async_runtime::block_on(call) {
                    Ok(path) => crate::logging::info(&format!("debug save: {path}")),
                    Err(err) => crate::logging::warn(&format!("debug save failed: {}", err.message)),
                }
            });
        });
        }
    }

    if armed() && std::env::var("MIX_NAV").is_ok() {
        let handle = app.handle().clone();
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(2500));
            let target = handle.clone();
            let _ = handle.run_on_main_thread(move || {
                let Some(win) = target.get_window(PRIMARY) else {
                    return;
                };
                let state = target.state::<AppState>();
                let call = crate::ipc::tab_navigate(
                    target.clone(),
                    win,
                    state,
                    "https://nonexistent-host-mix-test.invalid".to_string(),
                );
                let _ = tauri::async_runtime::block_on(call);
            });
        });
    }

    if armed() && std::env::var("MIX_ERROR_PAGE").is_ok() {
        let handle = app.handle().clone();
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(2500));
            let target = handle.clone();
            let _ = handle.run_on_main_thread(move || {
                if let Some(state) = target.try_state::<AppState>() {
                    let id = state
                        .try_read_core(PRIMARY)
                        .and_then(|core| core.selected.clone())
                        .unwrap_or_default();
                    crate::ipc::show_error_page(&target, &id, "hello world");
                }
            });
        });
    }

    if armed() && std::env::var("MIX_DEVTOOLS").is_ok() {
        let handle = app.handle().clone();
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(2500));
            let target = handle.clone();
            let _ = handle.run_on_main_thread(move || {
                let Some(win) = target.get_window(PRIMARY) else {
                    return;
                };
                let state = target.state::<AppState>();
                let call = crate::ipc::shortcut(
                    target.clone(),
                    win,
                    state,
                    "devtools".to_string(),
                );
                let _ = tauri::async_runtime::block_on(call);
            });
        });
    }
}

/// Debug hook: toggles one extension and logs the outcome.
fn ext_toggle(app: &tauri::AppHandle) {
    if !armed() {
        return;
    }
    let Ok(wanted) = std::env::var("MIX_EXT_TOGGLE") else {
        return;
    };
    let handle = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_secs(3));
        let Some(webview) = handle.get_webview("w1-tab-1") else {
            return;
        };
        let before = crate::ext_live::list(&webview);
        let Some(target) = before.iter().find(|item| item.name.contains(&wanted)) else {
            crate::logging::info(&format!("ext toggle: no match for {wanted}"));
            return;
        };
        let ok = crate::ext_live::set_enabled(&webview, &target.id, !target.enabled);
        crate::logging::info(&format!("ext toggle {} -> {}: {ok}", target.name, !target.enabled));
        let after = crate::ext_live::list(&webview);
        if let Some(seen) = after.iter().find(|item| item.id == target.id) {
            crate::logging::info(&format!("ext toggle after: enabled={}", seen.enabled));
        }
    });
}

/// Debug hook: runs a synthetic left gesture in the active tab.
fn gesture_test(app: &tauri::AppHandle) {
    if !armed() {
        return;
    }
    if std::env::var("MIX_GESTURE_TEST").is_err() {
        return;
    }
    let handle = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_secs(4));
        let Some(webview) = handle.get_webview("w1-tab-1") else {
            return;
        };
        let script = r#"(function(){
          function fire(type, x, y) {
            var event = new MouseEvent(type, { bubbles: true, cancelable: true, button: 2, buttons: 2, clientX: x, clientY: y });
            window.dispatchEvent(event);
          }
          fire('mousedown', 600, 400);
          fire('mousemove', 520, 400);
          fire('mousemove', 440, 400);
          fire('mouseup', 380, 400);
          return 'gesture synthetic done';
        })();"#;
        match webview.eval(script) {
            Ok(()) => crate::logging::info("gesture test: synthetic left drag sent"),
            Err(err) => crate::logging::warn(&format!("gesture test failed: {err}")),
        }
    });
}
