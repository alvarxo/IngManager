use std::path::PathBuf;

use tauri::State;

use crate::db;
use crate::error::{AppError, AppResult};
use crate::models::AppStatus;
use crate::project_paths::{self, AuthorizedRoot};
use crate::state::AppState;

#[tauri::command]
pub async fn get_app_status(state: State<'_, AppState>) -> AppResult<AppStatus> {
    let roots = project_paths::list_authorized_roots(&state.pool).await?;
    let root = roots.iter().find(|r| r.is_primary == 1).cloned();
    let root_path = root.as_ref().map(|r| r.absolute_path.clone()).or(
        db::get_setting(&state.pool, "root_path").await?,
    );
    let root_valid = match &root_path {
        Some(p) => PathBuf::from(p).is_dir(),
        None => false,
    };

    let project_count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM projects")
        .fetch_one(&state.pool)
        .await?;

    Ok(AppStatus {
        root_path,
        root_valid,
        project_count: project_count.0,
        db_path: state.db_path.display().to_string(),
        authorized_roots: roots,
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
    project_paths::upsert_primary_root(&state.pool, &canonical).await?;

    get_app_status(state).await
}

#[tauri::command]
pub async fn get_root_path(state: State<'_, AppState>) -> AppResult<Option<String>> {
    let roots = project_paths::list_authorized_roots(&state.pool).await?;
    Ok(roots
        .into_iter()
        .find(|r| r.is_primary == 1)
        .map(|r| r.absolute_path)
        .or(db::get_setting(&state.pool, "root_path").await?))
}

#[tauri::command]
pub async fn list_authorized_roots(
    state: State<'_, AppState>,
) -> AppResult<Vec<AuthorizedRoot>> {
    project_paths::list_authorized_roots(&state.pool).await
}

#[tauri::command]
pub async fn add_authorized_root(
    state: State<'_, AppState>,
    path: String,
    label: Option<String>,
    make_primary: Option<bool>,
) -> AppResult<AuthorizedRoot> {
    let pb = PathBuf::from(path.trim());
    if !pb.exists() {
        std::fs::create_dir_all(&pb)?;
    }
    let test = pb.join(".ingmanager");
    std::fs::create_dir_all(&test)?;
    let probe = test.join(".write_probe");
    std::fs::write(&probe, b"ok")?;
    let _ = std::fs::remove_file(&probe);

    project_paths::add_authorized_root(
        &state.pool,
        &pb,
        label.as_deref(),
        make_primary.unwrap_or(false),
    )
    .await
}

#[tauri::command]
pub async fn remove_authorized_root(
    state: State<'_, AppState>,
    root_id: String,
) -> AppResult<()> {
    project_paths::remove_authorized_root(&state.pool, &root_id).await
}
