use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use sqlx::SqlitePool;
use tokio::sync::RwLock;

use crate::watcher::WatcherHandle;

pub struct AppState {
    pub pool: SqlitePool,
    pub db_path: PathBuf,
    pub indexing: Arc<RwLock<bool>>,
    pub watcher: Mutex<Option<WatcherHandle>>,
}

impl AppState {
    pub fn new(pool: SqlitePool, db_path: PathBuf) -> Self {
        Self {
            pool,
            db_path,
            indexing: Arc::new(RwLock::new(false)),
            watcher: Mutex::new(None),
        }
    }
}
