use serde::{Deserialize, Serialize};
use tauri::State;

use crate::error::{AppError, AppResult};
use crate::state::AppState;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct AuditEntry {
    pub id: String,
    pub entity_type: Option<String>,
    pub entity_id: Option<String>,
    pub action: String,
    pub detail: Option<String>,
    pub created_at: String,
    pub project_id: Option<String>,
}

#[tauri::command]
pub async fn list_project_audit(
    state: State<'_, AppState>,
    project_id: String,
    limit: Option<i64>,
) -> AppResult<Vec<AuditEntry>> {
    let lim = limit.unwrap_or(100).clamp(1, 500);
    sqlx::query_as::<_, AuditEntry>(
        "SELECT id, entity_type, entity_id, action, detail, created_at, project_id
         FROM audit_log
         WHERE project_id = ?
            OR (entity_type = 'project' AND entity_id = ?)
         ORDER BY created_at DESC
         LIMIT ?",
    )
    .bind(&project_id)
    .bind(&project_id)
    .bind(lim)
    .fetch_all(&state.pool)
    .await
    .map_err(AppError::from)
}
