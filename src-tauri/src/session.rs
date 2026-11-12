use serde::{Deserialize, Serialize};
use std::path::Path;

pub const SCHEMA: u32 = 3;

/// Only urls are stored; the back/forward stack is not restored.
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
    Full {
        url: String,
        scroll: f64,
        #[serde(default)]
        pinned: bool,
    },
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

    #[allow(dead_code)]
    pub fn new(url: String, scroll: f64) -> Self {
        SessionTab::Full {
            url,
            scroll,
            pinned: false,
        }
    }

    pub fn pinned(&self) -> bool {
        match self {
            SessionTab::Url(_) => false,
            SessionTab::Full { pinned, .. } => *pinned,
        }
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
    /// Tab groups, with members as indexes into "tabs".
    #[serde(default)]
    pub groups: Vec<SessionGroup>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct SessionGroup {
    pub name: String,
    pub color: String,
    pub collapsed: bool,
    pub members: Vec<usize>,
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
        crate::paths::write_atomic(path, json.as_bytes())
    }

    /// Writes this process's final state with a timestamp, so a later launch can tell
    /// which process closed last.
    pub fn save_closed(&self, path: &Path, closed_at: u64) -> std::io::Result<()> {
        let body = serde_json::json!({
            "schema_version": self.schema_version,
            "windows": self.windows,
            "closed_at": closed_at,
        });
        let text = serde_json::to_string_pretty(&body).unwrap_or_else(|_| "{}".into());
        std::fs::write(path, text)
    }
}

/// The state left by the process that closed last, if any. Reading it consumes the
/// files: they describe a run that is over.
pub fn latest_closed(dir: &Path) -> Option<Session> {
    let mut newest: Option<(u64, Session)> = None;
    let Ok(entries) = std::fs::read_dir(dir) else {
        return None;
    };
    let mut files = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or_default();
        if !name.starts_with("session-") || !name.ends_with(".json") {
            continue;
        }
        files.push(path.clone());
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) else {
            continue;
        };
        let closed_at = value.get("closed_at").and_then(|v| v.as_u64()).unwrap_or(0);
        if closed_at == 0 {
            continue;
        }
        let Ok(session) = serde_json::from_value::<Session>(value) else {
            continue;
        };
        if newest.as_ref().map(|(at, _)| closed_at > *at).unwrap_or(true) {
            newest = Some((closed_at, session));
        }
    }
    for path in files {
        let _ = std::fs::remove_file(path);
    }
    newest.map(|(_, session)| session)
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
                    groups: Vec::new(),
                },
                WindowSession {
                    tabs: vec![
                        SessionTab::new("https://b.test".into(), 120.0),
                        SessionTab::new("https://c.test".into(), 0.0),
                    ],
                    selected: 1,
                    groups: Vec::new(),
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