use serde::{Deserialize, Serialize};
use std::path::Path;

const SCHEMA: u32 = 1;
const MAX_VISITS: usize = 5000;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Visit {
    pub url: String,
    pub title: String,
    pub at: u64,
    /// Favicon url, filled in when the page reports one.
    #[serde(default)]
    pub icon: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Bookmark {
    pub url: String,
    pub title: String,
    pub at: u64,
    /// Optional folder name; an empty string means the top level.
    #[serde(default)]
    pub folder: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct VisitsFile {
    schema_version: u32,
    visits: Vec<Visit>,
}

#[derive(Debug, Serialize, Deserialize)]
struct BookmarksFile {
    schema_version: u32,
    bookmarks: Vec<Bookmark>,
}

pub fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Only http(s) pages are worth remembering; about:blank and file paths are not.
pub fn worth_recording(url: &str) -> bool {
    (url.starts_with("https://") || url.starts_with("http://")) && !url.contains("127.0.0.1")
}

pub fn load_visits(path: &Path) -> Vec<Visit> {
    let Ok(raw) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    match serde_json::from_str::<VisitsFile>(&raw) {
        Ok(file) => file.visits,
        Err(err) => {
            keep_unreadable(path, "history", &err.to_string());
            Vec::new()
        }
    }
}

/// A file that cannot be parsed is moved aside rather than left in place: the
/// next save would otherwise overwrite it with an empty list and the entries
/// would be gone for good.
fn keep_unreadable(path: &Path, what: &str, reason: &str) {
    let extension = path
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or("json");
    let backup = path.with_extension(format!("{extension}.corrupt"));
    crate::logging::warn(&format!("{what} unreadable ({reason}); keeping {}", backup.display()));
    let _ = std::fs::rename(path, &backup);
}

pub fn save_visits(path: &Path, visits: &[Visit]) -> std::io::Result<()> {
    let file = VisitsFile {
        schema_version: SCHEMA,
        visits: visits.to_vec(),
    };
    crate::paths::write_atomic(path, serde_json::to_string(&file)?.as_bytes())
}

/// Newest first, one entry per url, capped so the file stays small.
pub fn push_visit(visits: &mut Vec<Visit>, url: &str, title: &str) {
    let entry = Visit {
        url: url.to_string(),
        title: title.to_string(),
        at: now(),
        icon: None,
    };
    visits.retain(|visit| visit.url != entry.url);
    visits.insert(0, entry);
    visits.truncate(MAX_VISITS);
}

pub fn load_bookmarks(path: &Path) -> Vec<Bookmark> {
    let Ok(raw) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    match serde_json::from_str::<BookmarksFile>(&raw) {
        Ok(file) => file.bookmarks,
        Err(err) => {
            keep_unreadable(path, "bookmarks", &err.to_string());
            Vec::new()
        }
    }
}

pub fn save_bookmarks(path: &Path, bookmarks: &[Bookmark]) -> std::io::Result<()> {
    let file = BookmarksFile {
        schema_version: SCHEMA,
        bookmarks: bookmarks.to_vec(),
    };
    crate::paths::write_atomic(path, serde_json::to_string(&file)?.as_bytes())
}

/// Returns true when the page was added, false when it was removed.
pub fn toggle_bookmark(bookmarks: &mut Vec<Bookmark>, url: &str, title: &str) -> bool {
    if let Some(index) = bookmarks.iter().position(|bookmark| bookmark.url == url) {
        bookmarks.remove(index);
        return false;
    }
    bookmarks.insert(
        0,
        Bookmark {
            url: url.to_string(),
            title: if title.is_empty() {
                url.to_string()
            } else {
                title.to_string()
            },
            at: now(),
            folder: String::new(),
        },
    );
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn visits_are_deduped_and_newest_first() {
        let mut visits = Vec::new();
        push_visit(&mut visits, "https://a.test", "A");
        push_visit(&mut visits, "https://b.test", "B");
        push_visit(&mut visits, "https://a.test", "A again");
        assert_eq!(visits.len(), 2);
        assert_eq!(visits[0].url, "https://a.test");
        assert_eq!(visits[0].title, "A again");
    }

    #[test]
    fn only_web_pages_are_recorded() {
        assert!(worth_recording("https://example.com"));
        assert!(!worth_recording("about:blank"));
        assert!(!worth_recording("http://127.0.0.1:8080"));
    }

    #[test]
    fn toggling_a_bookmark_adds_then_removes() {
        let mut bookmarks = Vec::new();
        assert!(toggle_bookmark(&mut bookmarks, "https://a.test", "A"));
        assert_eq!(bookmarks.len(), 1);
        assert!(!toggle_bookmark(&mut bookmarks, "https://a.test", "A"));
        assert!(bookmarks.is_empty());
    }
}
