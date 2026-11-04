//! Hotfix: model identity independent of titles.
//!
//! A — UUID / business_code / title / physical paths are independent
//! B — Technical family key ≠ title; rename title leaves group_key
//! C — Explicit association wins over title/name inference (auto_version)
//! D — Sync adds missing disk versions onto 1126-MOD-001 (v01–v03)
//! E — bulk_register on already-registered group adds missing versions
//! F — Manual associate existing .sdb as new version (link, no copy)
//! G — Edit title keeps file links; second sync is idempotent

use std::fs;
use std::path::{Path, PathBuf};

use tempfile::tempdir;

use crate::auto_version::{parse_version_filename, process_path};
use crate::commands::engineering::{
    associate_model_version_inner, bulk_register_engineering_docs_inner,
    sync_model_versions_inner, AssociateModelVersionInput, BulkRegisterInput,
    SyncModelVersionsInput,
};
use crate::db;
use crate::engineering_files::{group_files, normalize_group_key};
use crate::project_paths::upsert_primary_root;

fn write_dummy_sdb(path: &Path) {
    fs::write(path, b"INGMANAGER_FIXTURE_SDB").unwrap();
}

async fn setup_root(pool: &sqlx::SqlitePool, root: &Path) -> String {
    let abs = root.display().to_string();
    db::set_setting(pool, "root_path", &abs).await.unwrap();
    upsert_primary_root(pool, root).await.unwrap().id
}

async fn seed_project(
    pool: &sqlx::SqlitePool,
    root_id: &str,
    code: Option<&str>,
    folder: &str,
) -> String {
    let id = uuid::Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO projects (
            id, code, name, status, authorized_root_id, root_relative_path,
            import_mode, layout_mode, created_at, updated_at
         ) VALUES (?, ?, ?, 'active', ?, ?, 'linked', 'mapped', datetime('now'), datetime('now'))",
    )
    .bind(&id)
    .bind(code)
    .bind(folder)
    .bind(root_id)
    .bind(folder)
    .execute(pool)
    .await
    .unwrap();

    for (key, name, rel, order) in [
        ("antecedentes", "Antecedentes", "01_ANTECEDENTES", 1),
        ("modelos", "Modelos", "02_MODELOS", 2),
        ("planos", "Planos", "03_PLANOS", 3),
        ("memorias", "Memorias", "04_MEMORIAS", 4),
        ("admin", "Administración", "05_ADMINISTRACION", 5),
        ("transmisiones", "Transmisiones", "06_TRANSMISIONES", 6),
        ("otros", "Otros", "07_OTROS", 7),
    ] {
        sqlx::query(
            "INSERT INTO categories (id, project_id, key, name, relative_path, sort_order)
             VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(&id)
        .bind(key)
        .bind(name)
        .bind(rel)
        .bind(order)
        .execute(pool)
        .await
        .unwrap();
    }
    id
}

async fn index_file(
    pool: &sqlx::SqlitePool,
    project_id: &str,
    root_id: &str,
    rel: &str,
    abs: &str,
    file_name: &str,
    category_key: &str,
) {
    let id = uuid::Uuid::new_v4().to_string();
    let ext = Path::new(file_name)
        .extension()
        .map(|e| e.to_string_lossy().to_string());
    sqlx::query(
        "INSERT INTO indexed_files (
            id, project_id, authorized_root_id, relative_path, absolute_path,
            file_name, extension, size_bytes, is_directory, is_available,
            category_key, indexed_at
         ) VALUES (?, ?, ?, ?, ?, ?, ?, 20, 0, 1, ?, datetime('now'))",
    )
    .bind(&id)
    .bind(project_id)
    .bind(root_id)
    .bind(rel)
    .bind(abs)
    .bind(file_name)
    .bind(&ext)
    .bind(category_key)
    .execute(pool)
    .await
    .unwrap();
}

async fn fixture_1126_modelos(
    pool: &sqlx::SqlitePool,
    root: &Path,
    versions: &[&str],
) -> (String, String, PathBuf) {
    let root_id = setup_root(pool, root).await;
    let modelos = root.join("1126").join("02_MODELOS");
    fs::create_dir_all(&modelos).unwrap();
    let project_id = seed_project(pool, &root_id, Some("1126"), "1126").await;

    for name in versions {
        let path = modelos.join(name);
        write_dummy_sdb(&path);
        index_file(
            pool,
            &project_id,
            &root_id,
            &format!("1126/02_MODELOS/{name}"),
            &path.display().to_string(),
            name,
            "modelos",
        )
        .await;
    }
    (project_id, root_id, modelos)
}

/// A — UUID / code / title / physical paths are independent fields.
#[tokio::test]
async fn test_a_uuid_code_title_files_independent() {
    let dir = tempdir().unwrap();
    let pool = db::init_pool(&dir.path().join("t.db")).await.unwrap();
    let (project_id, _, modelos) = fixture_1126_modelos(
        &pool,
        dir.path(),
        &["1126 - MODELO_v01.sdb"],
    )
    .await;

    let result = bulk_register_engineering_docs_inner(
        &pool,
        BulkRegisterInput {
            project_id: project_id.clone(),
            category_key: "modelos".into(),
            group_keys: None,
            assign_codes: Some(true),
        },
    )
    .await
    .unwrap();
    assert_eq!(result.created, 1);
    let doc_id = &result.document_ids[0];

    let row: (String, Option<String>, String) =
        sqlx::query_as("SELECT id, business_code, title FROM documents WHERE id = ?")
            .bind(doc_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_ne!(row.0, row.2, "UUID must not equal title");
    let code = row.1.expect("business code assigned");
    assert!(code.contains("MOD"), "expected MOD code, got {code}");
    assert_ne!(code, row.2, "code ≠ title");
    assert_ne!(code, row.0, "code ≠ UUID");

    let path: (String,) = sqlx::query_as(
        "SELECT f.relative_path FROM document_files f
         JOIN document_versions v ON v.id = f.document_version_id
         WHERE v.document_id = ? LIMIT 1",
    )
    .bind(doc_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(
        path.0.contains("MODELO_v01"),
        "physical path independent of UUID/code/title"
    );
    assert!(modelos.join("1126 - MODELO_v01.sdb").is_file());
}

/// B — Technical family key ≠ title; renaming title leaves group_key.
#[tokio::test]
async fn test_b_family_key_not_title() {
    let dir = tempdir().unwrap();
    let pool = db::init_pool(&dir.path().join("t.db")).await.unwrap();
    let (project_id, _, _) = fixture_1126_modelos(
        &pool,
        dir.path(),
        &["1126 - MODELO_v01.sdb", "1126 - MODELO_v02.sdb"],
    )
    .await;

    let result = bulk_register_engineering_docs_inner(
        &pool,
        BulkRegisterInput {
            project_id: project_id.clone(),
            category_key: "modelos".into(),
            group_keys: None,
            assign_codes: Some(true),
        },
    )
    .await
    .unwrap();
    let doc_id = result.document_ids[0].clone();

    let before: (String, String) = sqlx::query_as(
        "SELECT group_key, title FROM engineering_groups WHERE document_id = ?",
    )
    .bind(&doc_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    let family = before.0.clone();
    assert_eq!(
        family,
        normalize_group_key("1126 - MODELO_v01", &["1126".into()])
    );

    // Same columns as update_document — title only; never touches group_key / paths.
    sqlx::query(
        "UPDATE documents SET title = ?, updated_at = datetime('now') WHERE id = ?",
    )
    .bind("Puente Norte custom")
    .bind(&doc_id)
    .execute(&pool)
    .await
    .unwrap();

    let after_doc: (String,) = sqlx::query_as("SELECT title FROM documents WHERE id = ?")
        .bind(&doc_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(after_doc.0, "Puente Norte custom");

    let after_eg: (String,) =
        sqlx::query_as("SELECT group_key FROM engineering_groups WHERE document_id = ?")
            .bind(&doc_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(after_eg.0, family, "group_key must not change with title");

    let file_key: (Option<String>,) = sqlx::query_as(
        "SELECT f.engineering_group_key FROM document_files f
         JOIN document_versions v ON v.id = f.document_version_id
         WHERE v.document_id = ? LIMIT 1",
    )
    .bind(&doc_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(file_key.0.as_deref(), Some(family.as_str()));
}

/// C — Explicit association wins over title inference for auto_version.
#[tokio::test]
async fn test_c_explicit_wins_over_title_inference() {
    let dir = tempdir().unwrap();
    let pool = db::init_pool(&dir.path().join("t.db")).await.unwrap();
    let (project_id, _, modelos) = fixture_1126_modelos(
        &pool,
        dir.path(),
        &["1126 - MODELO_v01.sdb"],
    )
    .await;

    let result = bulk_register_engineering_docs_inner(
        &pool,
        BulkRegisterInput {
            project_id: project_id.clone(),
            category_key: "modelos".into(),
            group_keys: None,
            assign_codes: Some(true),
        },
    )
    .await
    .unwrap();
    let doc_id = result.document_ids[0].clone();

    // Rename title to something that would falsely match a different stem if titles were used.
    sqlx::query("UPDATE documents SET title = ? WHERE id = ?")
        .bind("otro modelo distinto")
        .bind(&doc_id)
        .execute(&pool)
        .await
        .unwrap();

    // Create a decoy document whose title matches the file stem — must NOT steal association.
    let decoy = uuid::Uuid::new_v4().to_string();
    let cat: (String,) =
        sqlx::query_as("SELECT id FROM categories WHERE project_id = ? AND key = 'modelos'")
            .bind(&project_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    sqlx::query(
        "INSERT INTO documents (
            id, project_id, category_id, business_code, title, doc_type,
            current_version_id, status, created_at, updated_at
         ) VALUES (?, ?, ?, '1126-MOD-999', 'modelo', 'MOD', NULL, 'active', datetime('now'), datetime('now'))",
    )
    .bind(&decoy)
    .bind(&project_id)
    .bind(&cat.0)
    .execute(&pool)
    .await
    .unwrap();

    let v2 = modelos.join("1126 - MODELO_v02.sdb");
    write_dummy_sdb(&v2);
    let parsed = parse_version_filename("1126 - MODELO_v02.sdb");
    assert!(parsed.is_version_pattern);

    let outcome = process_path(&pool, &v2).await.unwrap();
    let Some(res) = outcome else {
        panic!("expected auto-version result");
    };
    assert_eq!(
        res.document_id.as_deref(),
        Some(doc_id.as_str()),
        "must attach to explicit family doc, not title-matching decoy; got {:?}",
        res
    );
    assert_ne!(res.document_id.as_deref(), Some(decoy.as_str()));
}

/// D — Sync adds missing versions for 1126-MOD-001 (v01–v03).
#[tokio::test]
async fn test_d_sync_1126_mod_001_v01_v03() {
    let dir = tempdir().unwrap();
    let pool = db::init_pool(&dir.path().join("t.db")).await.unwrap();
    let (project_id, _, _) = fixture_1126_modelos(
        &pool,
        dir.path(),
        &[
            "1126 - MODELO_v01.sdb",
            "1126 - MODELO_v02.sdb",
            "1126 - MODELO_v03.sdb",
        ],
    )
    .await;

    // Register only via first two by filtering… create with all three then delete v03 link? 
    // Better: register all, delete v03 version rows to simulate incomplete registration,
    // then sync.
    let first = bulk_register_engineering_docs_inner(
        &pool,
        BulkRegisterInput {
            project_id: project_id.clone(),
            category_key: "modelos".into(),
            group_keys: None,
            assign_codes: Some(true),
        },
    )
    .await
    .unwrap();
    assert_eq!(first.created, 1);
    let doc_id = first.document_ids[0].clone();

    // Force business code to 1126-MOD-001 for the case name.
    sqlx::query("UPDATE documents SET business_code = '1126-MOD-001' WHERE id = ?")
        .bind(&doc_id)
        .execute(&pool)
        .await
        .unwrap();

    // Remove the highest version (v03) from SQLite only — file stays on disk.
    let victim: (String,) = sqlx::query_as(
        "SELECT id FROM document_versions WHERE document_id = ?
         ORDER BY version_number DESC LIMIT 1",
    )
    .bind(&doc_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    let keep: (String,) = sqlx::query_as(
        "SELECT id FROM document_versions WHERE document_id = ?
         ORDER BY version_number ASC LIMIT 1",
    )
    .bind(&doc_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    sqlx::query("DELETE FROM document_files WHERE document_version_id = ?")
        .bind(&victim.0)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM document_versions WHERE id = ?")
        .bind(&victim.0)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE documents SET current_version_id = ? WHERE id = ?")
        .bind(&keep.0)
        .bind(&doc_id)
        .execute(&pool)
        .await
        .unwrap();

    let count_before: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM document_versions WHERE document_id = ?")
            .bind(&doc_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(count_before.0, 2);

    let sync = sync_model_versions_inner(
        &pool,
        SyncModelVersionsInput {
            project_id: project_id.clone(),
            category_key: Some("modelos".into()),
            document_id: Some(doc_id.clone()),
        },
    )
    .await
    .unwrap();
    assert_eq!(sync.versions_added, 1, "must add missing v03");
    assert_eq!(sync.documents_touched, 1);

    let count_after: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM document_versions WHERE document_id = ?")
            .bind(&doc_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(count_after.0, 3);

    let code: (String,) = sqlx::query_as("SELECT business_code FROM documents WHERE id = ?")
        .bind(&doc_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(code.0, "1126-MOD-001");
}

/// E — bulk_register on already-registered group adds missing versions (not permanent skip).
#[tokio::test]
async fn test_e_bulk_register_adds_missing_versions() {
    let dir = tempdir().unwrap();
    let pool = db::init_pool(&dir.path().join("t.db")).await.unwrap();
    let (project_id, root_id, modelos) = fixture_1126_modelos(
        &pool,
        dir.path(),
        &["1126 - MODELO_v01.sdb"],
    )
    .await;

    let first = bulk_register_engineering_docs_inner(
        &pool,
        BulkRegisterInput {
            project_id: project_id.clone(),
            category_key: "modelos".into(),
            group_keys: None,
            assign_codes: Some(true),
        },
    )
    .await
    .unwrap();
    assert_eq!(first.created, 1);
    assert_eq!(first.versions_added, 0);

    // Appear v02 + v03 on disk after initial register.
    for name in ["1126 - MODELO_v02.sdb", "1126 - MODELO_v03.sdb"] {
        let path = modelos.join(name);
        write_dummy_sdb(&path);
        index_file(
            &pool,
            &project_id,
            &root_id,
            &format!("1126/02_MODELOS/{name}"),
            &path.display().to_string(),
            name,
            "modelos",
        )
        .await;
    }

    let second = bulk_register_engineering_docs_inner(
        &pool,
        BulkRegisterInput {
            project_id: project_id.clone(),
            category_key: "modelos".into(),
            group_keys: None,
            assign_codes: Some(true),
        },
    )
    .await
    .unwrap();
    assert_eq!(second.created, 0, "must not create a second MOD");
    assert_eq!(second.versions_added, 2, "must add v02 and v03");

    let doc_id = &first.document_ids[0];
    let n: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM document_versions WHERE document_id = ?")
            .bind(doc_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(n.0, 3);

    let docs: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM documents WHERE project_id = ?")
            .bind(&project_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(docs.0, 1);
}

/// F — Manual associate existing .sdb as new version (link, no copy).
#[tokio::test]
async fn test_f_manual_associate_as_version() {
    let dir = tempdir().unwrap();
    let pool = db::init_pool(&dir.path().join("t.db")).await.unwrap();
    let (project_id, _, modelos) = fixture_1126_modelos(
        &pool,
        dir.path(),
        &["1126 - MODELO_v01.sdb"],
    )
    .await;

    let first = bulk_register_engineering_docs_inner(
        &pool,
        BulkRegisterInput {
            project_id: project_id.clone(),
            category_key: "modelos".into(),
            group_keys: None,
            assign_codes: Some(true),
        },
    )
    .await
    .unwrap();
    let doc_id = first.document_ids[0].clone();

    let v2 = modelos.join("1126 - MODELO_v02.sdb");
    write_dummy_sdb(&v2);
    let before_bytes = fs::read(&v2).unwrap();

    let ver = associate_model_version_inner(
        &pool,
        AssociateModelVersionInput {
            document_id: doc_id.clone(),
            absolute_path: v2.display().to_string(),
            notes: Some("manual".into()),
        },
    )
    .await
    .unwrap();
    assert_eq!(ver.version_number, 2);
    assert_eq!(ver.notes.as_deref(), Some("manual"));

    // No copy under snapshots
    let snap = dir.path().join(".ingmanager").join("snapshots");
    assert!(!snap.exists() || fs::read_dir(&snap).unwrap().count() == 0);
    assert_eq!(fs::read(&v2).unwrap(), before_bytes);
    assert!(v2.is_file(), "original remains in place");

    let linked: (String,) = sqlx::query_as(
        "SELECT f.relative_path FROM document_files f WHERE f.document_version_id = ?",
    )
    .bind(&ver.id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(linked.0.ends_with("1126 - MODELO_v02.sdb"));
}

/// G — Edit title keeps links; second sync is idempotent.
#[tokio::test]
async fn test_g_title_edit_and_idempotent_sync() {
    let dir = tempdir().unwrap();
    let pool = db::init_pool(&dir.path().join("t.db")).await.unwrap();
    let (project_id, _, _) = fixture_1126_modelos(
        &pool,
        dir.path(),
        &[
            "1126 - MODELO_v01.sdb",
            "1126 - MODELO_v02.sdb",
            "1126 - MODELO_v03.sdb",
        ],
    )
    .await;

    let first = bulk_register_engineering_docs_inner(
        &pool,
        BulkRegisterInput {
            project_id: project_id.clone(),
            category_key: "modelos".into(),
            group_keys: None,
            assign_codes: Some(true),
        },
    )
    .await
    .unwrap();
    let doc_id = first.document_ids[0].clone();

    let paths_before: Vec<(String,)> = sqlx::query_as(
        "SELECT f.relative_path FROM document_files f
         JOIN document_versions v ON v.id = f.document_version_id
         WHERE v.document_id = ? ORDER BY f.relative_path",
    )
    .bind(&doc_id)
    .fetch_all(&pool)
    .await
    .unwrap();

    sqlx::query(
        "UPDATE documents SET title = ?, notes = ?, updated_at = datetime('now') WHERE id = ?",
    )
    .bind("Título personalizado 1126")
    .bind("nota")
    .bind(&doc_id)
    .execute(&pool)
    .await
    .unwrap();

    let paths_after: Vec<(String,)> = sqlx::query_as(
        "SELECT f.relative_path FROM document_files f
         JOIN document_versions v ON v.id = f.document_version_id
         WHERE v.document_id = ? ORDER BY f.relative_path",
    )
    .bind(&doc_id)
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(paths_before, paths_after, "title edit must not break file links");

    let sync1 = sync_model_versions_inner(
        &pool,
        SyncModelVersionsInput {
            project_id: project_id.clone(),
            category_key: Some("modelos".into()),
            document_id: Some(doc_id.clone()),
        },
    )
    .await
    .unwrap();
    assert_eq!(sync1.versions_added, 0);

    let sync2 = sync_model_versions_inner(
        &pool,
        SyncModelVersionsInput {
            project_id: project_id.clone(),
            category_key: Some("modelos".into()),
            document_id: Some(doc_id.clone()),
        },
    )
    .await
    .unwrap();
    assert_eq!(sync2.versions_added, 0, "idempotent");
    assert_eq!(sync2.already_linked, 3);

    let n: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM document_versions WHERE document_id = ?")
            .bind(&doc_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(n.0, 3);

    // Sanity: family still groups three principals
    let names = vec![
        "1126 - MODELO_v01.sdb".into(),
        "1126 - MODELO_v02.sdb".into(),
        "1126 - MODELO_v03.sdb".into(),
    ];
    let groups = group_files(&names, &["1126".into()]);
    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].version_principals.len(), 3);
}
