use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use sqlx::SqlitePool;
use tokio::sync::RwLock;

use crate::watcher::WatcherHandle;

pub struct AppState {
    pub pool: SqlitePool,
    pub db_path: PathBuf,
    pub indexing: Arc<RwLock<bool>>,
    pub watcher: Mutex<Option<WatcherHandle>>,
    /// Cancel flag for the in-flight open reconcile job.
    pub open_reconcile_cancel: Arc<AtomicBool>,
    /// Monotonic generation so a newer open supersedes an older reconcile.
    pub open_reconcile_generation: Arc<AtomicU64>,
}

impl AppState {
    pub fn new(pool: SqlitePool, db_path: PathBuf) -> Self {
        Self {
            pool,
            db_path,
            indexing: Arc::new(RwLock::new(false)),
            watcher: Mutex::new(None),
            open_reconcile_cancel: Arc::new(AtomicBool::new(false)),
            open_reconcile_generation: Arc::new(AtomicU64::new(0)),
        }
    }

    pub fn cancel_open_reconcile(&self) {
        self.open_reconcile_cancel.store(true, Ordering::SeqCst);
    }

    pub fn begin_open_reconcile(&self) -> u64 {
        self.open_reconcile_cancel.store(true, Ordering::SeqCst);
        let gen = self
            .open_reconcile_generation
            .fetch_add(1, Ordering::SeqCst)
            + 1;
        self.open_reconcile_cancel.store(false, Ordering::SeqCst);
        gen
    }
}
