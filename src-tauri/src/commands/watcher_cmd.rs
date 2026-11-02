use tauri::{AppHandle, State};

use crate::error::{AppError, AppResult};
use crate::project_paths;
use crate::state::AppState;
use crate::watcher;

#[tauri::command]
pub async fn start_file_watcher(app: AppHandle, state: State<'_, AppState>) -> AppResult<()> {
    let roots = project_paths::load_root_paths(&state.pool).await?;
    if roots.is_empty() {
        return Err(AppError::msg("Configure la carpeta raíz primero"));
    }

    let handle = watcher::start_roots_watcher(
        app,
        state.pool.clone(),
        roots,
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
