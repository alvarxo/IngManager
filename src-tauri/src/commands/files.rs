use std::path::{Path, PathBuf};

use tauri::State;

use crate::db;
use crate::error::{AppError, AppResult};
use crate::models::IndexedFile;
use crate::path_security::{ensure_under_root, join_under_root};
use crate::state::AppState;

async fn require_root(state: &AppState) -> AppResult<PathBuf> {
    let root = db::get_setting(&state.pool, "root_path")
        .await?
        .ok_or_else(|| AppError::msg("Configure la carpeta raíz antes de continuar"))?;
    let path = PathBuf::from(&root);
    if !path.is_dir() {
        return Err(AppError::msg(
            "La carpeta raíz configurada no es accesible",
        ));
    }
    Ok(path)
}

#[tauri::command]
pub async fn list_category_files(
    state: State<'_, AppState>,
    project_id: String,
    category_key: String,
) -> AppResult<Vec<IndexedFile>> {
    let files = sqlx::query_as::<_, IndexedFile>(
        "SELECT * FROM indexed_files
         WHERE project_id = ? AND category_key = ? AND is_directory = 0
         ORDER BY file_name COLLATE NOCASE",
    )
    .bind(project_id)
    .bind(category_key)
    .fetch_all(&state.pool)
    .await?;
    Ok(files)
}

#[tauri::command]
pub async fn open_file(state: State<'_, AppState>, absolute_path: String) -> AppResult<()> {
    let root = require_root(&state).await?;
    let path = ensure_under_root(&root, Path::new(&absolute_path))?;
    if !path.is_file() {
        return Err(AppError::msg("El archivo no existe o no es un archivo"));
    }
    open::that(&path).map_err(|e| AppError::msg(format!("No se pudo abrir el archivo: {e}")))?;
    Ok(())
}

#[tauri::command]
pub async fn reveal_in_explorer(
    state: State<'_, AppState>,
    absolute_path: String,
) -> AppResult<()> {
    let root = require_root(&state).await?;
    let path = ensure_under_root(&root, Path::new(&absolute_path))?;
    if !path.exists() {
        return Err(AppError::msg("La ruta no existe"));
    }

    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("explorer")
            .arg("/select,")
            .arg(&path)
            .spawn()
            .map_err(|e| AppError::msg(format!("No se pudo revelar: {e}")))?;
    }

    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg("-R")
            .arg(&path)
            .spawn()
            .map_err(|e| AppError::msg(format!("No se pudo revelar: {e}")))?;
    }

    #[cfg(target_os = "linux")]
    {
        let parent = if path.is_dir() {
            path.clone()
        } else {
            path.parent()
                .ok_or_else(|| AppError::msg("Sin carpeta padre"))?
                .to_path_buf()
        };
        open::that(&parent)
            .map_err(|e| AppError::msg(format!("No se pudo revelar: {e}")))?;
    }

    Ok(())
}

#[tauri::command]
pub async fn open_project_folder(
    state: State<'_, AppState>,
    project_id: String,
) -> AppResult<()> {
    let root = require_root(&state).await?;
    let project: (String,) =
        sqlx::query_as("SELECT root_relative_path FROM projects WHERE id = ?")
            .bind(project_id)
            .fetch_optional(&state.pool)
            .await?
            .ok_or_else(|| AppError::msg("Proyecto no encontrado"))?;

    let path = join_under_root(&root, &project.0)?;
    if !path.is_dir() {
        return Err(AppError::msg("La carpeta del proyecto no existe"));
    }
    open::that(&path).map_err(|e| AppError::msg(format!("No se pudo abrir la carpeta: {e}")))?;
    Ok(())
}

