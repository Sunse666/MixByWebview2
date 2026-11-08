//! Downloads, backups, userscripts, clipboard, diagnostics and the other
//! shell-level commands.

use super::*;

#[tauri::command]
pub async fn download_list(_window: tauri::Window, state: State<'_, AppState>) -> Result<Vec<crate::downloads::DownloadTask>, AppError> {
    Ok(state.downloads.read().unwrap_or_else(|e| e.into_inner()).clone())
}

/// Opens the file a download produced.
/// Drops finished entries from the list; files on disk stay.
#[tauri::command]
pub async fn download_clear(webview: tauri::Webview, state: State<'_, AppState>) -> Result<(), AppError> {
    require_trusted(&webview)?;
    let mut tasks = state.downloads.write().unwrap_or_else(|e| e.into_inner());
    tasks.retain(|task| task.state == "started");
    Ok(())
}

#[tauri::command]
pub async fn download_open(
    _window: tauri::Window,
    state: State<'_, AppState>,
    id: u64,
) -> Result<(), AppError> {
    let path = {
        let tasks = state.downloads.read().unwrap_or_else(|e| e.into_inner());
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
#[tauri::command]
pub async fn download_open_dir(_window: tauri::Window, state: State<'_, AppState>, id: u64) -> Result<(), AppError> {
    let path = {
        let tasks = state.downloads.read().unwrap_or_else(|e| e.into_inner());
        tasks
            .iter()
            .find(|task| task.id == id)
            .map(|task| task.path.clone())
    };
    match path {
        Some(path) if !path.is_empty() => reveal(std::path::Path::new(&path)),
        _ => reveal(&state.dirs.downloads),
    }?;
    Ok(())
}

/// Zips the whole profile so it can be restored on another machine.
#[tauri::command]
pub async fn backup_export(webview: tauri::Webview, state: State<'_, AppState>) -> Result<String, AppError> {
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

    let file = std::fs::File::create(&target).map_err(|err| AppError::new("backup", err.to_string()))?;
    let mut zip = zip::ZipWriter::new(file);
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);

    // Only data, never the WebView2 profile: it is huge and machine specific.
    let skip = ["profile", "profile-ui", "logs", "diag", "extensions", "downloads"];
    let mut count = 0;
    let mut stack = vec![state.dirs.user_data.clone()];

    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let name = path.file_name().and_then(|value| value.to_str()).unwrap_or_default();
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

    zip.finish().map_err(|err| AppError::new("backup", err.to_string()))?;
    crate::logging::info(&format!("backup written: {} ({count} files)", target.display()));
    Ok(target.display().to_string())
}

/// Restores a backup over the current profile; the app restarts afterwards.
#[tauri::command]
pub async fn backup_import(webview: tauri::Webview, state: State<'_, AppState>) -> Result<String, AppError> {
    require_trusted(&webview)?;
    let Some(source) = rfd::FileDialog::new()
        .add_filter("zip", &["zip"])
        .pick_file()
    else {
        return Err(AppError::new("cancelled", "no file chosen"));
    };

    let file = std::fs::File::open(&source).map_err(|err| AppError::new("backup", err.to_string()))?;
    let mut archive = zip::ZipArchive::new(file).map_err(|err| AppError::new("backup", err.to_string()))?;

    let mut restored = 0;
    for index in 0..archive.len() {
        let Ok(mut entry) = archive.by_index(index) else {
            continue;
        };
        let Some(name) = entry.enclosed_name() else {
            continue;
        };
        let target = state.dirs.user_data.join(&name);
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

/// Makes Mix a candidate default browser, then opens the Windows picker: the
/// app must never take handlers away from the user silently.
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
    // Names and ids, so the page's cards can be told apart from what the browser has:
    // matching them by name is all we have, and a mismatch silently looks like "off".
    let described: Vec<String> = listed
        .iter()
        .map(|ext| format!("{}={} ({})", ext.name, ext.enabled, &ext.id[..ext.id.len().min(8)]))
        .collect();
    crate::logging::info(&format!(
        "live extensions ({}): {}",
        listed.len(),
        described.join(", ")
    ));
    reconcile(&state, &listed);
    Ok(listed)
}


/// Marks the profile for a clean extension reload when the browser contradicts us.
///
/// The record only claims an extension should be on after the browser confirmed it, so
/// finding it off again afterwards means the profile is stuck - the one case where
/// clearing the state is the right answer rather than overriding a choice.
fn reconcile(state: &AppState, listed: &[crate::ext_live::LiveExtension]) {
    let enabled: std::collections::BTreeMap<String, bool> = state
        .settings
        .read()
        .extension_state
        .iter()
        .map(|(dir, value)| (dir.clone(), *value))
        .collect();
    let stuck: Vec<String> = state
        .extensions
        .iter()
        .filter(|ext| ext.status == "loaded")
        .filter(|ext| enabled.get(&ext.dir) == Some(&true))
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

/// Remembers the state the browser confirmed, so the extensions page does not have to
/// guess from a list that sometimes omits it.
fn remember_extension_state(state: &AppState, dir: &str, enabled: bool) {
    let mut settings = state.settings.write();
    settings
        .extension_state
        .insert(dir.to_string(), enabled);
    if let Err(err) = settings.save(&state.dirs.settings_file) {
        crate::logging::warn(&format!("cannot save extension state: {err}"));
    }
}

/// Switches one extension off or on, by taking its unpacked folder away or putting it
/// back.
///
/// The browser's own enable call is one way: switching an extension off there is
/// durable and cannot be undone through the same API, which is why a switch built on it
/// could never remember "on". The folder is the handle this app controls, and the list
/// of names it belongs to lives in settings, where it survives restarts.
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

    let window = webview.window();
    let label = selected_label(&state, &window);
    if let Some(page) = app.get_webview(&label) {
        if enabled {
            // The folder is what the next start reads, so putting it back is the part
            // that matters; asking the browser to load it now is a bonus.
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
    Ok(())
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
    match crate::ext_live::add(&page, &dir) {
        Ok(true) => {
            crate::logging::info(&format!("extension add {dir}: enabled"));
            remember_extension_state(&state, &dir, true);
            Ok("enabled".to_string())
        }
        Ok(false) => {
            // The browser took the folder and left it switched off. Its own record of
            // "off" cannot be changed through the API, so ask for a clean reload on the
            // next start instead of pretending it worked.
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
    }
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
    }
    Ok(ok)
}
#[tauri::command]
pub async fn ext_list(_window: tauri::Window, state: State<'_, AppState>) -> Result<Vec<ExtInfo>, AppError> {
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

/// Restarts so extension changes are picked up: WebView2 loads them at startup.
#[tauri::command]
pub async fn app_restart(app: AppHandle, webview: tauri::Webview) -> Result<(), AppError> {
    require_trusted(&webview)?;
    let exe = std::env::current_exe().map_err(|err| AppError::new("restart", err.to_string()))?;
    let mut command = std::process::Command::new(exe);
    if let Ok(dir) = std::env::current_dir() {
        command.current_dir(dir);
    }
    command.spawn().map_err(|err| AppError::new("restart", err.to_string()))?;
    save_sessions(&app);
    flush_history(&app);
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

/// The chrome ui has no console we can read, so it reports through here.
#[tauri::command]
pub async fn ui_log(message: String) -> Result<(), AppError> {
    crate::logging::info(&format!("ui: {message}"));
    Ok(())
}

/// A userscript path arrives from the page, so it is resolved and checked
/// against the scripts folder before anything is renamed or deleted.
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
        return Err(AppError::new("bad_script", "script is outside the scripts folder"));
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
        std::fs::rename(&source, &target).map_err(|err| AppError::new("script", err.to_string()))?;
    }
    crate::logging::info(&format!("userscript {} -> {}", source.display(), target.display()));
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
pub async fn script_list(_window: tauri::Window, state: State<'_, AppState>) -> Result<Vec<crate::userscripts::ScriptInfo>, AppError> {
    Ok(state.scripts.clone())
}

/// Creates the user stylesheet if missing, then reveals the folder.
#[tauri::command]
pub async fn custom_css_open(webview: tauri::Webview, state: State<'_, AppState>) -> Result<String, AppError> {
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
pub async fn open_dir(webview: tauri::Webview, state: State<'_, AppState>, which: String) -> Result<(), AppError> {
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
/// The address field cannot read the clipboard itself: the webview clipboard API
/// is gated behind a permission prompt that never appears, so go through the OS.
#[tauri::command]
pub async fn clipboard_read(webview: tauri::Webview) -> Result<String, AppError> {
    require_trusted(&webview)?;
    let mut board = arboard::Clipboard::new().map_err(|err| AppError::new("clipboard", err.to_string()))?;
    board.get_text().map_err(|err| AppError::new("clipboard", err.to_string()))
}

#[tauri::command]
pub async fn clipboard_write(webview: tauri::Webview, text: String) -> Result<(), AppError> {
    require_trusted(&webview)?;
    let mut board = arboard::Clipboard::new().map_err(|err| AppError::new("clipboard", err.to_string()))?;
    board.set_text(text).map_err(|err| AppError::new("clipboard", err.to_string()))
}

/// The super drag engine runs inside the page, so it gets one narrow entry point
/// instead of the commands it would otherwise need. A remote origin must not be
/// able to open tabs, write the clipboard or start downloads by itself, and the
/// page already decided the action - this only checks the wording.
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
    // Only a page webview may call this: the gesture starts on the page.
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
            tab_new(app, window, state, Some(url), None, Some(background.unwrap_or(false))).await?;
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
            let _ = save_url(app, &state, url, None).await?;
        }
        other => return Err(AppError::new("bad_action", other)),
    }
    Ok(())
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
    save_url(app, &state, url, file_name).await
}

/// Fetches a url into the downloads folder; shared with the support hook.
pub(crate) async fn save_url(
    app: AppHandle,
    state: &AppState,
    url: String,
    file_name: Option<String>,
) -> Result<String, AppError> {
    if !url.starts_with("http://") && !url.starts_with("https://") {
        return Err(AppError::new("bad_url", "only http(s) files can be saved"));
    }

    let response = reqwest::get(&url)
        .await
        .map_err(|err| AppError::new("net", err.to_string()))?;
    if !response.status().is_success() {
        return Err(AppError::new(
            "net",
            format!("server answered {}", response.status()),
        ));
    }

    let bytes = response
        .bytes()
        .await
        .map_err(|err| AppError::new("net", err.to_string()))?;
    // 128 MB is plenty for an image and keeps a stray click from filling the disk.
    if bytes.len() > 128 * 1024 * 1024 {
        return Err(AppError::new("too_big", "file is larger than 128 MB"));
    }

    let suggested = file_name.unwrap_or_default();
    let target = crate::downloads::target_for(&state.dirs.downloads, &url, &suggested);
    // Same rule as the state files: a half written file must not appear.
    crate::paths::write_atomic(&target, &bytes)?;
    crate::downloads::record_done(&app, &url, &target);
    crate::logging::info(&format!("saved {}", target.display()));
    Ok(target.display().to_string())
}

/// Wipes cookies, storage and caches for every webview we own.
#[tauri::command]
pub async fn app_clear_data(app: AppHandle, window: tauri::Window, _state: State<'_, AppState>) -> Result<(), AppError> {
    let labels: Vec<String> = {
        let state = app.state::<AppState>();
        let core = state.read_core(window.label());
        core.tabs.iter().map(|tab| tab.id.clone()).collect()
    };

    for label in labels {
        if let Some(webview) = app.get_webview(&label) {
            let _ = webview.clear_all_browsing_data();
        }
    }
    for label in ["w1-chrome"] {
        if let Some(webview) = app.get_webview(label) {
            let _ = webview.clear_all_browsing_data();
        }
    }
    crate::logging::info("browsing data cleared");
    Ok(())
}