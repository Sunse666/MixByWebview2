//! Settings, theme and autostart.

use super::*;

#[tauri::command]
pub async fn theme_get(
    _window: tauri::Window,
    state: State<'_, AppState>,
) -> Result<Theme, AppError> {
    Ok(state.theme.read().clone())
}

#[tauri::command]
pub async fn theme_apply(
    app: AppHandle,
    webview: tauri::Webview,
    state: State<'_, AppState>,
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
pub async fn theme_revert(
    app: AppHandle,
    _window: tauri::Window,
    state: State<'_, AppState>,
) -> Result<Theme, AppError> {
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
        .windows
        .overlay
        .read()
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
pub async fn settings_get(
    webview: tauri::Webview,
    state: State<'_, AppState>,
) -> Result<Settings, AppError> {
    require_trusted(&webview)?;
    Ok(state.settings.read().clone())
}

#[tauri::command]
pub async fn settings_set(
    webview: tauri::Webview,
    state: State<'_, AppState>,
    patch: serde_json::Value,
) -> Result<Settings, AppError> {
    require_trusted(&webview)?;
    // The sidebar has to be told what changed, and the patch itself is the only thing
    // that says so.
    let asked_side = patch.get("sidebar").is_some_and(|value| !value.is_null());
    let asked_views = patch
        .get("sidebar_views")
        .is_some_and(|value| !value.is_null());
    let settings = apply_settings_patch(&state, patch)?;
    crate::tabs::apply_settings(&state.dirs, &settings);
    crate::tabs::broadcast_shortcuts(webview.app_handle());
    let label = webview.window().label().to_string();
    let _ = webview
        .app_handle()
        .emit_to(label.clone(), "settings:changed", ());
    crate::sidebar::ensure(webview.app_handle(), &label);
    if asked_side {
        crate::sidebar::tell_side(webview.app_handle(), &label, &settings.sidebar);
    }
    if asked_views {
        crate::sidebar::tell_views(webview.app_handle(), &label, &settings.sidebar_views);
    }
    let _ = crate::window::relayout(webview.app_handle(), &label);
    Ok(settings)
}

/// Live width of the sidebar while it is being dragged. Only the final call persists,
/// so a drag does not rewrite settings.json on every frame.
#[tauri::command]
pub async fn sidebar_resize(
    webview: tauri::Webview,
    state: State<'_, AppState>,
    width: f64,
    persist: bool,
) -> Result<f64, AppError> {
    require_trusted(&webview)?;
    let clamped = width.clamp(180.0, 640.0);
    state.settings.write().sidebar_width = clamped;
    let label = webview.window().label().to_string();
    let _ = crate::window::relayout(webview.app_handle(), &label);
    if persist {
        let snapshot = state.settings.read().clone();
        snapshot.save(&state.dirs.settings_file)?;
    }
    Ok(clamped)
}

/// Fields the ui may not write: they belong to the app, not to a setting.
///
/// The chrome strip measures its own height, the extension state is reconciled against
/// what the browser confirms at runtime, and a reset request is a one-shot instruction
/// the app clears itself. `download_dir` is a path, and comes from a file dialog.
const PROTECTED: [&str; 5] = [
    "schema_version",
    "chrome_height",
    "extension_state",
    "reset_extensions",
    "download_dir",
];

/// Applies a partial settings object and writes the result.
///
/// The page sends only the fields it changed, as plain json. That object is merged into
/// the settings as they are - not into the defaults, so a field left out keeps its current
/// value - then deserialized and clamped by [`Settings::sanitize`]. One merge replaces the
/// field-by-field copy this used to do, and a new setting needs no code here at all.
pub(crate) fn apply_settings_patch(
    state: &AppState,
    patch: serde_json::Value,
) -> Result<Settings, AppError> {
    let settings = {
        let mut guard = state.settings.write();
        let next = merge_settings(&guard, &patch)?;
        *guard = next;
        guard.clone()
    };

    settings.save(&state.dirs.settings_file)?;
    crate::logging::info(&format!(
        "settings saved: restore_session={} max_live_tabs={}",
        settings.restore_session, settings.max_live_tabs
    ));
    Ok(settings)
}

/// Copies the fields a patch names onto these settings and clamps the result.
///
/// A field the patch leaves out, or sends as null, keeps the value it already had. A key
/// that is not a setting at all is ignored, which is what lets an older build survive a
/// newer ui.
pub(crate) fn merge_settings(
    current: &Settings,
    patch: &serde_json::Value,
) -> Result<Settings, AppError> {
    let asked = patch
        .as_object()
        .ok_or_else(|| AppError::new("bad_settings", "a settings patch has to be an object"))?;

    let mut merged = serde_json::to_value(current)
        .map_err(|err| AppError::new("bad_settings", err.to_string()))?;
    let fields = merged
        .as_object_mut()
        .ok_or_else(|| AppError::new("bad_settings", "settings are not an object"))?;
    for (key, value) in asked {
        if value.is_null() || PROTECTED.contains(&key.as_str()) {
            continue;
        }
        fields.insert(key.clone(), value.clone());
    }

    let mut settings: Settings = serde_json::from_value(merged)
        .map_err(|err| AppError::new("bad_settings", err.to_string()))?;
    settings.sanitize();
    Ok(settings)
}

#[cfg(test)]
mod settings_patch_tests {
    use super::*;

    fn patched(patch: serde_json::Value) -> Settings {
        merge_settings(&Settings::default(), &patch).expect("patch applies")
    }

    #[test]
    fn a_named_field_changes_and_the_rest_stay() {
        let settings = patched(serde_json::json!({ "max_live_tabs": 7 }));
        assert_eq!(settings.max_live_tabs, 7);
        assert_eq!(settings.language, Settings::default().language);
        assert_eq!(settings.toolbar, Settings::default().toolbar);
    }

    #[test]
    fn a_null_field_keeps_what_was_there() {
        let settings = patched(serde_json::json!({ "language": "zh", "theme_mode": null }));
        assert_eq!(settings.language, "zh");
        assert_eq!(settings.theme_mode, Settings::default().theme_mode);
    }

    #[test]
    fn the_ui_cannot_write_what_the_app_owns() {
        let settings = patched(serde_json::json!({
            "schema_version": 99,
            "chrome_height": 12.0,
            "reset_extensions": true,
            "download_dir": "C:/windows"
        }));
        assert_eq!(settings.schema_version, crate::state::SCHEMA);
        assert_eq!(settings.chrome_height, Settings::default().chrome_height);
        assert!(!settings.reset_extensions);
        assert_eq!(settings.download_dir, None);
    }

    #[test]
    fn an_out_of_range_value_is_clamped_not_stored() {
        let settings = patched(serde_json::json!({
            "max_live_tabs": 999,
            "sidebar_width": 5000.0,
            "default_zoom": 9.0
        }));
        assert_eq!(settings.max_live_tabs, 64);
        assert_eq!(settings.sidebar_width, 640.0);
        assert_eq!(settings.default_zoom, 2.0);
    }

    #[test]
    fn an_unknown_value_falls_back_to_a_legal_one() {
        let settings = patched(serde_json::json!({ "theme_mode": "solarized", "language": "fr" }));
        assert_eq!(settings.theme_mode, "dark");
        assert_eq!(settings.language, "en");
    }

    #[test]
    fn an_unknown_key_is_ignored() {
        let settings = patched(serde_json::json!({ "not_a_setting": 1, "language": "zh" }));
        assert_eq!(settings.language, "zh");
    }

    #[test]
    fn a_wrong_type_is_an_error_rather_than_a_silent_default() {
        let error = merge_settings(
            &Settings::default(),
            &serde_json::json!({ "font_size": "big" }),
        );
        assert!(error.is_err());
    }

    #[test]
    fn a_patch_that_is_not_an_object_is_refused() {
        let error = merge_settings(&Settings::default(), &serde_json::json!([1, 2, 3]));
        assert!(error.is_err());
    }
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
