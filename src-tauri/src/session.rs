use serde::{Deserialize, Serialize};
use std::path::Path;

pub const SCHEMA: u32 = 1;

/// Only URLs are stored: the webview back/forward stack cannot be restored,
/// so the UI shows a disabled back button instead of pretending otherwise.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Session {
    pub schema_version: u32,
    pub tabs: Vec<String>,
    pub selected: usize,
}

impl Default for Session {
    fn default() -> Self {
        Self {
            schema_version: SCHEMA,
            tabs: Vec::new(),
            selected: 0,
        }
    }
}

impl Session {
    pub fn load(path: &Path) -> Option<Self> {
        let raw = std::fs::read_to_string(path).ok()?;
        let session: Session = serde_json::from_str(&raw).ok()?;
        if session.schema_version > SCHEMA {
            return None;
        }
        Some(session)
    }

    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        let json = serde_json::to_string_pretty(self)?;
        std::fs::write(path, json)
    }
}
