use std::path::{Component, Path, PathBuf};

use crate::error::{AppError, AppResult};

/// Resolve `candidate` and ensure it stays under `authorized_root`.
pub fn ensure_under_root(authorized_root: &Path, candidate: &Path) -> AppResult<PathBuf> {
    let root = canonicalize_existing_or_parent(authorized_root)?;
    let target = if candidate.is_absolute() {
        canonicalize_existing_or_parent(candidate)?
    } else {
        canonicalize_existing_or_parent(&root.join(candidate))?
    };

    if !path_is_under(&root, &target) {
        return Err(AppError::msg(format!(
            "Ruta fuera de la carpeta raíz autorizada: {}",
            target.display()
        )));
    }

    Ok(target)
}

pub fn join_under_root(authorized_root: &Path, relative: &str) -> AppResult<PathBuf> {
    let relative = relative.trim_start_matches(['/', '\\']);
    if relative.is_empty() {
        return ensure_under_root(authorized_root, authorized_root);
    }
    if Path::new(relative)
        .components()
        .any(|c| matches!(c, Component::ParentDir | Component::RootDir | Component::Prefix(_)))
    {
        return Err(AppError::msg(
            "Ruta relativa inválida: no se permiten '..' ni prefijos absolutos",
        ));
    }
    ensure_under_root(authorized_root, &authorized_root.join(relative))
}

pub fn path_is_under(root: &Path, candidate: &Path) -> bool {
    let root_comps: Vec<_> = root.components().collect();
    let cand_comps: Vec<_> = candidate.components().collect();
    if cand_comps.len() < root_comps.len() {
        return false;
    }
    root_comps.iter().zip(cand_comps.iter()).all(|(a, b)| a == b)
}

fn canonicalize_existing_or_parent(path: &Path) -> AppResult<PathBuf> {
    if path.exists() {
        return Ok(dunce_canonicalize(path)?);
    }

    let mut current = path.to_path_buf();
    let mut missing = Vec::new();
    while !current.exists() {
        if let Some(name) = current.file_name() {
            missing.push(name.to_os_string());
        } else {
            break;
        }
        current = current
            .parent()
            .ok_or_else(|| AppError::msg("No se pudo resolver la ruta"))?
            .to_path_buf();
    }

    let mut resolved = dunce_canonicalize(&current)?;
    for part in missing.into_iter().rev() {
        resolved.push(part);
    }
    Ok(resolved)
}

fn dunce_canonicalize(path: &Path) -> AppResult<PathBuf> {
    Ok(std::fs::canonicalize(path)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn accepts_path_under_root() {
        let dir = tempdir().unwrap();
        let root = dir.path();
        let child = root.join("2601").join("02_MODELOS");
        fs::create_dir_all(&child).unwrap();
        let ok = ensure_under_root(root, &child).unwrap();
        assert!(ok.ends_with("02_MODELOS"));
    }

    #[test]
    fn rejects_path_outside_root() {
        let a = tempdir().unwrap();
        let b = tempdir().unwrap();
        let result = ensure_under_root(a.path(), &b.path().join("secret.txt"));
        assert!(result.is_err());
    }

    #[test]
    fn rejects_parent_dir_escape() {
        let dir = tempdir().unwrap();
        let result = join_under_root(dir.path(), "../outside.txt");
        assert!(result.is_err());
    }
}
