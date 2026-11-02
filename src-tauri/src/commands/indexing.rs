use std::path::PathBuf;
use std::time::SystemTime;

use chrono::{DateTime, Utc};
use tauri::{AppHandle, Emitter, State};
use uuid::Uuid;
use walkdir::WalkDir;

use crate::category_mapping::infer_category_key_flexible;
use crate::db;
use crate::error::{AppError, AppResult};
use crate::models::IndexProgress;
use crate::path_security::ensure_under_root;
use crate::project_paths;
use crate::state::AppState;

use crate::index_sync;

fn system_time_to_rfc3339(time: SystemTime) -> Option<String> {
    let dt: DateTime<Utc> = time.into();
    Some(dt.to_rfc3339())
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
    let roots = match project_paths::load_root_paths(&pool).await {
        Ok(r) if !r.is_empty() => r,
        _ => {
            *indexing.write().await = false;
            return Err(AppError::msg("Configure la carpeta raíz primero"));
        }
    };

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
        let result = run_index_all(&app, &pool, &roots).await;
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
    let roots = match project_paths::load_root_paths(&pool).await {
        Ok(r) if !r.is_empty() => r,
        _ => {
            *indexing.write().await = false;
            return Err(AppError::msg("Configure la carpeta raíz primero"));
        }
    };

    let _ = db::set_setting(&pool, "last_full_index_at", "").await;
    sqlx::query("DELETE FROM app_settings WHERE key = 'last_full_index_at'")
        .execute(&pool)
        .await
        .ok();

    tokio::spawn(async move {
        let result = run_index_all(&app, &pool, &roots).await;
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

async fn run_index_all(
    app: &AppHandle,
    pool: &sqlx::SqlitePool,
    roots: &[PathBuf],
) -> AppResult<()> {
    let auth_roots = project_paths::list_authorized_roots(pool).await?;
    let projects: Vec<(String, String, String)> =
        sqlx::query_as("SELECT id, authorized_root_id, root_relative_path FROM projects")
            .fetch_all(pool)
            .await?;

    sqlx::query("DELETE FROM indexed_files").execute(pool).await?;

    let mut scanned: u64 = 0;
    let mut batch = Vec::new();

    for root_row in &auth_roots {
        let root = PathBuf::from(&root_row.absolute_path);
        if !root.is_dir() {
            continue;
        }
        let _ = app.emit(
            "index-progress",
            IndexProgress {
                phase: "scanning".into(),
                scanned,
                total_hint: None,
                current_path: Some(root.display().to_string()),
                done: false,
                message: Some(format!("Escaneando raíz {}…", root.display())),
            },
        );

        for entry in WalkDir::new(&root)
            .follow_links(false)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy();
            if index_sync::should_skip_name(&name) {
                continue;
            }
            if ensure_under_root(&root, path).is_err() {
                continue;
            }
            let rel = match path.strip_prefix(&root) {
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

            let project = projects.iter().filter(|(_, rid, prel)| {
                *rid == root_row.id && (rel == *prel || rel.starts_with(&format!("{prel}/")))
            }).max_by_key(|(_, _, prel)| prel.len());

            let project_id = project.map(|(id, _, _)| id.clone());
            let category_key = project.and_then(|(_, _, prel)| {
                let rest = rel.strip_prefix(prel).map(|s| s.trim_start_matches('/'))?;
                if rest.is_empty() {
                    None
                } else {
                    infer_category_key_flexible(rest).map(|s| s.to_string())
                }
            });

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
                root_row.id.clone(),
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
    }

    // Also touch unused `roots` param for API symmetry / future filters
    let _ = roots;

    if !batch.is_empty() {
        flush_batch(pool, &batch).await?;
    }

    db::set_setting(pool, "last_full_index_at", &Utc::now().to_rfc3339())
        .await?;

    let _ = app.emit(
        "index-progress",
        IndexProgress {
            phase: "fts".into(),
            scanned,
            total_hint: Some(scanned),
            current_path: None,
            done: false,
            message: Some("Reconstruyendo índice de búsqueda…".into()),
        },
    );
    let fts_count = crate::search_index::rebuild_all(pool).await.unwrap_or(0);

    let _ = app.emit(
        "index-progress",
        IndexProgress {
            phase: "done".into(),
            scanned,
            total_hint: Some(scanned),
            current_path: None,
            done: true,
            message: Some(format!(
                "Indexación completa: {scanned} entradas · FTS {fts_count} filas"
            )),
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
                id, project_id, authorized_root_id, relative_path, absolute_path, file_name, extension,
                size_bytes, modified_at, is_directory, is_available, category_key, indexed_at
             ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, datetime('now'))
             ON CONFLICT(authorized_root_id, relative_path) DO UPDATE SET
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
        .bind(&row.8)
        .bind(row.9)
        .bind(row.10)
        .bind(&row.11)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(())
}
