use crate::ipc::AppState;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Mutex;
use tauri::{AppHandle, Manager};

#[derive(Default)]
pub struct PermissionStore {
    /// "origin|kind" -> allow
    pub decisions: Mutex<HashMap<String, bool>>,
    /// Outstanding prompts, answered by permission_reply.
    pub pending: Mutex<HashMap<u64, Sender<bool>>>,
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
        .unwrap_or_else(|e| e.into_inner());
    decisions.get(key).copied()
}

pub fn remember(app: &AppHandle, key: &str, allow: bool) {
    let state = app.state::<AppState>();
    let mut decisions = state
        .permissions
        .decisions
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    decisions.insert(key.to_string(), allow);
}

pub fn open_prompt(app: &AppHandle) -> (u64, Receiver<bool>) {
    let (sender, receiver) = channel();
    let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
    let state = app.state::<AppState>();
    let mut pending = state
        .permissions
        .pending
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    pending.insert(id, sender);
    (id, receiver)
}

pub fn answer(app: &AppHandle, id: u64, allow: bool) -> bool {
    let state = app.state::<AppState>();
    let sender = state
        .permissions
        .pending
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .remove(&id);
    match sender {
        Some(sender) => sender.send(allow).is_ok(),
        None => false,
    }
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
