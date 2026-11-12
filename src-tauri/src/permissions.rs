use crate::ipc::AppState;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use parking_lot::Mutex;
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

#[derive(Default)]
pub struct PermissionStore {
    /// "origin|kind" -> allow
    pub decisions: Mutex<HashMap<String, bool>>,
    /// Outstanding prompts, keyed by "origin|kind".
    pub pending: Mutex<HashMap<u64, Pending>>,
}

static NEXT_ID: AtomicU64 = AtomicU64::new(1);

pub fn key(origin: &str, kind: &str) -> String {
    format!("{origin}|{kind}")
}

pub fn remembered(app: &AppHandle, key: &str) -> Option<bool> {
    let state = app.state::<AppState>();
    let decisions = state
        .permissions
        .decisions
        .lock()
        ;
    decisions.get(key).copied()
}

pub fn remember(app: &AppHandle, key: &str, allow: bool) {
    let state = app.state::<AppState>();
    let mut decisions = state
        .permissions
        .decisions
        .lock()
        ;
    decisions.insert(key.to_string(), allow);
}

pub fn open_prompt(app: &AppHandle, window: &str, key: &str) -> u64 {
    let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
    let state = app.state::<AppState>();
    let mut pending = state
        .permissions
        .pending
        .lock()
        ;
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
    let mut pending = state
        .permissions
        .pending
        .lock()
        ;
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
        let pending = state
            .permissions
            .pending
            .lock()
            ;
        match pending.get(&id) {
            None => return Some(false),
            Some(prompt) => match prompt.answer {
                None => return None,
                Some(answer) => (answer, prompt.key.clone()),
            },
        }
    };
    remember(app, &key, answer);
    let state = app.state::<AppState>();
    state
        .permissions
        .pending
        .lock()
        .remove(&id);
    Some(answer)
}

/// A prompt whose overlay closed without an answer counts as denied.
pub fn cancel_window(app: &AppHandle, window: &str) -> usize {
    let state = app.state::<AppState>();
    let mut pending = state
        .permissions
        .pending
        .lock()
        ;
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
    use super::{encode, key};

    #[test]
    fn keys_are_origin_scoped() {
        assert_eq!(key("https://a.test", "camera"), "https://a.test|camera");
        assert_ne!(key("https://a.test", "camera"), key("https://b.test", "camera"));
    }

    #[test]
    fn encodes_prompt_payload() {
        assert_eq!(encode("http://127.0.0.1:8767"), "http:%2F%2F127.0.0.1:8767");
        assert_eq!(encode("camera"), "camera");
    }
}
