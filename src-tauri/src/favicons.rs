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

/// Cached icon file for a host, when one was downloaded.
pub fn cache_path(dir: &Path, host: &str) -> Option<std::path::PathBuf> {
    for extension in ["png", "ico", "svg", "jpg", "webp"] {
        let candidate = dir.join(format!("{host}.{extension}"));
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

/// Writes the icon bytes and returns the file it landed in.
pub fn store_icon(dir: &Path, host: &str, bytes: &[u8], content_type: &str) -> Option<std::path::PathBuf> {
    if bytes.is_empty() || bytes.len() > 256 * 1024 {
        return None;
    }
    let extension = if content_type.contains("svg") || bytes.starts_with(b"<svg") {
        "svg"
    } else if content_type.contains("jpeg") || content_type.contains("jpg") {
        "jpg"
    } else if content_type.contains("webp") {
        "webp"
    } else if content_type.contains("icon") || host.is_empty() {
        "ico"
    } else {
        "png"
    };
    std::fs::create_dir_all(dir).ok()?;
    let path = dir.join(format!("{host}.{extension}"));
    std::fs::write(&path, bytes).ok()?;
    Some(path)
}

/// Data url for a cached icon, so pages need no extra round trip.
pub fn data_url(path: &Path) -> Option<String> {
    let bytes = std::fs::read(path).ok()?;
    let mime = match path.extension().and_then(|ext| ext.to_str()) {
        Some("svg") => "image/svg+xml",
        Some("ico") => "image/x-icon",
        Some("jpg") => "image/jpeg",
        Some("webp") => "image/webp",
        _ => "image/png",
    };
    Some(format!("data:{mime};base64,{}", crate::extensions::base64(&bytes)))
}

/// Host part of a url, without a leading "www." so both forms share an entry.
pub fn host_of(url: &str) -> String {
    let rest = url.split("://").nth(1).unwrap_or(url);
    let host = rest.split(['/', '?', '#']).next().unwrap_or("");
    let host = host.split('@').next_back().unwrap_or(host);
    let host = host.split(':').next().unwrap_or(host);
    host.trim_start_matches("www.").to_ascii_lowercase()
}
