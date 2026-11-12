//! Settings, theme and autostart.

use super::*;

#[tauri::command]
pub async fn theme_get(_window: tauri::Window, state: State<'_, AppState>) -> Result<Theme, AppError> {
    Ok(state.theme.read().clone())
}

#[tauri::command]
pub async fn theme_apply(
    app: AppHandle,
    webview: tauri::Webview, state: State<'_, AppState>,
    theme: Theme,
    persist: bool,
) -> Result<Theme, AppError> {
    require_trusted(&webview)?;
    let theme = theme.sanitized();

    {
        *state.theme.write() = theme.clone();
        if persist {
            *state.baseline.write() = theme.clone();
        }
    }

    broadcast_theme(&app, &theme);

    if persist {
        theme.save(&state.dirs.theme_file)?;
        let _ = app.emit("theme:changed", &theme);
    }
    Ok(theme)
}

#[tauri::command]
pub async fn theme_revert(app: AppHandle, _window: tauri::Window, state: State<'_, AppState>) -> Result<Theme, AppError> {
    let baseline = {
        let value = state.baseline.read().clone();
        *state.theme.write() = value.clone();
        value
    };
    broadcast_theme(&app, &baseline);
    Ok(baseline)
}

fn broadcast_theme(app: &AppHandle, theme: &Theme) {
    let json = serde_json::to_string(theme).unwrap_or_else(|_| "{}".into());
    let script = format!("window.__mixApplyTheme && window.__mixApplyTheme({json})");

    let mut labels = vec!["w1-chrome".to_string()];
    for label in app
        .state::<AppState>()
        .overlay
        .read()
        .unwrap_or_else(|e| e.into_inner())
        .values()
        .cloned()
        .collect::<Vec<_>>()
    {
        labels.push(label);
    }

    for label in labels {
        if let Some(webview) = app.get_webview(&label) {
            let _ = webview.eval(&script);
        }
    }
}

#[tauri::command]
pub async fn settings_get(webview: tauri::Webview, state: State<'_, AppState>) -> Result<Settings, AppError> {
    require_trusted(&webview)?;
    Ok(state.settings.read().clone())
}

#[tauri::command]
pub async fn settings_set(
    webview: tauri::Webview, state: State<'_, AppState>,
    patch: SettingsPatch,
) -> Result<Settings, AppError> {
    require_trusted(&webview)?;
    let settings = apply_settings_patch(&state, patch)?;
    crate::tabs::apply_settings(&state.dirs, &settings);
    crate::tabs::broadcast_shortcuts(webview.app_handle());
    let label = webview.window().label().to_string();
    let _ = webview
        .app_handle()
        .emit_to(label.clone(), "settings:changed", ());
    crate::sidebar::ensure(webview.app_handle(), &label);
    let _ = crate::window::relayout(webview.app_handle(), &label);
    Ok(settings)
}

/// Mutates and writes the settings.
pub(crate) fn apply_settings_patch(
    state: &AppState,
    patch: SettingsPatch,
) -> Result<Settings, AppError> {
    let settings = {
        let mut guard = state.settings.write();
        if let Some(value) = patch.restore_session {
            guard.restore_session = value;
        }
        if let Some(value) = patch.show_bookmarks {
            guard.show_bookmarks = value;
        }
        if let Some(value) = patch.proxy_mode {
            guard.proxy_mode = match value.as_str() {
                "off" | "manual" => value,
                _ => "system".to_string(),
            };
        }
        if let Some(value) = patch.proxy_server {
            guard.proxy_server = value.trim().to_string();
        }
        if let Some(value) = patch.sidebar {
            guard.sidebar = match value.as_str() {
                "left" | "right" => value,
                _ => "off".to_string(),
            };
        }
        if let Some(value) = patch.sidebar_width {
            guard.sidebar_width = value.clamp(180.0, 640.0);
        }
        if let Some(value) = patch.keep_alive_hosts {
            guard.keep_alive_hosts = value.trim().to_string();
        }
        if let Some(value) = patch.tab_hover_preview {
            guard.tab_hover_preview = value;
        }
        if let Some(value) = patch.freeze_background {
            guard.freeze_background = value;
        }
        if let Some(value) = patch.live_thumbs {
            guard.live_thumbs = value;
        }
        if let Some(value) = patch.mark_suspended {
            guard.mark_suspended = value;
        }
        if let Some(value) = patch.memory_saver {
            guard.memory_saver = value;
        }
        if let Some(value) = patch.drag_hint {
            guard.drag_hint = value;
        }
        if let Some(value) = patch.drag_threshold {
            guard.drag_threshold = value.clamp(8, 120);
        }
        if let Some(value) = patch.drag_map {
            guard.drag_map = value;
        }
        if let Some(value) = patch.super_drag {
            guard.super_drag = value;
        }
        if let Some(value) = patch.gesture_trail {
            guard.gesture_trail = value;
        }
        if let Some(value) = patch.gesture_color {
            guard.gesture_color = value.trim().to_string();
        }
        if let Some(value) = patch.gesture_width {
            guard.gesture_width = value.clamp(1, 10);
        }
        if let Some(value) = patch.gesture_threshold {
            guard.gesture_threshold = value.clamp(10, 200);
        }
        if let Some(value) = patch.gesture_map {
            guard.gesture_map = value;
        }
        if let Some(value) = patch.gesture_blocked {
            guard.gesture_blocked = value.trim().to_string();
        }
        if let Some(value) = patch.gestures {
            guard.gestures = value;
        }
        if let Some(value) = patch.theme_mode {
            guard.theme_mode = match value.as_str() {
                "light" => "light".into(),
                "system" => "system".into(),
                _ => "dark".into(),
            };
        }
        if let Some(value) = patch.font_family {
            guard.font_family = value.trim().to_string();
        }
        if let Some(value) = patch.font_mono {
            guard.font_mono = value.trim().to_string();
        }
        if let Some(value) = patch.font_size {
            guard.font_size = value.min(32);
        }
        if let Some(value) = patch.font_min_size {
            guard.font_min_size = value.min(24);
        }
        if let Some(value) = patch.home_url {
            guard.home_url = value.trim().to_string();
        }
        if let Some(value) = patch.default_zoom {
            guard.default_zoom = value.clamp(0.5, 2.0);
        }
        if let Some(value) = patch.tab_double_click_close {
            guard.tab_double_click_close = value;
        }
        if let Some(value) = patch.tab_show_close {
            guard.tab_show_close = value;
        }
        if let Some(value) = patch.tab_close_goes_left {
            guard.tab_close_goes_left = value;
        }
        if let Some(value) = patch.custom_search {
            guard.custom_search = value.trim().to_string();
        }
        if let Some(value) = patch.newtab_show_search {
            guard.newtab_show_search = value;
        }
        if let Some(value) = patch.newtab_show_sites {
            guard.newtab_show_sites = value;
        }
        if let Some(value) = patch.newtab_url {
            guard.newtab_url = value.trim().to_string();
        }
        if let Some(value) = patch.newtab_tiles {
            guard.newtab_tiles = value.clamp(1, 40);
        }
        if let Some(value) = patch.newtab_columns {
            guard.newtab_columns = value.clamp(1, 12);
        }
        if let Some(value) = patch.newtab_radius {
            guard.newtab_radius = value.min(50);
        }
        if let Some(value) = patch.shortcuts {
            guard.shortcuts = value
                .into_iter()
                .filter(|(_, combo)| !combo.trim().is_empty())
                .map(|(action, combo)| (action, normalize_combo(&combo)))
                .collect();
        }
        if let Some(value) = patch.search_engine {
            guard.search_engine = match value.as_str() {
                "bing" | "google" | "ddg" => value,
                _ => "baidu".to_string(),
            };
        }
        if let Some(value) = patch.new_tab_page {
            guard.new_tab_page = value;
        }
        if let Some(value) = patch.language {
            guard.language = match value.as_str() {
                "zh" => "zh".to_string(),
                _ => "en".to_string(),
            };
        }
        if let Some(value) = patch.translate_to {
            if !value.trim().is_empty() {
                guard.translate_to = value.trim().to_string();
            }
        }
        if let Some(value) = patch.toolbar {
            let known = [
                "downloads",
                "translate",
                "find",
                "zoom",
                "bookmark",
                "extensions",
                "home",
                "reopen",
                "mute",
                "shot",
            ];
            guard.toolbar = value
                .into_iter()
                .filter(|id| known.contains(&id.as_str()))
                .collect();
        }
        if let Some(value) = patch.hidden_extensions {
            guard.hidden_extensions = value;
        }
        if let Some(value) = patch.disabled_extensions {
            guard.disabled_extensions = value;
        }
        if let Some(value) = patch.start_url {
            if !value.trim().is_empty() {
                guard.start_url = value;
            }
        }
        if let Some(value) = patch.max_live_tabs {
            guard.max_live_tabs = value.clamp(1, 64);
        }
        guard.clone()
    };

    settings.save(&state.dirs.settings_file)?;
    crate::logging::info(&format!(
        "settings saved: restore_session={} max_live_tabs={}",
        settings.restore_session, settings.max_live_tabs
    ));
    Ok(settings)
}

#[tauri::command]
pub async fn autostart_get(app: AppHandle) -> Result<bool, AppError> {
    use tauri_plugin_autostart::ManagerExt;
    app.autolaunch()
        .is_enabled()
        .map_err(|err| AppError::new("autostart", err.to_string()))
}

#[tauri::command]
pub async fn autostart_set(app: AppHandle, enabled: bool) -> Result<bool, AppError> {
    use tauri_plugin_autostart::ManagerExt;
    let manager = app.autolaunch();
    let result = if enabled {
        manager.enable()
    } else {
        manager.disable()
    };
    result.map_err(|err| AppError::new("autostart", err.to_string()))?;
    let now = manager
        .is_enabled()
        .map_err(|err| AppError::new("autostart", err.to_string()))?;
    crate::logging::info(&format!("autostart -> {now}"));
    Ok(now)
}

/// Writes the current theme next to the settings so it can be shared.
#[tauri::command]
pub async fn theme_export(app: AppHandle, _window: tauri::Window) -> Result<String, AppError> {
    let state = app.state::<AppState>();
    let theme = state.theme.read().clone();
    let path = state.dirs.user_data.join("theme-export.json");
    theme.save(&path)?;
    crate::logging::info(&format!("theme exported to {}", path.display()));
    Ok(path.display().to_string())
}

/// Applies a pasted theme. Invalid JSON is rejected and the old theme stays.
#[tauri::command]
pub async fn theme_import(
    app: AppHandle,
    webview: tauri::Webview,
    state: State<'_, AppState>,
    json: String,
) -> Result<Theme, AppError> {
    require_trusted(&webview)?;
    let window = webview.window();
    let parsed: Theme = serde_json::from_str(&json)
        .map_err(|err| AppError::new("bad_theme", format!("not valid theme json: {err}")))?;
    let theme = parsed.sanitized();

    *state.theme.write() = theme.clone();
    *state.baseline.write() = theme.clone();
    theme.save(&state.dirs.theme_file)?;
    broadcast_theme(&app, &theme);
    let _ = window.emit("theme:changed", &theme);
    crate::logging::info("theme imported");
    Ok(theme)
}
