use tauri::{AppHandle, State};

use crate::db;
use crate::error::{AppError, AppResult};
use crate::project_paths;
use crate::state::AppState;
use crate::watcher;

#[tauri::command]
pub async fn start_file_watcher(app: AppHandle, state: State<'_, AppState>) -> AppResult<()> {
    // Default OFF: recursive watch of large engineering roots (e.g. 1090 + junctions)
    // can flood the process after any FS touch during open/list.
    let enabled = db::get_setting(&state.pool, "watch_fs")
        .await
        .ok()
        .flatten()
        .map(|v| {
            let t = v.trim().to_ascii_lowercase();
            matches!(t.as_str(), "1" | "true" | "yes" | "on")
        })
        .unwrap_or(false);
    if !enabled {
        crate::diagnostics::log_event(
            "watcher",
            "start skipped (enable Settings watch_fs)",
        );
        return Ok(());
    }

    let roots = project_paths::load_root_paths(&state.pool).await?;
    if roots.is_empty() {
        return Err(AppError::msg("Configure la carpeta raíz primero"));
    }

    crate::diagnostics::log_event(
        "watcher",
        &format!("start roots={}", roots.len()),
    );
    let handle = match watcher::start_roots_watcher(
        app,
        state.pool.clone(),
        roots,
        state.indexing.clone(),
    ) {
        Ok(h) => h,
        Err(e) => {
            crate::diagnostics::log_event("watcher", &format!("start err: {e}"));
            return Err(AppError::msg(e));
        }
    };

    let mut guard = state
        .watcher
        .lock()
        .map_err(|_| AppError::msg("Watcher lock poisoned"))?;
    *guard = Some(handle);
    crate::diagnostics::log_event("watcher", "start ok");
    Ok(())
}
