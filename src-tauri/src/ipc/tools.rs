//! Downloads, backups, userscripts, clipboard and the other shell-level commands.

use super::*;

#[tauri::command]
pub async fn download_list(
    _window: tauri::Window,
    state: State<'_, AppState>,
) -> Result<Vec<crate::downloads::DownloadTask>, AppError> {
    Ok(state.downloads.read().clone())
}

/// Drops finished entries from the list; files on disk stay.
#[tauri::command]
pub async fn download_clear(
    webview: tauri::Webview,
    state: State<'_, AppState>,
) -> Result<(), AppError> {
    require_trusted(&webview)?;
    let mut tasks = state.downloads.write();
    tasks.retain(|task| task.state == "started");
    Ok(())
}

/// Opens the file a download produced.
#[tauri::command]
pub async fn download_open(
    _window: tauri::Window,
    state: State<'_, AppState>,
    id: u64,
) -> Result<(), AppError> {
    let path = {
        let tasks = state.downloads.read();
        tasks
            .iter()
            .find(|task| task.id == id)
            .map(|task| task.path.clone())
    };
    let Some(path) = path else {
        return Err(AppError::new("bad_download", "unknown download"));
    };
    shell_open(std::path::Path::new(&path))
}

/// Shows a finished download in Explorer.
#[tauri::command]
pub async fn download_reveal(
    _window: tauri::Window,
    state: State<'_, AppState>,
    id: u64,
) -> Result<(), AppError> {
    let path = {
        let tasks = state.downloads.read();
        tasks
            .iter()
            .find(|task| task.id == id)
            .map(|task| task.path.clone())
    };
    let Some(path) = path else {
        return Err(AppError::new("bad_download", "unknown download"));
    };
    reveal_path(std::path::Path::new(&path))
}

/// Hands a file to the shell so it opens in its associated program.
pub(crate) fn shell_open(path: &std::path::Path) -> Result<(), AppError> {
    #[cfg(windows)]
    {
        std::process::Command::new("cmd")
            .args(["/C", "start", "", &path.to_string_lossy()])
            .spawn()
            .map(|_| ())
            .map_err(|err| AppError::new("open", err.to_string()))
    }
    #[cfg(not(windows))]
    {
        let opener = if cfg!(target_os = "macos") {
            "open"
        } else {
            "xdg-open"
        };
        std::process::Command::new(opener)
            .arg(path)
            .spawn()
            .map(|_| ())
            .map_err(|err| AppError::new("open", err.to_string()))
    }
}
/// Zips the whole profile so it can be restored on another machine.
#[tauri::command]
pub async fn backup_export(
    webview: tauri::Webview,
    state: State<'_, AppState>,
) -> Result<String, AppError> {
    require_trusted(&webview)?;
    let default_name = format!(
        "Mix-backup-{}.zip",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|value| value.as_secs())
            .unwrap_or(0)
    );
    let Some(target) = rfd::FileDialog::new()
        .set_file_name(&default_name)
        .add_filter("zip", &["zip"])
        .save_file()
    else {
        return Err(AppError::new("cancelled", "no file chosen"));
    };

    let file =
        std::fs::File::create(&target).map_err(|err| AppError::new("backup", err.to_string()))?;
    let mut zip = zip::ZipWriter::new(file);
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);

    let skip = [
        "profile",
        "profile-ui",
        "logs",
        "diag",
        "extensions",
        "downloads",
    ];
    let mut count = 0;
    let mut stack = vec![state.dirs.user_data.clone()];

    // The drop folders live beside UserData rather than inside it, so a backup that only
    // walked UserData produced a package that restored a browser with no extensions and
    // no userscripts - the two things the folder layout exists to make portable. They are
    // stored under their own names at the top of the archive, which is how the import
    // tells them apart from the profile.
    for extra in [&state.dirs.crx, &state.dirs.scripts] {
        count += add_tree(&mut zip, &state.dirs.root, extra, options);
    }

    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let name = path
                .file_name()
                .and_then(|value| value.to_str())
                .unwrap_or_default();
            if path.is_dir() {
                if dir == state.dirs.user_data && skip.contains(&name) {
                    continue;
                }
                stack.push(path);
                continue;
            }
            let Ok(relative) = path.strip_prefix(&state.dirs.user_data) else {
                continue;
            };
            let entry_name = relative.display().to_string().replace('\\', "/");
            let Ok(bytes) = std::fs::read(&path) else {
                continue;
            };
            if zip.start_file(entry_name, options).is_err() {
                continue;
            }
            let _ = std::io::Write::write_all(&mut zip, &bytes);
            count += 1;
        }
    }

    zip.finish()
        .map_err(|err| AppError::new("backup", err.to_string()))?;
    crate::logging::info(&format!(
        "backup written: {} ({count} files)",
        target.display()
    ));
    Ok(target.display().to_string())
}

/// Adds every file under `dir` to the archive, under a path relative to `base`.
///
/// Shared by the profile walk and the two drop folders: the only difference between them
/// is which directory the stored name is counted from, and that was the thing worth
/// not writing twice.
fn add_tree(
    zip: &mut zip::ZipWriter<std::fs::File>,
    base: &std::path::Path,
    dir: &std::path::Path,
    options: zip::write::SimpleFileOptions,
) -> usize {
    let mut added = 0;
    let mut stack = vec![dir.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            let Ok(relative) = path.strip_prefix(base) else {
                continue;
            };
            let name = relative.display().to_string().replace('\\', "/");
            let Ok(bytes) = std::fs::read(&path) else {
                continue;
            };
            if zip.start_file(name, options).is_err() {
                continue;
            }
            let _ = std::io::Write::write_all(&mut *zip, &bytes);
            added += 1;
        }
    }
    added
}

/// Restores a backup over the current profile; the app restarts afterwards.
#[tauri::command]
pub async fn backup_import(
    webview: tauri::Webview,
    state: State<'_, AppState>,
) -> Result<String, AppError> {
    require_trusted(&webview)?;
    let Some(source) = rfd::FileDialog::new()
        .add_filter("zip", &["zip"])
        .pick_file()
    else {
        return Err(AppError::new("cancelled", "no file chosen"));
    };

    let file =
        std::fs::File::open(&source).map_err(|err| AppError::new("backup", err.to_string()))?;
    let mut archive =
        zip::ZipArchive::new(file).map_err(|err| AppError::new("backup", err.to_string()))?;

    let mut restored = 0;
    for index in 0..archive.len() {
        let Ok(mut entry) = archive.by_index(index) else {
            continue;
        };
        let Some(name) = entry.enclosed_name() else {
            continue;
        };
        // Extension and userscript folders are stored under their own names, because they
        // live beside UserData rather than inside it. Everything else is profile data and
        // goes back where it came from. `enclosed_name` has already refused any path that
        // tried to climb out, so a name can only start with one of these two.
        let landed = |first: &str, dir: &std::path::Path| -> std::path::PathBuf {
            match name.strip_prefix(first) {
                Ok(rest) => dir.join(rest),
                Err(_) => state.dirs.user_data.join(&name),
            }
        };
        let target = match name.iter().next().and_then(|part| part.to_str()) {
            Some("crx") => landed("crx", &state.dirs.crx),
            Some("javascript") => landed("javascript", &state.dirs.scripts),
            _ => state.dirs.user_data.join(&name),
        };
        if let Some(parent) = target.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let mut buffer = Vec::new();
        if std::io::Read::read_to_end(&mut entry, &mut buffer).is_err() {
            continue;
        }
        if std::fs::write(&target, buffer).is_ok() {
            restored += 1;
        }
    }

    crate::logging::info(&format!(
        "backup restored: {} ({restored} files)",
        source.display()
    ));
    Ok(format!("{restored} files"))
}

/// Makes Mix a candidate default browser, then opens the Windows picker.
#[tauri::command]
pub async fn register_browser(webview: tauri::Webview) -> Result<String, AppError> {
    require_trusted(&webview)?;
    let exe = crate::browser_reg::register()?;
    #[cfg(windows)]
    let _ = std::process::Command::new("cmd")
        .args(["/C", "start", "", "ms-settings:defaultapps"])
        .spawn();
    Ok(exe)
}

/// Reveals a file in Explorer, or opens its folder when it is missing.
pub(crate) fn reveal_path(path: &std::path::Path) -> Result<(), AppError> {
    #[cfg(windows)]
    {
        let target = if path.is_dir() {
            path.to_path_buf()
        } else {
            path.parent()
                .map(|parent| parent.to_path_buf())
                .unwrap_or_else(|| path.to_path_buf())
        };
        std::process::Command::new("explorer")
            .arg(target)
            .spawn()
            .map(|_| ())
            .map_err(|err| AppError::new("open", err.to_string()))
    }
    #[cfg(not(windows))]
    {
        let _ = path;
        Err(AppError::new("open", "not supported on this platform"))
    }
}

/// The extensions WebView2 actually loaded, with the ids it assigned.
#[tauri::command]
pub async fn extension_live_list(
    app: AppHandle,
    window: tauri::Window,
    state: State<'_, AppState>,
) -> Result<Vec<crate::ext_live::LiveExtension>, AppError> {
    let label = selected_label(&state, &window);
    let Some(webview) = app.get_webview(&label) else {
        return Ok(Vec::new());
    };
    let listed = crate::ext_live::list(&webview);
    let described: Vec<String> = listed
        .iter()
        .map(|ext| {
            format!(
                "{}={} ({})",
                ext.name,
                ext.enabled,
                &ext.id[..ext.id.len().min(8)]
            )
        })
        .collect();
    crate::logging::info(&format!(
        "live extensions ({}): {}",
        listed.len(),
        described.join(", ")
    ));
    reconcile(&state, &listed);
    Ok(listed)
}

/// Marks the profile for a clean extension reload on the next start.
fn reconcile(state: &AppState, listed: &[crate::ext_live::LiveExtension]) {
    let switched_off: Vec<String> = state.settings.read().disabled_extensions.clone();
    let stuck: Vec<String> = state
        .extensions
        .iter()
        .filter(|ext| ext.status == "loaded")
        .filter(|ext| !switched_off.contains(&ext.name))
        .filter(|ext| {
            listed
                .iter()
                .any(|live| live.name == ext.name && !live.enabled)
        })
        .map(|ext| ext.name.clone())
        .collect();
    if stuck.is_empty() {
        return;
    }
    crate::logging::warn(&format!(
        "extensions the browser keeps off: {}; they load again on the next start",
        stuck.join(", ")
    ));
    let mut settings = state.settings.write();
    if !settings.reset_extensions {
        settings.reset_extensions = true;
        if let Err(err) = settings.save(&state.dirs.settings_file) {
            crate::logging::warn(&format!("cannot save settings: {err}"));
        }
    }
}
/// Records that the profile's extension state has to be cleared on the next start.
fn request_extension_reset(state: &AppState) {
    let mut settings = state.settings.write();
    if settings.reset_extensions {
        return;
    }
    settings.reset_extensions = true;
    if let Err(err) = settings.save(&state.dirs.settings_file) {
        crate::logging::warn(&format!("cannot save settings: {err}"));
    } else {
        crate::logging::info("extension state will be cleared on the next start");
    }
}

/// Remembers the state the browser confirmed for each extension.
fn remember_extension_state(state: &AppState, dir: &str, enabled: bool) {
    let mut settings = state.settings.write();
    settings.extension_state.insert(dir.to_string(), enabled);
    if let Err(err) = settings.save(&state.dirs.settings_file) {
        crate::logging::warn(&format!("cannot save extension state: {err}"));
    }
}

/// Switches one extension off or on by taking its folder away or putting it back.
#[tauri::command]
pub async fn extension_set(
    app: AppHandle,
    webview: tauri::Webview,
    state: State<'_, AppState>,
    name: String,
    enabled: bool,
    dir: Option<String>,
) -> Result<(), AppError> {
    require_trusted(&webview)?;
    {
        let mut settings = state.settings.write();
        settings.disabled_extensions.retain(|entry| entry != &name);
        if !enabled {
            settings.disabled_extensions.push(name.clone());
        }
        settings
            .save(&state.dirs.settings_file)
            .map_err(|err| AppError::new("settings", err.to_string()))?;
    }
    crate::logging::info(&format!("extension {name} enabled={enabled}"));
    if enabled {
        request_extension_reset(&state);
    }

    let window = webview.window();
    let label = selected_label(&state, &window);
    if let Some(page) = app.get_webview(&label) {
        if enabled {
            if let Some(dir) = dir.as_deref() {
                let _ = crate::ext_live::add(&page, dir);
            }
        } else if let Some(entry) = crate::ext_live::list(&page)
            .into_iter()
            .find(|live| live.name == name)
        {
            let _ = crate::ext_live::set_enabled(&page, &entry.id, false);
        }
    }
    extensions_changed(&app);
    Ok(())
}

/// Tells the toolbar that the set of extensions it draws has moved.
///
/// The chrome listens for this and re-reads the list; nothing used to send it, so a switch
/// thrown from the app menu left the toolbar showing the old set until something else
/// happened to refresh it.
fn extensions_changed(app: &AppHandle) {
    let _ = app.emit("ext:changed", ());
}

/// Loads one extension from its folder again: the way back from a disabled switch.
#[tauri::command]
pub async fn extension_add(
    app: AppHandle,
    webview: tauri::Webview,
    state: State<'_, AppState>,
    dir: String,
) -> Result<String, AppError> {
    require_trusted(&webview)?;
    let window = webview.window();
    let label = selected_label(&state, &window);
    let Some(page) = app.get_webview(&label) else {
        return Err(AppError::new("extension", "no page to load it into"));
    };
    let result = match crate::ext_live::add(&page, &dir) {
        Ok(true) => {
            crate::logging::info(&format!("extension add {dir}: enabled"));
            remember_extension_state(&state, &dir, true);
            Ok("enabled".to_string())
        }
        Ok(false) => {
            crate::logging::warn(&format!("extension add {dir}: added but still off"));
            remember_extension_state(&state, &dir, false);
            request_extension_reset(&state);
            Err(AppError::new(
                "extension",
                "the browser keeps it off; restart Mix and it will load again",
            ))
        }
        Err(err) => {
            crate::logging::warn(&format!("extension add {dir} failed: {err}"));
            request_extension_reset(&state);
            Err(AppError::new("extension", err))
        }
    };
    // Added but off still counts as a change: the list the toolbar draws is about what is
    // there, not about what is running.
    extensions_changed(&app);
    result
}

/// Enables or disables a loaded extension without restarting the app.
#[tauri::command]
pub async fn extension_live_set(
    app: AppHandle,
    webview: tauri::Webview,
    state: State<'_, AppState>,
    id: String,
    enabled: bool,
    dir: Option<String>,
) -> Result<bool, AppError> {
    require_trusted(&webview)?;
    let window = webview.window();
    let label = selected_label(&state, &window);
    let Some(webview) = app.get_webview(&label) else {
        return Ok(false);
    };
    let ok = crate::ext_live::set_enabled(&webview, &id, enabled);
    crate::logging::info(&format!("extension {id} enabled={enabled}: {ok}"));
    if ok {
        if let Some(dir) = dir {
            remember_extension_state(&state, &dir, enabled);
        }
        extensions_changed(&app);
    }
    Ok(ok)
}
#[tauri::command]
pub async fn ext_list(
    _window: tauri::Window,
    state: State<'_, AppState>,
) -> Result<Vec<ExtInfo>, AppError> {
    Ok(state.extensions.clone())
}

#[derive(Serialize)]
pub struct AppInfo {
    version: &'static str,
    git: &'static str,
    os: &'static str,
    webview: String,
}

#[tauri::command]
pub async fn app_info() -> Result<AppInfo, AppError> {
    Ok(AppInfo {
        version: env!("CARGO_PKG_VERSION"),
        git: env!("MIX_GIT"),
        os: std::env::consts::OS,
        webview: tauri::webview_version().unwrap_or_else(|_| "unknown".into()),
    })
}

#[tauri::command]
pub async fn diag_export(app: AppHandle, webview: tauri::Webview) -> Result<String, AppError> {
    require_trusted(&webview)?;
    let path = crate::diag::export(&app).map_err(|err| AppError::new("diag", err))?;
    crate::logging::info(&format!("diagnostics written: {}", path.display()));
    Ok(path.display().to_string())
}

/// Restarts so a startup-only setting is picked up: WebView2 loads extensions and the proxy
/// when it is created.
///
/// The wait is the whole point. The profile folder and the single-instance lock belong to
/// this process until it is gone, and a launch that arrives first is forwarded to the
/// instance that is about to exit, then quits - so the browser would simply disappear. A
/// detached command starts the next one a couple of seconds later.
#[tauri::command]
pub async fn app_restart(app: AppHandle, webview: tauri::Webview) -> Result<(), AppError> {
    require_trusted(&webview)?;
    if crate::paths::instance_mode() {
        return Err(AppError::new(
            "restart",
            "this window runs on its own; restart Mix from the main window",
        ));
    }
    let exe = std::env::current_exe()
        .map_err(|err| AppError::new("restart", format!("cannot find this program: {err}")))?;
    let command = format!(
        "ping -n 3 127.0.0.1 >nul & start \"\" \"{}\"",
        exe.display()
    );
    let mut started = std::process::Command::new("cmd");
    started.args(["/c", &command]);
    if let Ok(dir) = std::env::current_dir() {
        started.current_dir(dir);
    }
    started
        .spawn()
        .map_err(|err| AppError::new("restart", format!("cannot start the next one: {err}")))?;
    save_sessions(&app);
    flush_history(&app);
    crate::logging::info("restarting");
    app.exit(0);
    Ok(())
}

/// Quits for real: the window close button only hides to the tray.
#[tauri::command]
pub async fn app_quit(app: AppHandle, webview: tauri::Webview) -> Result<(), AppError> {
    require_trusted(&webview)?;
    save_sessions(&app);
    flush_history(&app);
    crate::logging::info("quit from the app menu");
    app.exit(0);
    Ok(())
}

/// What the running webviews were started with, and whether the settings have moved on.
///
/// The proxy reaches WebView2 as a browser argument at creation, so a change cannot take
/// effect before the next start. Saying which of the two is running is the difference
/// between "I changed it and nothing happened" and a restart that is actually wanted.
#[tauri::command]
pub async fn proxy_state(
    webview: tauri::Webview,
    state: State<'_, AppState>,
) -> Result<serde_json::Value, AppError> {
    require_trusted(&webview)?;
    let settings = state.settings.read();
    let wanted = crate::tabs::proxy_label(&settings.proxy_mode, &settings.proxy_server);
    let running = crate::tabs::proxy_applied();
    Ok(serde_json::json!({
        "applied": running,
        "configured": wanted,
        "pending": running != wanted,
    }))
}

/// Restarts Mix: this process leaves, and a detached command starts the next one.
/// The chrome ui has no console we can read, so it reports through here.
#[tauri::command]
pub async fn ui_log(message: String) -> Result<(), AppError> {
    crate::logging::info(&format!("ui: {message}"));
    Ok(())
}

/// Runs a COM call on the thread that created the webview.
#[allow(dead_code)]
fn on_main<T: Send + 'static>(
    app: &AppHandle,
    run: impl FnOnce() -> T + Send + 'static,
) -> Option<T> {
    let (sender, receiver) = std::sync::mpsc::channel();
    app.run_on_main_thread(move || {
        let _ = sender.send(run());
    })
    .ok()?;
    receiver
        .recv_timeout(std::time::Duration::from_secs(4))
        .ok()
}

/// Pauses or resumes a download, whichever engine owns it.
#[tauri::command]
pub async fn download_pause(
    app: AppHandle,
    state: State<'_, AppState>,
    id: u64,
    paused: bool,
) -> Result<bool, AppError> {
    let _ = &state;
    let ok = if paused {
        crate::fetch::pause(id)
    } else {
        crate::fetch::resume(app.clone(), id)
    };
    if ok {
        let state = app.try_state::<AppState>();
        if let Some(state) = state.as_ref() {
            mark_paused(&app, state, id, paused);
        }
    }
    crate::logging::info(&format!(
        "download {id} {}{}{}",
        if paused { "pause" } else { "resume" },
        ": ",
        ok
    ));
    Ok(ok)
}

/// Cancels a download, whichever engine owns it.
#[tauri::command]
pub async fn download_cancel(_app: AppHandle, id: u64) -> Result<bool, AppError> {
    Ok(crate::fetch::cancel(id))
}

/// Fetches this download again with the internal engine.
#[tauri::command]
pub async fn download_reconcile(app: AppHandle) -> Result<(), AppError> {
    crate::downloads::reconcile(&app);
    Ok(())
}

/// Fetches this download again with the engine, from whatever is on disk.
#[tauri::command]
pub async fn download_retry(
    app: AppHandle,
    state: State<'_, AppState>,
    id: u64,
) -> Result<u64, AppError> {
    let Some((url, path)) = task_parts(&state, id) else {
        return Err(AppError::new("bad_download", "unknown download"));
    };
    let target = std::path::PathBuf::from(path);
    Ok(crate::downloads::start_with_engine(&app, &url, &target))
}

fn task_parts(state: &AppState, id: u64) -> Option<(String, String)> {
    let tasks = state.downloads.read();
    tasks
        .iter()
        .find(|task| task.id == id)
        .map(|task| (task.url.clone(), task.path.clone()))
}

#[allow(dead_code)]
fn task_url(state: &AppState, id: u64) -> Option<String> {
    task_parts(state, id).map(|(url, _)| url)
}

fn mark_paused(app: &AppHandle, state: &AppState, id: u64, paused: bool) {
    let tasks = {
        let mut tasks = state.downloads.write();
        if let Some(task) = tasks.iter_mut().find(|task| task.id == id) {
            task.paused = paused;
            task.state = crate::downloads::Phase::after_pause(paused)
                .as_str()
                .to_string();
            if paused {
                task.speed = 0;
            }
        }
        tasks.clone()
    };
    let _ = app.emit("download:changed", tasks);
}

/// Resolves a userscript path against the scripts folder before using it.
fn script_path_inside(state: &AppState, path: &str) -> Result<std::path::PathBuf, AppError> {
    let candidate = std::path::PathBuf::from(path);
    let resolved = candidate
        .canonicalize()
        .map_err(|err| AppError::new("bad_script", err.to_string()))?;
    let root = state
        .dirs
        .scripts
        .canonicalize()
        .unwrap_or_else(|_| state.dirs.scripts.clone());
    if !resolved.starts_with(&root) {
        crate::logging::warn(&format!("script path outside the scripts folder: {path}"));
        return Err(AppError::new(
            "bad_script",
            "script is outside the scripts folder",
        ));
    }
    Ok(resolved)
}

/// Userscripts are files, so disabling means renaming the suffix: the scanner
/// only picks up names ending in ".user.js".
#[tauri::command]
pub async fn script_set_enabled(
    webview: tauri::Webview,
    state: State<'_, AppState>,
    path: String,
    enabled: bool,
) -> Result<String, AppError> {
    require_trusted(&webview)?;
    let source = script_path_inside(&state, &path)?;
    let name = source
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default()
        .to_string();
    let target = if enabled {
        if name.ends_with(".user.js") {
            source.clone()
        } else if name.ends_with(".user.js.disabled") {
            source.with_file_name(name.trim_end_matches(".disabled"))
        } else {
            return Err(AppError::new("bad_script", "unexpected file name"));
        }
    } else if name.ends_with(".user.js.disabled") {
        source.clone()
    } else if name.ends_with(".user.js") {
        source.with_file_name(format!("{name}.disabled"))
    } else {
        return Err(AppError::new("bad_script", "unexpected file name"));
    };

    if target != source {
        std::fs::rename(&source, &target)
            .map_err(|err| AppError::new("script", err.to_string()))?;
    }
    crate::logging::info(&format!(
        "userscript {} -> {}",
        source.display(),
        target.display()
    ));
    Ok(target.display().to_string())
}

#[tauri::command]
pub async fn script_remove(
    webview: tauri::Webview,
    state: State<'_, AppState>,
    path: String,
) -> Result<(), AppError> {
    require_trusted(&webview)?;
    let target = script_path_inside(&state, &path)?;
    std::fs::remove_file(&target).map_err(|err| AppError::new("script", err.to_string()))?;
    crate::logging::info(&format!("userscript removed: {}", target.display()));
    Ok(())
}

#[tauri::command]
pub async fn script_list(
    _window: tauri::Window,
    state: State<'_, AppState>,
) -> Result<Vec<crate::userscripts::ScriptInfo>, AppError> {
    Ok(state.scripts.clone())
}

/// Creates the user stylesheet if missing, then reveals the folder.
#[tauri::command]
pub async fn custom_css_open(
    webview: tauri::Webview,
    state: State<'_, AppState>,
) -> Result<String, AppError> {
    require_trusted(&webview)?;
    let path = state.dirs.user_data.join("custom.css");
    if !path.exists() {
        let starter = "/* Styles here are applied to every page. */\n\n/*\n.example {\n  outline: 1px solid #4c8dff;\n}\n*/\n";
        std::fs::write(&path, starter).map_err(|err| AppError::new("css", err.to_string()))?;
    }
    reveal_path(&path)?;
    Ok(path.display().to_string())
}
#[tauri::command]
pub async fn open_dir(
    webview: tauri::Webview,
    state: State<'_, AppState>,
    which: String,
) -> Result<(), AppError> {
    require_trusted(&webview)?;
    let dirs = &state.dirs;
    let path = match which.as_str() {
        "userdata" => &dirs.user_data,
        "crx" => &dirs.crx,
        "scripts" => &dirs.scripts,
        "logs" => &dirs.logs,
        "downloads" => &dirs.downloads,
        "diag" => &dirs.diag,
        "root" => &dirs.root,
        other => return Err(AppError::new("bad_dir", other)),
    };
    reveal(path)?;
    Ok(())
}

pub(crate) fn reveal(path: &std::path::Path) -> std::io::Result<()> {
    #[cfg(windows)]
    let mut cmd = {
        let mut c = std::process::Command::new("explorer");
        c.arg(path);
        c
    };
    #[cfg(target_os = "macos")]
    let mut cmd = {
        let mut c = std::process::Command::new("open");
        c.arg(path);
        c
    };
    #[cfg(target_os = "linux")]
    let mut cmd = {
        let mut c = std::process::Command::new("xdg-open");
        c.arg(path);
        c
    };
    cmd.spawn().map(|_| ())
}
/// Reads the clipboard through the OS.
#[tauri::command]
pub async fn clipboard_read(webview: tauri::Webview) -> Result<String, AppError> {
    require_trusted(&webview)?;
    let mut board =
        arboard::Clipboard::new().map_err(|err| AppError::new("clipboard", err.to_string()))?;
    board
        .get_text()
        .map_err(|err| AppError::new("clipboard", err.to_string()))
}

#[tauri::command]
pub async fn clipboard_write(webview: tauri::Webview, text: String) -> Result<(), AppError> {
    require_trusted(&webview)?;
    let mut board =
        arboard::Clipboard::new().map_err(|err| AppError::new("clipboard", err.to_string()))?;
    board
        .set_text(text)
        .map_err(|err| AppError::new("clipboard", err.to_string()))
}

/// The page-side entry point for super drag and gestures.
#[tauri::command]
pub async fn drag_action(
    app: AppHandle,
    webview: tauri::Webview,
    state: State<'_, AppState>,
    kind: String,
    url: Option<String>,
    text: Option<String>,
    background: Option<bool>,
) -> Result<(), AppError> {
    let label = webview.label().to_string();
    let window_label = window_of_tab(&label).to_string();
    if window_label == label {
        return Err(AppError::new("untrusted", "super drag comes from a page"));
    }
    let web_url = |value: &str| value.starts_with("http://") || value.starts_with("https://");
    match kind.as_str() {
        "open" => {
            let Some(url) = url.filter(|value| web_url(value)) else {
                return Ok(());
            };
            let window = app
                .get_window(&window_label)
                .ok_or_else(|| AppError::new("no_window", window_label.clone()))?;
            tab_new(
                app,
                window,
                state,
                Some(url),
                None,
                Some(background.unwrap_or(false)),
            )
            .await?;
        }
        "search" => {
            let text = text.unwrap_or_default();
            if text.trim().is_empty() {
                return Ok(());
            }
            let window = app
                .get_window(&window_label)
                .ok_or_else(|| AppError::new("no_window", window_label.clone()))?;
            search_new_tab(app, window, state, text).await?;
        }
        "copy" => {
            let text = text.unwrap_or_default();
            if text.is_empty() {
                return Ok(());
            }
            let mut board = arboard::Clipboard::new()
                .map_err(|err| AppError::new("clipboard", err.to_string()))?;
            board
                .set_text(text)
                .map_err(|err| AppError::new("clipboard", err.to_string()))?;
        }
        "save" => {
            let Some(url) = url.filter(|value| web_url(value)) else {
                return Ok(());
            };
            let _ = save_url(app, &state, url, None, Some(label)).await?;
        }
        other => return Err(AppError::new("bad_action", other)),
    }
    Ok(())
}

/// Saves the file a video is playing from, at the address it is playing from.
///
/// This is the only honest way to download a video: the browser is not what serves it. A
/// player streams its media through requests the page makes - often in ranges, often through
/// a blob the page assembled - so what `save_url` fetches is whatever that address answers.
/// For a segmented stream that is one segment, not the video, which is why the row in the
/// page says what it does instead of promising a file that will not play.
///
/// Refused for pages that are not the app's own, like every other privileged command.
#[tauri::command]
pub async fn video_download(
    app: AppHandle,
    webview: tauri::Webview,
    state: State<'_, AppState>,
    url: String,
    file_name: Option<String>,
) -> Result<String, AppError> {
    if !url.starts_with("http://") && !url.starts_with("https://") {
        return Err(AppError::new("bad_url", "only http(s) media can be saved"));
    }
    let label = webview.label().to_string();
    save_url(app, &state, url, file_name, Some(label)).await
}
#[tauri::command]
pub async fn download_url(
    app: AppHandle,
    webview: tauri::Webview,
    state: State<'_, AppState>,
    url: String,
    file_name: Option<String>,
) -> Result<String, AppError> {
    require_trusted(&webview)?;
    let tab = webview.label().to_string();
    save_url(app, &state, url, file_name, Some(tab)).await
}

/// Fetches a url into the downloads folder; shared with the support hook.
///
/// `tab` is the webview the url was found in. It supplies the referrer and the cookies:
/// an image behind a hotlink check or a file behind a login answers a bare fetch with an
/// error page, which used to be saved under the picture's name.
pub(crate) async fn save_url(
    app: AppHandle,
    state: &AppState,
    url: String,
    file_name: Option<String>,
    tab: Option<String>,
) -> Result<String, AppError> {
    if !url.starts_with("http://") && !url.starts_with("https://") {
        return Err(AppError::new("bad_url", "only http(s) files can be saved"));
    }

    let request = tab
        .as_deref()
        .map(|label| crate::downloads::request_for(&app, label))
        .unwrap_or_default();
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(600))
        .user_agent(crate::fetch::USER_AGENT)
        .build()
        .map_err(|err| AppError::new("net", err.to_string()))?;
    let mut builder = client.get(&url);
    if let Some(referer) = request.referer.as_deref() {
        builder = builder.header(reqwest::header::REFERER, referer);
    }
    if let Some(cookie) = request.cookie.as_deref() {
        builder = builder.header(reqwest::header::COOKIE, cookie);
    }
    let response = builder
        .send()
        .await
        .map_err(|err| AppError::new("net", err.to_string()))?;
    if !response.status().is_success() {
        return Err(AppError::new(
            "net",
            format!("server answered {}", response.status()),
        ));
    }

    // The size limit is enforced while the body arrives, not after it is all in memory.
    // Reading it first and checking afterwards meant the limit stopped nothing: a 4 GB url
    // was 4 GB of ram before the answer was "too big". The declared length is checked first
    // because that is free, and the running total catches a response that did not declare
    // one or lied about it.
    const LIMIT: u64 = 128 * 1024 * 1024;
    // Saved before the body is consumed: after that the response has no length to report.
    let declared = response.content_length();
    if let Some(declared) = declared {
        if declared > LIMIT {
            return Err(AppError::new(
                "too_big",
                format!("file is larger than {} MB", LIMIT / (1024 * 1024)),
            ));
        }
    }

    let suggested = file_name.unwrap_or_default();
    let target = crate::downloads::target_for(&state.dirs.downloads, &url, &suggested);

    // Streamed to the temporary file `write_atomic` would have used, for the same reason it
    // uses one: a transfer that fails or runs past the limit must not leave a truncated file
    // under the real name.
    {
        use std::io::Write;
        let dir = target.parent().unwrap_or_else(|| std::path::Path::new("."));
        std::fs::create_dir_all(dir)?;
        let tmp = crate::paths::atomic_tmp(&target);
        let mut file = std::fs::File::create(&tmp)?;
        let mut response = response;
        let mut written: u64 = 0;
        loop {
            match response.chunk().await {
                Ok(Some(chunk)) => {
                    written += chunk.len() as u64;
                    if written > LIMIT {
                        drop(file);
                        let _ = std::fs::remove_file(&tmp);
                        return Err(AppError::new(
                            "too_big",
                            format!("file is larger than {} MB", LIMIT / (1024 * 1024)),
                        ));
                    }
                    if let Err(err) = file.write_all(&chunk) {
                        drop(file);
                        let _ = std::fs::remove_file(&tmp);
                        return Err(AppError::new("io", err.to_string()));
                    }
                }
                // A transfer that stops early is not a saved file: the loop ends the same
                // way whether the body finished or the connection went away, so the length
                // it promised is what decides.
                Ok(None) => break,
                Err(err) => {
                    drop(file);
                    let _ = std::fs::remove_file(&tmp);
                    return Err(AppError::new("net", err.to_string()));
                }
            }
        }
        let _ = file.sync_all();
        drop(file);
        if let Some(declared) = declared {
            if written != declared {
                let _ = std::fs::remove_file(&tmp);
                return Err(AppError::new(
                    "net",
                    format!("the server sent {written} bytes of {declared}"),
                ));
            }
        }
        std::fs::rename(&tmp, &target)?;
    }
    crate::downloads::record_done(&app, &url, &target);
    crate::logging::info(&format!("saved {}", target.display()));
    Ok(target.display().to_string())
}

/// Wipes cookies, storage and caches for every webview we own.
#[tauri::command]
pub async fn app_clear_data(
    app: AppHandle,
    window: tauri::Window,
    _state: State<'_, AppState>,
) -> Result<(), AppError> {
    // Every window, not just the caller's: the label used to be hardcoded to the primary
    // strip, so a second window kept its signed-in sites while this one reported success.
    let labels = super::all_webviews(&app);
    for label in &labels {
        if let Some(webview) = app.get_webview(label) {
            let _ = webview.clear_all_browsing_data();
        }
    }
    let _ = window;
    crate::logging::info(&format!(
        "browsing data cleared across {} webview(s)",
        labels.len()
    ));
    Ok(())
}
