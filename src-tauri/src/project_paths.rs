//! Resolve project physical paths independently of business codes.
//! Multi authorized-root support + path security helpers.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use uuid::Uuid;

use crate::db;
use crate::error::{AppError, AppResult};
use crate::path_security::{ensure_under_root, join_under_root, path_is_under};

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct AuthorizedRoot {
    pub id: String,
    pub absolute_path: String,
    pub label: Option<String>,
    pub is_primary: i64,
    pub created_at: String,
}

#[derive(Debug, Clone)]
pub struct ProjectLocation {
    pub project_id: String,
    pub authorized_root_id: String,
    pub root_path: PathBuf,
    pub root_relative_path: String,
    pub absolute_path: PathBuf,
    pub code: Option<String>,
    /// Prefix for ENV/MOD business codes: code if set, else folder basename.
    pub business_code_prefix: String,
}

/// Sanitize a folder name for use under an authorized root (no path separators).
pub fn sanitize_folder_name(raw: &str) -> AppResult<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(AppError::msg("El nombre de carpeta no puede estar vacío"));
    }
    if trimmed.contains(['/', '\\', ':', '*', '?', '"', '<', '>', '|']) {
        return Err(AppError::msg(
            "El nombre de carpeta contiene caracteres no permitidos",
        ));
    }
    if trimmed == "." || trimmed == ".." || trimmed.starts_with('.') {
        return Err(AppError::msg("Nombre de carpeta inválido"));
    }
    Ok(trimmed.to_string())
}

/// Business-code prefix for document/transmission codes.
pub fn business_code_prefix(code: Option<&str>, root_relative_path: &str) -> String {
    if let Some(c) = code.map(str::trim).filter(|s| !s.is_empty()) {
        return c.to_string();
    }
    root_relative_path
        .split(['/', '\\'])
        .next_back()
        .filter(|s| !s.is_empty())
        .unwrap_or("PRJ")
        .to_string()
}

pub async fn list_authorized_roots(pool: &SqlitePool) -> AppResult<Vec<AuthorizedRoot>> {
    let rows = sqlx::query_as::<_, AuthorizedRoot>(
        "SELECT id, absolute_path, label, is_primary, created_at
         FROM authorized_roots
         WHERE absolute_path != '__UNSET_ROOT__'
         ORDER BY is_primary DESC, created_at ASC",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

pub async fn primary_root(pool: &SqlitePool) -> AppResult<AuthorizedRoot> {
    let roots = list_authorized_roots(pool).await?;
    roots
        .into_iter()
        .find(|r| r.is_primary == 1)
        .or_else(|| None)
        .ok_or_else(|| AppError::msg("Configure al menos una carpeta raíz autorizada"))
}

/// Ensure legacy `root_path` setting and authorized_roots stay aligned after set_root_path.
pub async fn upsert_primary_root(pool: &SqlitePool, absolute_path: &Path) -> AppResult<AuthorizedRoot> {
    let path_str = absolute_path.display().to_string();
    let existing: Option<AuthorizedRoot> = sqlx::query_as(
        "SELECT id, absolute_path, label, is_primary, created_at
         FROM authorized_roots WHERE absolute_path = ?",
    )
    .bind(&path_str)
    .fetch_optional(pool)
    .await?;

    // Clear other primaries
    sqlx::query("UPDATE authorized_roots SET is_primary = 0")
        .execute(pool)
        .await?;

    if let Some(mut row) = existing {
        sqlx::query("UPDATE authorized_roots SET is_primary = 1, label = COALESCE(label, 'Raíz principal') WHERE id = ?")
            .bind(&row.id)
            .execute(pool)
            .await?;
        row.is_primary = 1;
        db::set_setting(pool, "root_path", &path_str).await?;
        return Ok(row);
    }

    // Update legacy placeholder if present
    let placeholder: Option<(String,)> = sqlx::query_as(
        "SELECT id FROM authorized_roots WHERE absolute_path = '__UNSET_ROOT__' LIMIT 1",
    )
    .fetch_optional(pool)
    .await?;

    let id = if let Some((pid,)) = placeholder {
        sqlx::query(
            "UPDATE authorized_roots SET absolute_path = ?, label = 'Raíz principal', is_primary = 1 WHERE id = ?",
        )
        .bind(&path_str)
        .bind(&pid)
        .execute(pool)
        .await?;
        pid
    } else {
        let id = Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT INTO authorized_roots (id, absolute_path, label, is_primary, created_at)
             VALUES (?, ?, 'Raíz principal', 1, datetime('now'))",
        )
        .bind(&id)
        .bind(&path_str)
        .execute(pool)
        .await?;
        id
    };

    db::set_setting(pool, "root_path", &path_str).await?;

    Ok(AuthorizedRoot {
        id,
        absolute_path: path_str,
        label: Some("Raíz principal".into()),
        is_primary: 1,
        created_at: chrono::Utc::now().to_rfc3339(),
    })
}

pub async fn add_authorized_root(
    pool: &SqlitePool,
    absolute_path: &Path,
    label: Option<&str>,
    make_primary: bool,
) -> AppResult<AuthorizedRoot> {
    if !absolute_path.is_dir() {
        return Err(AppError::msg("La ruta autorizada debe ser una carpeta existente"));
    }
    let canonical = std::fs::canonicalize(absolute_path)?;
    let path_str = canonical.display().to_string();

    if let Some(existing) = sqlx::query_as::<_, AuthorizedRoot>(
        "SELECT id, absolute_path, label, is_primary, created_at FROM authorized_roots WHERE absolute_path = ?",
    )
    .bind(&path_str)
    .fetch_optional(pool)
    .await?
    {
        if make_primary && existing.is_primary == 0 {
            return upsert_primary_root(pool, &canonical).await;
        }
        return Ok(existing);
    }

    if make_primary {
        return upsert_primary_root(pool, &canonical).await;
    }

    let id = Uuid::new_v4().to_string();
    let label = label
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| {
            canonical
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| "Raíz".into())
        });

    sqlx::query(
        "INSERT INTO authorized_roots (id, absolute_path, label, is_primary, created_at)
         VALUES (?, ?, ?, 0, datetime('now'))",
    )
    .bind(&id)
    .bind(&path_str)
    .bind(&label)
    .execute(pool)
    .await?;

    Ok(AuthorizedRoot {
        id,
        absolute_path: path_str,
        label: Some(label),
        is_primary: 0,
        created_at: chrono::Utc::now().to_rfc3339(),
    })
}

pub async fn remove_authorized_root(pool: &SqlitePool, root_id: &str) -> AppResult<()> {
    let root: AuthorizedRoot = sqlx::query_as(
        "SELECT id, absolute_path, label, is_primary, created_at FROM authorized_roots WHERE id = ?",
    )
    .bind(root_id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| AppError::msg("Raíz no encontrada"))?;

    if root.is_primary == 1 {
        return Err(AppError::msg(
            "No se puede eliminar la raíz principal. Cambie la raíz principal primero.",
        ));
    }

    let in_use: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM projects WHERE authorized_root_id = ?")
            .bind(root_id)
            .fetch_one(pool)
            .await?;
    if in_use.0 > 0 {
        return Err(AppError::msg(
            "Hay proyectos vinculados a esta raíz; reasigne o elimine esos proyectos primero",
        ));
    }

    sqlx::query("DELETE FROM authorized_roots WHERE id = ?")
        .bind(root_id)
        .execute(pool)
        .await?;
    Ok(())
}

/// Find which authorized root (if any) contains `candidate`.
pub async fn find_root_for_path(
    pool: &SqlitePool,
    candidate: &Path,
) -> AppResult<Option<(AuthorizedRoot, PathBuf, String)>> {
    let roots = list_authorized_roots(pool).await?;
    let cand = if candidate.exists() {
        std::fs::canonicalize(candidate).unwrap_or_else(|_| candidate.to_path_buf())
    } else {
        candidate.to_path_buf()
    };

    // Prefer the deepest (longest) matching root
    let mut best: Option<(AuthorizedRoot, PathBuf, String)> = None;
    for root in roots {
        let root_pb = PathBuf::from(&root.absolute_path);
        let root_canon = std::fs::canonicalize(&root_pb).unwrap_or(root_pb.clone());
        if path_is_under(&root_canon, &cand) {
            let rel = cand
                .strip_prefix(&root_canon)
                .map(|r| r.to_string_lossy().replace('\\', "/"))
                .unwrap_or_default();
            let better = best
                .as_ref()
                .map(|(_, rp, _)| root_canon.as_os_str().len() > rp.as_os_str().len())
                .unwrap_or(true);
            if better {
                best = Some((root, root_canon, rel));
            }
        }
    }
    Ok(best)
}

pub fn ensure_under_any_root<'a>(
    roots: &'a [PathBuf],
    candidate: &Path,
) -> AppResult<(&'a Path, PathBuf)> {
    let mut last_err = AppError::msg("Ruta fuera de las carpetas raíz autorizadas");
    for root in roots {
        match ensure_under_root(root, candidate) {
            Ok(p) => return Ok((root.as_path(), p)),
            Err(e) => last_err = e,
        }
    }
    Err(last_err)
}

pub async fn load_root_paths(pool: &SqlitePool) -> AppResult<Vec<PathBuf>> {
    Ok(list_authorized_roots(pool)
        .await?
        .into_iter()
        .map(|r| PathBuf::from(r.absolute_path))
        .filter(|p| p.is_dir())
        .collect())
}

pub async fn require_primary_root_path(pool: &SqlitePool) -> AppResult<PathBuf> {
    let root = primary_root(pool).await?;
    let path = PathBuf::from(&root.absolute_path);
    if !path.is_dir() {
        return Err(AppError::msg(
            "La carpeta raíz configurada no es accesible",
        ));
    }
    Ok(path)
}

pub async fn resolve_project_location(
    pool: &SqlitePool,
    project_id: &str,
) -> AppResult<ProjectLocation> {
    let row: (String, Option<String>, String, String, String) = sqlx::query_as(
        "SELECT p.id, p.code, p.root_relative_path, p.authorized_root_id, r.absolute_path
         FROM projects p
         JOIN authorized_roots r ON r.id = p.authorized_root_id
         WHERE p.id = ?",
    )
    .bind(project_id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| AppError::msg("Proyecto no encontrado"))?;

    let (id, code, rel, root_id, root_abs) = row;
    let root_path = PathBuf::from(&root_abs);
    if !root_path.is_dir() {
        return Err(AppError::msg(format!(
            "La raíz autorizada del proyecto no es accesible: {root_abs}"
        )));
    }
    let absolute_path = join_under_root(&root_path, &rel)?;
    let prefix = business_code_prefix(code.as_deref(), &rel);
    Ok(ProjectLocation {
        project_id: id,
        authorized_root_id: root_id,
        root_path,
        root_relative_path: rel,
        absolute_path,
        code,
        business_code_prefix: prefix,
    })
}

/// Resolve a path that must live under the project's authorized root.
pub fn ensure_under_project_root(loc: &ProjectLocation, candidate: &Path) -> AppResult<PathBuf> {
    ensure_under_root(&loc.root_path, candidate)
}

pub fn join_under_project(loc: &ProjectLocation, relative_under_project: &str) -> AppResult<PathBuf> {
    let rel = relative_under_project.trim_start_matches(['/', '\\']);
    if rel.is_empty() {
        return Ok(loc.absolute_path.clone());
    }
    let combined = format!("{}/{}", loc.root_relative_path.trim_end_matches('/'), rel);
    join_under_root(&loc.root_path, &combined)
}

/// Relative path from the project's authorized root to `abs`.
pub fn rel_from_project_root(loc: &ProjectLocation, abs: &Path) -> AppResult<String> {
    Ok(abs
        .strip_prefix(&loc.root_path)
        .map_err(|_| AppError::msg("Ruta fuera de la raíz del proyecto"))?
        .to_string_lossy()
        .replace('\\', "/"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefix_prefers_code() {
        assert_eq!(business_code_prefix(Some("2601"), "MyFolder"), "2601");
        assert_eq!(business_code_prefix(None, "Client/Foo"), "Foo");
        assert_eq!(business_code_prefix(Some("  "), "Bar"), "Bar");
    }

    #[test]
    fn sanitize_rejects_separators() {
        assert!(sanitize_folder_name("a/b").is_err());
        assert!(sanitize_folder_name("ok_name").is_ok());
    }
}
