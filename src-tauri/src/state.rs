use serde::{Deserialize, Serialize};
use std::path::Path;

/// Bumped when the on-disk shape changes. Old files are migrated, not ignored.
pub const SCHEMA: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub schema_version: u32,
    pub start_url: String,
    pub restore_session: bool,
    pub chrome_height: f64,
    /// Adds a bookmarks row under the address bar, which also grows the chrome.
    pub show_bookmarks: bool,
    /// Toolbar buttons the user kept, in display order.
    pub toolbar: Vec<String>,
    /// Extensions the user hid from the toolbar, by name.
    pub hidden_extensions: Vec<String>,
    /// Extensions that must not load at all; takes effect on the next start.
    pub disabled_extensions: Vec<String>,
    /// Baidu translate open platform credentials; empty means the free fallback.
    pub baidu_appid: String,
    pub baidu_secret: String,
    pub translate_to: String,
    /// A search template with %s; wins over search_engine when set.
    pub custom_search: String,
    /// Right button gestures such as L for back.
    pub gestures: bool,
    /// Drag a link, image or text to act on it.
    pub super_drag: bool,
    /// Show the little badge while dragging.
    pub drag_hint: bool,
    /// Pixels before a drag counts.
    pub drag_threshold: u32,
    /// Direction -> action, e.g. "L" -> "open", "U" -> "copy".
    pub drag_map: std::collections::BTreeMap<String, String>,
    /// Draw the trail line while a gesture is in progress.
    pub gesture_trail: bool,
    /// Trail colour, any css colour.
    pub gesture_color: String,
    /// Trail width in pixels.
    pub gesture_width: u32,
    /// Minimum movement before a direction counts, in pixels.
    pub gesture_threshold: u32,
    /// Pattern -> action, e.g. "UD" -> "reload".
    pub gesture_map: std::collections::BTreeMap<String, String>,
    /// Comma or space separated hosts where gestures are off.
    pub gesture_blocked: String,
    /// Zoom applied to every page when it opens, 1.0 is 100%.
    pub default_zoom: f64,
    /// Double click on a tab closes it.
    pub tab_double_click_close: bool,
    /// Show the close button on each tab.
    pub tab_show_close: bool,
    /// After closing, activate the tab to the left instead of the right.
    pub tab_close_goes_left: bool,
    /// New tab page: show the search field.
    pub newtab_show_search: bool,
    /// New tab page: show the frequent site tiles.
    pub newtab_show_sites: bool,
    /// Opens instead of the built-in page when set.
    pub newtab_url: String,
    /// Tiles shown at most.
    pub newtab_tiles: u32,
    /// Grid columns for the tiles.
    pub newtab_columns: u32,
    /// Tile corner radius in percent.
    pub newtab_radius: u32,
    /// Action -> combo, e.g. "new_tab" -> "ctrl+t". Empty means the default.
    #[serde(default)]
    pub shortcuts: std::collections::BTreeMap<String, String>,
    /// Engine used when the address bar input is not a url: baidu | bing | google | ddg.
    pub search_engine: String,
    /// New tabs open the built-in page instead of the start url.
    pub new_tab_page: bool,
    /// Shows the menu bar row above the address field.
    pub menu_bar: bool,
    /// Interface language: "en" or "zh".
    pub language: String,
    /// Suspend tabs beyond the live limit to give memory back.
    pub memory_saver: bool,
    /// Hosts that are never suspended, comma separated.
    pub keep_alive_hosts: String,
    /// Draw the suspended marker on idle tabs.
    pub mark_suspended: bool,
    pub max_live_tabs: usize,
    pub download_dir: Option<String>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            schema_version: SCHEMA,
            start_url: "https://example.com".into(),
            restore_session: true,
            chrome_height: 68.0,
        show_bookmarks: false,
        toolbar: vec![
            "translate".into(),
            "zoom".into(),
            "bookmark".into(),
            "extensions".into(),
            "downloads".into(),
        ],
        hidden_extensions: Vec::new(),
        disabled_extensions: Vec::new(),
            baidu_appid: String::new(),
            baidu_secret: String::new(),
            translate_to: "zh".into(),
            gestures: true,
            super_drag: true,
            drag_hint: true,
            drag_threshold: 28,
            drag_map: std::collections::BTreeMap::new(),
            gesture_trail: true,
            gesture_color: "#4c8dff".into(),
            gesture_width: 3,
            gesture_threshold: 34,
            gesture_map: std::collections::BTreeMap::new(),
            gesture_blocked: String::new(),
            default_zoom: 1.0,
            tab_double_click_close: true,
            tab_show_close: true,
            tab_close_goes_left: true,
            custom_search: String::new(),
            newtab_show_search: true,
            newtab_show_sites: true,
            newtab_url: String::new(),
            newtab_tiles: 12,
            newtab_columns: 6,
            newtab_radius: 50,
            shortcuts: std::collections::BTreeMap::new(),
            search_engine: "baidu".into(),
            new_tab_page: true,
            menu_bar: true,
            language: "en".into(),
            memory_saver: true,
            keep_alive_hosts: String::new(),
            mark_suspended: true,
            max_live_tabs: 12,
            download_dir: None,
        }
    }
}

impl Settings {
    /// Keeps older profiles in step with changes to the default toolbar.
    fn migrate(mut self) -> Self {
        // The magnifier slot became the zoom control; "find" still exists but is
        // no longer part of the default strip.
        if !self.toolbar.iter().any(|id| id == "zoom") {
            for id in self.toolbar.iter_mut() {
                if id == "find" {
                    *id = "zoom".to_string();
                }
            }
        }
        // An empty list means "nothing enabled", but it also happens when a
        // profile predates the field, where falling back to the default is right.
        if self.toolbar.is_empty() {
            self.toolbar = Settings::default().toolbar;
        }
        self
    }

    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        let json = serde_json::to_string_pretty(self)?;
        std::fs::write(path, json)
    }

    pub fn load(path: &Path) -> Self {
        let Ok(raw) = std::fs::read_to_string(path) else {
            return Self::default();
        };
        match serde_json::from_str::<Settings>(&raw) {
            Ok(mut value) => {
                if value.schema_version > SCHEMA {
                    crate::logging::warn("settings.json is newer than this build; using defaults");
                    return Self::default();
                }
                value.schema_version = SCHEMA;
                value.migrate()
            }
            Err(err) => {
                crate::logging::warn(&format!("settings.json unreadable ({err}); keeping a backup"));
                let _ = std::fs::rename(path, path.with_extension("json.bak"));
                Self::default()
            }
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn write(dir: &str, body: &str) -> std::path::PathBuf {
        let path = std::env::temp_dir().join(dir);
        std::fs::create_dir_all(&path).unwrap();
        let file = path.join("settings.json");
        std::fs::write(&file, body).unwrap();
        file
    }

    #[test]
    fn an_old_toolbar_gains_the_zoom_button() {
        let file = write(
            "mix-settings-migrate",
            r#"{"schema_version":1,"toolbar":["downloads","translate","find","bookmark","extensions"]}"#,
        );
        let settings = Settings::load(&file);
        assert!(settings.toolbar.iter().any(|id| id == "zoom"));
        assert!(!settings.toolbar.iter().any(|id| id == "find"));
    }

    #[test]
    fn a_missing_toolbar_falls_back_to_the_default() {
        let file = write("mix-settings-empty", r#"{"schema_version":1}"#);
        let settings = Settings::load(&file);
        assert_eq!(settings.toolbar, Settings::default().toolbar);
    }

    #[test]
    fn an_explicit_choice_is_kept() {
        let file = write(
            "mix-settings-keep",
            r#"{"schema_version":1,"toolbar":["zoom","bookmark"]}"#,
        );
        let settings = Settings::load(&file);
        assert_eq!(settings.toolbar, vec!["zoom".to_string(), "bookmark".to_string()]);
    }
}
