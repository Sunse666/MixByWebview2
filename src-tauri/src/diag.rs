use std::io::Write;
use std::path::{Path, PathBuf};

/// Everything that goes into a support bundle.
pub struct Bundle<'a> {
    pub version: &'a str,
    pub git: &'a str,
    pub os: &'a str,
    pub webview: &'a str,
    pub settings: &'a str,
    pub theme: &'a str,
    pub session: &'a str,
    pub paths: &'a str,
    pub extensions: &'a str,
    pub scripts: &'a str,
    pub log_tail: &'a [u8],
}

/// Writes <diag>/Mix-diag-v<version>-<epoch>.zip and returns the path.
pub fn write_bundle(diag_dir: &Path, bundle: &Bundle<'_>) -> std::io::Result<PathBuf> {
    std::fs::create_dir_all(diag_dir)?;

    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let path = diag_dir.join(format!("Mix-diag-v{}-{stamp}.zip", bundle.version));

    let file = std::fs::File::create(&path)?;
    let mut zip = zip::ZipWriter::new(file);
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);

    let summary = format!(
        "Mix diagnostic bundle. version {} git {} os {} webview {} created {}",
        bundle.version, bundle.git, bundle.os, bundle.webview, stamp
    );

    let entries: [(&str, &[u8]); 8] = [
        ("summary.txt", summary.as_bytes()),
        ("settings.json", bundle.settings.as_bytes()),
        ("theme.json", bundle.theme.as_bytes()),
        ("session.json", bundle.session.as_bytes()),
        ("paths.json", bundle.paths.as_bytes()),
        ("extensions.json", bundle.extensions.as_bytes()),
        ("userscripts.json", bundle.scripts.as_bytes()),
        ("mix.log", bundle.log_tail),
    ];

    for (name, body) in entries {
        zip.start_file(name, options)?;
        zip.write_all(body)?;
    }

    zip.start_file("README.txt", options)?;
    zip.write_all(README.as_bytes())?;
    zip.finish()?;
    Ok(path)
}

const README: &str = "Mix support bundle. Nothing here is uploaded anywhere; send this zip yourself. \
It contains settings, tabs and the tail of the log. Page content and cookies are not included.";


/// Replaces the user profile path so a bundle can be shared without leaking a name.
pub fn redact(input: &str) -> String {
    let mut output = input.to_string();
    for key in ["USERPROFILE", "HOME"] {
        if let Ok(home) = std::env::var(key) {
            if home.len() > 3 {
                output = output.replace(&home, "<home>");
                output = output.replace(&home.replace('\\', "/"), "<home>");
            }
        }
    }
    output
}

/// Last chunk of the log, so a huge file cannot blow up the bundle.
pub fn log_tail(path: &Path, limit: usize) -> Vec<u8> {
    let Ok(bytes) = std::fs::read(path) else {
        return b"log file is missing".to_vec();
    };
    if bytes.len() <= limit {
        return bytes;
    }
    let start = bytes.len() - limit;
    let start = bytes[start..]
        .iter()
        .position(|byte| *byte == b'\n')
        .map(|offset| start + offset + 1)
        .unwrap_or(start);
    bytes[start..].to_vec()
}
#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("mix-diag-test-{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    fn bundle<'a>(settings: &'a str) -> Bundle<'a> {
        Bundle {
            version: "1.2.3",
            git: "abc1234",
            os: "windows",
            webview: "1.0",
            settings,
            theme: "{}",
            session: "{}",
            paths: "{}",
            extensions: "[]",
            scripts: "[]",
            log_tail: b"line one\nline two\n",
        }
    }

    #[test]
    fn writes_a_readable_zip() {
        let dir = scratch("zip");
        let path = write_bundle(&dir, &bundle("{}")).expect("write");
        assert!(path.exists());

        let file = std::fs::File::open(&path).expect("open");
        let mut archive = zip::ZipArchive::new(file).expect("zip");
        let names: Vec<String> = (0..archive.len())
            .map(|index| archive.by_index(index).expect("entry").name().to_string())
            .collect();
        assert!(names.contains(&"summary.txt".to_string()), "{names:?}");
        assert!(names.contains(&"mix.log".to_string()), "{names:?}");
        assert!(names.contains(&"README.txt".to_string()), "{names:?}");
    }

    #[test]
    fn log_tail_keeps_the_end_only() {
        let dir = scratch("tail");
        std::fs::create_dir_all(&dir).expect("dir");
        let log = dir.join("mix.log");
        std::fs::write(&log, b"aaaaaaaaaa\nbbbbbbbbbb\ncccccccccc\n").expect("write");

        let tail = log_tail(&log, 25);
        let text = String::from_utf8(tail).expect("utf8");
        assert!(text.ends_with("cccccccccc\n"), "got {text:?}");
        assert!(!text.starts_with("aaaaaaaaaa"), "got {text:?}");
    }

    #[test]
    fn missing_log_is_not_fatal() {
        let tail = log_tail(&scratch("missing").join("nope.log"), 1024);
        assert!(!tail.is_empty());
    }
}

/// Gathers the current state and writes a bundle.
pub fn export(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    use tauri::Manager;

    let state = app.state::<crate::ipc::AppState>();
    let dirs = &state.dirs;

    let read = |path: &Path| {
        std::fs::read_to_string(path)
            .map(|text| redact(&text))
            .unwrap_or_else(|_| "{\"missing\": true}".to_string())
    };

    let settings = read(&dirs.settings_file);
    let theme = read(&dirs.theme_file);
    let session = read(&dirs.session_file);

    let paths = serde_json::json!({
        "root": dirs.root.display().to_string(),
        "user_data": dirs.user_data.display().to_string(),
        "crx": dirs.crx.display().to_string(),
        "scripts": dirs.scripts.display().to_string(),
        "fell_back": dirs.fell_back,
    })
    .to_string();
    let paths = redact(&paths);

    let extensions = serde_json::to_string(&state.extensions).unwrap_or_else(|_| "[]".into());
    let scripts = serde_json::to_string(&state.scripts).unwrap_or_else(|_| "[]".into());
    let webview = tauri::webview_version().unwrap_or_else(|_| "unknown".into());
    let log = log_tail(&dirs.logs.join("mix.log"), 2 * 1024 * 1024);

    let bundle = Bundle {
        version: env!("CARGO_PKG_VERSION"),
        git: env!("MIX_GIT"),
        os: std::env::consts::OS,
        webview: &webview,
        settings: &settings,
        theme: &theme,
        session: &session,
        paths: &paths,
        extensions: &extensions,
        scripts: &scripts,
        log_tail: &log,
    };

    write_bundle(&dirs.diag, &bundle).map_err(|err| err.to_string())
}
