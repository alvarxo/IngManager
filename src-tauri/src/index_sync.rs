use std::path::{Path, PathBuf};
use std::time::SystemTime;

use chrono::{DateTime, Utc};
use sqlx::SqlitePool;
use uuid::Uuid;
use walkdir::WalkDir;

use crate::error::AppResult;
use crate::path_security::ensure_under_root;

pub fn should_skip_name(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower == ".ingmanager"
        || lower == ".ds_store"
        || lower == "thumbs.db"
        || lower.ends_with(".tmp")
        || lower.ends_with(".bak")
        || lower.starts_with("~$")
        || lower.ends_with(".sv$")
}

fn system_time_to_rfc3339(time: SystemTime) -> Option<String> {
    let dt: DateTime<Utc> = time.into();
    Some(dt.to_rfc3339())
}

pub fn infer_category_key(project_relative: &str) -> Option<&'static str> {
    let first = project_relative.split(['/', '\\']).next().unwrap_or("");
    match first {
        "01_ANTECEDENTES" => Some("antecedentes"),
        "02_MODELOS" => Some("modelos"),
        "03_PLANOS" => Some("planos"),
        "04_MEMORIAS" => Some("memorias"),
        "05_ADMINISTRACION" => Some("admin"),
        "06_TRANSMISIONES" => Some("transmisiones"),
        "07_OTROS" => Some("otros"),
        _ => None,
    }
}

async fn project_map(pool: &SqlitePool) -> AppResult<std::collections::HashMap<String, String>> {
    let projects: Vec<(String, String)> =
        sqlx::query_as("SELECT id, root_relative_path FROM projects")
            .fetch_all(pool)
            .await?;
    Ok(projects.into_iter().map(|(id, rel)| (rel, id)).collect())
}

pub async fn upsert_path(pool: &SqlitePool, root: &Path, path: &Path) -> AppResult<()> {
    if ensure_under_root(root, path).is_err() {
        return Ok(());
    }
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    if should_skip_name(&name) {
        return Ok(());
    }

    let rel = match path.strip_prefix(root) {
        Ok(r) => r.to_string_lossy().replace('\\', "/"),
        Err(_) => return Ok(()),
    };
    if rel.is_empty() {
        return Ok(());
    }

    if !path.exists() {
        // Mark unavailable — keep history
        sqlx::query(
            "UPDATE indexed_files SET is_available = 0, indexed_at = datetime('now')
             WHERE relative_path = ?",
        )
        .bind(&rel)
        .execute(pool)
        .await?;
        return Ok(());
    }

    let meta = std::fs::metadata(path)?;
    let is_dir = meta.is_dir();
    let project_map = project_map(pool).await?;
    let project_code = rel.split('/').next().unwrap_or("").to_string();
    let project_id = project_map.get(&project_code).cloned();
    let category_key = if let Some(rest) = rel.strip_prefix(&format!("{project_code}/")) {
        infer_category_key(rest).map(|s| s.to_string())
    } else {
        None
    };
    let extension = if is_dir {
        None
    } else {
        path.extension()
            .map(|e| e.to_string_lossy().to_ascii_lowercase())
    };
    let modified = meta.modified().ok().and_then(system_time_to_rfc3339);
    let size = if is_dir { None } else { Some(meta.len() as i64) };

    sqlx::query(
        "INSERT INTO indexed_files (
            id, project_id, relative_path, absolute_path, file_name, extension,
            size_bytes, modified_at, is_directory, is_available, category_key, indexed_at
         ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, 1, ?, datetime('now'))
         ON CONFLICT(relative_path) DO UPDATE SET
            project_id = excluded.project_id,
            absolute_path = excluded.absolute_path,
            file_name = excluded.file_name,
            extension = excluded.extension,
            size_bytes = excluded.size_bytes,
            modified_at = excluded.modified_at,
            is_directory = excluded.is_directory,
            is_available = 1,
            category_key = excluded.category_key,
            indexed_at = datetime('now')",
    )
    .bind(Uuid::new_v4().to_string())
    .bind(&project_id)
    .bind(&rel)
    .bind(path.display().to_string())
    .bind(&name)
    .bind(&extension)
    .bind(size)
    .bind(&modified)
    .bind(if is_dir { 1i64 } else { 0 })
    .bind(&category_key)
    .execute(pool)
    .await?;

    Ok(())
}

pub async fn remove_or_mark_unavailable(
    pool: &SqlitePool,
    root: &Path,
    absolute_or_rel: &Path,
) -> AppResult<()> {
    let abs = if absolute_or_rel.is_absolute() {
        absolute_or_rel.to_path_buf()
    } else {
        root.join(absolute_or_rel)
    };
    let rel = match abs.strip_prefix(root) {
        Ok(r) => r.to_string_lossy().replace('\\', "/"),
        Err(_) => return Ok(()),
    };
    if abs.exists() {
        upsert_path(pool, root, &abs).await?;
    } else {
        sqlx::query(
            "UPDATE indexed_files SET is_available = 0, indexed_at = datetime('now')
             WHERE relative_path = ? OR relative_path LIKE ?",
        )
        .bind(&rel)
        .bind(format!("{rel}/%"))
        .execute(pool)
        .await?;
    }
    Ok(())
}

pub async fn refresh_subtree(pool: &SqlitePool, root: &Path, dir: &Path) -> AppResult<()> {
    if !dir.exists() {
        remove_or_mark_unavailable(pool, root, dir).await?;
        return Ok(());
    }
    upsert_path(pool, root, dir).await?;
    for entry in WalkDir::new(dir)
        .follow_links(false)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy();
        if should_skip_name(&name) {
            continue;
        }
        if ensure_under_root(root, path).is_err() {
            continue;
        }
        upsert_path(pool, root, path).await?;
    }
    Ok(())
}

pub async fn reconcile_paths(pool: &SqlitePool, root: &Path, paths: &[PathBuf]) -> AppResult<()> {
    for path in paths {
        if path.exists() {
            if path.is_dir() {
                refresh_subtree(pool, root, path).await?;
            } else {
                upsert_path(pool, root, path).await?;
            }
        } else {
            remove_or_mark_unavailable(pool, root, path).await?;
        }
    }
    Ok(())
}

pub async fn log_file_operation(
    pool: &SqlitePool,
    project_id: Option<&str>,
    operation: &str,
    source: Option<&str>,
    target: Option<&str>,
    status: &str,
    detail: Option<&str>,
) -> AppResult<String> {
    let id = Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO file_operations (
            id, project_id, operation, source_path, target_path, status, detail, created_at, completed_at
         ) VALUES (?, ?, ?, ?, ?, ?, ?, datetime('now'),
            CASE WHEN ? IN ('completed', 'failed') THEN datetime('now') ELSE NULL END)",
    )
    .bind(&id)
    .bind(project_id)
    .bind(operation)
    .bind(source)
    .bind(target)
    .bind(status)
    .bind(detail)
    .bind(status)
    .execute(pool)
    .await?;
    Ok(id)
}
