use crate::ipc::AppState;
use parking_lot::Mutex;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Manager};

/// A prompt that was never answered is not worth keeping around.
const PROMPT_TTL: Duration = Duration::from_secs(300);

/// One outstanding question, answered by polling.
pub struct Pending {
    /// Window whose page asked; the answer only belongs to it.
    pub window: String,
    /// "origin|kind", remembered once the answer is collected.
    pub key: String,
    /// None until the user answers.
    pub answer: Option<bool>,
    pub at: Instant,
}

/// Bumped when the on-disk shape changes.
const SCHEMA: u32 = 1;

/// What `permissions.json` holds.
#[derive(serde::Serialize, serde::Deserialize)]
#[serde(default)]
struct DecisionsFile {
    schema_version: u32,
    /// "origin|kind" -> allow
    decisions: HashMap<String, bool>,
}

impl Default for DecisionsFile {
    fn default() -> Self {
        Self {
            schema_version: SCHEMA,
            decisions: HashMap::new(),
        }
    }
}

#[derive(Default)]
pub struct PermissionStore {
    /// "origin|kind" -> allow
    pub decisions: Mutex<HashMap<String, bool>>,
    /// Outstanding prompts, keyed by "origin|kind".
    pub pending: Mutex<HashMap<u64, Pending>>,
}

impl PermissionStore {
    /// Reads the answers this browser has given before.
    ///
    /// They used to live in memory only, so "Allow" meant "until Mix restarts" - and the
    /// site asked again the next day, which is not what the button says.
    pub fn load(path: &std::path::Path) -> Self {
        let mut decisions = HashMap::new();
        if let Ok(raw) = std::fs::read_to_string(path) {
            // A byte order mark is what an editor on Windows puts in front of a file it
            // saves, and serde reads it as the first character of a value. Refusing the
            // whole file because of it would silently drop every answer the user gave.
            let raw = raw.trim_start_matches('\u{feff}');
            match serde_json::from_str::<DecisionsFile>(raw) {
                Ok(file) if file.schema_version <= SCHEMA => decisions = file.decisions,
                Ok(_) => crate::logging::warn("permissions.json is newer than this build"),
                Err(err) => crate::logging::warn(&format!(
                    "permissions.json unreadable ({err}); starting empty"
                )),
            }
        }
        Self {
            decisions: Mutex::new(decisions),
            pending: Mutex::new(HashMap::new()),
        }
    }

    /// Writes the answers back. Small file, written whenever one changes.
    fn save(&self, path: &std::path::Path) {
        let file = DecisionsFile {
            schema_version: SCHEMA,
            decisions: self.decisions.lock().clone(),
        };
        let Ok(json) = serde_json::to_string_pretty(&file) else {
            return;
        };
        if let Err(err) = crate::paths::write_atomic(path, json.as_bytes()) {
            crate::logging::warn(&format!("permissions.json not written: {err}"));
        }
    }
}

static NEXT_ID: AtomicU64 = AtomicU64::new(1);

pub fn key(origin: &str, kind: &str) -> String {
    format!("{origin}|{kind}")
}

pub fn remembered(app: &AppHandle, key: &str) -> Option<bool> {
    let state = app.state::<AppState>();
    let decisions = state.permissions.decisions.lock();
    decisions.get(key).copied()
}

pub fn remember(app: &AppHandle, key: &str, allow: bool) {
    let state = app.state::<AppState>();
    {
        let mut decisions = state.permissions.decisions.lock();
        decisions.insert(key.to_string(), allow);
    }
    state.permissions.save(&state.dirs.permissions_file);
}

/// Every answer this browser remembers, in a stable order, as the settings page draws them.
pub fn decisions(app: &AppHandle) -> Vec<serde_json::Value> {
    let state = app.state::<AppState>();
    let mut out: Vec<(String, bool)> = {
        let map = state.permissions.decisions.lock();
        map.iter()
            .map(|(key, allow)| (key.clone(), *allow))
            .collect()
    };
    out.sort();
    out.into_iter()
        .map(|(key, allow)| {
            let (origin, kind) = key.split_once('|').unwrap_or((key.as_str(), ""));
            serde_json::json!({ "origin": origin, "kind": kind, "allow": allow, "key": key })
        })
        .collect()
}

/// Forgets one answer, or all of them when no key is given.
pub fn forget(app: &AppHandle, key: Option<&str>) {
    let state = app.state::<AppState>();
    {
        let mut decisions = state.permissions.decisions.lock();
        match key {
            Some(key) => {
                decisions.remove(key);
            }
            None => decisions.clear(),
        }
    }
    state.permissions.save(&state.dirs.permissions_file);
    crate::logging::info(&format!(
        "permission decisions forgotten: {}",
        key.unwrap_or("all")
    ));
}

pub fn open_prompt(app: &AppHandle, window: &str, key: &str) -> u64 {
    let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
    let state = app.state::<AppState>();
    let mut pending = state.permissions.pending.lock();
    pending.retain(|_, prompt| prompt.at.elapsed() < PROMPT_TTL);
    pending.insert(
        id,
        Pending {
            window: window.to_string(),
            key: key.to_string(),
            answer: None,
            at: Instant::now(),
        },
    );
    id
}

/// Records the answer; the page collects it on its next poll.
pub fn answer(app: &AppHandle, id: u64, allow: bool) -> bool {
    let state = app.state::<AppState>();
    let mut pending = state.permissions.pending.lock();
    match pending.get_mut(&id) {
        Some(prompt) => {
            prompt.answer = Some(allow);
            true
        }
        None => false,
    }
}

/// The answer, once; None means the user has not decided yet.
pub fn poll(app: &AppHandle, id: u64) -> Option<bool> {
    let (answer, key) = {
        let state = app.state::<AppState>();
        let pending = state.permissions.pending.lock();
        match pending.get(&id) {
            None => return Some(false),
            Some(prompt) => (prompt.answer?, prompt.key.clone()),
        }
    };
    remember(app, &key, answer);
    let state = app.state::<AppState>();
    state.permissions.pending.lock().remove(&id);
    Some(answer)
}

/// A prompt whose overlay closed without an answer counts as denied.
pub fn cancel_window(app: &AppHandle, window: &str) -> usize {
    let state = app.state::<AppState>();
    let mut pending = state.permissions.pending.lock();
    let mut cancelled = 0;
    for prompt in pending.values_mut() {
        if prompt.window == window && prompt.answer.is_none() {
            prompt.answer = Some(false);
            cancelled += 1;
        }
    }
    cancelled
}

/// Minimal percent encoding: only alphanumerics and a few safe characters survive.
pub fn encode(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for byte in input.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_' | b':') {
            out.push(byte as char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}
#[cfg(test)]
mod tests {
    use super::{encode, key, PermissionStore};

    fn scratch(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("mix-permissions-{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("dir");
        dir.join("permissions.json")
    }

    #[test]
    fn keys_are_origin_scoped() {
        assert_eq!(key("https://a.test", "camera"), "https://a.test|camera");
        assert_ne!(
            key("https://a.test", "camera"),
            key("https://b.test", "camera")
        );
    }

    #[test]
    fn encodes_prompt_payload() {
        assert_eq!(encode("http://127.0.0.1:8767"), "http:%2F%2F127.0.0.1:8767");
        assert_eq!(encode("camera"), "camera");
    }

    #[test]
    fn an_answer_survives_a_restart() {
        let path = scratch("restart");
        let store = PermissionStore::default();
        {
            let mut decisions = store.decisions.lock();
            decisions.insert("https://a.test|camera".to_string(), true);
            decisions.insert("https://b.test|geolocation".to_string(), false);
        }
        store.save(&path);

        let reloaded = PermissionStore::load(&path);
        let decisions = reloaded.decisions.lock();
        assert_eq!(decisions.get("https://a.test|camera"), Some(&true));
        assert_eq!(decisions.get("https://b.test|geolocation"), Some(&false));
    }

    #[test]
    fn a_broken_file_starts_empty_instead_of_failing() {
        let path = scratch("broken");
        std::fs::write(&path, b"{ not json").expect("write");
        let store = PermissionStore::load(&path);
        assert!(store.decisions.lock().is_empty());
    }

    #[test]
    fn a_byte_order_mark_does_not_lose_the_answers() {
        let path = scratch("bom");
        let body = "\u{feff}{\"schema_version\":1,\"decisions\":{\"https://a.test|camera\":true}}";
        std::fs::write(&path, body.as_bytes()).expect("write");
        let store = PermissionStore::load(&path);
        assert_eq!(
            store.decisions.lock().get("https://a.test|camera"),
            Some(&true)
        );
    }
}
