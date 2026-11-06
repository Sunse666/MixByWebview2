use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

const SCHEMA: u32 = 1;
const MIN: f64 = 0.5;
const MAX: f64 = 2.0;

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(default)]
struct File {
    schema_version: u32,
    /// host -> zoom level; 1.0 is stored as "no entry".
    sites: HashMap<String, f64>,
}

/// Per site zoom, so a site keeps its size across tabs and restarts.
#[derive(Debug, Default, Clone)]
pub struct ZoomStore {
    sites: HashMap<String, f64>,
}

impl ZoomStore {
    pub fn load(path: &Path) -> Self {
        let Ok(raw) = std::fs::read_to_string(path) else {
            return Self::default();
        };
        let Ok(file) = serde_json::from_str::<File>(&raw) else {
            return Self::default();
        };
        if file.schema_version > SCHEMA {
            return Self::default();
        }
        Self { sites: file.sites }
    }

    /// Drops every remembered level, used by the clear-data action.
    pub fn forget_all(&mut self) {
        self.sites.clear();
    }

    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        let file = File {
            schema_version: SCHEMA,
            sites: self.sites.clone(),
        };
        std::fs::write(path, serde_json::to_string_pretty(&file)?)
    }

    pub fn level_for(&self, url: &str) -> f64 {
        host_of(url)
            .and_then(|host| self.sites.get(&host).copied())
            .unwrap_or(1.0)
    }

    /// Remembers a level for the host behind `url`; 1.0 forgets the entry.
    pub fn remember(&mut self, url: &str, level: f64) -> bool {
        let Some(host) = host_of(url) else {
            return false;
        };
        let level = normalize(level);
        if (level - 1.0).abs() < f64::EPSILON {
            self.sites.remove(&host);
        } else {
            self.sites.insert(host, level);
        }
        true
    }
}

pub fn normalize(level: f64) -> f64 {
    (((level * 10.0).round()) / 10.0).clamp(MIN, MAX)
}

/// "https://www.example.com/a?b" -> "www.example.com".
pub fn host_of(url: &str) -> Option<String> {
    let rest = url.split_once("://").map(|(_, tail)| tail)?;
    let authority = rest.split(['/', '?', '#']).next().unwrap_or_default();
    let host = authority
        .rsplit_once('@')
        .map(|(_, tail)| tail)
        .unwrap_or(authority);
    let host = host.split(':').next().unwrap_or(host);
    if host.is_empty() || host == "about:blank" {
        return None;
    }
    Some(host.to_lowercase())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn host_is_extracted_from_real_urls() {
        assert_eq!(host_of("https://www.example.com/a?b=1").as_deref(), Some("www.example.com"));
        assert_eq!(host_of("http://example.com:8080").as_deref(), Some("example.com"));
        assert_eq!(host_of("https://user:pw@Example.COM/").as_deref(), Some("example.com"));
        assert_eq!(host_of("about:blank"), None);
    }

    #[test]
    fn level_defaults_to_one_and_round_trips() {
        let mut store = ZoomStore::default();
        assert_eq!(store.level_for("https://a.test/"), 1.0);
        store.remember("https://a.test/", 1.3);
        assert_eq!(store.level_for("https://a.test/x"), 1.3);
        assert_eq!(store.level_for("https://b.test/"), 1.0);
    }

    #[test]
    fn resetting_removes_the_entry() {
        let mut store = ZoomStore::default();
        store.remember("https://a.test/", 1.5);
        store.remember("https://a.test/", 1.0);
        assert!(store.sites.is_empty());
    }

    #[test]
    fn levels_are_clamped_and_rounded() {
        assert_eq!(normalize(5.0), 2.0);
        assert_eq!(normalize(0.1), 0.5);
        assert_eq!(normalize(1.234), 1.2);
    }

    #[test]
    fn store_survives_a_round_trip() {
        let dir = std::env::temp_dir().join("mix-zoom-test");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("zoom.json");
        let mut store = ZoomStore::default();
        store.remember("https://a.test/", 0.8);
        store.save(&path).unwrap();
        assert_eq!(ZoomStore::load(&path).level_for("https://a.test/"), 0.8);
    }
}
