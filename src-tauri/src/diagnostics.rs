//! Persistent open/diagnostic logs under the user data directory.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;

use chrono::Utc;

use crate::db;
use crate::error::{AppError, AppResult};

static LOG_LOCK: Mutex<()> = Mutex::new(());

pub fn diagnostics_dir() -> AppResult<PathBuf> {
    let db_path = db::default_db_path()?;
    let parent = db_path
        .parent()
        .ok_or_else(|| AppError::msg("Ruta de datos inválida"))?;
    let dir = parent.join("diagnostics");
    fs::create_dir_all(&dir)?;
    Ok(dir)
}

pub fn open_log_path() -> AppResult<PathBuf> {
    Ok(diagnostics_dir()?.join("open-project.log"))
}

/// Append a single-line diagnostic entry (best-effort; never panics).
pub fn log_event(scope: &str, message: &str) {
    let line = format!(
        "{} [{}] {}\n",
        Utc::now().to_rfc3339(),
        scope,
        message.replace('\n', " ")
    );
    let _guard = LOG_LOCK.lock();
    if let Ok(path) = open_log_path() {
        if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(path) {
            let _ = f.write_all(line.as_bytes());
            let _ = f.flush();
        }
    }
    #[cfg(debug_assertions)]
    eprintln!("{line}");
}
