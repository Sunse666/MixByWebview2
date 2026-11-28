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
    bili_test(app.handle());
    download_states_test(app.handle());
    queue_test(app.handle());
    retry_credentials_test(app.handle());
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
                std::thread::sleep(std::time::Duration::from_millis(if round == 0 {
                    0
                } else {
                    5000
                }));
                let target = handle.clone();
                let _ = handle.run_on_main_thread(move || {
                    let Some(win) = target.get_window(PRIMARY) else {
                        return;
                    };
                    let state = target.state::<AppState>();
                    let call = crate::ipc::translate_page_inplace(target.clone(), win, state);
                    match tauri::async_runtime::block_on(call) {
                        Ok(()) => crate::logging::info("debug translate: started"),
                        Err(err) => crate::logging::warn(&format!(
                            "debug translate failed: {}",
                            err.message
                        )),
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
                    Err(err) => {
                        crate::logging::warn(&format!("debug patch failed: {}", err.message))
                    }
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
                    let call = crate::ipc::save_url(target.clone(), &state, url, None, None);
                    match tauri::async_runtime::block_on(call) {
                        Ok(path) => crate::logging::info(&format!("debug save: {path}")),
                        Err(err) => {
                            crate::logging::warn(&format!("debug save failed: {}", err.message))
                        }
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
                // The chrome strip's own webview: the action is about the window, so it has to
                // come from a caller that is allowed to ask for it.
                let Some(chrome) = target.get_webview(&format!("{PRIMARY}-chrome")) else {
                    return;
                };
                let state = target.state::<AppState>();
                let call = crate::ipc::shortcut(
                    target.clone(),
                    win,
                    chrome,
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
        crate::logging::info(&format!(
            "ext toggle {} -> {}: {ok}",
            target.name, !target.enabled
        ));
        let after = crate::ext_live::list(&webview);
        if let Some(seen) = after.iter().find(|item| item.id == target.id) {
            crate::logging::info(&format!("ext toggle after: enabled={}", seen.enabled));
        }
    });
}

/// Debug hook: starts more downloads than there are slots, and reports who runs.
///
/// `MIX_QUEUE_TEST=1` starts `MIX_QUEUE_COUNT` (default 6) transfers at once against
/// `MIX_DL_TEST_URL`. With `MAX_ACTIVE` at four, the extra ones have to wait rather than open
/// their own connections - and each queued row has to say so instead of showing 0 B as if it
/// were stalled.
fn queue_test(app: &tauri::AppHandle) {
    if !armed() {
        return;
    }
    if std::env::var("MIX_QUEUE_TEST").is_err() {
        return;
    }
    let count: usize = std::env::var("MIX_QUEUE_COUNT")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(6);
    let url = std::env::var("MIX_DL_TEST_URL")
        .unwrap_or_else(|_| "https://speed.cloudflare.com/__down?bytes=4000000".to_string());
    let handle = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(2500));
        if handle.try_state::<AppState>().is_none() {
            return;
        }
        let dir = crate::paths::AppDirs::resolve().downloads;
        let mut ids = Vec::new();
        for index in 0..count {
            let path = dir.join(format!("mix-queue-{index}.bin"));
            let _ = std::fs::remove_file(&path);
            ids.push(crate::downloads::start_with_engine(&handle, &url, &path));
        }
        crate::logging::info(&format!("queue test started {count}: {ids:?}"));

        // Right after starting, more are queued than running; that is the point.
        std::thread::sleep(std::time::Duration::from_millis(600));
        let (running, queued) = handle
            .try_state::<AppState>()
            .map(|state| {
                let tasks = state.downloads.read();
                let queued = tasks
                    .iter()
                    .filter(|task| crate::fetch::is_queued(task.id))
                    .count();
                let running = tasks
                    .iter()
                    .filter(|task| task.state == "started" && !crate::fetch::is_queued(task.id))
                    .count();
                (running, queued)
            })
            .unwrap_or((0, 0));
        crate::logging::info(&format!(
            "queue test after start: running={running} queued={queued}"
        ));

        // And they drain: waiting for every one to leave `started` proves the queue moves
        // rather than simply holding the extras forever.
        for _ in 0..120 {
            std::thread::sleep(std::time::Duration::from_millis(500));
            let left = handle
                .try_state::<AppState>()
                .map(|state| {
                    state
                        .downloads
                        .read()
                        .iter()
                        .filter(|task| ids.contains(&task.id) && task.state == "started")
                        .count()
                })
                .unwrap_or(0);
            if left == 0 {
                break;
            }
        }
        let summary: Vec<String> = handle
            .try_state::<AppState>()
            .map(|state| {
                state
                    .downloads
                    .read()
                    .iter()
                    .filter(|task| ids.contains(&task.id))
                    .map(|task| format!("{}:{}:{}B", task.id, task.state, task.bytes))
                    .collect()
            })
            .unwrap_or_default();
        crate::logging::info(&format!("queue test final: {}", summary.join(" ")));
    });
}

/// Debug hook: checks that a retry carries the credentials the first attempt did.
///
/// `MIX_RETRY_TEST=1` navigates the first tab to a page that sets a cookie, downloads a file
/// from the same origin behind a cookie check, fails it, then retries and reports what the
/// server recorded. `MIX_RETRY_URL` points at a server that logs request headers; the one in
/// `scripts/test-header-server.ps1` refuses the file without a cookie, so a retry that drops
/// them fails the same way the first attempt did.
///
/// This cannot be a unit test: the whole question is whether a *different* code path - the
/// retry command - rebuilds the request context, and that needs a real webview with real
/// cookies in it.
fn retry_credentials_test(app: &tauri::AppHandle) {
    if !armed() {
        return;
    }
    let Ok(base) = std::env::var("MIX_RETRY_URL") else {
        return;
    };
    let handle = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(2500));
        // A page on the same origin, so the cookie it sets belongs to the file's origin too.
        // The tab has to *be* on that origin: `request_for_task` reads the referer and the
        // cookie jar from the webview's current document, so a tab left on another site
        // sends that site's credentials and this test measures nothing.
        let target = handle.clone();
        let page = format!("{base}/page");
        // The label of the tab that was opened, carried back out so the download can be
        // attributed to it. A hardcoded label would read whichever tab happened to be first.
        let opened = std::sync::Arc::new(parking_lot::Mutex::new(String::new()));
        let slot = opened.clone();
        let _ = handle.run_on_main_thread(move || {
            let Some(win) = target.get_window(PRIMARY) else {
                return;
            };
            let state = target.state::<AppState>();
            // `tab_new` rather than `tab_navigate`: the tab that is already open belongs to
            // whatever the session restored, and navigating it away loses the reader's page.
            let call = crate::ipc::tab_new(target.clone(), win, state, Some(page), None, None);
            match tauri::async_runtime::block_on(call) {
                Ok(id) => {
                    crate::logging::info(&format!("retry test opened {id}"));
                    *slot.lock() = id;
                }
                Err(err) => {
                    crate::logging::warn(&format!("retry test nav failed: {}", err.message))
                }
            }
        });

        // Long enough for the page to load and the cookie to land in the jar.
        std::thread::sleep(std::time::Duration::from_millis(4000));

        let Some(state) = handle.try_state::<AppState>() else {
            return;
        };
        let tab = opened.lock().clone();
        if tab.is_empty() {
            crate::logging::warn("retry test: no tab was opened");
            return;
        }
        let dir = crate::paths::AppDirs::resolve().downloads;
        let path = dir.join("mix-retry-test.bin");
        let _ = std::fs::remove_file(&path);

        // What the tab would send for that file, which is what the first attempt uses.
        let request = crate::downloads::request_for_task(&handle, &tab, &format!("{base}/file"));
        crate::logging::info(&format!(
            "retry test first attempt: referer={:?} cookie={}",
            request.referer,
            request.cookie.as_deref().map(str::len).unwrap_or(0)
        ));

        let id =
            crate::downloads::start_with_engine_from(&handle, &format!("{base}/file"), &path, &tab);
        crate::logging::info(&format!("retry test download id={id}"));
        std::thread::sleep(std::time::Duration::from_millis(2500));
        let first = state
            .downloads
            .read()
            .iter()
            .find(|task| task.id == id)
            .map(|task| (task.state.clone(), task.error.clone(), task.source.clone()));
        crate::logging::info(&format!("retry test after first attempt: {first:?}"));

        // Now the retry, through the command the page calls.
        let state_ref = handle.state::<AppState>();
        let retried = tauri::async_runtime::block_on(crate::ipc::download_retry(
            handle.clone(),
            state_ref,
            id,
        ));
        crate::logging::info(&format!("retry test download_retry -> {retried:?}"));

        for _ in 0..30 {
            std::thread::sleep(std::time::Duration::from_millis(500));
            let now = handle.try_state::<AppState>().and_then(|state| {
                state
                    .downloads
                    .read()
                    .iter()
                    .find(|task| task.id == id)
                    .map(|task| (task.state.clone(), task.error.clone()))
            });
            if let Some((state, error)) = now {
                if state != "started" {
                    crate::logging::info(&format!(
                        "retry test ended state={state} err={error:?} size={:?}",
                        std::fs::metadata(&path).map(|meta| meta.len()).ok()
                    ));
                    break;
                }
            }
        }

        // The contrast, so the fix has something to be measured against: the plain
        // `start_with_engine` is what the retry used to call. It has no source, so it sends
        // no cookie, and the same server refuses it. Without this line the test would only
        // show that the new path works, not that the old one did not.
        let plain_path = dir.join("mix-retry-plain.bin");
        let _ = std::fs::remove_file(&plain_path);
        let plain =
            crate::downloads::start_with_engine(&handle, &format!("{base}/file"), &plain_path);
        for _ in 0..20 {
            std::thread::sleep(std::time::Duration::from_millis(400));
            let now = handle.try_state::<AppState>().and_then(|state| {
                state
                    .downloads
                    .read()
                    .iter()
                    .find(|task| task.id == plain)
                    .map(|task| (task.state.clone(), task.error.clone()))
            });
            if let Some((state, error)) = now {
                if state != "started" {
                    crate::logging::info(&format!(
                        "retry test without a source ended state={state} err={error:?}"
                    ));
                    break;
                }
            }
        }
    });
}

/// Debug hook: walks a download through the states the page's buttons put it in.
///
/// `MIX_DL_TEST=1` downloads a real file, pauses it, resumes it, pauses it again, cancels it,
/// and finally downloads it to completion - printing what the task list says at each step.
/// Exists because every one of those transitions had a fault that was invisible from a unit
/// test: they are about a loop being aborted, a claim being held, a row being left behind.
///
/// A local http server is not needed: the file is fetched from the network the same way a
/// click on a link would fetch it.
fn download_states_test(app: &tauri::AppHandle) {
    if !armed() {
        return;
    }
    if std::env::var("MIX_DL_TEST").is_err() {
        return;
    }
    let url = std::env::var("MIX_DL_TEST_URL").unwrap_or_else(|_| {
        // Small enough to check quickly, large enough that a pause lands mid-transfer.
        "https://speed.cloudflare.com/__down?bytes=8000000".to_string()
    });
    let handle = app.clone();
    std::thread::spawn(move || {
        let report = |stage: &str| {
            let Some(state) = handle.try_state::<AppState>() else {
                return;
            };
            let tasks = state.downloads.read();
            for task in tasks.iter().rev().take(4) {
                crate::logging::info(&format!(
                    "dl test [{stage}] id={} state={} bytes={} total={:?} err={:?}",
                    task.id, task.state, task.bytes, task.total, task.error
                ));
            }
        };

        std::thread::sleep(std::time::Duration::from_millis(2500));
        // Waits for the state to exist; the transfer itself is started through it.
        if handle.try_state::<AppState>().is_none() {
            return;
        }
        let target = crate::paths::AppDirs::resolve()
            .downloads
            .join("mix-dl-test.bin");
        let _ = std::fs::remove_file(&target);
        let id = crate::downloads::start_with_engine(&handle, &url, &target);
        crate::logging::info(&format!("dl test started id={id}"));
        report("started");

        // Wait until bytes are actually arriving before pausing. Pausing during the connect
        // phase is a legitimate thing to do, but it exercises nothing: the file is still
        // empty, so there is no progress to lose and no resume offset to get wrong.
        for _ in 0..40 {
            std::thread::sleep(std::time::Duration::from_millis(100));
            let arrived = std::fs::metadata(&target)
                .map(|meta| meta.len() > 0)
                .unwrap_or(false);
            if arrived {
                break;
            }
        }
        crate::logging::info(&format!(
            "dl test on disk before pause: {:?}",
            std::fs::metadata(&target).map(|meta| meta.len()).ok()
        ));

        // Through the real command, not the engine underneath it: what the page calls is
        // what has to work, and the faults this was written for - the row that offers
        // "Resume" and the paused transfer that keeps its file claimed - live in the command
        // and in the pause path, not in the transfer loop.
        let state = handle.state::<AppState>();
        let paused = tauri::async_runtime::block_on(crate::ipc::download_pause(
            handle.clone(),
            state,
            id,
            true,
        ));
        crate::logging::info(&format!("dl test pause command -> {paused:?}"));
        std::thread::sleep(std::time::Duration::from_millis(400));
        report("paused");

        let state = handle.state::<AppState>();
        let resumed = tauri::async_runtime::block_on(crate::ipc::download_pause(
            handle.clone(),
            state,
            id,
            false,
        ));
        crate::logging::info(&format!("dl test resume command -> {resumed:?}"));
        std::thread::sleep(std::time::Duration::from_millis(1200));
        report("resumed");

        // Cancel: must drop the row and the partial file, and the file must be gone after.
        let state = handle.state::<AppState>();
        let cancelled =
            tauri::async_runtime::block_on(crate::ipc::download_cancel(handle.clone(), state, id));
        crate::logging::info(&format!(
            "dl test cancel command -> {cancelled:?}; file still there: {}",
            target.exists()
        ));
        std::thread::sleep(std::time::Duration::from_millis(300));
        report("cancelled");

        // And a clean run to the end, so "finished" is exercised too.
        let id = crate::downloads::start_with_engine(&handle, &url, &target);
        crate::logging::info(&format!("dl test second start id={id}"));
        for _ in 0..40 {
            std::thread::sleep(std::time::Duration::from_millis(500));
            let done = handle
                .try_state::<AppState>()
                .map(|state| {
                    state
                        .downloads
                        .read()
                        .iter()
                        .any(|task| task.id == id && task.state != "started")
                })
                .unwrap_or(false);
            if done {
                break;
            }
        }
        report("finished");
        let final_state = handle.try_state::<AppState>().and_then(|state| {
            state
                .downloads
                .read()
                .iter()
                .find(|task| task.id == id)
                .map(|task| task.state.clone())
        });
        crate::logging::info(&format!(
            "dl test result: state={final_state:?} bytes={:?}",
            std::fs::metadata(&target).map(|meta| meta.len()).ok()
        ));

        // The 416 path: ask to resume a file that is already complete. The old engine read
        // the server's refusal as a failure, so a finished download looked broken and could
        // not be opened. It has to end up finished, not failed.
        if matches!(final_state.as_deref(), Some("finished")) {
            crate::downloads::set_state(&handle, id, "interrupted");
            let resumed = crate::fetch::resume(
                handle.clone(),
                id,
                Some((url.clone(), target.clone(), Default::default())),
            );
            crate::logging::info(&format!("dl test resume-of-complete -> {resumed}"));
            // Wait for the transfer to settle on a terminal state. Polling for "not started"
            // caught the `interrupted` this block had just set and reported the wrong answer.
            for _ in 0..40 {
                std::thread::sleep(std::time::Duration::from_millis(250));
                let (state, error) = handle
                    .try_state::<AppState>()
                    .and_then(|state| {
                        state
                            .downloads
                            .read()
                            .iter()
                            .find(|task| task.id == id)
                            .map(|task| (task.state.clone(), task.error.clone()))
                    })
                    .unwrap_or_default();
                if state == "finished" || state == "failed" {
                    crate::logging::info(&format!(
                        "dl test resume-of-complete ended state={state} err={error:?}"
                    ));
                    break;
                }
            }
        }
    });
}

/// Debug hook: opens a bilibili watch page and asks for its video.
///
/// `MIX_BILI=1` runs the two steps a reader would take: navigate the selected tab to
/// `MIX_BILI_URL` (or a fixed video when that is unset), then call `bili_download` with no
/// arguments - exactly the call the injected script makes. Passing no address is the point:
/// what gets exercised is the derivation from the tab, not a url handed in from outside.
///
/// The settling delay is longer than the other hooks' because the playurl endpoint is a
/// second round trip after the page's own first paint, and the tab's url has to be the watch
/// page by then or the command has nothing to parse.
fn bili_test(app: &tauri::AppHandle) {
    if !armed() {
        return;
    }
    if std::env::var("MIX_BILI").is_err() {
        return;
    }
    let url = std::env::var("MIX_BILI_URL")
        .unwrap_or_else(|_| "https://www.bilibili.com/video/BV1GJ411x7h7".to_string());
    let handle = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(2500));
        let target = handle.clone();
        let nav = url.clone();
        let _ = handle.run_on_main_thread(move || {
            let Some(win) = target.get_window(PRIMARY) else {
                return;
            };
            let state = target.state::<AppState>();
            let call = crate::ipc::tab_navigate(target.clone(), win, state, nav);
            match tauri::async_runtime::block_on(call) {
                Ok(()) => crate::logging::info("debug bili: navigated"),
                Err(err) => {
                    crate::logging::warn(&format!("debug bili nav failed: {}", err.message))
                }
            }
        });

        std::thread::sleep(std::time::Duration::from_millis(6000));
        let target = handle.clone();
        let _ = handle.run_on_main_thread(move || {
            let label = crate::ipc::tab_label(PRIMARY, 1);
            let Some(webview) = target.get_webview(&label) else {
                crate::logging::warn("debug bili: no tab to ask");
                return;
            };
            let state = target.state::<AppState>();
            let call = crate::ipc::bili_download(target.clone(), webview, state);
            match tauri::async_runtime::block_on(call) {
                Ok(path) => crate::logging::info(&format!("debug bili: {path}")),
                Err(err) => crate::logging::warn(&format!("debug bili failed: {}", err.message)),
            }
        });
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
