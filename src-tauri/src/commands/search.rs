use serde::{Deserialize, Serialize};
use tauri::State;

use crate::error::{AppError, AppResult};
use crate::search_index::{self, build_fts_query};
use crate::state::AppState;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct SearchHit {
    pub entity_type: String,
    pub entity_id: String,
    pub project_id: String,
    pub title: String,
    pub code: String,
    pub body: String,
    pub file_name: String,
    pub rank: f64,
}

#[tauri::command]
pub async fn global_search(
    state: State<'_, AppState>,
    query: String,
    limit: Option<i64>,
) -> AppResult<Vec<SearchHit>> {
    let q = query.trim();
    if q.is_empty() {
        return Ok(vec![]);
    }
    let lim = limit.unwrap_or(40).clamp(1, 100);
    let match_q = build_fts_query(q);

    let rows = sqlx::query_as::<_, SearchHit>(
        "SELECT entity_type, entity_id, project_id, title, code, body, file_name,
                bm25(search_fts) AS rank
         FROM search_fts
         WHERE search_fts MATCH ?
         ORDER BY rank
         LIMIT ?",
    )
    .bind(&match_q)
    .bind(lim)
    .fetch_all(&state.pool)
    .await;

    match rows {
        Ok(r) => Ok(r),
        Err(e) => {
            // If FTS empty/corrupt, try rebuild once soft-fail
            let msg = e.to_string();
            if msg.contains("no such table") || msg.contains("fts5") {
                return Err(AppError::msg(
                    "Índice FTS no disponible. Ejecute reconstrucción desde Ajustes.",
                ));
            }
            // MATCH syntax / no results edge
            if msg.contains("syntax error") || msg.contains("malformed") {
                return Ok(vec![]);
            }
            Err(AppError::from(e))
        }
    }
}

#[tauri::command]
pub async fn rebuild_search_index(state: State<'_, AppState>) -> AppResult<u64> {
    search_index::rebuild_all(&state.pool).await
}
