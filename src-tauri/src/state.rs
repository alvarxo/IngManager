use std::path::PathBuf;
use std::sync::Arc;

use sqlx::SqlitePool;
use tokio::sync::RwLock;

#[derive(Clone)]
pub struct AppState {
    pub pool: SqlitePool,
    pub db_path: PathBuf,
    pub indexing: Arc<RwLock<bool>>,
}

impl AppState {
    pub fn new(pool: SqlitePool, db_path: PathBuf) -> Self {
        Self {
            pool,
            db_path,
            indexing: Arc::new(RwLock::new(false)),
        }
    }
}
