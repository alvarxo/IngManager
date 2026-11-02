use tauri::State;

use crate::error::AppResult;
use crate::inbox::{self, InboxAssignInput, InboxAssignResult, InboxItem};
use crate::state::AppState;

#[tauri::command]
pub async fn inbox_ensure_dir(state: State<'_, AppState>) -> AppResult<String> {
    inbox::ensure_inbox(&state.pool).await
}

#[tauri::command]
pub async fn inbox_list(state: State<'_, AppState>) -> AppResult<Vec<InboxItem>> {
    inbox::list_items(&state.pool).await
}

#[tauri::command]
pub async fn inbox_stage_files(
    state: State<'_, AppState>,
    paths: Vec<String>,
    copy_into: Option<bool>,
) -> AppResult<Vec<InboxItem>> {
    inbox::stage_paths(&state.pool, &paths, copy_into.unwrap_or(true)).await
}

#[tauri::command]
pub async fn inbox_remove_items(
    state: State<'_, AppState>,
    ids: Vec<String>,
    delete_files: Option<bool>,
) -> AppResult<usize> {
    inbox::remove_items(&state.pool, &ids, delete_files.unwrap_or(true)).await
}

#[tauri::command]
pub async fn inbox_assign(
    state: State<'_, AppState>,
    input: InboxAssignInput,
) -> AppResult<InboxAssignResult> {
    inbox::assign(&state.pool, input).await
}
