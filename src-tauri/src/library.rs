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
    /// How many times this url has been visited.
    ///
    /// The list holds one row per url, so counting rows cannot answer "which sites do I use
    /// most": a site opened daily and a site opened once are both one row, and the new tab
    /// page's tiles were ranked by that. This is what they rank by now. A file written before
    /// the field existed comes back as 1, which is the count one recorded visit has.
    #[serde(default = "default_visits")]
    pub visits: u32,
}

/// The count a visit recorded before the field existed has.
fn default_visits() -> u32 {
    1
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

/// Milliseconds since the epoch, for ordering events that two processes both date.
pub fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

pub fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Only http(s) pages are worth remembering.
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

/// A file that cannot be parsed is moved aside rather than left in place.
fn keep_unreadable(path: &Path, what: &str, reason: &str) {
    let extension = path
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or("json");
    let backup = path.with_extension(format!("{extension}.corrupt"));
    crate::logging::warn(&format!(
        "{what} unreadable ({reason}); keeping {}",
        backup.display()
    ));
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
    // One row per url, so visiting a page again moves it to the front and adds to its count
    // rather than adding a row. The count is what the new tab page ranks tiles by; without it
    // a site opened fifty times and a site opened once looked identical.
    let carried = visits
        .iter()
        .find(|visit| visit.url == url)
        .map(|visit| visit.visits)
        .unwrap_or(0);
    let entry = Visit {
        url: url.to_string(),
        title: title.to_string(),
        at: now(),
        icon: None,
        visits: carried.saturating_add(1),
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
///
/// `newest_first` is a setting because both orders are defensible: a reader who adds pages
/// while browsing wants the newest at hand, and one who curates a bar wants a new page to
/// land at the end instead of in front of everything they arranged.
pub fn toggle_bookmark_at(
    bookmarks: &mut Vec<Bookmark>,
    url: &str,
    title: &str,
    newest_first: bool,
) -> bool {
    if let Some(index) = bookmarks.iter().position(|bookmark| bookmark.url == url) {
        bookmarks.remove(index);
        return false;
    }
    let entry = Bookmark {
        url: url.to_string(),
        title: if title.is_empty() {
            url.to_string()
        } else {
            title.to_string()
        },
        at: now(),
        folder: String::new(),
    };
    if newest_first {
        bookmarks.insert(0, entry);
    } else {
        bookmarks.push(entry);
    }
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
    fn a_page_opened_again_counts_as_opened_again() {
        // One row per url means the rows cannot say how often a site is used, which is what
        // the new tab page ranks its tiles by. The count on the row carries it.
        let mut visits = Vec::new();
        push_visit(&mut visits, "https://a.test", "A");
        push_visit(&mut visits, "https://b.test", "B");
        push_visit(&mut visits, "https://a.test", "A");
        push_visit(&mut visits, "https://a.test", "A");

        assert_eq!(visits.len(), 2, "still one row per url");
        let a = visits
            .iter()
            .find(|v| v.url == "https://a.test")
            .expect("a");
        assert_eq!(a.visits, 3, "opened three times");
        let b = visits
            .iter()
            .find(|v| v.url == "https://b.test")
            .expect("b");
        assert_eq!(b.visits, 1);
    }

    #[test]
    fn a_visit_written_before_the_field_existed_reads_as_one() {
        // history.json from an older build has no "visits" key. Reading it as 0 would make
        // every remembered site rank below every new one.
        let old = r#"{"url":"https://a.test","title":"A","at":1}"#;
        let visit: Visit = serde_json::from_str(old).expect("old shape");
        assert_eq!(visit.visits, 1);
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
        assert!(toggle_bookmark_at(
            &mut bookmarks,
            "https://a.test",
            "A",
            true
        ));
        assert_eq!(bookmarks.len(), 1);
        assert!(!toggle_bookmark_at(
            &mut bookmarks,
            "https://a.test",
            "A",
            true
        ));
        assert!(bookmarks.is_empty());
    }

    #[test]
    fn a_new_bookmark_lands_where_the_setting_says() {
        // Newest first is the default and the order the bar has always used.
        let mut front = Vec::new();
        toggle_bookmark_at(&mut front, "https://first.test", "First", true);
        toggle_bookmark_at(&mut front, "https://second.test", "Second", true);
        assert_eq!(
            front.iter().map(|b| b.title.as_str()).collect::<Vec<_>>(),
            ["Second", "First"]
        );

        // Appending instead leaves what the reader arranged alone.
        let mut back = Vec::new();
        toggle_bookmark_at(&mut back, "https://first.test", "First", false);
        toggle_bookmark_at(&mut back, "https://second.test", "Second", false);
        assert_eq!(
            back.iter().map(|b| b.title.as_str()).collect::<Vec<_>>(),
            ["First", "Second"]
        );
    }
}
