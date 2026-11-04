//! Hotfix Tests A–E: remove project registration without opening.
//!
//! A — remove by UUID deletes SQLite row; physical files kept
//! B — typed code confirm required (wrong code rejected)
//! C — remove path never depends on open/reconcile/index
//! D — UI list refresh basis: removed id absent from projects; code free
//! E — fingerprint samples large files without full-file read (1090 OOM)

use std::fs;
use std::io::Write;

use sqlx::SqlitePool;
use tempfile::tempdir;

use crate::auto_version::fingerprint_meta;
use crate::commands::projects::{
    expected_remove_confirm, remove_project_registration_inner,
};
use crate::db;
use crate::models::Project;
use crate::project_paths::upsert_primary_root;

async fn pool_in(dir: &std::path::Path) -> SqlitePool {
    let db_path = dir.join("test.db");
    db::init_pool(&db_path).await.expect("pool")
}

async fn seed_project(
    pool: &SqlitePool,
    root_id: &str,
    id: &str,
    code: Option<&str>,
    folder: &str,
) {
    sqlx::query(
        "INSERT INTO projects (
            id, code, name, status, authorized_root_id, root_relative_path,
            import_mode, layout_mode, created_at, updated_at
         ) VALUES (?, ?, 'Proyecto 1090', 'active', ?, ?, 'created', 'standard',
            datetime('now'), datetime('now'))",
    )
    .bind(id)
    .bind(code)
    .bind(root_id)
    .bind(folder)
    .execute(pool)
    .await
    .unwrap();
}

/// A — remove registration keeps the physical folder/files.
#[tokio::test]
async fn a_remove_keeps_physical_files() {
    let dir = tempdir().unwrap();
    let pool = pool_in(dir.path()).await;
    let root = upsert_primary_root(&pool, dir.path()).await.unwrap();
    let folder = dir.path().join("1090");
    fs::create_dir_all(folder.join("MODELOS")).unwrap();
    fs::write(folder.join("MODELOS/huge.rvt"), b"KEEP_ME").unwrap();
    seed_project(&pool, &root.id, "uuid-1090", Some("1090"), "1090").await;

    let result = remove_project_registration_inner(&pool, "uuid-1090", "1090")
        .await
        .expect("remove");

    assert_eq!(result.id, "uuid-1090");
    assert_eq!(result.code.as_deref(), Some("1090"));
    assert!(folder.join("MODELOS/huge.rvt").exists());
    assert_eq!(
        fs::read(folder.join("MODELOS/huge.rvt")).unwrap(),
        b"KEEP_ME"
    );

    let left: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM projects WHERE id = 'uuid-1090'")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(left.0, 0);
}

/// B — confirm must match business code (wrong code keeps registration + files).
#[tokio::test]
async fn b_typed_code_confirm_required() {
    let dir = tempdir().unwrap();
    let pool = pool_in(dir.path()).await;
    let root = upsert_primary_root(&pool, dir.path()).await.unwrap();
    let folder = dir.path().join("1090");
    fs::create_dir_all(&folder).unwrap();
    fs::write(folder.join("keep.txt"), b"x").unwrap();
    seed_project(&pool, &root.id, "uuid-b", Some("1090"), "1090").await;

    let err = remove_project_registration_inner(&pool, "uuid-b", "wrong")
        .await
        .expect_err("must reject");
    assert!(
        err.to_string().contains("CONFIRM_CODE"),
        "got: {err}"
    );

    let still: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM projects WHERE id = 'uuid-b'")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(still.0, 1);
    assert!(folder.join("keep.txt").exists());

    let project: Project = sqlx::query_as("SELECT * FROM projects WHERE id = 'uuid-b'")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(expected_remove_confirm(&project), "1090");
}

/// C — remove works by UUID alone; no open/reconcile/index side effects required.
#[tokio::test]
async fn c_remove_by_uuid_without_open_or_index() {
    let dir = tempdir().unwrap();
    let pool = pool_in(dir.path()).await;
    let root = upsert_primary_root(&pool, dir.path()).await.unwrap();
    let folder = dir.path().join("1090");
    fs::create_dir_all(&folder).unwrap();
    // Seed an indexed_files row as if previously scanned — remove must not re-index.
    seed_project(&pool, &root.id, "uuid-c", Some("1090"), "1090").await;
    sqlx::query(
        "INSERT INTO indexed_files (
            id, project_id, authorized_root_id, relative_path, absolute_path,
            file_name, is_directory, is_available, indexed_at
         ) VALUES ('f1', 'uuid-c', ?, '1090/a.txt', ?, 'a.txt', 0, 1, datetime('now'))",
    )
    .bind(&root.id)
    .bind(folder.join("a.txt").to_string_lossy().as_ref())
    .execute(&pool)
    .await
    .unwrap();
    fs::write(folder.join("a.txt"), b"indexed-once").unwrap();

    // last_opened_at stays null — we never called open_project.
    let before: (Option<String>,) =
        sqlx::query_as("SELECT last_opened_at FROM projects WHERE id = 'uuid-c'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(before.0.is_none());

    remove_project_registration_inner(&pool, "uuid-c", "1090")
        .await
        .expect("remove");

    // Cascaded DB rows gone; physical file remains; no new index activity needed.
    let idx: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM indexed_files WHERE project_id = 'uuid-c'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(idx.0, 0);
    assert!(folder.join("a.txt").exists());
}

/// D — after remove, project absent from list and code 1090 is free to reuse.
#[tokio::test]
async fn d_list_refresh_and_code_reusable() {
    let dir = tempdir().unwrap();
    let pool = pool_in(dir.path()).await;
    let root = upsert_primary_root(&pool, dir.path()).await.unwrap();
    fs::create_dir_all(dir.path().join("1090")).unwrap();
    seed_project(&pool, &root.id, "uuid-d", Some("1090"), "1090").await;

    remove_project_registration_inner(&pool, "uuid-d", "1090")
        .await
        .unwrap();

    let listed: Vec<(String,)> =
        sqlx::query_as("SELECT id FROM projects WHERE status = 'active'")
            .fetch_all(&pool)
            .await
            .unwrap();
    assert!(!listed.iter().any(|r| r.0 == "uuid-d"));

    crate::commands::projects::assert_code_unique(&pool, "1090", None)
        .await
        .expect("code free after remove");

    seed_project(&pool, &root.id, "uuid-d2", Some("1090"), "1090").await;
    let row: (String,) = sqlx::query_as("SELECT code FROM projects WHERE id = 'uuid-d2'")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(row.0, "1090");
}

/// E — fingerprint never loads a multi-MiB file wholly into RAM (1090 crash class).
#[test]
fn e_fingerprint_samples_large_file_without_full_read() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("1090-model.rvt");
    let mut f = fs::File::create(&path).unwrap();
    let chunk = vec![0xABu8; 1024 * 64];
    for _ in 0..(8 * 1024 * 1024 / chunk.len()) {
        f.write_all(&chunk).unwrap();
    }
    drop(f);
    let (fp, size, _) = fingerprint_meta(&path).expect("fingerprint");
    assert_eq!(size, 8 * 1024 * 1024);
    assert_eq!(fp.len(), 64);
}
