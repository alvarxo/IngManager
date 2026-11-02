use serde::{Deserialize, Serialize};
use tauri::State;

use crate::error::AppResult;
use crate::organizer::{
    self, OrganizerApplyResult, OrganizerPlanResult, OrganizerScanItem, OrganizerSession,
};
use crate::state::AppState;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FolderMappingRow {
    pub id: String,
    pub category_key: String,
    pub physical_relative_path: String,
    pub mapping_mode: String,
}

#[tauri::command]
pub async fn organizer_scan(
    state: State<'_, AppState>,
    scope_absolute_path: String,
) -> AppResult<Vec<OrganizerScanItem>> {
    organizer::scan_scope(&state.pool, &scope_absolute_path).await
}

#[tauri::command]
pub async fn organizer_build_plan(
    state: State<'_, AppState>,
    scope_absolute_path: String,
    mode: String,
    project_ids: Option<Vec<String>>,
) -> AppResult<OrganizerPlanResult> {
    organizer::build_plan(&state.pool, &scope_absolute_path, &mode, project_ids).await
}

#[tauri::command]
pub async fn organizer_get_plan(
    state: State<'_, AppState>,
    session_id: String,
) -> AppResult<OrganizerPlanResult> {
    organizer::get_plan(&state.pool, &session_id).await
}

#[tauri::command]
pub async fn organizer_apply_plan(
    state: State<'_, AppState>,
    session_id: String,
) -> AppResult<OrganizerApplyResult> {
    organizer::apply_plan(&state.pool, &session_id).await
}

#[tauri::command]
pub async fn organizer_update_plan_item(
    state: State<'_, AppState>,
    input: organizer::UpdatePlanItemInput,
) -> AppResult<organizer::OrganizerPlanItem> {
    organizer::update_plan_item(&state.pool, input).await
}

#[tauri::command]
pub async fn organizer_list_sessions(
    state: State<'_, AppState>,
) -> AppResult<Vec<OrganizerSession>> {
    let rows = sqlx::query_as::<_, OrganizerSession>(
        "SELECT id, authorized_root_id, scope_absolute_path, mode, status,
                backup_absolute_path, notes, created_at, applied_at
         FROM organizer_sessions
         ORDER BY created_at DESC
         LIMIT 50",
    )
    .fetch_all(&state.pool)
    .await?;
    Ok(rows)
}

#[tauri::command]
pub async fn list_project_folder_mappings(
    state: State<'_, AppState>,
    project_id: String,
) -> AppResult<Vec<FolderMappingRow>> {
    let rows = organizer::list_mappings(&state.pool, &project_id).await?;
    Ok(rows
        .into_iter()
        .map(|(id, category_key, physical_relative_path, mapping_mode)| FolderMappingRow {
            id,
            category_key,
            physical_relative_path,
            mapping_mode,
        })
        .collect())
}
