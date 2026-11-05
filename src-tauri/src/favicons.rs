//! Site favicons, keyed by host, kept across restarts so bookmarks, history
//! and the new tab page can show a real icon without reloading the site.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

const MAX_ENTRIES: usize = 512;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Favicons {
    pub schema_version: u32,
    /// host -> icon url
    #[serde(default)]
    pub hosts: BTreeMap<String, String>,
}

impl Favicons {
    pub fn load(path: &Path) -> Self {
        let Ok(raw) = std::fs::read_to_string(path) else {
            return Self::default();
        };
        let mut value: Favicons =
            serde_json::from_str(raw.trim_start_matches('\u{feff}')).unwrap_or_default();
        value.schema_version = 1;
        value
    }

    pub fn save(&self, path: &Path) {
        if let Ok(json) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write(path, json);
        }
    }

    pub fn get(&self, host: &str) -> Option<&str> {
        self.hosts.get(host).map(|value| value.as_str())
    }

    /// Returns true when the map changed and should be written back.
    pub fn set(&mut self, host: &str, icon: &str) -> bool {
        if host.is_empty() || icon.is_empty() {
            return false;
        }
        if self.hosts.get(host).map(|value| value.as_str()) == Some(icon) {
            return false;
        }
        self.hosts.insert(host.to_string(), icon.to_string());
        while self.hosts.len() > MAX_ENTRIES {
            let Some(first) = self.hosts.keys().next().cloned() else {
                break;
            };
            self.hosts.remove(&first);
        }
        true
    }
}

/// Host part of a url, without a leading "www." so both forms share an entry.
pub fn host_of(url: &str) -> String {
    let rest = url.split("://").nth(1).unwrap_or(url);
    let host = rest.split(['/', '?', '#']).next().unwrap_or("");
    let host = host.split('@').next_back().unwrap_or(host);
    let host = host.split(':').next().unwrap_or(host);
    host.trim_start_matches("www.").to_ascii_lowercase()
}
