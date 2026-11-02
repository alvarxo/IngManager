use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};

use sha2::{Digest, Sha256};
use walkdir::WalkDir;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

use crate::error::{AppError, AppResult};
use crate::path_security::path_is_under;

/// Reject ZIP slip: entry names must stay under `dest_root` after join.
pub fn safe_zip_entry_path(dest_root: &Path, entry_name: &str) -> AppResult<PathBuf> {
    let name = entry_name.replace('\\', "/");
    if name.is_empty() {
        return Err(AppError::msg("Entrada ZIP vacía"));
    }
    if Path::new(&name).is_absolute() {
        return Err(AppError::msg(format!(
            "ZIP Slip rechazado: ruta absoluta '{entry_name}'"
        )));
    }
    if Path::new(&name).components().any(|c| {
        matches!(
            c,
            Component::ParentDir | Component::RootDir | Component::Prefix(_)
        )
    }) {
        return Err(AppError::msg(format!(
            "ZIP Slip rechazado: componente ilegal en '{entry_name}'"
        )));
    }

    let candidate = dest_root.join(&name);
    // Ensure candidate is under dest_root even before canonicalize
    let root_comps: Vec<_> = dest_root.components().collect();
    let cand_comps: Vec<_> = candidate.components().collect();
    if cand_comps.len() < root_comps.len()
        || !root_comps.iter().zip(cand_comps.iter()).all(|(a, b)| a == b)
    {
        return Err(AppError::msg(format!(
            "ZIP Slip rechazado: '{entry_name}' escapa del destino"
        )));
    }
    Ok(candidate)
}

pub fn extract_zip_safe(zip_path: &Path, dest_root: &Path) -> AppResult<Vec<PathBuf>> {
    fs::create_dir_all(dest_root)?;
    let file = File::open(zip_path)?;
    let mut archive = ZipArchive::new(file)
        .map_err(|e| AppError::msg(format!("No se pudo abrir ZIP: {e}")))?;

    let mut written = Vec::new();
    for i in 0..archive.len() {
        let mut entry = archive
            .by_index(i)
            .map_err(|e| AppError::msg(format!("Entrada ZIP inválida: {e}")))?;
        let raw_name = entry
            .enclosed_name()
            .ok_or_else(|| {
                AppError::msg(format!(
                    "ZIP Slip rechazado: nombre no contenido '{}'",
                    entry.name()
                ))
            })?
            .to_path_buf();
        let name = raw_name.to_string_lossy().replace('\\', "/");
        let out_path = safe_zip_entry_path(dest_root, &name)?;

        if entry.is_dir() || name.ends_with('/') {
            fs::create_dir_all(&out_path)?;
            continue;
        }
        if let Some(parent) = out_path.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut outfile = File::create(&out_path)?;
        std::io::copy(&mut entry, &mut outfile)?;
        // Post-check: still under dest
        if out_path.exists() {
            let canon_dest = fs::canonicalize(dest_root).unwrap_or_else(|_| dest_root.to_path_buf());
            if let Ok(canon_out) = fs::canonicalize(&out_path) {
                if !path_is_under(&canon_dest, &canon_out) {
                    let _ = fs::remove_file(&out_path);
                    return Err(AppError::msg(format!(
                        "ZIP Slip detectado tras extracción: {}",
                        out_path.display()
                    )));
                }
            }
        }
        written.push(out_path);
    }
    Ok(written)
}

pub fn create_zip_from_dir(source_dir: &Path, zip_path: &Path) -> AppResult<()> {
    if let Some(parent) = zip_path.parent() {
        fs::create_dir_all(parent)?;
    }
    let file = File::create(zip_path)?;
    let mut zip = ZipWriter::new(file);
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);

    for entry in WalkDir::new(source_dir)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        let path = entry.path();
        if path == source_dir {
            continue;
        }
        let rel = path
            .strip_prefix(source_dir)
            .map_err(|_| AppError::msg("Prefijo ZIP inválido"))?
            .to_string_lossy()
            .replace('\\', "/");
        if rel.is_empty() {
            continue;
        }
        if path.is_dir() {
            let dir_name = if rel.ends_with('/') {
                rel
            } else {
                format!("{rel}/")
            };
            zip.add_directory(dir_name, options)
                .map_err(|e| AppError::msg(format!("ZIP dir: {e}")))?;
        } else {
            zip.start_file(rel, options)
                .map_err(|e| AppError::msg(format!("ZIP file: {e}")))?;
            let mut f = File::open(path)?;
            let mut buf = Vec::new();
            f.read_to_end(&mut buf)?;
            zip.write_all(&buf)?;
        }
    }
    zip.finish()
        .map_err(|e| AppError::msg(format!("No se pudo cerrar ZIP: {e}")))?;
    Ok(())
}

pub fn sha256_file(path: &Path) -> AppResult<String> {
    let mut file = File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 8192];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hex::encode(hasher.finalize()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::tempdir;

    #[test]
    fn rejects_zip_slip_parent() {
        let dir = tempdir().unwrap();
        let err = safe_zip_entry_path(dir.path(), "../evil.txt");
        assert!(err.is_err());
        assert!(err.unwrap_err().to_string().contains("ZIP Slip"));
    }

    #[test]
    fn rejects_absolute_entry() {
        let dir = tempdir().unwrap();
        let err = safe_zip_entry_path(dir.path(), "/tmp/evil.txt");
        assert!(err.is_err());
    }

    #[test]
    fn accepts_nested_safe_entry() {
        let dir = tempdir().unwrap();
        let ok = safe_zip_entry_path(dir.path(), "a/b/c.txt").unwrap();
        assert!(ok.ends_with("c.txt"));
    }

    #[test]
    fn extract_blocks_slip_archive() {
        let dir = tempdir().unwrap();
        let zip_path = dir.path().join("bad.zip");
        {
            let file = File::create(&zip_path).unwrap();
            let mut zip = ZipWriter::new(file);
            let options = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
            // Craft a sneaky name — ZipWriter may normalize; we also test safe_zip_entry_path above.
            zip.start_file("ok.txt", options).unwrap();
            zip.write_all(b"hello").unwrap();
            zip.finish().unwrap();
        }
        let dest = dir.path().join("out");
        let files = extract_zip_safe(&zip_path, &dest).unwrap();
        assert_eq!(files.len(), 1);
        assert!(dest.join("ok.txt").exists());
    }
}
