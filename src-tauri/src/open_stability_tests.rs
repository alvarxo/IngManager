//! Regression tests for project-1090 open hang/OOM class.
//!
//! - fingerprint never loads whole multi-MiB files
//! - open reconcile walks/cancels without reading entire bodies
//! - empty categories are seeded on open (DB only)

use std::fs::{self, File};
use std::io::Write;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Instant;

use sqlx::SqlitePool;
use tempfile::tempdir;

use crate::auto_version::{
    collect_version_pattern_paths, collect_version_pattern_paths_catch_unwind, fingerprint_meta,
    reconcile_project_cancelable, FINGERPRINT_HEAD_MAX, FINGERPRINT_TAIL_MAX,
};
use crate::commands::projects::ensure_standard_categories_if_empty;
use crate::db;
use crate::project_paths::upsert_primary_root;

async fn pool_in(dir: &std::path::Path) -> SqlitePool {
    let db_path = dir.join("test.db");
    db::init_pool(&db_path).await.expect("pool")
}

fn write_large_file(path: &std::path::Path, size: usize) {
    let mut f = File::create(path).unwrap();
    let chunk = vec![0x5Au8; 64 * 1024];
    let mut written = 0usize;
    while written < size {
        let n = chunk.len().min(size - written);
        f.write_all(&chunk[..n]).unwrap();
        written += n;
    }
}

#[test]
fn fingerprint_bounded_sample_constants() {
    assert_eq!(FINGERPRINT_HEAD_MAX, 4096);
    assert_eq!(FINGERPRINT_TAIL_MAX, 1024);
}

#[test]
fn fingerprint_large_file_completes_quickly_without_full_read() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("1090-huge.sdb");
    // 32 MiB synthetic — full read would be slow and memory-heavy.
    write_large_file(&path, 32 * 1024 * 1024);
    let start = Instant::now();
    let (fp, size, _) = fingerprint_meta(&path).expect("fingerprint");
    let elapsed = start.elapsed();
    assert_eq!(size, 32 * 1024 * 1024);
    assert_eq!(fp.len(), 64);
    assert!(
        elapsed.as_millis() < 5_000,
        "fingerprint took too long ({elapsed:?}) — likely full-file read"
    );
}

#[test]
fn fingerprint_handles_path_with_spaces() {
    let dir = tempdir().unwrap();
    let nested = dir.path().join("EBR ing manager").join("ALVARO ARAVENA");
    fs::create_dir_all(&nested).unwrap();
    let path = nested.join("model.sdb");
    fs::write(&path, b"hello-space-path").unwrap();
    let (fp, size, _) = fingerprint_meta(&path).expect("spaces ok");
    assert_eq!(size, 16);
    assert_eq!(fp.len(), 64);
}

#[tokio::test]
async fn reconcile_cancelable_on_synthetic_heavy_tree() {
    let dir = tempdir().unwrap();
    let pool = pool_in(dir.path()).await;
    let root = upsert_primary_root(&pool, dir.path()).await.unwrap();

    let proj = dir.path().join("1090");
    fs::create_dir_all(proj.join("02_MODELOS")).unwrap();
    for i in 0..40 {
        let p = proj
            .join("02_MODELOS")
            .join(format!("MODEL_{i:03}_v01.sdb"));
        write_large_file(&p, 2 * 1024 * 1024);
    }
    for i in 0..200 {
        fs::write(
            proj.join("02_MODELOS").join(format!("notes_{i}.txt")),
            b"x",
        )
        .unwrap();
    }

    let pid = "proj-1090-stress";
    sqlx::query(
        "INSERT INTO projects (
            id, code, name, status, authorized_root_id, root_relative_path,
            import_mode, layout_mode, created_at, updated_at
         ) VALUES (?, '1090', 'TRUCK SHOP', 'active', ?, '1090', 'linked', 'mapped',
            datetime('now'), datetime('now'))",
    )
    .bind(pid)
    .bind(&root.id)
    .execute(&pool)
    .await
    .unwrap();

    let cancel = AtomicBool::new(false);
    let gen = AtomicU64::new(1);
    let start = Instant::now();
    let _results = reconcile_project_cancelable(&pool, pid, Some(&cancel), Some((&gen, 1)))
        .await
        .expect("reconcile");
    let elapsed = start.elapsed();
    assert!(
        elapsed.as_secs() < 30,
        "reconcile too slow on synthetic tree: {elapsed:?}"
    );

    cancel.store(true, Ordering::SeqCst);
    let _cancelled = reconcile_project_cancelable(&pool, pid, Some(&cancel), Some((&gen, 1)))
        .await
        .expect("cancel");
}

#[tokio::test]
async fn open_seeds_categories_when_empty() {
    let dir = tempdir().unwrap();
    let pool = pool_in(dir.path()).await;
    let root = upsert_primary_root(&pool, dir.path()).await.unwrap();
    let proj = dir.path().join("1090");
    fs::create_dir_all(&proj).unwrap();

    let pid = "proj-empty-cats";
    sqlx::query(
        "INSERT INTO projects (
            id, code, name, status, authorized_root_id, root_relative_path,
            import_mode, layout_mode, created_at, updated_at
         ) VALUES (?, '1090', 'Empty Cats', 'active', ?, '1090', 'linked', 'mapped',
            datetime('now'), datetime('now'))",
    )
    .bind(pid)
    .bind(&root.id)
    .execute(&pool)
    .await
    .unwrap();

    let before: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM categories WHERE project_id = ?")
        .bind(pid)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(before.0, 0);

    ensure_standard_categories_if_empty(&pool, pid)
        .await
        .expect("seed");

    let after: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM categories WHERE project_id = ?")
        .bind(pid)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert!(after.0 >= 7);

    // Idempotent — second call must not duplicate.
    ensure_standard_categories_if_empty(&pool, pid)
        .await
        .expect("seed2");
    let again: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM categories WHERE project_id = ?")
        .bind(pid)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(again.0, after.0);
}

#[test]
fn walk_terminates_on_symlink_loop_and_huge_sdb() {
    let dir = tempdir().unwrap();
    let root = dir.path().join("EBR ing manager").join("1090");
    let modelos = root.join("02_MODELOS");
    fs::create_dir_all(&modelos).unwrap();

    // Huge mock SDB with version pattern — must not hang/OOM the walk.
    write_large_file(&modelos.join("CENTINELA_v01.sdb"), 16 * 1024 * 1024);

    // Symlink loop (Linux stand-in for Windows junction loops).
    #[cfg(unix)]
    {
        let a = root.join("loop_a");
        let b = root.join("loop_b");
        fs::create_dir_all(&a).unwrap();
        fs::create_dir_all(&b).unwrap();
        let _ = std::os::unix::fs::symlink(&b, a.join("to_b"));
        let _ = std::os::unix::fs::symlink(&a, b.join("to_a"));
    }

    let start = Instant::now();
    let (paths, stats) = collect_version_pattern_paths(&root);
    let elapsed = start.elapsed();
    assert!(
        elapsed.as_secs() < 10,
        "walk hung on loop/huge tree: {elapsed:?} stats={stats:?}"
    );
    assert!(
        paths.iter().any(|p| {
            p.file_name()
                .map(|n| n.to_string_lossy().contains("CENTINELA_v01"))
                .unwrap_or(false)
        }),
        "expected version-pattern sdb in walk results"
    );

    let catch = collect_version_pattern_paths_catch_unwind(root.clone());
    assert!(catch.is_ok(), "catch_unwind wrapper must succeed");
}

#[tokio::test]
async fn reconcile_on_open_setting_defaults_off() {
    let dir = tempdir().unwrap();
    let pool = pool_in(dir.path()).await;
    let v = db::get_setting(&pool, "reconcile_on_open")
        .await
        .unwrap();
    assert!(v.is_none(), "unset setting must mean open skips reconcile");
}
