//! Tests A–J: organizer, inbox, auto-versioning (automatable cases).

use std::fs;

use sqlx::SqlitePool;
use tempfile::tempdir;

use crate::auto_version::{self, parse_version_filename};
use crate::db;
use crate::inbox::{self, InboxAssignInput};
use crate::organizer;
use crate::project_paths::upsert_primary_root;

async fn pool_in(dir: &std::path::Path) -> SqlitePool {
    let db_path = dir.join("test.db");
    db::init_pool(&db_path).await.expect("pool")
}

async fn seed_project(pool: &SqlitePool, root: &std::path::Path, folder: &str, id: &str) -> String {
    let ar = upsert_primary_root(pool, root).await.unwrap();
    let proj = root.join(folder);
    fs::create_dir_all(&proj).unwrap();
    // Unique optional code per project id (UNIQUE when set).
    let code = format!("26{}", &id.chars().filter(|c| c.is_ascii_alphanumeric()).collect::<String>());
    sqlx::query(
        "INSERT INTO projects (
            id, code, name, status, authorized_root_id, root_relative_path,
            import_mode, layout_mode, created_at, updated_at
         ) VALUES (?, ?, ?, 'active', ?, ?, 'created', 'standard',
            datetime('now'), datetime('now'))",
    )
    .bind(id)
    .bind(&code)
    .bind(folder)
    .bind(&ar.id)
    .bind(folder)
    .execute(pool)
    .await
    .unwrap();

    // Minimal category rows
    for (key, name, rel, ord) in [
        ("modelos", "Modelos", "02_MODELOS", 2),
        ("planos", "Planos", "03_PLANOS", 3),
        ("memorias", "Memorias", "04_MEMORIAS", 4),
    ] {
        sqlx::query(
            "INSERT INTO categories (id, project_id, key, name, relative_path, sort_order)
             VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(id)
        .bind(key)
        .bind(name)
        .bind(rel)
        .bind(ord)
        .execute(pool)
        .await
        .unwrap();
    }
    id.to_string()
}

async fn seed_document_with_file(
    pool: &SqlitePool,
    project_id: &str,
    title: &str,
    file_path: &std::path::Path,
    root: &std::path::Path,
) -> (String, String) {
    let cat: (String,) =
        sqlx::query_as("SELECT id FROM categories WHERE project_id = ? AND key = 'modelos'")
            .bind(project_id)
            .fetch_one(pool)
            .await
            .unwrap();
    let doc_id = uuid::Uuid::new_v4().to_string();
    let ver_id = uuid::Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO documents (
            id, project_id, category_id, title, current_version_id, status,
            created_at, updated_at
         ) VALUES (?, ?, ?, ?, ?, 'draft', datetime('now'), datetime('now'))",
    )
    .bind(&doc_id)
    .bind(project_id)
    .bind(&cat.0)
    .bind(title)
    .bind(&ver_id)
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO document_versions (id, document_id, version_number, label, created_at, is_snapshot)
         VALUES (?, ?, 1, 'v01', datetime('now'), 0)",
    )
    .bind(&ver_id)
    .bind(&doc_id)
    .execute(pool)
    .await
    .unwrap();

    let rel = file_path
        .strip_prefix(root)
        .unwrap()
        .to_string_lossy()
        .replace('\\', "/");
    let fname = file_path.file_name().unwrap().to_string_lossy().to_string();
    sqlx::query(
        "INSERT INTO document_files (
            id, document_version_id, relative_path, file_name, extension,
            is_available, created_at
         ) VALUES (?, ?, ?, ?, ?, 1, datetime('now'))",
    )
    .bind(uuid::Uuid::new_v4().to_string())
    .bind(&ver_id)
    .bind(&rel)
    .bind(&fname)
    .bind(file_path.extension().map(|e| e.to_string_lossy().to_string()))
    .execute(pool)
    .await
    .unwrap();

    auto_version::register_baseline(pool, project_id, &doc_id, Some(&ver_id), file_path)
        .await
        .unwrap();

    (doc_id, ver_id)
}

/// A — Organizer scan / classify
#[tokio::test]
async fn a_organizer_scan_classify() {
    let dir = tempdir().unwrap();
    let pool = pool_in(dir.path()).await;
    let pid = seed_project(&pool, dir.path(), "ProjA", "pa").await;
    let proj = dir.path().join("ProjA");
    fs::create_dir_all(proj.join("Modelos")).unwrap();
    fs::create_dir_all(proj.join("Planos")).unwrap();
    fs::create_dir_all(proj.join("misc_xyz")).unwrap();

    let scan = organizer::scan_scope(&pool, &dir.path().display().to_string())
        .await
        .unwrap();
    let item = scan.iter().find(|s| s.project_id.as_deref() == Some(&pid));
    assert!(item.is_some());
    let item = item.unwrap();
    assert!(item
        .classified
        .iter()
        .any(|c| c.category_key == "modelos" && c.matched));
    assert!(item.unmatched_folders.iter().any(|u| u == "misc_xyz"));
}

/// B — Organizer plan with virtual mappings
#[tokio::test]
async fn b_organizer_virtual_plan_and_apply() {
    let dir = tempdir().unwrap();
    let pool = pool_in(dir.path()).await;
    let pid = seed_project(&pool, dir.path(), "ProjB", "pb").await;
    let proj = dir.path().join("ProjB");
    fs::create_dir_all(proj.join("Modelos")).unwrap();

    let plan = organizer::build_plan(
        &pool,
        &dir.path().display().to_string(),
        "virtual",
        Some(vec![pid.clone()]),
    )
    .await
    .unwrap();
    assert_eq!(plan.session.mode, "virtual");
    assert!(plan.items.iter().any(|i| i.action == "map_virtual"));

    let applied = organizer::apply_plan(&pool, &plan.session.id).await.unwrap();
    assert!(applied.applied > 0);

    let maps = organizer::list_mappings(&pool, &pid).await.unwrap();
    assert!(maps.iter().any(|(_, k, p, m)| {
        k == "modelos" && p == "Modelos" && m == "virtual"
    }));
}

/// C — Physical reorganize with backup + journal
#[tokio::test]
async fn c_organizer_physical_backup_journal() {
    let dir = tempdir().unwrap();
    let pool = pool_in(dir.path()).await;
    let pid = seed_project(&pool, dir.path(), "ProjC", "pc").await;
    let proj = dir.path().join("ProjC");
    fs::create_dir_all(proj.join("Modelos")).unwrap();
    fs::write(proj.join("Modelos").join("a.sdb"), b"data").unwrap();

    let plan = organizer::build_plan(
        &pool,
        &dir.path().display().to_string(),
        "physical",
        Some(vec![pid]),
    )
    .await
    .unwrap();
    assert!(plan.items.iter().any(|i| i.action == "rename_folder"));

    let applied = organizer::apply_plan(&pool, &plan.session.id).await.unwrap();
    assert!(applied.backup_path.is_some());
    let backup = std::path::PathBuf::from(applied.backup_path.unwrap());
    assert!(backup.exists());
    assert!(proj.join("02_MODELOS").is_dir());
    assert!(!proj.join("Modelos").exists());
    assert!(applied.journal_entries > 0);
}

/// D — Global inbox staging
#[tokio::test]
async fn d_inbox_staging() {
    let dir = tempdir().unwrap();
    let pool = pool_in(dir.path()).await;
    let _ = upsert_primary_root(&pool, dir.path()).await.unwrap();
    let src = dir.path().join("drop.pdf");
    fs::write(&src, b"%PDF").unwrap();

    let staged = inbox::stage_paths(&pool, &[src.display().to_string()], true)
        .await
        .unwrap();
    assert_eq!(staged.len(), 1);
    assert_eq!(staged[0].status, "staged");
    assert!(std::path::Path::new(&staged[0].absolute_path).is_file());

    let listed = inbox::list_items(&pool).await.unwrap();
    assert_eq!(listed.len(), 1);
}

/// E — Inbox multi-project assign (default copy)
#[tokio::test]
async fn e_inbox_multi_project_copy() {
    let dir = tempdir().unwrap();
    let pool = pool_in(dir.path()).await;
    let p1 = seed_project(&pool, dir.path(), "P1", "p1").await;
    let p2 = seed_project(&pool, dir.path(), "P2", "p2").await;
    fs::create_dir_all(dir.path().join("P1").join("02_MODELOS")).unwrap();
    fs::create_dir_all(dir.path().join("P2").join("02_MODELOS")).unwrap();

    let src = dir.path().join("shared.docx");
    fs::write(&src, b"doc").unwrap();
    let staged = inbox::stage_paths(&pool, &[src.display().to_string()], true)
        .await
        .unwrap();

    let result = inbox::assign(
        &pool,
        InboxAssignInput {
            item_ids: vec![staged[0].id.clone()],
            project_ids: vec![p1, p2],
            category_key: "modelos".into(),
            mode: Some("copy".into()),
        },
    )
    .await
    .unwrap();
    assert_eq!(result.assigned, 2);
    assert!(dir.path().join("P1/02_MODELOS/shared.docx").is_file());
    assert!(dir.path().join("P2/02_MODELOS/shared.docx").is_file());
    // Default copy keeps inbox file
    assert!(std::path::Path::new(&staged[0].absolute_path).is_file());
}

/// F — Auto-version from Save As patterns
#[tokio::test]
async fn f_auto_version_save_as_pattern() {
    let dir = tempdir().unwrap();
    let pool = pool_in(dir.path()).await;
    let pid = seed_project(&pool, dir.path(), "PV", "pv").await;
    let modelos = dir.path().join("PV/02_MODELOS");
    fs::create_dir_all(&modelos).unwrap();
    let base = modelos.join("Estructura.sdb");
    fs::write(&base, b"v1").unwrap();
    let (doc_id, _) = seed_document_with_file(&pool, &pid, "Estructura", &base, dir.path()).await;

    let v2 = modelos.join("Estructura_v02.sdb");
    fs::write(&v2, b"v2-content").unwrap();
    let result = auto_version::process_path(&pool, &v2).await.unwrap().unwrap();
    assert_eq!(result.action, "version_created");
    assert_eq!(result.document_id.as_deref(), Some(doc_id.as_str()));

    let max: (i64,) = sqlx::query_as(
        "SELECT MAX(version_number) FROM document_versions WHERE document_id = ?",
    )
    .bind(&doc_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(max.0, 2);
}

/// G — Overwrite same path does not create false version
#[tokio::test]
async fn g_overwrite_no_false_version() {
    let dir = tempdir().unwrap();
    let pool = pool_in(dir.path()).await;
    let pid = seed_project(&pool, dir.path(), "PO", "po").await;
    let modelos = dir.path().join("PO/02_MODELOS");
    fs::create_dir_all(&modelos).unwrap();
    let base = modelos.join("Frame.sdb");
    fs::write(&base, b"orig").unwrap();
    let (doc_id, _) = seed_document_with_file(&pool, &pid, "Frame", &base, dir.path()).await;

    fs::write(&base, b"overwritten").unwrap();
    let result = auto_version::process_path(&pool, &base).await.unwrap().unwrap();
    assert_eq!(result.action, "overwrite_update");

    let max: (i64,) = sqlx::query_as(
        "SELECT MAX(version_number) FROM document_versions WHERE document_id = ?",
    )
    .bind(&doc_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(max.0, 1);
}

/// H — Ambiguous stem → pending
#[tokio::test]
async fn h_ambiguous_goes_pending() {
    let dir = tempdir().unwrap();
    let pool = pool_in(dir.path()).await;
    let pid = seed_project(&pool, dir.path(), "PA2", "pa2").await;
    let modelos = dir.path().join("PA2/02_MODELOS");
    fs::create_dir_all(&modelos).unwrap();

    let a = modelos.join("Shared.sdb");
    let b = modelos.join("Shared_backup.sdb");
    fs::write(&a, b"a").unwrap();
    fs::write(&b, b"b").unwrap();
    // Two docs with same stem via title
    let _ = seed_document_with_file(&pool, &pid, "Shared", &a, dir.path()).await;
    // Second document also titled Shared (ambiguous)
    let cat: (String,) =
        sqlx::query_as("SELECT id FROM categories WHERE project_id = ? AND key = 'modelos'")
            .bind(&pid)
            .fetch_one(&pool)
            .await
            .unwrap();
    let doc2 = uuid::Uuid::new_v4().to_string();
    let ver2 = uuid::Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO documents (id, project_id, category_id, title, current_version_id, status, created_at, updated_at)
         VALUES (?, ?, ?, 'Shared', ?, 'draft', datetime('now'), datetime('now'))",
    )
    .bind(&doc2)
    .bind(&pid)
    .bind(&cat.0)
    .bind(&ver2)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO document_versions (id, document_id, version_number, label, created_at, is_snapshot)
         VALUES (?, ?, 1, 'v01', datetime('now'), 0)",
    )
    .bind(&ver2)
    .bind(&doc2)
    .execute(&pool)
    .await
    .unwrap();
    auto_version::register_baseline(&pool, &pid, &doc2, Some(&ver2), &b)
        .await
        .unwrap();

    let neu = modelos.join("Shared_v03.sdb");
    fs::write(&neu, b"new").unwrap();
    let result = auto_version::process_path(&pool, &neu).await.unwrap().unwrap();
    assert_eq!(result.action, "pending");
    assert!(result.pending_id.is_some());

    let pending = auto_version::list_pending(&pool, Some(&pid)).await.unwrap();
    assert!(!pending.is_empty());
    assert_eq!(pending[0].reason, "ambiguous");
}

/// I — Reconcile on open
#[tokio::test]
async fn i_reconcile_on_open() {
    let dir = tempdir().unwrap();
    let pool = pool_in(dir.path()).await;
    let pid = seed_project(&pool, dir.path(), "PR", "pr").await;
    let modelos = dir.path().join("PR/02_MODELOS");
    fs::create_dir_all(&modelos).unwrap();
    let base = modelos.join("Columna.sdb");
    fs::write(&base, b"1").unwrap();
    let (doc_id, _) = seed_document_with_file(&pool, &pid, "Columna", &base, dir.path()).await;

    let v2 = modelos.join("Columna_v02.sdb");
    fs::write(&v2, b"2").unwrap();

    let results = auto_version::reconcile_project(&pool, &pid).await.unwrap();
    assert!(results.iter().any(|r| r.action == "version_created"));
    let max: (i64,) = sqlx::query_as(
        "SELECT MAX(version_number) FROM document_versions WHERE document_id = ?",
    )
    .bind(&doc_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(max.0, 2);
}

/// J — Migration incremental + authorized roots + no .ingmanager cycles
#[tokio::test]
async fn j_migration_roots_and_ignore_internal() {
    let dir = tempdir().unwrap();
    let pool = pool_in(dir.path()).await;
    // Migration 006 applied
    let tables: Vec<(String,)> = sqlx::query_as(
        "SELECT name FROM sqlite_master WHERE type='table' AND name IN (
            'project_folder_mappings', 'inbox_items', 'pending_version_events',
            'organizer_sessions', 'file_version_tracking'
         )",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(tables.len(), 5);

    let mig: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM __migrations WHERE version = 6")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(mig.0, 1);

    // Prior migrations preserved
    let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM __migrations")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert!(count.0 >= 6);

    let _ = seed_project(&pool, dir.path(), "PJ", "pj").await;
    let internal = dir
        .path()
        .join("PJ/.ingmanager/inbox/secret.txt");
    fs::create_dir_all(internal.parent().unwrap()).unwrap();
    fs::write(&internal, b"x").unwrap();
    let r = auto_version::process_path(&pool, &internal).await.unwrap();
    assert!(r.is_none(), "must ignore .ingmanager paths");
    assert!(organizer::is_organizer_internal(&internal));
}

#[test]
fn parse_patterns_unit() {
    assert!(parse_version_filename("a_v01.docx").is_version_pattern);
    assert!(parse_version_filename("a (1).docx").is_version_pattern);
    assert!(!parse_version_filename("a.docx").is_version_pattern);
}
