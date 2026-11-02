use sqlx::SqlitePool;
use uuid::Uuid;

use crate::error::AppResult;

pub async fn log_event(
    pool: &SqlitePool,
    project_id: Option<&str>,
    entity_type: &str,
    entity_id: Option<&str>,
    action: &str,
    detail: Option<&str>,
) -> AppResult<()> {
    sqlx::query(
        "INSERT INTO audit_log (id, entity_type, entity_id, action, detail, created_at, project_id)
         VALUES (?, ?, ?, ?, ?, datetime('now'), ?)",
    )
    .bind(Uuid::new_v4().to_string())
    .bind(entity_type)
    .bind(entity_id)
    .bind(action)
    .bind(detail)
    .bind(project_id)
    .execute(pool)
    .await?;
    Ok(())
}
