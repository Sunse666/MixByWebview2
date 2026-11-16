use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager};

/// The words this menu shows, in the two languages the app speaks.
///
/// A native menu is drawn by the platform and cannot reach `src/i18n.js`, so these three
/// strings live next to the menu that shows them rather than in the page dictionary.
/// English is the key, the same way the page dictionary works, and anything the app does
/// not translate stays as it is.
///
/// Read once, when the icon is created: the menu keeps the language the process started
/// with, like the other things that only a restart picks up.
fn say(language: &str, english: &'static str) -> &'static str {
    if language != "zh" {
        return english;
    }
    match english {
        "Show Mix" => "显示 Mix",
        "New tab" => "新建标签页",
        "Quit" => "退出",
        other => other,
    }
}

pub fn install(app: &AppHandle) -> tauri::Result<()> {
    let language = app
        .try_state::<crate::ipc::AppState>()
        .map(|state| state.settings.read().language.clone())
        .unwrap_or_else(|| "en".to_string());

    let show = MenuItem::with_id(app, "show", say(&language, "Show Mix"), true, None::<&str>)?;
    let new_tab = MenuItem::with_id(
        app,
        "new-tab",
        say(&language, "New tab"),
        true,
        None::<&str>,
    )?;
    let quit = MenuItem::with_id(app, "quit", say(&language, "Quit"), true, None::<&str>)?;
    let menu = Menu::with_items(
        app,
        &[&show, &new_tab, &PredefinedMenuItem::separator(app)?, &quit],
    )?;

    let mut builder = TrayIconBuilder::with_id("main")
        .tooltip("Mix")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "show" => crate::window::show_or_focus(app),
            "new-tab" => {
                crate::window::show_or_focus(app);
                let target = crate::ipc::focused_window(app)
                    .map(|win| win.label().to_string())
                    .unwrap_or_else(|| crate::window::PRIMARY.to_string());
                let _ = app.emit_to(target, "ui:new-tab", ());
            }
            "quit" => {
                crate::ipc::save_sessions(app);
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

#[cfg(test)]
mod tests {
    use super::say;

    #[test]
    fn the_menu_speaks_the_language_the_process_started_in() {
        assert_eq!(say("en", "Quit"), "Quit");
        assert_eq!(say("zh", "Quit"), "退出");
        assert_eq!(say("zh", "Show Mix"), "显示 Mix");
    }

    #[test]
    fn a_word_without_a_translation_is_left_alone() {
        assert_eq!(say("zh", "Something new"), "Something new");
        assert_eq!(say("de", "Quit"), "Quit");
    }
}
