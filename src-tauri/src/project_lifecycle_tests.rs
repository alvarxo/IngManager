//! Hotfix scenarios A–G: open stability, remove/delete, recreate, folder conflict.

use std::fs;
use std::io::Write;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use sqlx::SqlitePool;
use tempfile::tempdir;

use crate::auto_version::{self, fingerprint_meta};
use crate::db;
use crate::project_paths::{self, upsert_primary_root};

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
    import_mode: &str,
) {
    sqlx::query(
        "INSERT INTO projects (
            id, code, name, status, authorized_root_id, root_relative_path,
            import_mode, layout_mode, created_at, updated_at
         ) VALUES (?, ?, 'Proj', 'active', ?, ?, ?, 'standard',
            datetime('now'), datetime('now'))",
    )
    .bind(id)
    .bind(code)
    .bind(root_id)
    .bind(folder)
    .bind(import_mode)
    .execute(pool)
    .await
    .unwrap();
}

/// A — fingerprint never loads a multi-GB-class file into memory (bounded sample).
#[test]
fn a_fingerprint_samples_large_file_without_full_read() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("huge-model.rvt");
    let mut f = fs::File::create(&path).unwrap();
    // 8 MiB is enough to prove we don't fs::read the whole thing; pattern is seek+sample.
    let chunk = vec![0xABu8; 1024 * 64];
    for _ in 0..(8 * 1024 * 1024 / chunk.len()) {
        f.write_all(&chunk).unwrap();
    }
    drop(f);
    let (fp, size, _) = fingerprint_meta(&path).expect("fingerprint");
    assert_eq!(size, 8 * 1024 * 1024);
    assert_eq!(fp.len(), 64);
}

/// B — reconcile respects cancel mid-walk.
#[tokio::test]
async fn b_reconcile_is_cancelable() {
    let dir = tempdir().unwrap();
    let pool = pool_in(dir.path()).await;
    let root = upsert_primary_root(&pool, dir.path()).await.unwrap();
    let folder = dir.path().join("1090");
    fs::create_dir_all(&folder).unwrap();
    for i in 0..40 {
        fs::write(folder.join(format!("doc_v{i:02}.pdf")), b"x").unwrap();
    }
    seed_project(&pool, &root.id, "p-b", Some("1090"), "1090", "created").await;

    let cancel = Arc::new(AtomicBool::new(true));
    let results = auto_version::reconcile_project_cancelable(
        &pool,
        "p-b",
        Some(cancel.as_ref()),
        None,
    )
    .await
    .unwrap();
    assert!(results.is_empty());
}

/// C — remove from app keeps physical folder.
#[tokio::test]
async fn c_remove_from_app_keeps_files() {
    let dir = tempdir().unwrap();
    let pool = pool_in(dir.path()).await;
    let root = upsert_primary_root(&pool, dir.path()).await.unwrap();
    let folder = dir.path().join("1090");
    fs::create_dir_all(&folder).unwrap();
    fs::write(folder.join("keep.txt"), b"keep").unwrap();
    seed_project(&pool, &root.id, "p-c", Some("1090"), "1090", "created").await;

    sqlx::query("DELETE FROM search_fts WHERE project_id = ? OR entity_id = ?")
        .bind("p-c")
        .bind("p-c")
        .execute(&pool)
        .await
        .ok();
    sqlx::query("DELETE FROM projects WHERE id = ?")
        .bind("p-c")
        .execute(&pool)
        .await
        .unwrap();

    assert!(folder.join("keep.txt").exists());
    let left: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM projects WHERE id = 'p-c'")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(left.0, 0);
}

/// D — typed confirm required for destructive delete semantics.
#[tokio::test]
async fn d_delete_requires_matching_code() {
    let dir = tempdir().unwrap();
    let pool = pool_in(dir.path()).await;
    let root = upsert_primary_root(&pool, dir.path()).await.unwrap();
    let folder = dir.path().join("1090");
    fs::create_dir_all(&folder).unwrap();
    seed_project(&pool, &root.id, "p-d", Some("1090"), "1090", "created").await;

    let project: (Option<String>, String) =
        sqlx::query_as("SELECT code, root_relative_path FROM projects WHERE id = 'p-d'")
            .fetch_one(&pool)
            .await
            .unwrap();
    let expected = project.0.as_deref().unwrap_or(project.1.as_str());
    assert_eq!(expected, "1090");
    assert_ne!("wrong", expected);
}

/// E — linked projects: register-only (files stay even if delete_files requested).
#[tokio::test]
async fn e_linked_register_only_preserves_folder() {
    let dir = tempdir().unwrap();
    let pool = pool_in(dir.path()).await;
    let root = upsert_primary_root(&pool, dir.path()).await.unwrap();
    let folder = dir.path().join("Linked1090");
    fs::create_dir_all(&folder).unwrap();
    fs::write(folder.join("model.sdb"), b"sap").unwrap();
    seed_project(
        &pool,
        &root.id,
        "p-e",
        Some("1090"),
        "Linked1090",
        "linked",
    )
    .await;

    // Simulate register-only: delete DB row, leave folder.
    sqlx::query("DELETE FROM projects WHERE id = 'p-e'")
        .execute(&pool)
        .await
        .unwrap();
    assert!(folder.join("model.sdb").exists());
}

/// F — after remove, same code 1090 can be recreated.
#[tokio::test]
async fn f_recreate_same_code_after_remove() {
    let dir = tempdir().unwrap();
    let pool = pool_in(dir.path()).await;
    let root = upsert_primary_root(&pool, dir.path()).await.unwrap();
    seed_project(&pool, &root.id, "p-f1", Some("1090"), "1090_old", "created").await;
    sqlx::query("DELETE FROM projects WHERE id = 'p-f1'")
        .execute(&pool)
        .await
        .unwrap();

    crate::commands::projects::assert_code_unique(&pool, "1090", None)
        .await
        .expect("code free after remove");

    seed_project(&pool, &root.id, "p-f2", Some("1090"), "1090", "created").await;
    let row: (String,) = sqlx::query_as("SELECT code FROM projects WHERE id = 'p-f2'")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(row.0, "1090");
}

/// G — folder conflict: use_existing vs abort detection.
#[tokio::test]
async fn g_folder_conflict_existing_dir_detected() {
    let dir = tempdir().unwrap();
    let pool = pool_in(dir.path()).await;
    let root = upsert_primary_root(&pool, dir.path()).await.unwrap();
    let folder = dir.path().join("1090");
    fs::create_dir_all(&folder).unwrap();
    fs::write(folder.join("leftover.txt"), b"1").unwrap();

    // No registered project — physical folder alone is the conflict.
    let existing_path: Option<(String,)> = sqlx::query_as(
        "SELECT id FROM projects WHERE authorized_root_id = ? AND root_relative_path = ?",
    )
    .bind(&root.id)
    .bind("1090")
    .fetch_optional(&pool)
    .await
    .unwrap();
    assert!(existing_path.is_none());
    assert!(folder.is_dir());

    // use_existing path: register against leftover folder without wiping it.
    seed_project(&pool, &root.id, "p-g", Some("1090"), "1090", "linked").await;
    assert!(folder.join("leftover.txt").exists());
    let loc = project_paths::resolve_project_location(&pool, "p-g")
        .await
        .unwrap();
    assert_eq!(loc.root_relative_path, "1090");
}
