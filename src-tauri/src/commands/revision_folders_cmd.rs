use tauri::State;

use crate::error::AppResult;
use crate::revision_folders::{
    self, RevisionFolderApplyResult, RevisionFolderPreview,
};
use crate::state::AppState;

#[tauri::command]
pub async fn preview_revision_folder_reorg(
    state: State<'_, AppState>,
    project_id: String,
) -> AppResult<RevisionFolderPreview> {
    revision_folders::preview_reorg(&state.pool, &project_id).await
}

#[tauri::command]
pub async fn apply_revision_folder_reorg(
    state: State<'_, AppState>,
    project_id: String,
    session_id: String,
) -> AppResult<RevisionFolderApplyResult> {
    revision_folders::apply_reorg(&state.pool, &project_id, &session_id).await
}
