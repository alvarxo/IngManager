use std::fs;
use std::path::{Path, PathBuf};

use chrono::Local;
use serde::{Deserialize, Serialize};
use tauri::State;
use walkdir::WalkDir;

use crate::error::{AppError, AppResult};
use crate::fs_util::copy_recursively;
use crate::path_security::ensure_under_root;
use crate::state::AppState;
use crate::zip_safe::create_zip_from_dir;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupResult {
    pub backup_dir: String,
    pub db_snapshot: String,
    pub manifest_path: String,
    pub zip_path: Option<String>,
    pub project_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupManifest {
    pub created_at: String,
    pub app: String,
    pub version: String,
    pub root_path: Option<String>,
    pub projects: Vec<BackupProjectEntry>,
    pub db_file: String,
    pub notes: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupProjectEntry {
    pub id: String,
    pub code: String,
    pub name: String,
    pub relative_path: String,
}

async fn require_root(state: &AppState) -> AppResult<PathBuf> {
    crate::project_paths::require_primary_root_path(&state.pool).await
}

/// Consistent SQLite snapshot via VACUUM INTO (safe while DB is open in WAL mode).
async fn snapshot_db(pool: &sqlx::SqlitePool, dest: &Path) -> AppResult<()> {
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent)?;
    }
    if dest.exists() {
        fs::remove_file(dest)?;
    }
    // VACUUM INTO requires absolute path string
    let dest_str = dest.display().to_string().replace('\'', "''");
    sqlx::query(&format!("VACUUM INTO '{dest_str}'"))
        .execute(pool)
        .await
        .map_err(|e| AppError::msg(format!("VACUUM INTO falló: {e}")))?;
    Ok(())
}

#[tauri::command]
pub async fn create_portable_backup(
    state: State<'_, AppState>,
    destination_dir: String,
    include_files: Option<bool>,
    project_ids: Option<Vec<String>>,
) -> AppResult<BackupResult> {
    let include_files = include_files.unwrap_or(true);
    let root = require_root(&state).await?;
    let dest_base = PathBuf::from(&destination_dir);
    if !dest_base.exists() {
        fs::create_dir_all(&dest_base)?;
    }
    if !dest_base.is_dir() {
        return Err(AppError::msg("El destino de backup no es una carpeta"));
    }

    let stamp = Local::now().format("%Y%m%d-%H%M%S").to_string();
    let backup_dir = dest_base.join(format!("ingmanager-backup-{stamp}"));
    fs::create_dir_all(backup_dir.join("db"))?;
    fs::create_dir_all(backup_dir.join("projects"))?;

    let db_snap = backup_dir.join("db").join("ingmanager.sqlite");
    snapshot_db(&state.pool, &db_snap).await?;

    let projects: Vec<(String, Option<String>, String, String)> = if let Some(ids) = &project_ids {
        if ids.is_empty() {
            vec![]
        } else {
            let mut out = Vec::new();
            for id in ids {
                if let Some(row) = sqlx::query_as::<_, (String, Option<String>, String, String)>(
                    "SELECT id, code, name, root_relative_path FROM projects WHERE id = ?",
                )
                .bind(id)
                .fetch_optional(&state.pool)
                .await?
                {
                    out.push(row);
                }
            }
            out
        }
    } else {
        sqlx::query_as("SELECT id, code, name, root_relative_path FROM projects")
            .fetch_all(&state.pool)
            .await?
    };

    let mut entries = Vec::new();
    if include_files {
        for (id, code, name, rel) in &projects {
            let loc = crate::project_paths::resolve_project_location(&state.pool, id).await?;
            let src = loc.absolute_path;
            let folder_label = code
                .clone()
                .unwrap_or_else(|| loc.root_relative_path.replace('/', "_"));
            if src.is_dir() {
                let dest = backup_dir.join("projects").join(&folder_label);
                copy_recursively(&src, &dest)?;
            }
            entries.push(BackupProjectEntry {
                id: id.clone(),
                code: code.clone().unwrap_or_else(|| folder_label.clone()),
                name: name.clone(),
                relative_path: rel.clone(),
            });
        }
    } else {
        for (id, code, name, rel) in projects {
            entries.push(BackupProjectEntry {
                id,
                code: code.unwrap_or_else(|| rel.clone()),
                name,
                relative_path: rel,
            });
        }
    }

    let manifest = BackupManifest {
        created_at: Local::now().to_rfc3339(),
        app: "IngManager".into(),
        version: "0.1.0".into(),
        root_path: Some(root.display().to_string()),
        projects: entries.clone(),
        db_file: "db/ingmanager.sqlite".into(),
        notes: "Portable backup. DB via VACUUM INTO (consistent). Project files copied when include_files=true.".into(),
    };
    let manifest_path = backup_dir.join("manifest.json");
    fs::write(
        &manifest_path,
        serde_json::to_string_pretty(&manifest).map_err(|e| AppError::msg(e.to_string()))?,
    )?;

    // Optional zip of the whole backup folder next to it
    let zip_path = dest_base.join(format!("ingmanager-backup-{stamp}.zip"));
    create_zip_from_dir(&backup_dir, &zip_path)?;

    crate::audit::log_event(
        &state.pool,
        None,
        "backup",
        None,
        "create_portable_backup",
        Some(&backup_dir.display().to_string()),
    )
    .await
    .ok();

    Ok(BackupResult {
        backup_dir: backup_dir.display().to_string(),
        db_snapshot: db_snap.display().to_string(),
        manifest_path: manifest_path.display().to_string(),
        zip_path: Some(zip_path.display().to_string()),
        project_count: entries.len(),
    })
}

#[tauri::command]
pub async fn list_backup_candidates(
    _state: State<'_, AppState>,
    path: String,
) -> AppResult<Vec<String>> {
    // Helper for UI: list files under a backup dir (path-validated as existing dir only; may be outside root for export destination inspection)
    let p = PathBuf::from(path);
    if !p.is_dir() {
        return Err(AppError::msg("Ruta no es carpeta"));
    }
    let mut out = Vec::new();
    for entry in WalkDir::new(&p).max_depth(2).into_iter().filter_map(|e| e.ok()) {
        if entry.path().is_file() {
            out.push(entry.path().display().to_string());
        }
    }
    Ok(out)
}

/// Ensure destination stays under authorized root when copying project trees into the live workspace.
#[allow(dead_code)]
fn assert_under(root: &Path, candidate: &Path) -> AppResult<()> {
    ensure_under_root(root, candidate).map(|_| ())
}
