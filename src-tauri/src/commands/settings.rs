use std::path::PathBuf;

use tauri::State;

use crate::db;
use crate::error::{AppError, AppResult};
use crate::models::AppStatus;
use crate::state::AppState;

#[tauri::command]
pub async fn get_app_status(state: State<'_, AppState>) -> AppResult<AppStatus> {
    let root = db::get_setting(&state.pool, "root_path").await?;
    let root_valid = match &root {
        Some(p) => {
            let path = PathBuf::from(p);
            path.is_dir()
        }
        None => false,
    };

    let project_count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM projects")
        .fetch_one(&state.pool)
        .await?;

    Ok(AppStatus {
        root_path: root,
        root_valid,
        project_count: project_count.0,
        db_path: state.db_path.display().to_string(),
    })
}

#[tauri::command]
pub async fn set_root_path(state: State<'_, AppState>, path: String) -> AppResult<AppStatus> {
    let trimmed = path.trim().to_string();
    if trimmed.is_empty() {
        return Err(AppError::msg("La carpeta raíz no puede estar vacía"));
    }

    let pb = PathBuf::from(&trimmed);
    if !pb.exists() {
        std::fs::create_dir_all(&pb)?;
    }
    if !pb.is_dir() {
        return Err(AppError::msg("La ruta seleccionada no es una carpeta"));
    }

    // Verify write access by creating a marker dir if needed
    let test = pb.join(".ingmanager");
    std::fs::create_dir_all(&test)?;
    let probe = test.join(".write_probe");
    std::fs::write(&probe, b"ok")?;
    let _ = std::fs::remove_file(&probe);

    let canonical = std::fs::canonicalize(&pb)?;
    db::set_setting(&state.pool, "root_path", &canonical.display().to_string()).await?;

    get_app_status(state).await
}

#[tauri::command]
pub async fn get_root_path(state: State<'_, AppState>) -> AppResult<Option<String>> {
    db::get_setting(&state.pool, "root_path").await
}
