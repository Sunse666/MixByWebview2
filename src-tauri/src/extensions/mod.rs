pub mod crx;

use crate::paths::AppDirs;
use serde::Serialize;
use std::path::Path;

#[derive(Debug, Clone, Serialize)]
pub struct ExtInfo {
    pub name: String,
    pub version: String,
    pub manifest_version: u32,
    /// Where the unpacked copy lives.
    pub dir: String,
    /// Original file name in crx/.
    pub source: String,
    /// loaded | skipped | error
    pub status: String,
    pub reason: String,
}

struct Manifest {
    name: String,
    version: String,
    manifest_version: u32,
}

/// Unpack every crx in the drop folder. Unpacked copies are a cache: they are
/// rebuilt whenever the source file is newer.
pub fn scan(dirs: &AppDirs) -> Vec<ExtInfo> {
    let Ok(entries) = std::fs::read_dir(&dirs.crx) else {
        return Vec::new();
    };

    let mut found = Vec::new();
    for entry in entries.flatten() {
        let source = entry.path();
        if source.extension().and_then(|ext| ext.to_str()) != Some("crx") {
            continue;
        }
        let label = source
            .file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or("extension")
            .to_string();
        let target = dirs.extensions.join(slug(&label));
        // A name that slugifies to nothing must never resolve to the shared root.
        if target == dirs.extensions {
            crate::logging::warn(&format!("extension {label}: unusable file name, skipped"));
            continue;
        }

        match install(&source, &target) {
            Ok((manifest, status, reason)) => {
                crate::logging::info(&format!(
                    "extension {label}: {} v{} ({status})",
                    manifest.name, manifest.version
                ));
                found.push(ExtInfo {
                    name: manifest.name,
                    version: manifest.version,
                    manifest_version: manifest.manifest_version,
                    dir: target.display().to_string(),
                    source: label,
                    status,
                    reason,
                });
            }
            Err(err) => {
                crate::logging::warn(&format!("extension {label} failed: {err}"));
                found.push(ExtInfo {
                    name: label.clone(),
                    version: String::new(),
                    manifest_version: 0,
                    dir: target.display().to_string(),
                    source: label,
                    status: "error".into(),
                    reason: err,
                });
            }
        }
    }
    found
}

fn install(source: &Path, target: &Path) -> Result<(Manifest, String, String), String> {
    let bytes = std::fs::read(source).map_err(|err| format!("cannot read file: {err}"))?;
    let parsed = crx::parse(&bytes)?;

    if target.exists() {
        std::fs::remove_dir_all(target).map_err(|err| format!("cannot reset target: {err}"))?;
    }
    std::fs::create_dir_all(target).map_err(|err| format!("cannot create target: {err}"))?;
    crx::extract(&parsed.zip, target)?;

    let manifest = read_manifest(target)?;
    if manifest.manifest_version == 3 {
        Ok((manifest, "loaded".into(), String::new()))
    } else {
        let reason = format!(
            "Manifest V{} is not supported by WebView2; convert it to a userscript",
            manifest.manifest_version
        );
        Ok((manifest, "skipped".into(), reason))
    }
}

fn read_manifest(dir: &Path) -> Result<Manifest, String> {
    let path = dir.join("manifest.json");
    let raw = std::fs::read_to_string(&path).map_err(|err| format!("no manifest.json: {err}"))?;
    // Some editors and PowerShell write a UTF-8 BOM; serde_json chokes on it.
    let raw = raw.trim_start_matches('\u{feff}');
    let json: serde_json::Value =
        serde_json::from_str(&raw).map_err(|err| format!("manifest.json is not valid JSON: {err}"))?;

    let manifest_version = json
        .get("manifest_version")
        .and_then(|value| value.as_u64())
        .unwrap_or(2) as u32;

    // Localized names ("__MSG_appName__") are not resolved here; fall back to the folder.
    let mut name = json
        .get("name")
        .and_then(|value| value.as_str())
        .unwrap_or_default()
        .to_string();
    if name.is_empty() || name.starts_with("__MSG_") {
        name = dir
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("extension")
            .to_string();
    }

    let version = json
        .get("version")
        .and_then(|value| value.as_str())
        .unwrap_or("0")
        .to_string();

    Ok(Manifest {
        name,
        version,
        manifest_version,
    })
}

/// Folder name for an unpacked extension: alphanumerics, dash and underscore only,
/// so a crafted file name can never climb out of the extensions folder.
fn slug(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for ch in input.chars() {
        if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_') {
            out.push(ch);
        } else {
            out.push('-');
        }
    }
    out.truncate(64);
    if out.trim_matches('-').is_empty() {
        "extension".into()
    } else {
        out
    }
}

#[cfg(test)]
mod tests {
    use super::slug;

    #[test]
    fn slug_keeps_safe_characters_only() {
        assert_eq!(slug("uBlock-Origin_1.2.3"), "uBlock-Origin_1-2-3");
        assert_eq!(slug("../etc/passwd"), "---etc-passwd");
        assert_eq!(slug(""), "extension");
        assert_eq!(slug("."), "extension");
        assert_eq!(slug(".."), "extension");
    }
}