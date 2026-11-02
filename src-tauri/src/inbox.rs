//! Global inbox: staging area under primary root `.ingmanager/inbox`.
//! Default assign mode = copy; supports multi-project assignment.

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use uuid::Uuid;

use crate::category_mapping::map_folder_name_to_key;
use crate::error::{AppError, AppResult};
use crate::models::STANDARD_FOLDERS;
use crate::path_security::ensure_under_root;
use crate::project_paths;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct InboxItem {
    pub id: String,
    pub absolute_path: String,
    pub file_name: String,
    pub extension: Option<String>,
    pub size_bytes: Option<i64>,
    pub modified_at: Option<String>,
    pub staged_at: String,
    pub status: String,
    pub notes: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InboxAssignInput {
    pub item_ids: Vec<String>,
    pub project_ids: Vec<String>,
    pub category_key: String,
    /// Default "copy". "move" removes from inbox after all assignments.
    pub mode: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InboxAssignResult {
    pub assigned: usize,
    pub failures: Vec<String>,
    pub dest_paths: Vec<String>,
}

pub async fn inbox_dir(pool: &SqlitePool) -> AppResult<PathBuf> {
    let root = project_paths::require_primary_root_path(pool).await?;
    let dir = root.join(".ingmanager").join("inbox");
    fs::create_dir_all(&dir)?;
    Ok(dir)
}

pub async fn ensure_inbox(pool: &SqlitePool) -> AppResult<String> {
    let dir = inbox_dir(pool).await?;
    Ok(dir.display().to_string())
}

/// Stage files into the inbox (copy by default from outside; re-link if already inside).
pub async fn stage_paths(
    pool: &SqlitePool,
    paths: &[String],
    copy_into: bool,
) -> AppResult<Vec<InboxItem>> {
    let dir = inbox_dir(pool).await?;
    let root = project_paths::require_primary_root_path(pool).await?;
    let mut staged = Vec::new();

    for raw in paths {
        let src = PathBuf::from(raw);
        if !src.is_file() {
            continue;
        }
        let file_name = src
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "file".into());
        let dest = if src.starts_with(&dir) {
            src.clone()
        } else if copy_into {
            let dest = unique_path(&dir, &file_name);
            fs::copy(&src, &dest)?;
            dest
        } else {
            // Require source under an authorized root for non-copy link
            let authorized = project_paths::list_authorized_roots(pool).await?;
            let ok = authorized.iter().any(|r| {
                let rp = PathBuf::from(&r.absolute_path);
                src.starts_with(&rp)
            });
            if !ok {
                // Fall back to copy into inbox (DnD from Desktop etc.)
                let dest = unique_path(&dir, &file_name);
                fs::copy(&src, &dest)?;
                dest
            } else {
                src
            }
        };

        // Prefer inbox-local path under primary root for DB uniqueness
        let abs = if dest.starts_with(&root) {
            dest
        } else {
            let d = unique_path(&dir, &file_name);
            fs::copy(&dest, &d)?;
            d
        };

        let item = upsert_item(pool, &abs).await?;
        staged.push(item);
    }
    Ok(staged)
}

fn unique_path(dir: &Path, file_name: &str) -> PathBuf {
    let candidate = dir.join(file_name);
    if !candidate.exists() {
        return candidate;
    }
    crate::fs_util::unique_destination(dir, file_name)
}

async fn upsert_item(pool: &SqlitePool, abs: &Path) -> AppResult<InboxItem> {
    let abs_str = abs.display().to_string();
    let existing: Option<InboxItem> = sqlx::query_as(
        "SELECT id, absolute_path, file_name, extension, size_bytes, modified_at,
                staged_at, status, notes
         FROM inbox_items WHERE absolute_path = ?",
    )
    .bind(&abs_str)
    .fetch_optional(pool)
    .await?;
    if let Some(mut item) = existing {
        if item.status != "staged" {
            sqlx::query(
                "UPDATE inbox_items SET status = 'staged', staged_at = datetime('now') WHERE id = ?",
            )
            .bind(&item.id)
            .execute(pool)
            .await?;
            item.status = "staged".into();
        }
        return Ok(item);
    }

    let meta = fs::metadata(abs)?;
    let file_name = abs
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    let extension = abs
        .extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase());
    let modified = meta.modified().ok().and_then(|t| {
        let dt: chrono::DateTime<chrono::Utc> = t.into();
        Some(dt.to_rfc3339())
    });
    let id = Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO inbox_items (
            id, absolute_path, file_name, extension, size_bytes, modified_at,
            staged_at, status
         ) VALUES (?, ?, ?, ?, ?, ?, datetime('now'), 'staged')",
    )
    .bind(&id)
    .bind(&abs_str)
    .bind(&file_name)
    .bind(&extension)
    .bind(meta.len() as i64)
    .bind(&modified)
    .execute(pool)
    .await?;

    sqlx::query_as::<_, InboxItem>(
        "SELECT id, absolute_path, file_name, extension, size_bytes, modified_at,
                staged_at, status, notes
         FROM inbox_items WHERE id = ?",
    )
    .bind(id)
    .fetch_one(pool)
    .await
    .map_err(AppError::from)
}

pub async fn list_items(pool: &SqlitePool) -> AppResult<Vec<InboxItem>> {
    // Reconcile FS: drop DB rows whose files vanished; pick up new files in inbox dir
    let dir = inbox_dir(pool).await?;
    if let Ok(rd) = fs::read_dir(&dir) {
        for entry in rd.flatten() {
            if entry.path().is_file() {
                let _ = upsert_item(pool, &entry.path()).await;
            }
        }
    }

    let items = sqlx::query_as::<_, InboxItem>(
        "SELECT id, absolute_path, file_name, extension, size_bytes, modified_at,
                staged_at, status, notes
         FROM inbox_items
         WHERE status = 'staged'
         ORDER BY staged_at DESC",
    )
    .fetch_all(pool)
    .await?;

    let mut live = Vec::new();
    for item in items {
        if Path::new(&item.absolute_path).is_file() {
            live.push(item);
        } else {
            sqlx::query("UPDATE inbox_items SET status = 'removed' WHERE id = ?")
                .bind(&item.id)
                .execute(pool)
                .await
                .ok();
        }
    }
    Ok(live)
}

pub async fn remove_items(pool: &SqlitePool, ids: &[String], delete_files: bool) -> AppResult<usize> {
    let mut n = 0usize;
    for id in ids {
        let row: Option<(String,)> =
            sqlx::query_as("SELECT absolute_path FROM inbox_items WHERE id = ?")
                .bind(id)
                .fetch_optional(pool)
                .await?;
        if let Some((abs,)) = row {
            if delete_files {
                let path = PathBuf::from(&abs);
                let dir = inbox_dir(pool).await?;
                if path.starts_with(&dir) && path.is_file() {
                    let _ = fs::remove_file(&path);
                }
            }
            sqlx::query("UPDATE inbox_items SET status = 'removed' WHERE id = ?")
                .bind(id)
                .execute(pool)
                .await?;
            n += 1;
        }
    }
    Ok(n)
}

fn category_folder_name(pool_mapped: Option<String>, category_key: &str) -> String {
    if let Some(m) = pool_mapped {
        if !m.is_empty() {
            return m;
        }
    }
    STANDARD_FOLDERS
        .iter()
        .find(|f| f.key == category_key)
        .map(|f| f.relative_path.to_string())
        .unwrap_or_else(|| category_key.to_string())
}

pub async fn assign(pool: &SqlitePool, input: InboxAssignInput) -> AppResult<InboxAssignResult> {
    if input.item_ids.is_empty() {
        return Err(AppError::msg("No hay elementos para asignar"));
    }
    if input.project_ids.is_empty() {
        return Err(AppError::msg("Seleccione al menos un proyecto"));
    }
    let cat = input.category_key.trim();
    if cat.is_empty() {
        return Err(AppError::msg("Categoría requerida"));
    }
    // Validate category key
    if STANDARD_FOLDERS.iter().all(|f| f.key != cat) && map_folder_name_to_key(cat).is_none() {
        return Err(AppError::msg(format!("Categoría desconocida: {cat}")));
    }
    let mode = input.mode.as_deref().unwrap_or("copy");
    if mode != "copy" && mode != "move" {
        return Err(AppError::msg("Modo debe ser copy o move"));
    }

    let mut assigned = 0usize;
    let mut failures = Vec::new();
    let mut dest_paths = Vec::new();

    for item_id in &input.item_ids {
        let item: Option<InboxItem> = sqlx::query_as(
            "SELECT id, absolute_path, file_name, extension, size_bytes, modified_at,
                    staged_at, status, notes
             FROM inbox_items WHERE id = ?",
        )
        .bind(item_id)
        .fetch_optional(pool)
        .await?;
        let Some(item) = item else {
            failures.push(format!("Ítem {item_id} no encontrado"));
            continue;
        };
        let src = PathBuf::from(&item.absolute_path);
        if !src.is_file() {
            failures.push(format!("{} ya no existe", item.file_name));
            continue;
        }

        for project_id in &input.project_ids {
            match assign_one(pool, &item, &src, project_id, cat, mode).await {
                Ok(dest) => {
                    assigned += 1;
                    dest_paths.push(dest);
                }
                Err(e) => failures.push(format!(
                    "{} → {}: {}",
                    item.file_name, project_id, e
                )),
            }
        }

        if mode == "move" && failures.is_empty() {
            let dir = inbox_dir(pool).await?;
            if src.starts_with(&dir) {
                let _ = fs::remove_file(&src);
            }
            sqlx::query("UPDATE inbox_items SET status = 'assigned' WHERE id = ?")
                .bind(item_id)
                .execute(pool)
                .await?;
        } else if mode == "copy" {
            // Keep staged for further multi-assign; mark note
            sqlx::query(
                "UPDATE inbox_items SET notes = COALESCE(notes, '') || ' assigned' WHERE id = ?",
            )
            .bind(item_id)
            .execute(pool)
            .await
            .ok();
        }
    }

    Ok(InboxAssignResult {
        assigned,
        failures,
        dest_paths,
    })
}

async fn assign_one(
    pool: &SqlitePool,
    item: &InboxItem,
    src: &Path,
    project_id: &str,
    category_key: &str,
    mode: &str,
) -> AppResult<String> {
    let loc = project_paths::resolve_project_location(pool, project_id).await?;
    let mapped: Option<(String,)> = sqlx::query_as(
        "SELECT physical_relative_path FROM project_folder_mappings
         WHERE project_id = ? AND category_key = ?",
    )
    .bind(project_id)
    .bind(category_key)
    .fetch_optional(pool)
    .await?;

    let cat_rel = if let Some((p,)) = mapped {
        p
    } else {
        let from_cat: Option<(String,)> = sqlx::query_as(
            "SELECT relative_path FROM categories WHERE project_id = ? AND key = ?",
        )
        .bind(project_id)
        .bind(category_key)
        .fetch_optional(pool)
        .await?;
        category_folder_name(from_cat.map(|r| r.0), category_key)
    };

    let dest_dir = loc.absolute_path.join(&cat_rel);
    fs::create_dir_all(&dest_dir)?;
    // Safety: dest must stay under project root
    let _ = ensure_under_root(&loc.absolute_path, &dest_dir)?;
    let dest = unique_path(&dest_dir, &item.file_name);
    fs::copy(src, &dest)?;

    let assign_id = Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO inbox_assignments (
            id, inbox_item_id, project_id, category_key, mode, dest_absolute_path, created_at
         ) VALUES (?, ?, ?, ?, ?, ?, datetime('now'))",
    )
    .bind(&assign_id)
    .bind(&item.id)
    .bind(project_id)
    .bind(category_key)
    .bind(mode)
    .bind(dest.display().to_string())
    .execute(pool)
    .await?;

    Ok(dest.display().to_string())
}
