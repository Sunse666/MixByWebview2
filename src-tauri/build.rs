fn main() {
    // Declaring the commands here turns them into ACL permissions, which lets
    // remote pages be granted a single command (see capabilities/remote.json).
    let commands: &'static [&'static str] = &[
        "layout_get",
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
        "paths_get",
        "open_dir",
        "ext_list",
        "download_list",
        "download_open_dir",
        "shortcut",
        "context_menu",
        "menu_context_get",
        "permission_request",
        "permission_reply",
        "snap",
    ];

    tauri_build::try_build(
        tauri_build::Attributes::new()
            .app_manifest(tauri_build::AppManifest::new().commands(commands)),
    )
    .expect("failed to run tauri-build");
}