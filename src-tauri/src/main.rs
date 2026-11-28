#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod audio;
mod bili;
mod browser_reg;
mod com;
#[cfg(debug_assertions)]
mod debug;
mod diag;
mod downloads;
mod ext_live;
mod extensions;
mod favicons;
mod fetch;
mod fonts;
mod freeze;
mod ipc;
mod library;
mod logging;
mod mem;
mod net;
mod overlay;
mod paths;
mod permissions;
mod persist;
mod platform;
mod screenshot;
mod session;
mod sidebar;
mod snap;
mod state;
mod tabs;
mod theme;
mod thumbs;
mod translate;
mod tray;
mod userscripts;
mod window;
mod zoom;

use tauri::Manager;

/// Draws the app at 96 dpi when the settings ask for it.
///
/// `DPI_AWARENESS_CONTEXT_UNAWARE_GDISCALED` is the flavour that keeps text crisp: the
/// window is laid out in real pixels and Windows stretches the result, which is what a
/// reader who turns this on is asking for - every control the size it would be on a 100%
/// display, at the cost of a slightly softer picture on a scaled one.
///
/// The setting is read from the file rather than from the running state, because this has
/// to happen before the first window exists and the state is built later, inside `setup`.
fn apply_dpi_preference(settings_file: &std::path::Path) {
    #[cfg(windows)]
    {
        if !state::Settings::load(settings_file).ignore_system_dpi {
            return;
        }
        use windows::Win32::UI::HiDpi::{
            SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_UNAWARE_GDISCALED,
        };
        match unsafe { SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_UNAWARE_GDISCALED) } {
            Ok(()) => logging::info("dpi: drawing at 96 dpi, the display's scaling is ignored"),
            Err(err) => logging::warn(&format!("dpi: cannot ignore the display's scaling: {err}")),
        }
    }
    #[cfg(not(windows))]
    {
        let _ = settings_file;
    }
}

fn main() {
    let dirs = paths::AppDirs::resolve().ensure();
    logging::init(&dirs.logs);
    logging::info(&format!(
        "mix v{} (git {})",
        env!("CARGO_PKG_VERSION"),
        env!("MIX_GIT")
    ));

    // Before any window exists. Windows fixes a process's dpi awareness when it starts and
    // never re-decides it, so this is the only moment the setting can be honoured - which
    // is why the row that changes it says a restart is needed. Reading it here rather than
    // through `AppState` because the state is built inside `setup`, and `setup` runs after
    // the first window would have been created.
    apply_dpi_preference(&dirs.settings_file);

    let secondary = paths::instance_mode();
    let builder = tauri::Builder::default();
    let builder = if secondary {
        builder
    } else {
        builder.plugin(tauri_plugin_single_instance::init(|app, args, _cwd| {
            crate::logging::info("second instance forwarded to the running window");
            crate::window::show_or_focus(app);
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
                    let _ =
                        ipc::tab_new(handle.clone(), window, state, Some(url), None, None).await;
                });
            }
        }))
    };
    builder
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .setup(move |app| {
            let mut settings = state::Settings::load(&dirs.settings_file);
            let theme = theme::Theme::load(&dirs.theme_file);
            if settings.reset_extensions {
                match ext_live::forget_disabled_state(&dirs.profile) {
                    Ok(true) => logging::info("extension state cleared; extensions load again"),
                    Ok(false) => logging::info("extension state already clear"),
                    Err(err) => logging::warn(&format!("cannot clear extension state: {err}")),
                }
                settings.reset_extensions = false;
                if let Err(err) = settings.save(&dirs.settings_file) {
                    logging::warn(&format!(
                        "cannot save settings after extension reset: {err}"
                    ));
                }
            }
            let proxy_args = match settings.proxy_mode.as_str() {
                "off" => Some("--no-proxy-server".to_string()),
                "manual" if !settings.proxy_server.is_empty() => {
                    Some(format!("--proxy-server={}", settings.proxy_server))
                }
                _ => None,
            };
            if let Some(args) = &proxy_args {
                logging::info(&format!("proxy: {args}"));
            }
            crate::tabs::set_proxy_args(proxy_args);
            let ext_infos = extensions::scan(&dirs, &settings.disabled_extensions);
            let scripts = userscripts::scan(&dirs.scripts);
            crate::tabs::apply_settings(&dirs, &settings);
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
            ipc::tab_mute,
            ipc::drag_action,
            ipc::thumb_show,
            ipc::thumb_card,
            ipc::tab_screenshot,
            ipc::tab_navigate,
            ipc::tab_nav,
            ipc::theme_get,
            ipc::theme_apply,
            ipc::theme_revert,
            ipc::overlay_open,
            ipc::overlay_close,
            ipc::overlay_resize,
            ipc::ext_list,
            ipc::extension_add,
            ipc::extension_set,
            ipc::group_add,
            ipc::group_toggle,
            ipc::group_style,
            ipc::group_dissolve,
            ipc::group_remove,
            ipc::thumb_stats,
            ipc::tab_menu,
            ipc::tab_search_all,
            ipc::tab_focus_any,
            ipc::download_pause,
            ipc::download_cancel,
            ipc::download_reveal,
            ipc::download_retry,
            ipc::download_reconcile,
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
            ipc::suggest_active,
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
            ipc::translate_site,
            ipc::translate_site_state,
            ipc::translate_toggle,
            ipc::page_failed,
            ipc::tab_scroll,
            ipc::url_type,
            ipc::url_key,
            ipc::tab_move,
            ipc::tab_icon,
            ipc::tab_reopen,
            ipc::tab_closed_list,
            ipc::window_new,
            ipc::tab_pop_out,
            ipc::favicon_map,
            ipc::history_list,
            ipc::history_count,
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
            ipc::proxy_state,
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
            ipc::view_source,
            ipc::source_ready,
            ipc::source_get,
            ipc::save_page,
            ipc::page_ready,
            ipc::page_qr,
            ipc::reading_mode,
            ipc::article_ready,
            ipc::article_get,
            ipc::context_menu,
            ipc::bar_reveal,
            ipc::video_pip,
            ipc::video_download,
            ipc::bili_download,
            ipc::snap,
            ipc::settings_get,
            ipc::font_list,
            ipc::settings_set,
            ipc::sidebar_resize,
            ipc::autostart_get,
            ipc::autostart_set,
            ipc::permission_request,
            ipc::permission_reply,
            ipc::permission_poll,
            ipc::permission_list,
            ipc::permission_forget,
            ipc::menu_context_get,
            ipc::download_list,
            ipc::download_open,
            ipc::download_clear,
            ipc::custom_css_open,
            ipc::open_dir,
        ])
        .build(tauri::generate_context!())
        .expect("mix failed to start")
        .run(|app, event| {
            // The last state of this process is written on the way out, with a
            // timestamp, so the next launch restores the window closed last.
            if let tauri::RunEvent::Exit = event {
                ipc::finalize_session(app);
            }
        });
}
