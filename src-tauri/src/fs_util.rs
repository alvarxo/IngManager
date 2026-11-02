use std::fs;
use std::path::{Path, PathBuf};

use crate::error::{AppError, AppResult};
use crate::path_security::path_is_under;

const ENGINEERING_EXTS: &[&str] = &[
    "rvt", "rfa", "rte", "rft", // Revit
    "dwg", "dxf", "dwt", // AutoCAD / XREF hosts
    "sdb", "s2k", "$2k", // SAP2000-ish
    "idea", "idea8", "idea10", // IDEA StatiCa
    "nwc", "nwd", "nwf", // Navisworks
    "ifc", "step", "stp",
];

pub fn is_engineering_model(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| {
            let lower = e.to_ascii_lowercase();
            ENGINEERING_EXTS.iter().any(|x| *x == lower)
        })
        .unwrap_or(false)
}

pub fn engineering_warning_message(paths: &[PathBuf]) -> Option<String> {
    let hits: Vec<String> = paths
        .iter()
        .filter(|p| is_engineering_model(p))
        .filter_map(|p| p.file_name().map(|n| n.to_string_lossy().to_string()))
        .collect();
    if hits.is_empty() {
        return None;
    }
    Some(format!(
        "Advertencia: mover/renombrar modelos de ingeniería ({}) no actualiza automáticamente referencias externas (XREF/Revit/SAP/IDEA). Revise enlaces manualmente.",
        hits.join(", ")
    ))
}

pub fn is_protected_system_name(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower == ".ingmanager" || lower == "." || lower == ".."
}

pub fn validate_entry_name(name: &str) -> AppResult<()> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err(AppError::msg("El nombre no puede estar vacío"));
    }
    if trimmed.contains('/') || trimmed.contains('\\') || trimmed.contains('\0') {
        return Err(AppError::msg("El nombre no puede contener separadores de ruta"));
    }
    if trimmed == "." || trimmed == ".." || is_protected_system_name(trimmed) {
        return Err(AppError::msg("Nombre reservado"));
    }
    if trimmed.chars().any(|c| ['<', '>', ':', '"', '|', '?', '*'].contains(&c)) {
        return Err(AppError::msg("El nombre contiene caracteres no permitidos"));
    }
    Ok(())
}

/// Reject moving/copying a directory into itself or a descendant.
pub fn ensure_not_into_self(source: &Path, dest_dir: &Path) -> AppResult<()> {
    if source.is_dir() && (source == dest_dir || path_is_under(source, dest_dir)) {
        return Err(AppError::msg(
            "No se puede mover o copiar una carpeta dentro de sí misma",
        ));
    }
    Ok(())
}

pub fn unique_destination(dest_dir: &Path, file_name: &str) -> PathBuf {
    let candidate = dest_dir.join(file_name);
    if !candidate.exists() {
        return candidate;
    }

    let path = Path::new(file_name);
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| file_name.to_string());
    let ext = path
        .extension()
        .map(|e| format!(".{}", e.to_string_lossy()))
        .unwrap_or_default();

    for i in 1..10_000 {
        let name = format!("{stem} ({i}){ext}");
        let cand = dest_dir.join(&name);
        if !cand.exists() {
            return cand;
        }
    }
    dest_dir.join(format!("{stem}-conflict-{ext}"))
}

pub fn copy_recursively(from: &Path, to: &Path) -> AppResult<()> {
    if from.is_dir() {
        fs::create_dir_all(to)?;
        for entry in fs::read_dir(from)? {
            let entry = entry?;
            let target = to.join(entry.file_name());
            copy_recursively(&entry.path(), &target)?;
        }
    } else {
        if let Some(parent) = to.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::copy(from, to)?;
    }
    Ok(())
}

pub fn move_entry(from: &Path, to: &Path) -> AppResult<()> {
    if let Some(parent) = to.parent() {
        fs::create_dir_all(parent)?;
    }
    match fs::rename(from, to) {
        Ok(()) => Ok(()),
        Err(_) => {
            // Cross-device fallback
            copy_recursively(from, to)?;
            if from.is_dir() {
                fs::remove_dir_all(from)?;
            } else {
                fs::remove_file(from)?;
            }
            Ok(())
        }
    }
}

pub fn trash_root(authorized_root: &Path) -> PathBuf {
    authorized_root.join(".ingmanager").join("trash")
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn detects_engineering_exts() {
        assert!(is_engineering_model(Path::new("a.rvt")));
        assert!(is_engineering_model(Path::new("b.DWG")));
        assert!(!is_engineering_model(Path::new("c.pdf")));
    }

    #[test]
    fn rejects_move_into_self() {
        let dir = tempdir().unwrap();
        let a = dir.path().join("a");
        let nested = a.join("b");
        fs::create_dir_all(&nested).unwrap();
        assert!(ensure_not_into_self(&a, &nested).is_err());
        assert!(ensure_not_into_self(&a, &a).is_err());
        assert!(ensure_not_into_self(&a, dir.path()).is_ok());
    }

    #[test]
    fn unique_names_increment() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("file.txt"), b"x").unwrap();
        let next = unique_destination(dir.path(), "file.txt");
        assert_eq!(
            next.file_name().unwrap().to_string_lossy(),
            "file (1).txt"
        );
    }
}
