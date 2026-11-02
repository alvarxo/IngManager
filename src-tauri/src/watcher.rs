use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use notify::{Event, RecommendedWatcher, RecursiveMode, Watcher};
use sqlx::SqlitePool;
use tauri::{AppHandle, Emitter};
use tokio::sync::RwLock;

use crate::index_sync;
use crate::models::IndexProgress;

pub struct WatcherHandle {
    _watcher: RecommendedWatcher,
}

pub fn start_root_watcher(
    app: AppHandle,
    pool: SqlitePool,
    root: PathBuf,
    indexing: Arc<RwLock<bool>>,
) -> Result<WatcherHandle, String> {
    start_roots_watcher(app, pool, vec![root], indexing)
}

pub fn start_roots_watcher(
    app: AppHandle,
    pool: SqlitePool,
    roots: Vec<PathBuf>,
    indexing: Arc<RwLock<bool>>,
) -> Result<WatcherHandle, String> {
    if roots.is_empty() {
        return Err("No authorized roots to watch".into());
    }
    let (tx, rx) = mpsc::channel::<notify::Result<Event>>();
    let mut watcher = notify::recommended_watcher(tx).map_err(|e| e.to_string())?;
    for root in &roots {
        watcher
            .watch(root, RecursiveMode::Recursive)
            .map_err(|e| e.to_string())?;
    }

    let pending: Arc<Mutex<HashSet<PathBuf>>> = Arc::new(Mutex::new(HashSet::new()));
    let pending_clone = pending.clone();
    let roots_clone = roots.clone();
    // Primary root used as fallback for reconcile when path matching fails
    let primary = roots[0].clone();

    std::thread::spawn(move || {
        let mut last_event = Instant::now();
        loop {
            match rx.recv_timeout(Duration::from_millis(200)) {
                Ok(Ok(event)) => {
                    if let Ok(mut set) = pending_clone.lock() {
                        for path in event.paths {
                            if roots_clone.iter().any(|r| should_ignore(r, &path)) {
                                continue;
                            }
                            set.insert(path);
                        }
                    }
                    last_event = Instant::now();
                }
                Ok(Err(_)) => {}
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    if last_event.elapsed() >= Duration::from_millis(400) {
                        let batch = {
                            let mut set = pending_clone.lock().unwrap();
                            if set.is_empty() {
                                continue;
                            }
                            let items: Vec<PathBuf> = set.drain().collect();
                            items
                        };
                        if batch.is_empty() {
                            continue;
                        }

                        let pool = pool.clone();
                        let root = primary.clone();
                        let app = app.clone();
                        let indexing = indexing.clone();
                        tauri::async_runtime::spawn(async move {
                            if *indexing.read().await {
                                return;
                            }
                            let _ = app.emit(
                                "fs-changed",
                                IndexProgress {
                                    phase: "reconcile".into(),
                                    scanned: batch.len() as u64,
                                    total_hint: None,
                                    current_path: batch.first().map(|p| p.display().to_string()),
                                    done: false,
                                    message: Some("Reconciliando cambios…".into()),
                                },
                            );
                            if let Err(e) = index_sync::reconcile_paths(&pool, &root, &batch).await
                            {
                                let _ = app.emit(
                                    "fs-changed",
                                    IndexProgress {
                                        phase: "error".into(),
                                        scanned: 0,
                                        total_hint: None,
                                        current_path: None,
                                        done: true,
                                        message: Some(e.to_string()),
                                    },
                                );
                            } else {
                                // Auto-version after index reconcile (debounce already applied).
                                let av = crate::auto_version::process_watcher_batch(&pool, &batch)
                                    .await
                                    .unwrap_or_default();
                                let msg = if av.iter().any(|r| r.action == "version_created") {
                                    let n = av
                                        .iter()
                                        .filter(|r| r.action == "version_created")
                                        .count();
                                    format!("Índice reconciliado · {n} auto-versión(es)")
                                } else if av.iter().any(|r| r.action == "pending") {
                                    "Índice reconciliado · versiones pendientes".into()
                                } else {
                                    "Índice reconciliado".into()
                                };
                                let _ = app.emit(
                                    "fs-changed",
                                    IndexProgress {
                                        phase: "done".into(),
                                        scanned: batch.len() as u64,
                                        total_hint: None,
                                        current_path: None,
                                        done: true,
                                        message: Some(msg),
                                    },
                                );
                            }
                        });
                        last_event = Instant::now();
                    }
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
            }
        }
    });

    Ok(WatcherHandle {
        _watcher: watcher,
    })
}

fn should_ignore(root: &Path, path: &Path) -> bool {
    let rel = path.strip_prefix(root).ok();
    if let Some(rel) = rel {
        if rel.components().any(|c| {
            c.as_os_str()
                .to_string_lossy()
                .eq_ignore_ascii_case(".ingmanager")
        }) {
            return true;
        }
    }
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    index_sync::should_skip_name(&name)
}
