use std::path::PathBuf;

use tauri::{AppHandle, State};

use crate::db;
use crate::error::{AppError, AppResult};
use crate::state::AppState;
use crate::watcher;

#[tauri::command]
pub async fn start_file_watcher(app: AppHandle, state: State<'_, AppState>) -> AppResult<()> {
    let root = db::get_setting(&state.pool, "root_path")
        .await?
        .ok_or_else(|| AppError::msg("Configure la carpeta raíz primero"))?;
    let root = PathBuf::from(root);
    if !root.is_dir() {
        return Err(AppError::msg("La carpeta raíz no es accesible"));
    }

    let handle = watcher::start_root_watcher(
        app,
        state.pool.clone(),
        root,
        state.indexing.clone(),
    )
    .map_err(AppError::msg)?;

    let mut guard = state
        .watcher
        .lock()
        .map_err(|_| AppError::msg("Watcher lock poisoned"))?;
    *guard = Some(handle);
    Ok(())
}
