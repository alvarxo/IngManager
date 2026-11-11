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
    apply_migration(
        pool,
        3,
        "003_transmissions_phase4",
        include_str!("../migrations/003_transmissions_phase4.sql"),
    )
    .await?;
    apply_migration(
        pool,
        4,
        "004_phase5_advanced",
        include_str!("../migrations/004_phase5_advanced.sql"),
    )
    .await?;
    apply_migration(
        pool,
        5,
        "005_project_flexibility",
        include_str!("../migrations/005_project_flexibility.sql"),
    )
    .await?;
    apply_migration(
        pool,
        6,
        "006_organizer_inbox_versioning",
        include_str!("../migrations/006_organizer_inbox_versioning.sql"),
    )
    .await?;
    apply_migration(
        pool,
        7,
        "007_engineering_grouping",
        include_str!("../migrations/007_engineering_grouping.sql"),
    )
    .await?;
    apply_migration(
        pool,
        8,
        "008_model_spreadsheets",
        include_str!("../migrations/008_model_spreadsheets.sql"),
    )
    .await?;
    apply_migration(
        pool,
        9,
        "009_model_subtypes",
        include_str!("../migrations/009_model_subtypes.sql"),
    )
    .await?;
    apply_migration(
        pool,
        10,
        "010_physical_revision_folders",
        include_str!("../migrations/010_physical_revision_folders.sql"),
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
        sqlx::query(&statement).execute(&mut *tx).await.map_err(|e| {
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

fn split_statements(sql: &str) -> Vec<String> {
    sql.split(';')
        .filter_map(|chunk| {
            let without_line_comments: String = chunk
                .lines()
                .map(str::trim)
                .filter(|line| !line.is_empty() && !line.starts_with("--"))
                .collect::<Vec<_>>()
                .join("\n");
            let trimmed = without_line_comments.trim().to_string();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed)
            }
        })
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

/// Persistent app database path under the **user profile**, never under the
/// install directory (Windows Program Files / NSIS `$INSTDIR`).
///
/// Resolution order:
/// 1. `%LOCALAPPDATA%\IngManager\ingmanager.db` (Windows) /
///    `~/.local/share/IngManager/ingmanager.db` (Linux) when already present
///    or as the preferred new location.
/// 2. Legacy `…/ingmanager/ingmanager.db` if that file already exists
///    (Phases 1–5 installs) so upgrades keep `root_path` and indexes.
pub fn default_db_path() -> AppResult<PathBuf> {
    let base = dirs::data_local_dir()
        .or_else(dirs::data_dir)
        .ok_or_else(|| {
            AppError::msg(
                "No se pudo resolver el directorio de datos del usuario (LOCALAPPDATA/APPDATA)",
            )
        })?;

    let preferred = base.join("IngManager").join("ingmanager.db");
    let legacy = base.join("ingmanager").join("ingmanager.db");

    if preferred.exists() {
        return Ok(preferred);
    }
    if legacy.exists() {
        return Ok(legacy);
    }
    // Brand-new installs write under the product folder (not CWD / install dir).
    Ok(preferred)
}

#[cfg(test)]
mod path_tests {
    use super::*;

    #[test]
    fn db_path_is_under_user_data_not_cwd() {
        let path = default_db_path().expect("data dir");
        let cwd = std::env::current_dir().expect("cwd");
        assert!(
            !path.starts_with(&cwd.join("src-tauri")),
            "DB must not live under the crate/install tree: {path:?}"
        );
        let s = path.to_string_lossy();
        assert!(
            s.contains("IngManager") || s.contains("ingmanager"),
            "unexpected db path: {s}"
        );
        assert!(path.ends_with("ingmanager.db"));
    }
}
