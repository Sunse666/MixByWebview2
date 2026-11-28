use std::path::{Path, PathBuf};

/// All writable state lives in <app root>/UserData.
#[derive(Debug, Clone)]
pub struct AppDirs {
    pub root: PathBuf,
    pub user_data: PathBuf,
    pub profile: PathBuf,
    /// The strip and the overlays: a second WebView2 environment.
    pub profile_ui: PathBuf,
    pub logs: PathBuf,
    pub downloads: PathBuf,
    pub extensions: PathBuf,
    pub diag: PathBuf,
    pub theme_assets: PathBuf,
    pub crx: PathBuf,
    pub favicons_file: PathBuf,
    pub favicons_dir: PathBuf,
    pub scripts: PathBuf,
    pub settings_file: PathBuf,
    pub theme_file: PathBuf,
    pub session_file: PathBuf,
    pub history_file: PathBuf,
    pub bookmarks_file: PathBuf,
    pub permissions_file: PathBuf,
    pub zoom_file: PathBuf,
    pub fell_back: bool,
}

/// A window that was dragged out of another one runs as its own process.
///
/// It is a window like any other, so it shares `UserData/profile` with the window it
/// came from: the tab that was dragged out keeps the site it was signed in to. That is
/// safe because the tab is closed in the window it came from before the drag ends, and
/// because a WebView2 profile is per directory rather than per process - what is not
/// allowed is one directory being held by two processes at the same instant, and the
/// two never overlap: the new process opens it after the tab that owned it is gone.
pub fn instance_mode() -> bool {
    std::env::args().any(|arg| arg == "--instance")
}

impl AppDirs {
    pub fn resolve() -> Self {
        let root = app_root();
        let mut dirs = Self::under(&root);
        if !is_writable(&dirs.user_data) {
            let fallback = fallback_data_dir();
            dirs = Self::under(&fallback);
            dirs.fell_back = true;
        }
        dirs
    }

    fn under(root: &Path) -> Self {
        let user_data = root.join("UserData");
        Self {
            root: root.to_path_buf(),
            profile: user_data.join("profile"),
            profile_ui: user_data.join("profile-ui"),
            logs: user_data.join("logs"),
            downloads: user_data.join("downloads"),
            extensions: user_data.join("extensions"),
            diag: user_data.join("diag"),
            theme_assets: user_data.join("theme"),
            settings_file: user_data.join("settings.json"),
            theme_file: user_data.join("theme.json"),
            session_file: user_data.join("session.json"),
            history_file: user_data.join("history.json"),
            bookmarks_file: user_data.join("bookmarks.json"),
            permissions_file: user_data.join("permissions.json"),
            zoom_file: user_data.join("zoom.json"),
            crx: root.join("crx"),
            favicons_file: user_data.join("favicons.json"),
            favicons_dir: user_data.join("favicons"),
            scripts: root.join("javascript"),
            fell_back: false,
            user_data,
        }
    }

    /// Create the tree and the two user facing drop folders.
    pub fn ensure(self) -> Self {
        for dir in [
            &self.user_data,
            &self.profile,
            &self.profile_ui,
            &self.logs,
            &self.downloads,
            &self.extensions,
            &self.diag,
            &self.theme_assets,
            &self.crx,
            &self.scripts,
        ] {
            if let Err(err) = std::fs::create_dir_all(dir) {
                eprintln!("[paths] cannot create {}: {err}", dir.display());
            }
        }
        write_readme(&self.crx, CRX_README);
        write_readme(&self.scripts, SCRIPT_README);
        self
    }
}

const CRX_README: &str = "Drop unpacked MV3 extensions or .crx files here.\n\
Mix loads them on the next start. WebView2 requires an unpacked directory,\n\
so .crx files get unpacked into UserData/extensions/ first.\n";

const SCRIPT_README: &str = "Drop *.user.js files here.\n\
Mix scans this folder on start and injects scripts by their @match rules.\n";

fn write_readme(dir: &Path, body: &str) {
    let file = dir.join("README.txt");
    if !file.exists() {
        let _ = std::fs::write(&file, body);
    }
}

fn is_writable(dir: &Path) -> bool {
    if std::fs::create_dir_all(dir).is_err() {
        return false;
    }
    let probe = dir.join(".write-probe");
    match std::fs::write(&probe, b"ok") {
        Ok(()) => {
            let _ = std::fs::remove_file(&probe);
            true
        }
        Err(_) => false,
    }
}

fn app_root() -> PathBuf {
    #[cfg(target_os = "linux")]
    if let Ok(appimage) = std::env::var("APPIMAGE") {
        if let Some(dir) = Path::new(&appimage).parent() {
            return dir.to_path_buf();
        }
    }

    let exe = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("."));
    let dir = exe
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));

    #[cfg(target_os = "macos")]
    if let Some(bundle) = dir
        .parent()
        .filter(|p| p.extension().is_some_and(|e| e == "app"))
    {
        if let Some(parent) = bundle.parent() {
            return parent.to_path_buf();
        }
    }

    dir
}

fn fallback_data_dir() -> PathBuf {
    #[cfg(windows)]
    if let Ok(base) = std::env::var("LOCALAPPDATA") {
        return PathBuf::from(base).join("Mix");
    }
    #[cfg(target_os = "macos")]
    if let Ok(home) = std::env::var("HOME") {
        return PathBuf::from(home).join("Library/Application Support/Mix");
    }
    #[cfg(target_os = "linux")]
    if let Ok(base) = std::env::var("XDG_DATA_HOME") {
        return PathBuf::from(base).join("mix");
    } else if let Ok(home) = std::env::var("HOME") {
        return PathBuf::from(home).join(".local/share/mix");
    }
    std::env::temp_dir().join("mix")
}
/// Reads one of this app's own json files, tolerating a byte order mark.
///
/// A BOM is what PowerShell's `Set-Content` and a few editors put at the front of a utf-8
/// file, and serde refuses the whole document over it: "expected value at line 1 column 1".
/// The app then treated the file as unreadable and moved it aside, which looks to the reader
/// like their history or settings vanished after they edited it by hand - and it has happened
/// here more than once, on permissions and on custom.css, each time patched at that one call
/// site. One helper, so the next file does not have to remember.
pub fn read_json(path: &Path) -> Option<String> {
    let raw = std::fs::read_to_string(path).ok()?;
    Some(raw.trim_start_matches('\u{feff}').to_string())
}

/// The sibling file a write goes through before it is renamed into place.
///
/// Shared by the buffered and the streaming write so a half-written file always has the same
/// name: if one of them left a differently named partial behind, `write_atomic` would happily
/// rename the other one's leftovers over a good file.
pub fn atomic_tmp(path: &Path) -> PathBuf {
    let suffix = path
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or("dat");
    path.with_extension(format!("{suffix}.tmp"))
}

/// Writes through a sibling temporary file and renames it into place.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let dir = path.parent().unwrap_or_else(|| Path::new("."));
    std::fs::create_dir_all(dir)?;
    let tmp = atomic_tmp(path);
    {
        let mut file = std::fs::File::create(&tmp)?;
        std::io::Write::write_all(&mut file, bytes)?;
        let _ = file.sync_all();
    }
    std::fs::rename(&tmp, path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_byte_order_mark_does_not_make_a_file_unreadable() {
        // PowerShell's Set-Content writes one, and serde refuses a whole document over it -
        // which the app then treated as a corrupt file and moved aside.
        let dir = std::env::temp_dir().join("mix-bom-read");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("with-bom.json");
        std::fs::write(&path, "\u{feff}{\"a\":1}").unwrap();

        let text = read_json(&path).expect("readable");
        assert!(!text.starts_with('\u{feff}'));
        let value: serde_json::Value = serde_json::from_str(&text).expect("parses");
        assert_eq!(value["a"], 1);

        // A file without one is unchanged, and a missing one is None rather than empty.
        let plain = dir.join("plain.json");
        std::fs::write(&plain, "{\"a\":2}").unwrap();
        assert_eq!(read_json(&plain).as_deref(), Some("{\"a\":2}"));
        assert!(read_json(&dir.join("nope.json")).is_none());
    }

    #[test]
    fn a_write_replaces_the_previous_file() {
        let dir = std::env::temp_dir().join("mix-atomic-write");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("state.json");
        write_atomic(&path, b"first").unwrap();
        write_atomic(&path, b"second").unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "second");
        assert!(!path.with_extension("json.tmp").exists());
    }

    #[test]
    fn the_streaming_write_and_the_buffered_one_agree_on_the_temporary_name() {
        // They have to agree: a download streams into this file and then renames it, and if
        // the buffered write used a different name in the same folder, one of them would
        // rename the other's half-written file over a good one.
        let path = std::path::Path::new("pictures.zip");
        assert_eq!(atomic_tmp(path), std::path::Path::new("pictures.zip.tmp"));

        // A name with no extension still gets one, so two writers cannot collide on the
        // bare name itself.
        assert_eq!(
            atomic_tmp(std::path::Path::new("notes")),
            std::path::Path::new("notes.dat.tmp")
        );

        // Only the last extension is the suffix; the rest of the name is kept.
        assert_eq!(
            atomic_tmp(std::path::Path::new("a.tar.gz")),
            std::path::Path::new("a.tar.gz.tmp")
        );
    }
}
