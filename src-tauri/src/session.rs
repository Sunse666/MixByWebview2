use serde::{Deserialize, Serialize};
use std::path::Path;

pub const SCHEMA: u32 = 3;

/// Only URLs are stored: the webview back/forward stack cannot be restored,
/// so the UI shows a disabled back button instead of pretending otherwise.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct WindowGeometry {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub maximized: bool,
}

/// A tab in the saved session. Old files stored bare urls, so both forms load.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum SessionTab {
    Url(String),
    Full { url: String, scroll: f64 },
}

impl SessionTab {
    pub fn url(&self) -> &str {
        match self {
            SessionTab::Url(url) => url,
            SessionTab::Full { url, .. } => url,
        }
    }

    pub fn scroll(&self) -> f64 {
        match self {
            SessionTab::Url(_) => 0.0,
            SessionTab::Full { scroll, .. } => *scroll,
        }
    }

    pub fn new(url: String, scroll: f64) -> Self {
        SessionTab::Full { url, scroll }
    }
}

/// One entry per window, in the order the windows were created.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct WindowSession {
    pub tabs: Vec<SessionTab>,
    pub selected: usize,
    /// Kept here instead of a plugin so it stays inside the portable folder.
    pub window: Option<WindowGeometry>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Session {
    pub schema_version: u32,
    pub windows: Vec<WindowSession>,
}

impl Session {
    pub fn load(path: &Path) -> Option<Self> {
        let raw = std::fs::read_to_string(path).ok()?;
        let session: Session = serde_json::from_str(&raw).ok()?;
        if session.schema_version > SCHEMA {
            return None;
        }
        if session.windows.is_empty() {
            return None;
        }
        Some(session)
    }

    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        let json = serde_json::to_string_pretty(self)?;
        std::fs::write(path, json)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_two_windows() {
        let dir = std::env::temp_dir().join("mix-session-test");
        std::fs::create_dir_all(&dir).expect("dir");
        let path = dir.join("session.json");

        let session = Session {
            schema_version: SCHEMA,
            windows: vec![
                WindowSession {
                    tabs: vec![SessionTab::new("https://a.test".into(), 0.0)],
                    selected: 0,
                    window: None,
                },
                WindowSession {
                    tabs: vec![
                        SessionTab::new("https://b.test".into(), 120.0),
                        SessionTab::new("https://c.test".into(), 0.0),
                    ],
                    selected: 1,
                    window: Some(WindowGeometry {
                        x: 10,
                        y: 20,
                        width: 800,
                        height: 600,
                        maximized: false,
                    }),
                },
            ],
        };
        session.save(&path).expect("save");

        let loaded = Session::load(&path).expect("load");
        assert_eq!(loaded.windows.len(), 2);
        assert_eq!(loaded.windows[1].tabs.len(), 2);
        assert_eq!(loaded.windows[1].tabs[0].scroll(), 120.0);
        assert_eq!(loaded.windows[1].tabs[0].url(), "https://b.test");
        assert_eq!(loaded.windows[1].selected, 1);
    }

    #[test]
    fn bare_url_sessions_still_load() {
        let dir = std::env::temp_dir().join("mix-session-legacy");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("session.json");
        std::fs::write(
            &path,
            "{\"schema_version\":2,\"windows\":[{\"tabs\":[\"https://a.test\"],\"selected\":0}]}",
        )
        .unwrap();
        let loaded = Session::load(&path).expect("load");
        assert_eq!(loaded.windows[0].tabs[0].url(), "https://a.test");
        assert_eq!(loaded.windows[0].tabs[0].scroll(), 0.0);
    }

    #[test]
    fn an_empty_file_is_ignored() {
        let dir = std::env::temp_dir().join("mix-session-empty");
        std::fs::create_dir_all(&dir).expect("dir");
        let path = dir.join("session.json");
        std::fs::write(&path, "{\"schema_version\":2,\"windows\":[]}").expect("write");
        assert!(Session::load(&path).is_none());
    }
}
