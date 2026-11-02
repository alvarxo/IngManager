use std::path::PathBuf;

use tauri::State;

use crate::auto_version::{self, AutoVersionResult, PendingVersionEvent};
use crate::error::AppResult;
use crate::state::AppState;

#[tauri::command]
pub async fn list_pending_versions(
    state: State<'_, AppState>,
    project_id: Option<String>,
) -> AppResult<Vec<PendingVersionEvent>> {
    auto_version::list_pending(&state.pool, project_id.as_deref()).await
}

#[tauri::command]
pub async fn resolve_pending_version(
    state: State<'_, AppState>,
    pending_id: String,
    document_id: String,
) -> AppResult<AutoVersionResult> {
    auto_version::resolve_pending(&state.pool, &pending_id, &document_id).await
}

#[tauri::command]
pub async fn dismiss_pending_version(
    state: State<'_, AppState>,
    pending_id: String,
) -> AppResult<()> {
    auto_version::dismiss_pending(&state.pool, &pending_id).await
}

#[tauri::command]
pub async fn reconcile_project_versions(
    state: State<'_, AppState>,
    project_id: String,
) -> AppResult<Vec<AutoVersionResult>> {
    auto_version::reconcile_project(&state.pool, &project_id).await
}

#[tauri::command]
pub async fn register_file_version_baseline(
    state: State<'_, AppState>,
    project_id: String,
    document_id: String,
    version_id: Option<String>,
    absolute_path: String,
) -> AppResult<()> {
    auto_version::register_baseline(
        &state.pool,
        &project_id,
        &document_id,
        version_id.as_deref(),
        &PathBuf::from(absolute_path),
    )
    .await
}

#[tauri::command]
pub async fn process_auto_version_path(
    state: State<'_, AppState>,
    absolute_path: String,
) -> AppResult<Option<AutoVersionResult>> {
    auto_version::process_path(&state.pool, &PathBuf::from(absolute_path)).await
}
