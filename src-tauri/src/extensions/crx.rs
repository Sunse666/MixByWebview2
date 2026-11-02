use std::fs::File;
use std::io::Cursor;
use std::path::Path;

/// Extensions are inflated on disk, so refuse anything unreasonable up front.
const MAX_ENTRIES: usize = 10_000;
const MAX_TOTAL_BYTES: u64 = 512 * 1024 * 1024;

pub struct Crx {
    pub zip: Vec<u8>,
}

/// CRX3 = "Cr24" + version + header length + header + zip.
/// CRX2 = "Cr24" + version + pubkey length + signature length + pubkey + signature + zip.
pub fn parse(bytes: &[u8]) -> Result<Crx, String> {
    if bytes.len() < 16 {
        return Err("file is too small to be a crx".into());
    }
    if &bytes[0..4] != b"Cr24" {
        return Err("missing Cr24 magic; unpack the zip manually and drop the folder instead".into());
    }

    let version = u32::from_le_bytes(bytes[4..8].try_into().expect("4 bytes"));
    let start = match version {
        3 => {
            let header_len = u32::from_le_bytes(bytes[8..12].try_into().expect("4 bytes")) as usize;
            12usize.saturating_add(header_len)
        }
        2 => {
            let pubkey_len = u32::from_le_bytes(bytes[8..12].try_into().expect("4 bytes")) as usize;
            let sig_len = u32::from_le_bytes(bytes[12..16].try_into().expect("4 bytes")) as usize;
            16usize
                .saturating_add(pubkey_len)
                .saturating_add(sig_len)
        }
        other => return Err(format!("unsupported crx version {other}")),
    };

    if start >= bytes.len() {
        return Err("crx header points past the end of the file".into());
    }

    Ok(Crx {
        zip: bytes[start..].to_vec(),
    })
}

/// Extract into dest, rejecting traversal, absolute paths and oversized archives.
pub fn extract(zip_bytes: &[u8], dest: &Path) -> Result<(), String> {
    let mut archive = zip::ZipArchive::new(Cursor::new(zip_bytes))
        .map_err(|err| format!("bad zip payload: {err}"))?;

    if archive.len() > MAX_ENTRIES {
        return Err(format!(
            "archive has {} entries, limit is {MAX_ENTRIES}",
            archive.len()
        ));
    }

    let mut total: u64 = 0;
    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .map_err(|err| format!("cannot read archive entry {index}: {err}"))?;

        let Some(relative) = entry.enclosed_name() else {
            return Err(format!("unsafe path in archive: {}", entry.name()));
        };
        let out = dest.join(&relative);
        if !out.starts_with(dest) {
            return Err(format!("entry escapes the target folder: {}", entry.name()));
        }

        if entry.is_dir() {
            std::fs::create_dir_all(&out).map_err(|err| format!("mkdir failed: {err}"))?;
            continue;
        }

        total = total.saturating_add(entry.size());
        if total > MAX_TOTAL_BYTES {
            return Err("archive expands past the 512 MB budget".into());
        }

        if let Some(parent) = out.parent() {
            std::fs::create_dir_all(parent).map_err(|err| format!("mkdir failed: {err}"))?;
        }
        let mut file = File::create(&out).map_err(|err| format!("cannot write {out:?}: {err}"))?;
        std::io::copy(&mut entry, &mut file).map_err(|err| format!("write failed: {err}"))?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use zip::write::SimpleFileOptions;

    fn zip_with(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let mut buffer = Cursor::new(Vec::new());
        {
            let mut writer = zip::ZipWriter::new(&mut buffer);
            let options = SimpleFileOptions::default();
            for (name, body) in entries {
                writer.start_file(*name, options).expect("start_file");
                writer.write_all(body).expect("write");
            }
            writer.finish().expect("finish");
        }
        buffer.into_inner()
    }

    fn crx3(zip: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(b"Cr24");
        out.extend_from_slice(&3u32.to_le_bytes());
        out.extend_from_slice(&4u32.to_le_bytes());
        out.extend_from_slice(&[0u8, 0, 0, 0]);
        out.extend_from_slice(zip);
        out
    }

    fn scratch(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("mix-crx-test-{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create scratch");
        dir
    }

    #[test]
    fn rejects_non_crx_input() {
        assert!(parse(b"PK\x03\x04not a crx file").is_err());
        assert!(parse(b"Cr24").is_err());
    }

    #[test]
    fn strips_the_crx3_header() {
        let zip = zip_with(&[("manifest.json", b"{\"manifest_version\":3}")]);
        let parsed = parse(&crx3(&zip)).expect("parse");
        assert_eq!(parsed.zip, zip);
    }

    #[test]
    fn extracts_files() {
        let dir = scratch("extract");
        let zip = zip_with(&[
            ("manifest.json", b"{\"manifest_version\":3}"),
            ("js/content.js", b"console.log(1)"),
        ]);
        extract(&zip, &dir).expect("extract");
        assert!(dir.join("manifest.json").exists());
        assert!(dir.join("js/content.js").exists());
    }

    #[test]
    fn rejects_parent_traversal() {
        let dir = scratch("traversal");
        let zip = zip_with(&[("../evil.txt", b"nope")]);
        let err = extract(&zip, &dir).expect_err("must reject");
        assert!(err.contains("unsafe") || err.contains("escapes"), "got: {err}");
    }
}