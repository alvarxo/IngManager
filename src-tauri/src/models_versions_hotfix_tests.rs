//! Hotfix tests: versioned models → one logical doc; Nueva versión; delete logical; ghosts.
//!
//! A — `_v01`/`_v02` share group identity (not MOD-001 + MOD-002)
//! B — SIN/CON remain distinct with version suffixes
//! C — bulk register two .sdb → one doc with v01+v02, link only (no copy)
//! D — Nueva versión refuses empty when files missing
//! E — Nueva versión links existing file without copy
//! F — delete logical keeps physical files
//! G — physical trash marks document_files unavailable
//! H — cleanup marks missing + removes empty logicals without opening files

use std::fs;
use std::path::{Path, PathBuf};

use tempfile::tempdir;

use crate::auto_version::{parse_version_filename, process_path, register_baseline};
use crate::commands::documents::{
    cleanup_document_availability_inner, create_document_version_inner,
    delete_logical_document_inner, mark_document_files_unavailable_for_path, CreateVersionInput,
};
use crate::commands::engineering::{bulk_register_engineering_docs_inner, BulkRegisterInput};
use crate::db;
use crate::engineering_files::{
    count_principals, group_files, normalize_group_key, stems_are_distinct_models,
};
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

/// A — version suffixes are not identity.
#[test]
fn test_a_version_suffix_not_identity() {
    let codes = vec!["1126".into()];
    assert_eq!(
        normalize_group_key("1126 - MODELO_v01", &codes),
        normalize_group_key("1126 - MODELO_v02", &codes)
    );
    let names = vec![
        "1126 - MODELO_v01.sdb".into(),
        "1126 - MODELO_v02.sdb".into(),
    ];
    let groups = group_files(&names, &codes);
    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].version_principals.len(), 2);
    assert_eq!(count_principals(&names, &codes), 1);
}

/// B — SIN/CON stay distinct with versions.
#[test]
fn test_b_sin_con_with_versions() {
    let codes = vec!["1090".into()];
    assert!(stems_are_distinct_models(
        "1090 - MODELO SIN CAMIONES_v02",
        "1090 - MODELO CON CAMIONES_v02",
        &codes
    ));
    let p_sin = parse_version_filename("1090 - MODELO SIN CAMIONES_v02.sdb");
    let p_con = parse_version_filename("1090 - MODELO CON CAMIONES_v02.sdb");
    assert_ne!(p_sin.base_stem, p_con.base_stem);
}

/// C — bulk register versioned pair → one MOD with v01/v02, no copy.
#[tokio::test]
async fn test_c_bulk_register_one_doc_two_versions() {
    let dir = tempdir().unwrap();
    let pool = db::init_pool(&dir.path().join("t.db")).await.unwrap();
    let root_id = setup_root(&pool, dir.path()).await;
    let modelos = dir.path().join("1126").join("MODELOS");
    fs::create_dir_all(&modelos).unwrap();
    let v1 = modelos.join("Puente_v01.sdb");
    let v2 = modelos.join("Puente_v02.sdb");
    write_dummy_sdb(&v1);
    write_dummy_sdb(&v2);

    let project_id = seed_project(&pool, &root_id, Some("1126"), "1126").await;
    sqlx::query(
        "UPDATE categories SET relative_path = ? WHERE project_id = ? AND key = 'modelos'",
    )
    .bind("MODELOS")
    .bind(&project_id)
    .execute(&pool)
    .await
    .unwrap();

    for (name, path) in [("Puente_v01.sdb", &v1), ("Puente_v02.sdb", &v2)] {
        index_file(
            &pool,
            &project_id,
            &root_id,
            &format!("1126/MODELOS/{name}"),
            &path.display().to_string(),
            name,
            "modelos",
        )
        .await;
    }

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

    assert_eq!(result.created, 1, "must be one logical document");
    assert_eq!(result.document_ids.len(), 1);

    let docs: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM documents WHERE project_id = ?")
            .bind(&project_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(docs.0, 1);

    let code: (String,) =
        sqlx::query_as("SELECT business_code FROM documents WHERE project_id = ?")
            .bind(&project_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(code.0.contains("MOD-001"), "got {}", code.0);

    let vers: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM document_versions v
         JOIN documents d ON d.id = v.document_id WHERE d.project_id = ?",
    )
    .bind(&project_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(vers.0, 2);

    // Original bytes untouched (link, no copy into snapshots)
    assert_eq!(fs::read(&v1).unwrap(), b"INGMANAGER_FIXTURE_SDB");
    assert_eq!(fs::read(&v2).unwrap(), b"INGMANAGER_FIXTURE_SDB");
    let snap = dir.path().join(".ingmanager").join("snapshots");
    if snap.exists() {
        for e in walkdir::WalkDir::new(&snap).into_iter().flatten() {
            if e.file_type().is_file() {
                assert_ne!(fs::read(e.path()).unwrap_or_default(), b"INGMANAGER_FIXTURE_SDB");
            }
        }
    }
}

/// D — Nueva versión with missing physical files must not leave empty v02.
#[tokio::test]
async fn test_d_nueva_version_refuses_empty() {
    let dir = tempdir().unwrap();
    let pool = db::init_pool(&dir.path().join("t.db")).await.unwrap();
    let root_id = setup_root(&pool, dir.path()).await;
    let project_id = seed_project(&pool, &root_id, Some("1126"), "1126").await;
    let modelos = dir.path().join("1126").join("MODELOS");
    fs::create_dir_all(&modelos).unwrap();
    let sdb = modelos.join("Solo.sdb");
    write_dummy_sdb(&sdb);

    let cat: (String,) =
        sqlx::query_as("SELECT id FROM categories WHERE project_id = ? AND key = 'modelos'")
            .bind(&project_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    let doc_id = uuid::Uuid::new_v4().to_string();
    let ver_id = uuid::Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO documents (
            id, project_id, category_id, business_code, title, doc_type,
            current_version_id, status, created_at, updated_at
         ) VALUES (?, ?, ?, '1126-MOD-001', 'Solo', 'MOD', ?, 'active', datetime('now'), datetime('now'))",
    )
    .bind(&doc_id)
    .bind(&project_id)
    .bind(&cat.0)
    .bind(&ver_id)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO document_versions (
            id, document_id, version_number, label, created_at, is_snapshot, source_relative_path
         ) VALUES (?, ?, 1, 'v01', datetime('now'), 0, ?)",
    )
    .bind(&ver_id)
    .bind(&doc_id)
    .bind("1126/MODELOS/Solo.sdb")
    .execute(&pool)
    .await
    .unwrap();
    let file_id = uuid::Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO document_files (
            id, document_version_id, relative_path, file_name, extension,
            size_bytes, is_available, created_at
         ) VALUES (?, ?, '1126/MODELOS/Solo.sdb', 'Solo.sdb', 'sdb', 20, 1, datetime('now'))",
    )
    .bind(&file_id)
    .bind(&ver_id)
    .execute(&pool)
    .await
    .unwrap();

    // Physical file gone → Nueva versión must fail and not leave empty v02
    fs::remove_file(&sdb).unwrap();

    let state = crate::state::AppState::new(pool.clone(), dir.path().join("t.db"));
    let err = create_document_version_inner(
        &state,
        CreateVersionInput {
            document_id: doc_id.clone(),
            notes: Some("should fail".into()),
            source_absolute_path: None,
            from_current: Some(true),
        },
    )
    .await
    .expect_err("must refuse empty version");
    let msg = err.to_string();
    assert!(
        msg.contains("No se creó") || msg.contains("no disponible") || msg.contains("archivo"),
        "{msg}"
    );

    let vers: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM document_versions WHERE document_id = ?")
            .bind(&doc_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(vers.0, 1, "must not leave empty v02");
    let cur: (Option<String>,) =
        sqlx::query_as("SELECT current_version_id FROM documents WHERE id = ?")
            .bind(&doc_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(cur.0.as_deref(), Some(ver_id.as_str()));
}

/// E — link existing file as new version without copying.
#[tokio::test]
async fn test_e_nueva_version_links_without_copy() {
    let dir = tempdir().unwrap();
    let pool = db::init_pool(&dir.path().join("t.db")).await.unwrap();
    let root_id = setup_root(&pool, dir.path()).await;
    let project_id = seed_project(&pool, &root_id, Some("1126"), "1126").await;
    let modelos = dir.path().join("1126").join("MODELOS");
    fs::create_dir_all(&modelos).unwrap();
    let v1 = modelos.join("Base.sdb");
    let v2 = modelos.join("Base_v02.sdb");
    write_dummy_sdb(&v1);
    fs::write(&v2, b"VERSION_TWO_BYTES_____").unwrap();

    let cat: (String,) =
        sqlx::query_as("SELECT id FROM categories WHERE project_id = ? AND key = 'modelos'")
            .bind(&project_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    let doc_id = uuid::Uuid::new_v4().to_string();
    let ver_id = uuid::Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO documents (
            id, project_id, category_id, title, doc_type, current_version_id, status,
            created_at, updated_at
         ) VALUES (?, ?, ?, 'Base', 'MOD', ?, 'active', datetime('now'), datetime('now'))",
    )
    .bind(&doc_id)
    .bind(&project_id)
    .bind(&cat.0)
    .bind(&ver_id)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO document_versions (
            id, document_id, version_number, label, created_at, is_snapshot
         ) VALUES (?, ?, 1, 'v01', datetime('now'), 0)",
    )
    .bind(&ver_id)
    .bind(&doc_id)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO document_files (
            id, document_version_id, relative_path, file_name, extension,
            size_bytes, is_available, created_at
         ) VALUES (?, ?, '1126/MODELOS/Base.sdb', 'Base.sdb', 'sdb', 20, 1, datetime('now'))",
    )
    .bind(uuid::Uuid::new_v4().to_string())
    .bind(&ver_id)
    .execute(&pool)
    .await
    .unwrap();

    let state = crate::state::AppState::new(pool.clone(), dir.path().join("t.db"));
    let created = create_document_version_inner(
        &state,
        CreateVersionInput {
            document_id: doc_id.clone(),
            notes: Some("link v02".into()),
            source_absolute_path: Some(v2.display().to_string()),
            from_current: Some(false),
        },
    )
    .await
    .unwrap();
    assert_eq!(created.version_number, 2);
    assert_eq!(fs::read(&v2).unwrap(), b"VERSION_TWO_BYTES_____");

    let rel: (String,) = sqlx::query_as(
        "SELECT relative_path FROM document_files WHERE document_version_id = ?",
    )
    .bind(&created.id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(!rel.0.contains(".ingmanager/snapshots"));
    assert!(rel.0.ends_with("Base_v02.sdb"));
}

/// F — delete logical keeps files on disk.
#[tokio::test]
async fn test_f_delete_logical_keeps_files() {
    let dir = tempdir().unwrap();
    let pool = db::init_pool(&dir.path().join("t.db")).await.unwrap();
    let root_id = setup_root(&pool, dir.path()).await;
    let project_id = seed_project(&pool, &root_id, Some("1126"), "1126").await;
    let modelos = dir.path().join("1126").join("MODELOS");
    fs::create_dir_all(&modelos).unwrap();
    let sdb = modelos.join("Keep.sdb");
    write_dummy_sdb(&sdb);

    let cat: (String,) =
        sqlx::query_as("SELECT id FROM categories WHERE project_id = ? AND key = 'modelos'")
            .bind(&project_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    let doc_id = uuid::Uuid::new_v4().to_string();
    let ver_id = uuid::Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO documents (
            id, project_id, category_id, title, doc_type, current_version_id, status,
            created_at, updated_at
         ) VALUES (?, ?, ?, 'Keep', 'MOD', ?, 'active', datetime('now'), datetime('now'))",
    )
    .bind(&doc_id)
    .bind(&project_id)
    .bind(&cat.0)
    .bind(&ver_id)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO document_versions (
            id, document_id, version_number, label, created_at, is_snapshot
         ) VALUES (?, ?, 1, 'v01', datetime('now'), 0)",
    )
    .bind(&ver_id)
    .bind(&doc_id)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO document_files (
            id, document_version_id, relative_path, file_name, extension,
            size_bytes, is_available, created_at
         ) VALUES (?, ?, '1126/MODELOS/Keep.sdb', 'Keep.sdb', 'sdb', 20, 1, datetime('now'))",
    )
    .bind(uuid::Uuid::new_v4().to_string())
    .bind(&ver_id)
    .execute(&pool)
    .await
    .unwrap();

    delete_logical_document_inner(&pool, &doc_id).await.unwrap();
    assert!(sdb.is_file(), "physical file must remain");
    let left: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM documents WHERE id = ?")
        .bind(&doc_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(left.0, 0);
}

/// G — marking path unavailable after physical delete (ghost prevention).
#[tokio::test]
async fn test_g_physical_delete_marks_unavailable() {
    let dir = tempdir().unwrap();
    let pool = db::init_pool(&dir.path().join("t.db")).await.unwrap();
    let root_id = setup_root(&pool, dir.path()).await;
    let project_id = seed_project(&pool, &root_id, Some("1126"), "1126").await;
    let cat: (String,) =
        sqlx::query_as("SELECT id FROM categories WHERE project_id = ? AND key = 'modelos'")
            .bind(&project_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    let doc_id = uuid::Uuid::new_v4().to_string();
    let ver_id = uuid::Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO documents (
            id, project_id, category_id, title, doc_type, current_version_id, status,
            created_at, updated_at
         ) VALUES (?, ?, ?, 'Ghost', 'MOD', ?, 'active', datetime('now'), datetime('now'))",
    )
    .bind(&doc_id)
    .bind(&project_id)
    .bind(&cat.0)
    .bind(&ver_id)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO document_versions (
            id, document_id, version_number, label, created_at, is_snapshot
         ) VALUES (?, ?, 1, 'v01', datetime('now'), 0)",
    )
    .bind(&ver_id)
    .bind(&doc_id)
    .execute(&pool)
    .await
    .unwrap();
    let fid = uuid::Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO document_files (
            id, document_version_id, relative_path, file_name, extension,
            size_bytes, is_available, created_at
         ) VALUES (?, ?, '1126/MODELOS/Ghost.sdb', 'Ghost.sdb', 'sdb', 20, 1, datetime('now'))",
    )
    .bind(&fid)
    .bind(&ver_id)
    .execute(&pool)
    .await
    .unwrap();

    let n = mark_document_files_unavailable_for_path(&pool, "1126/MODELOS/Ghost.sdb")
        .await
        .unwrap();
    assert_eq!(n, 1);
    let avail: (i64,) = sqlx::query_as("SELECT is_available FROM document_files WHERE id = ?")
        .bind(&fid)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(avail.0, 0);
}

/// H — cleanup marks missing and deletes empty logicals.
#[tokio::test]
async fn test_h_cleanup_missing_and_empty() {
    let dir = tempdir().unwrap();
    let pool = db::init_pool(&dir.path().join("t.db")).await.unwrap();
    let root_id = setup_root(&pool, dir.path()).await;
    let project_id = seed_project(&pool, &root_id, Some("1126"), "1126").await;
    let cat: (String,) =
        sqlx::query_as("SELECT id FROM categories WHERE project_id = ? AND key = 'modelos'")
            .bind(&project_id)
            .fetch_one(&pool)
            .await
            .unwrap();

    let doc_id = uuid::Uuid::new_v4().to_string();
    let ver_id = uuid::Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO documents (
            id, project_id, category_id, business_code, title, doc_type,
            current_version_id, status, created_at, updated_at
         ) VALUES (?, ?, ?, '1126-MOD-099', 'Bad1126', 'MOD', ?, 'active', datetime('now'), datetime('now'))",
    )
    .bind(&doc_id)
    .bind(&project_id)
    .bind(&cat.0)
    .bind(&ver_id)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO document_versions (
            id, document_id, version_number, label, created_at, is_snapshot
         ) VALUES (?, ?, 1, 'v01', datetime('now'), 0)",
    )
    .bind(&ver_id)
    .bind(&doc_id)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO document_files (
            id, document_version_id, relative_path, file_name, extension,
            size_bytes, is_available, created_at
         ) VALUES (?, ?, '1126/MODELOS/Missing.sdb', 'Missing.sdb', 'sdb', 20, 1, datetime('now'))",
    )
    .bind(uuid::Uuid::new_v4().to_string())
    .bind(&ver_id)
    .execute(&pool)
    .await
    .unwrap();

    let result = cleanup_document_availability_inner(&pool, &project_id, true)
        .await
        .unwrap();
    assert!(result.marked_unavailable >= 1);
    assert_eq!(result.deleted_empty_documents, 1);
    let left: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM documents WHERE id = ?")
        .bind(&doc_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(left.0, 0);
}

/// Auto-version Save-As still matches after unify + links without requiring snapshot copy.
#[tokio::test]
async fn test_auto_version_links_save_as() {
    let dir = tempdir().unwrap();
    let pool = db::init_pool(&dir.path().join("t.db")).await.unwrap();
    let root_id = setup_root(&pool, dir.path()).await;
    let modelos = dir.path().join("1126").join("MODELOS");
    fs::create_dir_all(&modelos).unwrap();
    let project_id = seed_project(&pool, &root_id, Some("1126"), "1126").await;
    let base = modelos.join("Columna.sdb");
    write_dummy_sdb(&base);

    let cat: (String,) =
        sqlx::query_as("SELECT id FROM categories WHERE project_id = ? AND key = 'modelos'")
            .bind(&project_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    let doc_id = uuid::Uuid::new_v4().to_string();
    let ver_id = uuid::Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO documents (
            id, project_id, category_id, title, doc_type, current_version_id, status,
            created_at, updated_at
         ) VALUES (?, ?, ?, 'Columna', 'MOD', ?, 'active', datetime('now'), datetime('now'))",
    )
    .bind(&doc_id)
    .bind(&project_id)
    .bind(&cat.0)
    .bind(&ver_id)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO document_versions (
            id, document_id, version_number, label, created_at, is_snapshot
         ) VALUES (?, ?, 1, 'v01', datetime('now'), 0)",
    )
    .bind(&ver_id)
    .bind(&doc_id)
    .execute(&pool)
    .await
    .unwrap();
    register_baseline(&pool, &project_id, &doc_id, Some(&ver_id), &base)
        .await
        .unwrap();

    let v2 = modelos.join("Columna_v02.sdb");
    fs::copy(&base, &v2).unwrap();
    let result = process_path(&pool, &v2).await.unwrap().unwrap();
    assert_eq!(result.action, "version_created");
    assert_eq!(result.document_id.as_deref(), Some(doc_id.as_str()));

    let rel: (String,) = sqlx::query_as(
        "SELECT f.relative_path FROM document_files f
         JOIN document_versions v ON v.id = f.document_version_id
         WHERE v.document_id = ? AND v.version_number = 2",
    )
    .bind(&doc_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(!rel.0.contains("snapshots"));
    assert!(rel.0.ends_with("Columna_v02.sdb"));
    let _ = PathBuf::from(&rel.0);
}
