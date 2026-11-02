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
            max_live_tabs: 12,
            download_dir: None,
        }
    }
}

impl Settings {
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
                value
            }
            Err(err) => {
                crate::logging::warn(&format!("settings.json unreadable ({err}); keeping a backup"));
                let _ = std::fs::rename(path, path.with_extension("json.bak"));
                Self::default()
            }
        }
    }
}