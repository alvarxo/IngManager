use std::path::{Path, PathBuf};
use std::time::SystemTime;

use chrono::{DateTime, Utc};
use tauri::{AppHandle, Emitter, State};
use uuid::Uuid;
use walkdir::WalkDir;

use crate::db;
use crate::error::{AppError, AppResult};
use crate::models::IndexProgress;
use crate::path_security::ensure_under_root;
use crate::state::AppState;

use super::files::infer_category_key;

fn system_time_to_rfc3339(time: SystemTime) -> Option<String> {
    let dt: DateTime<Utc> = time.into();
    Some(dt.to_rfc3339())
}

fn should_skip(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower == ".ingmanager"
        || lower == ".ds_store"
        || lower == "thumbs.db"
        || lower.ends_with(".tmp")
        || lower.ends_with(".bak")
        || lower.starts_with("~$")
        || lower.ends_with(".sv$")
}

#[tauri::command]
pub async fn start_initial_index(app: AppHandle, state: State<'_, AppState>) -> AppResult<()> {
    {
        let mut flag = state.indexing.write().await;
        if *flag {
            return Err(AppError::msg("Ya hay una indexación en curso"));
        }
        *flag = true;
    }

    let pool = state.pool.clone();
    let indexing = state.indexing.clone();
    let root = match db::get_setting(&pool, "root_path").await? {
        Some(r) => PathBuf::from(r),
        None => {
            *indexing.write().await = false;
            return Err(AppError::msg("Configure la carpeta raíz primero"));
        }
    };

    if !root.is_dir() {
        *indexing.write().await = false;
        return Err(AppError::msg("La carpeta raíz no es accesible"));
    }

    let last = db::get_setting(&pool, "last_full_index_at").await?;
    if last.is_some() {
        let _ = app.emit(
            "index-progress",
            IndexProgress {
                phase: "skip".into(),
                scanned: 0,
                total_hint: None,
                current_path: None,
                done: true,
                message: Some(
                    "Índice ya existe; no se re-escanea completo al abrir. Use reindexar si es necesario."
                        .into(),
                ),
            },
        );
        *indexing.write().await = false;
        return Ok(());
    }

    tokio::spawn(async move {
        let result = run_index(&app, &pool, &root).await;
        if let Err(e) = result {
            let _ = app.emit(
                "index-progress",
                IndexProgress {
                    phase: "error".into(),
                    scanned: 0,
                    total_hint: None,
                    current_path: None,
                    done: true,
                    message: Some(e.to_string()),
                },
            );
        }
        *indexing.write().await = false;
    });

    Ok(())
}

#[tauri::command]
pub async fn reindex_root(app: AppHandle, state: State<'_, AppState>) -> AppResult<()> {
    {
        let mut flag = state.indexing.write().await;
        if *flag {
            return Err(AppError::msg("Ya hay una indexación en curso"));
        }
        *flag = true;
    }

    let pool = state.pool.clone();
    let indexing = state.indexing.clone();
    let root = match db::get_setting(&pool, "root_path").await? {
        Some(r) => PathBuf::from(r),
        None => {
            *indexing.write().await = false;
            return Err(AppError::msg("Configure la carpeta raíz primero"));
        }
    };

    // Clear last marker so full scan runs
    let _ = db::set_setting(&pool, "last_full_index_at", "").await;
    sqlx::query("DELETE FROM app_settings WHERE key = 'last_full_index_at'")
        .execute(&pool)
        .await
        .ok();

    tokio::spawn(async move {
        let result = run_index(&app, &pool, &root).await;
        if let Err(e) = result {
            let _ = app.emit(
                "index-progress",
                IndexProgress {
                    phase: "error".into(),
                    scanned: 0,
                    total_hint: None,
                    current_path: None,
                    done: true,
                    message: Some(e.to_string()),
                },
            );
        }
        *indexing.write().await = false;
    });

    Ok(())
}

async fn run_index(app: &AppHandle, pool: &sqlx::SqlitePool, root: &Path) -> AppResult<()> {
    let _ = app.emit(
        "index-progress",
        IndexProgress {
            phase: "scanning".into(),
            scanned: 0,
            total_hint: None,
            current_path: Some(root.display().to_string()),
            done: false,
            message: Some("Escaneando carpeta raíz…".into()),
        },
    );

    // Map project folders (first-level dirs that match project codes)
    let projects: Vec<(String, String)> =
        sqlx::query_as("SELECT id, root_relative_path FROM projects")
            .fetch_all(pool)
            .await?;

    let project_map: std::collections::HashMap<String, String> = projects
        .into_iter()
        .map(|(id, rel)| (rel, id))
        .collect();

    sqlx::query("DELETE FROM indexed_files").execute(pool).await?;

    let mut scanned: u64 = 0;
    let mut batch = Vec::new();

    for entry in WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy();
        if should_skip(&name) {
            continue;
        }

        // Ensure still under root (symlink / escape guard)
        if ensure_under_root(root, path).is_err() {
            continue;
        }

        let rel = match path.strip_prefix(root) {
            Ok(r) => r.to_string_lossy().replace('\\', "/"),
            Err(_) => continue,
        };

        if rel.is_empty() {
            continue;
        }

        let meta = match entry.metadata() {
            Ok(m) => m,
            Err(_) => continue,
        };

        let is_dir = meta.is_dir();
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

        batch.push((
            Uuid::new_v4().to_string(),
            project_id,
            rel.clone(),
            path.display().to_string(),
            name.to_string(),
            extension,
            size,
            modified,
            if is_dir { 1i64 } else { 0 },
            1i64,
            category_key,
        ));

        scanned += 1;
        if scanned % 50 == 0 {
            let _ = app.emit(
                "index-progress",
                IndexProgress {
                    phase: "scanning".into(),
                    scanned,
                    total_hint: None,
                    current_path: Some(rel),
                    done: false,
                    message: None,
                },
            );
        }

        if batch.len() >= 100 {
            flush_batch(pool, &batch).await?;
            batch.clear();
        }
    }

    if !batch.is_empty() {
        flush_batch(pool, &batch).await?;
    }

    db::set_setting(
        pool,
        "last_full_index_at",
        &Utc::now().to_rfc3339(),
    )
    .await?;

    let _ = app.emit(
        "index-progress",
        IndexProgress {
            phase: "done".into(),
            scanned,
            total_hint: Some(scanned),
            current_path: None,
            done: true,
            message: Some(format!("Indexación completa: {scanned} entradas")),
        },
    );

    Ok(())
}

async fn flush_batch(
    pool: &sqlx::SqlitePool,
    batch: &[(
        String,
        Option<String>,
        String,
        String,
        String,
        Option<String>,
        Option<i64>,
        Option<String>,
        i64,
        i64,
        Option<String>,
    )],
) -> AppResult<()> {
    let mut tx = pool.begin().await?;
    for row in batch {
        sqlx::query(
            "INSERT INTO indexed_files (
                id, project_id, relative_path, absolute_path, file_name, extension,
                size_bytes, modified_at, is_directory, is_available, category_key, indexed_at
             ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, datetime('now'))
             ON CONFLICT(relative_path) DO UPDATE SET
                project_id = excluded.project_id,
                absolute_path = excluded.absolute_path,
                file_name = excluded.file_name,
                extension = excluded.extension,
                size_bytes = excluded.size_bytes,
                modified_at = excluded.modified_at,
                is_directory = excluded.is_directory,
                is_available = excluded.is_available,
                category_key = excluded.category_key,
                indexed_at = datetime('now')",
        )
        .bind(&row.0)
        .bind(&row.1)
        .bind(&row.2)
        .bind(&row.3)
        .bind(&row.4)
        .bind(&row.5)
        .bind(&row.6)
        .bind(&row.7)
        .bind(row.8)
        .bind(row.9)
        .bind(&row.10)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(())
}
