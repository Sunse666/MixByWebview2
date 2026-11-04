use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter};

pub fn install(app: &AppHandle) -> tauri::Result<()> {
    let show = MenuItem::with_id(app, "show", "Show Mix", true, None::<&str>)?;
    let new_tab = MenuItem::with_id(app, "new-tab", "New tab", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    let menu = Menu::with_items(
        app,
        &[
            &show,
            &new_tab,
            &PredefinedMenuItem::separator(app)?,
            &quit,
        ],
    )?;

    let mut builder = TrayIconBuilder::with_id("main")
        .tooltip("Mix")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "show" => crate::window::show_or_focus(app),
            "new-tab" => {
                crate::window::show_or_focus(app);
                // Creating a webview from this thread would deadlock on Windows,
                // so the chrome UI opens the tab for us. Only the focused window
                // should react.
                let target = crate::ipc::focused_window(app)
                    .map(|win| win.label().to_string())
                    .unwrap_or_else(|| crate::window::PRIMARY.to_string());
                let _ = app.emit_to(target, "ui:new-tab", ());
            }
            "quit" => {
                crate::ipc::save_sessions(&app);
                app.exit(0);
            }
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            let open = matches!(
                event,
                TrayIconEvent::Click {
                    button: MouseButton::Left,
                    button_state: MouseButtonState::Up,
                    ..
                } | TrayIconEvent::DoubleClick { .. }
            );
            if open {
                crate::window::show_or_focus(tray.app_handle());
            }
        });

    if let Some(icon) = app.default_window_icon().cloned() {
        builder = builder.icon(icon);
    }

    builder.build(app)?;
    crate::logging::info("tray icon installed");
    Ok(())
}