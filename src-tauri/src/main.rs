#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

// assets: frontend files are embedded at compile time, so editing src/*.js
// only takes effect after the crate is rebuilt.
mod browser_reg;
mod diag;
mod zoom;
mod downloads;
mod ext_live;
mod extensions;
mod favicons;
mod ipc;
mod library;
mod net;
mod logging;
mod overlay;
mod paths;
mod permissions;
mod platform;
mod session;
mod snap;
mod translate;
mod tray;
mod userscripts;
mod state;
mod tabs;
mod theme;
mod window;

use tauri::Manager;

// embed bump 103
fn main() {
    let dirs = paths::AppDirs::resolve().ensure();
    logging::init(&dirs.logs);
    logging::info(&format!(
        "mix v{} (git {})",
        env!("CARGO_PKG_VERSION"),
        env!("MIX_GIT")
    ));

    tauri::Builder::default()
        // Registered first so a second launch is caught before anything else starts.
        .plugin(tauri_plugin_single_instance::init(|app, args, _cwd| {
            crate::logging::info("second instance forwarded to the running window");
            crate::window::show_or_focus(app);
            // The shell hands over a url when Mix is the default browser.
            let url = args
                .iter()
                .find(|arg| arg.starts_with("http://") || arg.starts_with("https://"))
                .cloned();
            if let Some(url) = url {
                let handle = app.clone();
                tauri::async_runtime::spawn(async move {
                    let state = handle.state::<ipc::AppState>();
                    let Some(window) = handle.get_window(crate::window::PRIMARY) else {
                        return;
                    };
                    let _ = ipc::tab_new(handle.clone(), window, state, Some(url), None, None).await;
                });
            }
        }))
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))

        .setup(move |app| {
            let settings = state::Settings::load(&dirs.settings_file);
            let theme = theme::Theme::load(&dirs.theme_file);
            let disabled_extensions = crate::state::Settings::load(&dirs.settings_file).disabled_extensions;
    let ext_infos = extensions::scan(&dirs, &disabled_extensions);
            let scripts = userscripts::scan(&dirs.scripts);
            {
                let settings = crate::state::Settings::load(&dirs.settings_file);
                let map: std::collections::BTreeMap<String, String> = if settings.gesture_map.is_empty() {
                    crate::tabs::default_gestures()
                } else {
                    settings.gesture_map.clone()
                };
                let config = serde_json::json!({
                    "trail": settings.gesture_trail,
                    "color": settings.gesture_color,
                    "width": settings.gesture_width,
                    "threshold": settings.gesture_threshold,
                    "map": map,
                    "blocked": settings.gesture_blocked,
                });
                crate::tabs::set_gestures(crate::tabs::GestureSetup {
                    enabled: settings.gestures,
                    super_drag: settings.super_drag,
                    drag_js: {
                        let map: std::collections::BTreeMap<String, String> =
                            if settings.drag_map.is_empty() {
                                crate::tabs::default_drag()
                            } else {
                                settings.drag_map.clone()
                            };
                        let drag = serde_json::json!({
                            "hint": settings.drag_hint,
                            "threshold": settings.drag_threshold,
                            "map": map,
                        });
                        serde_json::to_string(&drag).unwrap_or_else(|_| "{}".to_string())
                    },
                    js: serde_json::to_string(&config).unwrap_or_else(|_| "{}".to_string()),
                });
            }
            crate::tabs::set_custom_css(
                std::fs::read_to_string(dirs.user_data.join("custom.css"))
                    .unwrap_or_default()
                    .trim_start_matches('\u{feff}')
                    .to_string(),
            );
            crate::tabs::set_shortcuts(
                crate::state::Settings::load(&dirs.settings_file).shortcuts,
            );
            app.manage(ipc::AppState::new(
                dirs.clone(),
                settings,
                theme,
                ext_infos,
                scripts,
            ));
            window::bootstrap(app)?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            ipc::extension_live_list,
            ipc::extension_live_set,
            ipc::window_control,
            ipc::window_drag,
            ipc::window_resize,
            ipc::tab_list,
            ipc::tab_select,
            ipc::tab_new,
            ipc::tab_pin,
            ipc::tab_close,
            ipc::tab_navigate,
            ipc::tab_nav,
            ipc::theme_get,
            ipc::theme_apply,
            ipc::theme_revert,
            ipc::overlay_open,
            ipc::overlay_close,
            ipc::ext_list,
            ipc::script_set_enabled,
            ipc::script_remove,
            ipc::script_list,
            ipc::ui_log,
            ipc::zoom_get,
            ipc::zoom_step,
            ipc::tab_print,
            ipc::window_fullscreen,
            ipc::app_clear_data,
            ipc::theme_export,
            ipc::theme_import,
            ipc::suggest_open,
            ipc::suggest_feed,
            ipc::suggest_refresh,
            ipc::overlay_toggle,
            ipc::download_url,
            ipc::find_in_page,
            ipc::find_result,
            ipc::find_status,
            ipc::translate_page_inplace,
            ipc::translate_state,
            ipc::translate_status,
            ipc::translate_restore,
            ipc::translate_done,
            ipc::translate_texts,
            ipc::translate_page,
            ipc::page_failed,
            ipc::tab_scroll,
            ipc::url_type,
            ipc::url_key,
            ipc::url_blur,
            ipc::tab_move,
            ipc::tab_icon,
            ipc::tab_reopen,
            ipc::window_new,
            ipc::favicon_map,
            ipc::history_list,
            ipc::clear_data,
            ipc::history_remove,
            ipc::history_clear,
            ipc::bookmark_list,
            ipc::bookmark_toggle,
            ipc::bookmark_add,
            ipc::bookmark_move,
            ipc::bookmark_set_folder,
            ipc::bookmark_update,
            ipc::bookmark_remove,
            ipc::diag_export,
            ipc::app_info,
            ipc::app_restart,
            ipc::app_quit,
            ipc::drag_probe,
            ipc::search_new_tab,
            ipc::shortcut,
            ipc::overlay_open_at,
            ipc::page_fullscreen,
            ipc::backup_export,
            ipc::backup_import,
            ipc::register_browser,
            ipc::clipboard_read,
            ipc::clipboard_write,
            ipc::chrome_fit,
            ipc::focus_page,
            ipc::chrome_menu,
            ipc::url_action,
            ipc::context_menu,
            ipc::snap,
            ipc::settings_get,
            ipc::settings_set,
            ipc::autostart_get,
            ipc::autostart_set,
            ipc::permission_request,
            ipc::permission_reply,
            ipc::menu_context_get,
            ipc::download_list,
            ipc::download_open,
            ipc::download_clear,
            ipc::download_open_dir,
            ipc::custom_css_open,
            ipc::open_dir,
        ])
        .run(tauri::generate_context!())
        .expect("mix failed to start");
}