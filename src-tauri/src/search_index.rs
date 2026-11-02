use sqlx::SqlitePool;

use crate::error::AppResult;

pub async fn clear_fts(pool: &SqlitePool) -> AppResult<()> {
    sqlx::query("DELETE FROM search_fts").execute(pool).await?;
    Ok(())
}

pub async fn upsert_fts(
    pool: &SqlitePool,
    entity_type: &str,
    entity_id: &str,
    project_id: Option<&str>,
    title: &str,
    code: Option<&str>,
    body: Option<&str>,
    file_name: Option<&str>,
) -> AppResult<()> {
    // FTS5 external content-less tables: delete then insert
    sqlx::query("DELETE FROM search_fts WHERE entity_type = ? AND entity_id = ?")
        .bind(entity_type)
        .bind(entity_id)
        .execute(pool)
        .await?;

    sqlx::query(
        "INSERT INTO search_fts (entity_type, entity_id, project_id, title, code, body, file_name)
         VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(entity_type)
    .bind(entity_id)
    .bind(project_id.unwrap_or(""))
    .bind(title)
    .bind(code.unwrap_or(""))
    .bind(body.unwrap_or(""))
    .bind(file_name.unwrap_or(""))
    .execute(pool)
    .await?;
    Ok(())
}

#[allow(dead_code)]
pub async fn remove_fts(pool: &SqlitePool, entity_type: &str, entity_id: &str) -> AppResult<()> {
    sqlx::query("DELETE FROM search_fts WHERE entity_type = ? AND entity_id = ?")
        .bind(entity_type)
        .bind(entity_id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn rebuild_all(pool: &SqlitePool) -> AppResult<u64> {
    clear_fts(pool).await?;
    let mut count: u64 = 0;

    let projects: Vec<(String, String, String, Option<String>)> = sqlx::query_as(
        "SELECT id, code, name, description FROM projects",
    )
    .fetch_all(pool)
    .await?;
    for (id, code, name, desc) in projects {
        upsert_fts(
            pool,
            "project",
            &id,
            Some(&id),
            &name,
            Some(&code),
            desc.as_deref(),
            None,
        )
        .await?;
        count += 1;
    }

    let docs: Vec<(
        String,
        String,
        Option<String>,
        String,
        Option<String>,
        Option<String>,
    )> = sqlx::query_as(
        "SELECT id, project_id, business_code, title, notes, origin FROM documents",
    )
    .fetch_all(pool)
    .await?;
    for (id, project_id, code, title, notes, origin) in docs {
        let body = [notes, origin]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>()
            .join(" ");
        upsert_fts(
            pool,
            "document",
            &id,
            Some(&project_id),
            &title,
            code.as_deref(),
            if body.is_empty() { None } else { Some(&body) },
            None,
        )
        .await?;
        count += 1;
    }

    let files: Vec<(String, Option<String>, String, String)> = sqlx::query_as(
        "SELECT id, project_id, file_name, relative_path FROM indexed_files WHERE is_directory = 0",
    )
    .fetch_all(pool)
    .await?;
    for (id, project_id, file_name, rel) in files {
        upsert_fts(
            pool,
            "file",
            &id,
            project_id.as_deref(),
            &file_name,
            None,
            Some(&rel),
            Some(&file_name),
        )
        .await?;
        count += 1;
    }

    let txs: Vec<(String, String, Option<String>, Option<String>, Option<String>)> =
        sqlx::query_as(
            "SELECT id, project_id, code, subject, counterpart FROM transmissions WHERE status = 'complete'",
        )
        .fetch_all(pool)
        .await?;
    for (id, project_id, code, subject, counterpart) in txs {
        upsert_fts(
            pool,
            "transmission",
            &id,
            Some(&project_id),
            subject.as_deref().unwrap_or("transmisión"),
            code.as_deref(),
            counterpart.as_deref(),
            None,
        )
        .await?;
        count += 1;
    }

    sqlx::query(
        "INSERT INTO search_meta (key, value, updated_at)
         VALUES ('last_rebuild_at', datetime('now'), datetime('now'))
         ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = datetime('now')",
    )
    .execute(pool)
    .await?;

    Ok(count)
}

/// Escape user query for FTS5 MATCH — quote tokens, strip quotes.
pub fn build_fts_query(raw: &str) -> String {
    let cleaned = raw.replace('"', " ").replace('*', " ");
    let tokens: Vec<String> = cleaned
        .split_whitespace()
        .filter(|t| !t.is_empty())
        .map(|t| format!("\"{t}\"*"))
        .collect();
    if tokens.is_empty() {
        return "\"\"".into();
    }
    tokens.join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_prefix_query() {
        let q = build_fts_query(r#"torre norte "x""#);
        assert!(q.contains("\"torre\"*"));
        assert!(q.contains("\"norte\"*"));
        assert!(!q.contains("\"x\"*\""));
    }
}
