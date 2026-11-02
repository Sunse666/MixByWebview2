#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod downloads;
mod extensions;
mod ipc;
mod logging;
mod overlay;
mod paths;
mod permissions;
mod platform;
mod session;
mod tray;
mod state;
mod tabs;
mod theme;
mod window;

use tauri::Manager;

fn main() {
    let dirs = paths::AppDirs::resolve().ensure();
    logging::init(&dirs.logs);

    tauri::Builder::default()
        // Registered first so a second launch is caught before anything else starts.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            crate::logging::info("second instance forwarded to the running window");
            crate::window::show_or_focus(app);
        }))
        .setup(move |app| {
            let settings = state::Settings::load(&dirs.settings_file);
            let theme = theme::Theme::load(&dirs.theme_file);
            let ext_infos = extensions::scan(&dirs);
            app.manage(ipc::AppState::new(dirs.clone(), settings, theme, ext_infos));
            window::bootstrap(app)?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            ipc::layout_get,
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
            ipc::shortcut,
            ipc::context_menu,
            ipc::permission_request,
            ipc::permission_reply,
            ipc::menu_context_get,
            ipc::download_list,
            ipc::download_open_dir,
            ipc::paths_get,
            ipc::open_dir,
        ])
        .run(tauri::generate_context!())
        .expect("mix failed to start");
}