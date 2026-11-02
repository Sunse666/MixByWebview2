use std::path::{Path, PathBuf};

/// All writable state lives in <app root>/UserData. Extensions and userscripts
/// are user-managed drop folders next to the binary (see DEV-PLAN 3.8).
#[derive(Debug, Clone)]
pub struct AppDirs {
    pub root: PathBuf,
    pub user_data: PathBuf,
    pub profile: PathBuf,
    pub logs: PathBuf,
    pub downloads: PathBuf,
    pub extensions: PathBuf,
    pub diag: PathBuf,
    pub theme_assets: PathBuf,
    pub crx: PathBuf,
    pub scripts: PathBuf,
    pub settings_file: PathBuf,
    pub theme_file: PathBuf,
    pub session_file: PathBuf,
    pub fell_back: bool,
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
            logs: user_data.join("logs"),
            downloads: user_data.join("downloads"),
            extensions: user_data.join("extensions"),
            diag: user_data.join("diag"),
            theme_assets: user_data.join("theme"),
            settings_file: user_data.join("settings.json"),
            theme_file: user_data.join("theme.json"),
            session_file: user_data.join("session.json"),
            crx: root.join("crx"),
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

    // A .app bundle is not writable, its parent is.
    #[cfg(target_os = "macos")]
    if let Some(bundle) = dir.parent().filter(|p| p.extension().is_some_and(|e| e == "app")) {
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
