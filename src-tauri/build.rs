fn main() {
    let git = std::process::Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .map(|hash| hash.trim().to_string())
        .unwrap_or_else(|| "unknown".to_string());
    println!("cargo:rustc-env=MIX_GIT={git}");

    // Declaring the commands here turns them into ACL permissions, which lets
    // remote pages be granted a single command (see capabilities/remote.json).
    let commands: &'static [&'static str] = &[
        "window_control",
        "window_drag",
        "window_resize",
        "tab_list",
        "tab_select",
        "tab_new",
        "tab_close",
        "tab_navigate",
        "tab_nav",
        "theme_get",
        "theme_apply",
        "theme_revert",
        "overlay_open",
        "overlay_close",
        "open_dir",
        "ext_list",
        "script_list",
        "ui_log",
        "zoom_get",
        "zoom_step",
        "tab_print",
        "window_fullscreen",
        "app_clear_data",
        "theme_export",
        "theme_import",
        "suggest_open",
        "suggest_feed",
        "suggest_refresh",
        "overlay_toggle",
        "tab_icon",
        "tab_reopen",
        "window_new",
        "history_list",
        "history_clear",
        "bookmark_list",
        "bookmark_toggle",
        "bookmark_remove",
        "diag_export",
        "app_info",
        "app_quit",
        "download_list",
        "download_open_dir",
        "shortcut",
        "context_menu",
        "menu_context_get",
        "permission_request",
        "permission_reply",
        "snap",
        "settings_get",
        "settings_set",
        "autostart_get",
        "autostart_set",
    ];

    tauri_build::try_build(
        tauri_build::Attributes::new()
            .app_manifest(tauri_build::AppManifest::new().commands(commands)),
    )
    .expect("failed to run tauri-build");
}