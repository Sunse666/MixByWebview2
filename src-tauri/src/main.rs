#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

// assets: frontend files are embedded at compile time, so editing src/*.js
// only takes effect after the crate is rebuilt.
mod diag;
mod zoom;
mod downloads;
mod extensions;
mod ipc;
mod library;
mod logging;
mod overlay;
mod paths;
mod permissions;
mod platform;
mod session;
mod snap;
mod tray;
mod userscripts;
mod state;
mod tabs;
mod theme;
mod window;

use tauri::Manager;

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
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            crate::logging::info("second instance forwarded to the running window");
            crate::window::show_or_focus(app);
        }))
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))

        .setup(move |app| {
            let settings = state::Settings::load(&dirs.settings_file);
            let theme = theme::Theme::load(&dirs.theme_file);
            let ext_infos = extensions::scan(&dirs);
            let scripts = userscripts::scan(&dirs.scripts);
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
            ipc::window_control,
            ipc::window_drag,
            ipc::window_resize,
            ipc::tab_list,
            ipc::tab_select,
            ipc::tab_new,
            ipc::tab_close,
            ipc::tab_navigate,
            ipc::tab_nav,
            ipc::theme_get,
            ipc::theme_apply,
            ipc::theme_revert,
            ipc::overlay_open,
            ipc::overlay_close,
            ipc::ext_list,
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
            ipc::tab_icon,
            ipc::tab_reopen,
            ipc::window_new,
            ipc::history_list,
            ipc::history_clear,
            ipc::bookmark_list,
            ipc::bookmark_toggle,
            ipc::bookmark_remove,
            ipc::diag_export,
            ipc::app_info,
            ipc::app_quit,
            ipc::shortcut,
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
            ipc::download_open_dir,
            ipc::open_dir,
        ])
        .run(tauri::generate_context!())
        .expect("mix failed to start");
}