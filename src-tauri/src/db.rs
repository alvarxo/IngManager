use std::path::PathBuf;

use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous};
use sqlx::SqlitePool;
use std::str::FromStr;

use crate::error::{AppError, AppResult};

pub async fn init_pool(db_path: &PathBuf) -> AppResult<SqlitePool> {
    if let Some(parent) = db_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let options = SqliteConnectOptions::from_str(&format!("sqlite:{}", db_path.display()))?
        .create_if_missing(true)
        .journal_mode(SqliteJournalMode::Wal)
        .synchronous(SqliteSynchronous::Normal)
        .foreign_keys(true)
        .busy_timeout(std::time::Duration::from_secs(5));

    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect_with(options)
        .await?;

    sqlx::query("PRAGMA foreign_keys = ON;")
        .execute(&pool)
        .await?;
    sqlx::query("PRAGMA busy_timeout = 5000;")
        .execute(&pool)
        .await?;

    run_migrations(&pool).await?;
    Ok(pool)
}

async fn run_migrations(pool: &SqlitePool) -> AppResult<()> {
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS __migrations (
            version INTEGER PRIMARY KEY NOT NULL,
            name TEXT NOT NULL,
            applied_at TEXT NOT NULL DEFAULT (datetime('now'))
        )",
    )
    .execute(pool)
    .await?;

    apply_migration(pool, 1, "001_initial", include_str!("../migrations/001_initial.sql")).await?;
    apply_migration(
        pool,
        2,
        "002_documents_phase3",
        include_str!("../migrations/002_documents_phase3.sql"),
    )
    .await?;

    Ok(())
}

async fn apply_migration(
    pool: &SqlitePool,
    version: i64,
    name: &str,
    sql: &str,
) -> AppResult<()> {
    let applied: Option<(i64,)> =
        sqlx::query_as("SELECT version FROM __migrations WHERE version = ?")
            .bind(version)
            .fetch_optional(pool)
            .await?;

    if applied.is_some() {
        return Ok(());
    }

    let mut tx = pool.begin().await?;
    for statement in split_statements(sql) {
        sqlx::query(statement).execute(&mut *tx).await.map_err(|e| {
            AppError::msg(format!(
                "Migration {name} failed on statement: {e}\nSQL: {statement}"
            ))
        })?;
    }
    sqlx::query("INSERT INTO __migrations (version, name) VALUES (?, ?)")
        .bind(version)
        .bind(name)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(())
}

fn split_statements(sql: &str) -> Vec<&str> {
    sql.split(';')
        .map(str::trim)
        .filter(|s| !s.is_empty() && !s.starts_with("--"))
        .collect()
}

pub async fn get_setting(pool: &SqlitePool, key: &str) -> AppResult<Option<String>> {
    let row: Option<(String,)> = sqlx::query_as("SELECT value FROM app_settings WHERE key = ?")
        .bind(key)
        .fetch_optional(pool)
        .await?;
    Ok(row.map(|r| r.0))
}

pub async fn set_setting(pool: &SqlitePool, key: &str, value: &str) -> AppResult<()> {
    sqlx::query(
        "INSERT INTO app_settings (key, value, updated_at)
         VALUES (?, ?, datetime('now'))
         ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = datetime('now')",
    )
    .bind(key)
    .bind(value)
    .execute(pool)
    .await?;
    Ok(())
}

pub fn default_db_path() -> AppResult<PathBuf> {
    let base = dirs::data_dir().ok_or_else(|| AppError::msg("No se pudo resolver data_dir"))?;
    Ok(base.join("ingmanager").join("ingmanager.db"))
}
