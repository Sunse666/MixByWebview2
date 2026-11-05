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
    /// Shows the menu bar row above the address field.
    pub menu_bar: bool,
    /// Interface language: "en" or "zh".
    pub language: String,
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
            menu_bar: true,
            language: "en".into(),
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
