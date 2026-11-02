use crate::ipc::AppState;
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use tauri::{webview::DownloadEvent, Emitter, Manager, Webview};

#[derive(Debug, Clone, Serialize)]
pub struct DownloadTask {
    pub id: u64,
    pub url: String,
    pub file_name: String,
    pub path: String,
    /// started | finished | failed
    pub state: String,
}

static NEXT_ID: AtomicU64 = AtomicU64::new(1);
const KEEP: usize = 50;

/// WebView2 has no download UI of its own beyond a prompt, so we take the
/// destination, record the task and let the chrome strip show it.
pub fn handle(webview: Webview, event: DownloadEvent<'_>) -> bool {
    let app = webview.app_handle().clone();

    match event {
        DownloadEvent::Requested { url, destination } => {
            let Some(state) = app.try_state::<AppState>() else {
                return true;
            };
            // WebView2 already resolved Content-Disposition into a file name; keep it
            // and only move the file into our portable folder.
            let suggested = destination
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or_default()
                .to_string();
            let target = target_for(&state.dirs.downloads, url.as_str(), &suggested);
            *destination = target.clone();
            record(&app, Some(url.as_str()), Some(&target), "started");
            true
        }
        DownloadEvent::Finished { url, path, success } => {
            let state = if success { "finished" } else { "failed" };
            record(&app, Some(url.as_str()), path.as_deref(), state);
            success
        }
        // The enum is non_exhaustive; unknown events keep the default behaviour.
        _ => true,
    }
}

fn record(app: &tauri::AppHandle, url: Option<&str>, path: Option<&Path>, state: &str) {
    let Some(guard) = app.try_state::<AppState>() else {
        return;
    };

    let tasks = {
        let mut tasks = guard.downloads.write().unwrap_or_else(|e| e.into_inner());
        match state {
            "started" => {
                let target = path.map(Path::to_path_buf).unwrap_or_default();
                tasks.push(DownloadTask {
                    id: NEXT_ID.fetch_add(1, Ordering::Relaxed),
                    url: url.unwrap_or_default().to_string(),
                    file_name: file_name(&target),
                    path: target.display().to_string(),
                    state: state.to_string(),
                });
                if tasks.len() > KEEP {
                    let drop_count = tasks.len() - KEEP;
                    tasks.drain(0..drop_count);
                }
            }
            _ => {
                // Finish the most recent task for this url; path may be absent on macOS.
                let target = tasks
                    .iter()
                    .rposition(|task| Some(task.url.as_str()) == url && task.state == "started");
                if let Some(index) = target {
                    tasks[index].state = state.to_string();
                    if let Some(path) = path {
                        tasks[index].path = path.display().to_string();
                        tasks[index].file_name = file_name(path);
                    }
                }
            }
        }
        tasks.clone()
    };

    crate::logging::info(&format!("download {state}: {}", url.unwrap_or_default()));
    let _ = app.emit("download:changed", tasks);
}

/// Prefer the name the server suggested, fall back to the URL, then avoid
/// overwriting an existing file.
pub fn target_for(dir: &Path, url: &str, suggested: &str) -> PathBuf {
    let name = if suggested.trim().is_empty() {
        derive_name(url)
    } else {
        sanitize(suggested)
    };

    let candidate = dir.join(&name);
    if !candidate.exists() {
        return candidate;
    }

    let (stem, ext) = split_name(&name);
    for counter in 1..1000 {
        let candidate = dir.join(format!("{stem} ({counter}){ext}"));
        if !candidate.exists() {
            return candidate;
        }
    }
    candidate
}

fn derive_name(url: &str) -> String {
    let raw = url.split(['?', '#']).next().unwrap_or_default();
    let last = raw
        .rsplit('/')
        .find(|segment| !segment.is_empty())
        .unwrap_or("download");
    sanitize(last)
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_default()
}

fn split_name(name: &str) -> (String, String) {
    match name.rsplit_once('.') {
        Some((stem, ext)) if !stem.is_empty() && !ext.is_empty() => {
            (stem.to_string(), format!(".{ext}"))
        }
        _ => (name.to_string(), String::new()),
    }
}

fn sanitize(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    for ch in name.chars() {
        if ch.is_ascii_alphanumeric() || matches!(ch, '.' | '-' | '_' | ' ') {
            out.push(ch);
        } else {
            out.push('-');
        }
    }
    if out.len() > 120 {
        out.truncate(120);
    }
    let trimmed = out.trim_matches(['.', ' ', '-']);
    if trimmed.is_empty() {
        "download".to_string()
    } else {
        trimmed.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("mix-dl-test-{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("scratch");
        dir
    }

    #[test]
    fn keeps_plain_names() {
        assert_eq!(sanitize("report.pdf"), "report.pdf");
        assert_eq!(sanitize("my file 1.zip"), "my file 1.zip");
    }

    #[test]
    fn strips_dangerous_characters() {
        assert_eq!(sanitize("../../etc/passwd"), "etc-passwd");
        assert_eq!(sanitize("a/b\\c:d"), "a-b-c-d");
        assert_eq!(sanitize("..."), "download");
        assert_eq!(sanitize(""), "download");
    }

    #[test]
    fn derives_name_from_url() {
        let dir = scratch("names");
        let path = target_for(&dir, "https://example.com/files/report.pdf?token=1", "");
        assert_eq!(path.file_name().unwrap(), "report.pdf");
    }

    #[test]
    fn prefers_the_suggested_name() {
        let dir = scratch("suggested");
        let path = target_for(&dir, "https://example.com/download?id=7", "Mozilla Installer.exe");
        assert_eq!(path.file_name().unwrap(), "Mozilla Installer.exe");
    }

    #[test]
    fn avoids_overwriting() {
        let dir = scratch("collide");
        std::fs::write(dir.join("report.pdf"), b"x").expect("seed");
        let path = target_for(&dir, "https://example.com/report.pdf", "");
        assert_eq!(path.file_name().unwrap(), "report (1).pdf");
    }
}