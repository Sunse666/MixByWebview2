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
    pub description: String,
    /// data: url of the toolbar icon, empty when the manifest has none.
    pub icon: String,
    /// Relative path of the action popup, empty when there is none.
    pub popup: String,
    pub permissions: Vec<String>,
}

#[derive(Default)]
struct Manifest {
    name: String,
    version: String,
    manifest_version: u32,
    description: String,
    icons: std::collections::HashMap<String, String>,
    action_icon: Option<String>,
    action_icon_map: std::collections::HashMap<String, String>,
    action_popup: String,
    permissions: Vec<String>,
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
        let is_crx = source.extension().and_then(|ext| ext.to_str()) == Some("crx");
        let is_unpacked = source.is_dir() && source.join("manifest.json").is_file();

        if !is_crx && !is_unpacked {
            // Say something: a silently ignored folder looks like a broken feature.
            if source.is_dir() {
                crate::logging::warn(&format!(
                    "extension folder {} has no manifest.json, ignored",
                    source.display()
                ));
            }
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

        let outcome = if is_crx {
            install_crx(&source, &target)
        } else {
            install_dir(&source, &target)
        };

        match outcome {
            Ok((manifest, status, reason)) => {
                crate::logging::info(&format!(
                    "extension {label}: {} v{} ({status})",
                    manifest.name, manifest.version
                ));
                let icon = resolve_icon(&target, &manifest);
                crate::logging::info(&format!(
                    "extension {label}: toolbar icon {} bytes, popup {:?}",
                    icon.len(),
                    manifest.action_popup
                ));
                found.push(ExtInfo {
                    name: manifest.name,
                    version: manifest.version,
                    manifest_version: manifest.manifest_version,
                    dir: target.display().to_string(),
                    source: label,
                    status,
                    reason,
                    description: manifest.description,
                    icon,
                    popup: manifest.action_popup,
                    permissions: manifest.permissions,
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
                    description: String::new(),
                    icon: String::new(),
                    popup: String::new(),
                    permissions: Vec::new(),
                });
            }
        }
    }
    found
}

/// Unpacked folders are used as-is: copy only when the source is newer, so the
/// user can keep editing their own copy.
fn install_dir(source: &Path, target: &Path) -> Result<(Manifest, String, String), String> {
    let manifest = read_manifest(source)?;
    if manifest.manifest_version == 3 {
        if !is_fresh(source, target) {
            copy_tree(source, target)?;
        }
        Ok((manifest, "loaded".into(), String::new()))
    } else {
        let reason = format!(
            "Manifest V{} is not supported by WebView2; convert it to a userscript",
            manifest.manifest_version
        );
        Ok((manifest, "skipped".into(), reason))
    }
}

fn is_fresh(source: &Path, target: &Path) -> bool {
    let source_stamp = std::fs::metadata(source.join("manifest.json"))
        .and_then(|meta| meta.modified())
        .ok();
    let target_stamp = std::fs::metadata(target.join("manifest.json"))
        .and_then(|meta| meta.modified())
        .ok();
    match (source_stamp, target_stamp) {
        (Some(source), Some(target)) => target >= source,
        _ => false,
    }
}

fn copy_tree(source: &Path, target: &Path) -> Result<(), String> {
    if target.exists() {
        std::fs::remove_dir_all(target).map_err(|err| format!("cannot reset target: {err}"))?;
    }
    let mut budget = Budget {
        files: 0,
        bytes: 0,
    };
    copy_dir(source, target, &mut budget, 0)
}

struct Budget {
    files: usize,
    bytes: u64,
}

const MAX_FILES: usize = 10_000;
const MAX_BYTES: u64 = 512 * 1024 * 1024;

fn copy_dir(from: &Path, to: &Path, budget: &mut Budget, depth: usize) -> Result<(), String> {
    if depth > 16 {
        return Err("extension folder is nested too deeply".into());
    }
    std::fs::create_dir_all(to).map_err(|err| format!("mkdir failed: {err}"))?;

    for entry in std::fs::read_dir(from).map_err(|err| format!("read_dir failed: {err}"))?.flatten() {
        let path = entry.path();
        let name = entry.file_name();
        let destination = to.join(&name);
        if path.is_dir() {
            copy_dir(&path, &destination, budget, depth + 1)?;
        } else {
            budget.files += 1;
            let size = entry.metadata().map(|meta| meta.len()).unwrap_or(0);
            budget.bytes = budget.bytes.saturating_add(size);
            if budget.files > MAX_FILES || budget.bytes > MAX_BYTES {
                return Err("extension folder is too large".into());
            }
            std::fs::copy(&path, &destination).map_err(|err| format!("copy failed: {err}"))?;
        }
    }
    Ok(())
}

fn install_crx(source: &Path, target: &Path) -> Result<(Manifest, String, String), String> {
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
    // MV3 uses "action", MV2 used "browser_action"; both may carry an icon.
    let action = json.get("action").or_else(|| json.get("browser_action"));
    let mut action_icon = None;
    let mut action_icon_map = std::collections::HashMap::new();
    let mut action_popup = String::new();
    if let Some(action) = action {
        action_popup = action
            .get("default_popup")
            .and_then(|value| value.as_str())
            .unwrap_or_default()
            .to_string();
        match action.get("default_icon") {
            Some(serde_json::Value::String(path)) => action_icon = Some(path.clone()),
            Some(serde_json::Value::Object(map)) => {
                for (size, path) in map {
                    if let Some(path) = path.as_str() {
                        action_icon_map.insert(size.clone(), path.to_string());
                    }
                }
            }
            _ => {}
        }
    }

    let mut icons = std::collections::HashMap::new();
    if let Some(map) = json.get("icons").and_then(|value| value.as_object()) {
        for (size, path) in map {
            if let Some(path) = path.as_str() {
                icons.insert(size.clone(), path.to_string());
            }
        }
    }

    let permissions = json
        .get("permissions")
        .and_then(|value| value.as_array())
        .map(|list| {
            list.iter()
                .filter_map(|item| item.as_str().map(str::to_string))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

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
        description: json
            .get("description")
            .and_then(|value| value.as_str())
            .filter(|text| !text.starts_with("__MSG_"))
            .unwrap_or_default()
            .to_string(),
        icons,
        action_icon,
        action_icon_map,
        action_popup,
        permissions,
    })
}

/// Picks the biggest declared icon and inlines it as a data url.
fn resolve_icon(dir: &Path, manifest: &Manifest) -> String {
    let candidates: Vec<&String> = manifest
        .action_icon_map
        .values()
        .chain(manifest.icons.values())
        .collect();
    let relative = manifest
        .action_icon
        .as_ref()
        .or_else(|| candidates.first().copied());

    if let Some(relative) = relative {
        if let Some(url) = icon_data_url(dir, relative) {
            return url;
        }
    }
    // Extensions often only ship icons/icon-16.png.
    for guess in ["icons/icon-32.png", "icons/icon-16.png", "icon.png"] {
        if let Some(url) = icon_data_url(dir, guess) {
            return url;
        }
    }
    String::new()
}

fn icon_data_url(dir: &Path, relative: &str) -> Option<String> {
    let path = dir.join(relative.trim_start_matches('/'));
    let bytes = std::fs::read(&path).ok()?;
    if bytes.len() > 256 * 1024 {
        return None;
    }
    let mime = match path.extension().and_then(|ext| ext.to_str()) {
        Some("svg") => "image/svg+xml",
        Some("jpg") | Some("jpeg") => "image/jpeg",
        Some("webp") => "image/webp",
        _ => "image/png",
    };
    Some(format!("data:{mime};base64,{}", base64(&bytes)))
}

fn base64(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = *chunk.get(1).unwrap_or(&0) as u32;
        let b2 = *chunk.get(2).unwrap_or(&0) as u32;
        let triple = (b0 << 16) | (b1 << 8) | b2;
        out.push(TABLE[((triple >> 18) & 0x3F) as usize] as char);
        out.push(TABLE[((triple >> 12) & 0x3F) as usize] as char);
        out.push(if chunk.len() > 1 {
            TABLE[((triple >> 6) & 0x3F) as usize] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            TABLE[(triple & 0x3F) as usize] as char
        } else {
            '='
        });
    }
    out
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