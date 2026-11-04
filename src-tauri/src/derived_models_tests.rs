//! Derived models — Tests A–G (Part A).
//!
//! A — New UUID + new MOD code; origin unchanged
//! B — SDB copied: same bytes, distinct path/stem/group_key
//! C — deriva_de relationship (derived → origin)
//! D — Optional calc copy: new paths; origin calc unchanged
//! E — Without copy_calc: derived has no calc; origin intact
//! F — No promote snapshots under origin for this op
//! G — Guards: non-modelos / missing SDB; engineering_groups distinct

use std::fs;
use std::path::Path;

use tempfile::tempdir;

use crate::db;
use crate::derived_models::{
    allocate_derived_working_path, create_derived_model_inner, CreateDerivedModelInput,
    RELATION_DERIVA_DE,
};
use crate::engineering_files::normalize_group_key;
use crate::model_spreadsheets::{
    associate_model_spreadsheet_inner, AssociateModelSpreadsheetInput, SHEET_KIND_CALC,
};
use crate::project_paths::upsert_primary_root;
use crate::state::AppState;

fn write_bytes(path: &Path, bytes: &[u8]) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(path, bytes).unwrap();
}

async fn setup_root(pool: &sqlx::SqlitePool, root: &Path) -> String {
    let abs = root.display().to_string();
    db::set_setting(pool, "root_path", &abs).await.unwrap();
    upsert_primary_root(pool, root).await.unwrap().id
}

async fn seed_project(pool: &sqlx::SqlitePool, root_id: &str, folder: &str) -> String {
    let id = uuid::Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO projects (
            id, code, name, status, authorized_root_id, root_relative_path,
            import_mode, layout_mode, created_at, updated_at
         ) VALUES (?, '1126', ?, 'active', ?, ?, 'linked', 'mapped', datetime('now'), datetime('now'))",
    )
    .bind(&id)
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

async fn seed_model_doc(
    pool: &sqlx::SqlitePool,
    project_id: &str,
    title: &str,
    business_code: &str,
    rel: &str,
    file_name: &str,
    abs: &Path,
    bytes: &[u8],
) -> (String, String, String) {
    let cat: (String,) =
        sqlx::query_as("SELECT id FROM categories WHERE project_id = ? AND key = 'modelos'")
            .bind(project_id)
            .fetch_one(pool)
            .await
            .unwrap();
    let doc_id = uuid::Uuid::new_v4().to_string();
    let ver_id = uuid::Uuid::new_v4().to_string();
    let file_id = uuid::Uuid::new_v4().to_string();
    write_bytes(abs, bytes);

    sqlx::query(
        "INSERT INTO documents (
            id, project_id, category_id, business_code, title, doc_type, status,
            created_at, updated_at, current_version_id
         ) VALUES (?, ?, ?, ?, ?, 'MOD', 'active', datetime('now'), datetime('now'), ?)",
    )
    .bind(&doc_id)
    .bind(project_id)
    .bind(&cat.0)
    .bind(business_code)
    .bind(title)
    .bind(&ver_id)
    .execute(pool)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO document_versions (
            id, document_id, version_number, label, created_at, is_snapshot
         ) VALUES (?, ?, 1, 'v01', datetime('now'), 0)",
    )
    .bind(&ver_id)
    .bind(&doc_id)
    .execute(pool)
    .await
    .unwrap();

    let gk = normalize_group_key(
        Path::new(file_name)
            .file_stem()
            .unwrap()
            .to_str()
            .unwrap(),
        &["1126".into()],
    );
    sqlx::query(
        "INSERT INTO document_files (
            id, document_version_id, relative_path, file_name, extension,
            size_bytes, is_available, format_label, created_at, file_role,
            engineering_group_key
         ) VALUES (?, ?, ?, ?, 'sdb', ?, 1, 'SDB', datetime('now'), 'principal', ?)",
    )
    .bind(&file_id)
    .bind(&ver_id)
    .bind(rel)
    .bind(file_name)
    .bind(bytes.len() as i64)
    .bind(&gk)
    .execute(pool)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO engineering_groups (
            id, project_id, category_key, group_key, title,
            principal_file_name, document_id, created_at, updated_at
         ) VALUES (?, ?, 'modelos', ?, ?, ?, ?, datetime('now'), datetime('now'))",
    )
    .bind(uuid::Uuid::new_v4().to_string())
    .bind(project_id)
    .bind(&gk)
    .bind(title)
    .bind(file_name)
    .bind(&doc_id)
    .execute(pool)
    .await
    .unwrap();

    (doc_id, ver_id, file_id)
}

async fn seed_memoria(pool: &sqlx::SqlitePool, project_id: &str, abs: &Path) -> String {
    let cat: (String,) =
        sqlx::query_as("SELECT id FROM categories WHERE project_id = ? AND key = 'memorias'")
            .bind(project_id)
            .fetch_one(pool)
            .await
            .unwrap();
    let doc_id = uuid::Uuid::new_v4().to_string();
    let ver_id = uuid::Uuid::new_v4().to_string();
    write_bytes(abs, b"DOCX");
    sqlx::query(
        "INSERT INTO documents (
            id, project_id, category_id, title, status, created_at, updated_at, current_version_id
         ) VALUES (?, ?, ?, 'Memoria', 'desarrollo', datetime('now'), datetime('now'), ?)",
    )
    .bind(&doc_id)
    .bind(project_id)
    .bind(&cat.0)
    .bind(&ver_id)
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO document_versions (
            id, document_id, version_number, label, created_at, is_snapshot
         ) VALUES (?, ?, 1, 'v01', datetime('now'), 0)",
    )
    .bind(&ver_id)
    .bind(&doc_id)
    .execute(pool)
    .await
    .unwrap();
    doc_id
}

#[tokio::test]
async fn test_a_new_uuid_and_mod_code_origin_unchanged() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("t.db");
    let pool = db::init_pool(&db_path).await.unwrap();
    let root = dir.path().join("root");
    let root_id = setup_root(&pool, &root).await;
    let project_id = seed_project(&pool, &root_id, "P1").await;
    let modelos = root.join("P1/02_MODELOS");
    let sdb = modelos.join("MODELO BASE.sdb");
    let (origin_id, _, _) = seed_model_doc(
        &pool,
        &project_id,
        "Modelo base",
        "1126-MOD-001",
        "P1/02_MODELOS/MODELO BASE.sdb",
        "MODELO BASE.sdb",
        &sdb,
        b"SDB_ORIGIN_BYTES_AAAA",
    )
    .await;
    let state = AppState::new(pool.clone(), db_path);

    let result = create_derived_model_inner(
        &state,
        CreateDerivedModelInput {
            source_document_id: origin_id.clone(),
            title: Some("Modelo derivado".into()),
            copy_calc_sheets: Some(false),
            notes: None,
        },
    )
    .await
    .unwrap();

    assert_ne!(result.document_id, origin_id);
    assert_eq!(result.business_code.as_deref(), Some("1126-MOD-002"));
    assert_eq!(result.title, "Modelo derivado");

    let origin: (String, Option<String>, String) = sqlx::query_as(
        "SELECT id, business_code, title FROM documents WHERE id = ?",
    )
    .bind(&origin_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(origin.1.as_deref(), Some("1126-MOD-001"));
    assert_eq!(origin.2, "Modelo base");
    assert!(sdb.is_file());
    assert_eq!(fs::read(&sdb).unwrap(), b"SDB_ORIGIN_BYTES_AAAA");
}

#[tokio::test]
async fn test_b_sdb_copied_distinct_path_and_group_key() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("t.db");
    let pool = db::init_pool(&db_path).await.unwrap();
    let root = dir.path().join("root");
    let root_id = setup_root(&pool, &root).await;
    let project_id = seed_project(&pool, &root_id, "P1").await;
    let sdb = root.join("P1/02_MODELOS/PUENTE.sdb");
    let (origin_id, _, _) = seed_model_doc(
        &pool,
        &project_id,
        "Puente",
        "1126-MOD-001",
        "P1/02_MODELOS/PUENTE.sdb",
        "PUENTE.sdb",
        &sdb,
        b"BYTES_PUENTE_SDB",
    )
    .await;
    let state = AppState::new(pool.clone(), db_path);

    let result = create_derived_model_inner(
        &state,
        CreateDerivedModelInput {
            source_document_id: origin_id,
            title: None,
            copy_calc_sheets: Some(false),
            notes: None,
        },
    )
    .await
    .unwrap();

    let dest = Path::new(&result.copied_sdb_absolute_path);
    assert!(dest.is_file());
    assert_ne!(dest, sdb.as_path());
    assert_eq!(fs::read(dest).unwrap(), b"BYTES_PUENTE_SDB");
    assert!(dest
        .file_name()
        .unwrap()
        .to_string_lossy()
        .contains("_derivado"));
    let origin_gk = normalize_group_key("PUENTE", &["1126".into()]);
    assert_ne!(result.group_key, origin_gk);
    assert!(result.group_key.contains("derivado"));
}

#[tokio::test]
async fn test_c_deriva_de_relationship() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("t.db");
    let pool = db::init_pool(&db_path).await.unwrap();
    let root = dir.path().join("root");
    let root_id = setup_root(&pool, &root).await;
    let project_id = seed_project(&pool, &root_id, "P1").await;
    let sdb = root.join("P1/02_MODELOS/M.sdb");
    let (origin_id, _, _) = seed_model_doc(
        &pool,
        &project_id,
        "M",
        "1126-MOD-001",
        "P1/02_MODELOS/M.sdb",
        "M.sdb",
        &sdb,
        b"XX",
    )
    .await;
    let state = AppState::new(pool.clone(), db_path);
    let result = create_derived_model_inner(
        &state,
        CreateDerivedModelInput {
            source_document_id: origin_id.clone(),
            title: None,
            copy_calc_sheets: Some(false),
            notes: None,
        },
    )
    .await
    .unwrap();

    let rel: (String, String, String) = sqlx::query_as(
        "SELECT source_document_id, target_document_id, relation_type
         FROM document_relationships WHERE id = ?",
    )
    .bind(&result.relationship_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(rel.0, result.document_id);
    assert_eq!(rel.1, origin_id);
    assert_eq!(rel.2, RELATION_DERIVA_DE);
}

#[tokio::test]
async fn test_d_optional_calc_copy_new_paths() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("t.db");
    let pool = db::init_pool(&db_path).await.unwrap();
    let root = dir.path().join("root");
    let root_id = setup_root(&pool, &root).await;
    let project_id = seed_project(&pool, &root_id, "P1").await;
    let sdb = root.join("P1/02_MODELOS/MOD.sdb");
    let (origin_id, ver_id, _) = seed_model_doc(
        &pool,
        &project_id,
        "Mod",
        "1126-MOD-001",
        "P1/02_MODELOS/MOD.sdb",
        "MOD.sdb",
        &sdb,
        b"SDB",
    )
    .await;
    let calc = root.join("P1/02_MODELOS/calc_live.xlsx");
    write_bytes(&calc, b"CALC_ORIGIN_XLSX");
    let state = AppState::new(pool.clone(), db_path);
    associate_model_spreadsheet_inner(
        &state,
        AssociateModelSpreadsheetInput {
            document_version_id: ver_id.clone(),
            absolute_path: calc.display().to_string(),
            sheet_kind: SHEET_KIND_CALC.into(),
            version_policy: None,
        },
    )
    .await
    .unwrap();

    let result = create_derived_model_inner(
        &state,
        CreateDerivedModelInput {
            source_document_id: origin_id,
            title: None,
            copy_calc_sheets: Some(true),
            notes: None,
        },
    )
    .await
    .unwrap();

    assert_eq!(result.copied_calc_absolute_paths.len(), 1);
    let dest_calc = Path::new(&result.copied_calc_absolute_paths[0]);
    assert!(dest_calc.is_file());
    assert_ne!(dest_calc, calc.as_path());
    assert_eq!(fs::read(dest_calc).unwrap(), b"CALC_ORIGIN_XLSX");
    assert_eq!(fs::read(&calc).unwrap(), b"CALC_ORIGIN_XLSX");

    let origin_calc_rel: (String,) = sqlx::query_as(
        "SELECT relative_path FROM document_files
         WHERE document_version_id = ? AND sheet_kind = 'calc' AND is_available = 1",
    )
    .bind(&ver_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    let derived_calc_rel: (String,) = sqlx::query_as(
        "SELECT relative_path FROM document_files
         WHERE document_version_id = ? AND sheet_kind = 'calc' AND is_available = 1",
    )
    .bind(&result.version_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_ne!(origin_calc_rel.0, derived_calc_rel.0);
}

#[tokio::test]
async fn test_e_without_copy_calc_no_sheets_on_derived() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("t.db");
    let pool = db::init_pool(&db_path).await.unwrap();
    let root = dir.path().join("root");
    let root_id = setup_root(&pool, &root).await;
    let project_id = seed_project(&pool, &root_id, "P1").await;
    let sdb = root.join("P1/02_MODELOS/MOD.sdb");
    let (origin_id, ver_id, _) = seed_model_doc(
        &pool,
        &project_id,
        "Mod",
        "1126-MOD-001",
        "P1/02_MODELOS/MOD.sdb",
        "MOD.sdb",
        &sdb,
        b"SDB",
    )
    .await;
    let calc = root.join("P1/02_MODELOS/calc.xlsx");
    write_bytes(&calc, b"CALC");
    let state = AppState::new(pool.clone(), db_path);
    associate_model_spreadsheet_inner(
        &state,
        AssociateModelSpreadsheetInput {
            document_version_id: ver_id.clone(),
            absolute_path: calc.display().to_string(),
            sheet_kind: SHEET_KIND_CALC.into(),
            version_policy: None,
        },
    )
    .await
    .unwrap();

    let result = create_derived_model_inner(
        &state,
        CreateDerivedModelInput {
            source_document_id: origin_id,
            title: None,
            copy_calc_sheets: Some(false),
            notes: None,
        },
    )
    .await
    .unwrap();

    assert!(result.copied_calc_absolute_paths.is_empty());
    let derived_calcs: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM document_files
         WHERE document_version_id = ? AND sheet_kind = 'calc' AND is_available = 1",
    )
    .bind(&result.version_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(derived_calcs.0, 0);
    let origin_calcs: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM document_files
         WHERE document_version_id = ? AND sheet_kind = 'calc' AND is_available = 1",
    )
    .bind(&ver_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(origin_calcs.0, 1);
}

#[tokio::test]
async fn test_f_no_promote_snapshots_under_origin() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("t.db");
    let pool = db::init_pool(&db_path).await.unwrap();
    let root = dir.path().join("root");
    let root_id = setup_root(&pool, &root).await;
    let project_id = seed_project(&pool, &root_id, "P1").await;
    let sdb = root.join("P1/02_MODELOS/MOD.sdb");
    let (origin_id, ver_id, _) = seed_model_doc(
        &pool,
        &project_id,
        "Mod",
        "1126-MOD-001",
        "P1/02_MODELOS/MOD.sdb",
        "MOD.sdb",
        &sdb,
        b"SDB",
    )
    .await;
    let calc = root.join("P1/02_MODELOS/calc.xlsx");
    write_bytes(&calc, b"CALC");
    let state = AppState::new(pool.clone(), db_path);
    associate_model_spreadsheet_inner(
        &state,
        AssociateModelSpreadsheetInput {
            document_version_id: ver_id.clone(),
            absolute_path: calc.display().to_string(),
            sheet_kind: SHEET_KIND_CALC.into(),
            version_policy: None,
        },
    )
    .await
    .unwrap();

    let _ = create_derived_model_inner(
        &state,
        CreateDerivedModelInput {
            source_document_id: origin_id.clone(),
            title: None,
            copy_calc_sheets: Some(true),
            notes: None,
        },
    )
    .await
    .unwrap();

    // promote would snapshot under .ingmanager/snapshots/{origin_id}/...
    let snap_dir = root.join(".ingmanager/snapshots").join(&origin_id);
    assert!(
        !snap_dir.exists(),
        "derived must not promote/snapshot onto origin"
    );
    // Origin still has exactly one version
    let versions: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM document_versions WHERE document_id = ?")
            .bind(&origin_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(versions.0, 1);
}

#[tokio::test]
async fn test_g_guards_and_distinct_engineering_groups() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("t.db");
    let pool = db::init_pool(&db_path).await.unwrap();
    let root = dir.path().join("root");
    let root_id = setup_root(&pool, &root).await;
    let project_id = seed_project(&pool, &root_id, "P1").await;
    let mem = root.join("P1/04_MEMORIAS/m.docx");
    let mem_id = seed_memoria(&pool, &project_id, &mem).await;
    let state = AppState::new(pool.clone(), db_path.clone());

    let err = create_derived_model_inner(
        &state,
        CreateDerivedModelInput {
            source_document_id: mem_id,
            title: None,
            copy_calc_sheets: Some(false),
            notes: None,
        },
    )
    .await
    .unwrap_err();
    assert!(
        err.to_string().to_lowercase().contains("modelos")
            || err.to_string().to_lowercase().contains("categoría"),
        "expected non-modelos guard, got {err}"
    );

    let sdb = root.join("P1/02_MODELOS/FAM.sdb");
    let (origin_id, _, _) = seed_model_doc(
        &pool,
        &project_id,
        "Fam",
        "1126-MOD-001",
        "P1/02_MODELOS/FAM.sdb",
        "FAM.sdb",
        &sdb,
        b"SDB",
    )
    .await;
    let result = create_derived_model_inner(
        &state,
        CreateDerivedModelInput {
            source_document_id: origin_id.clone(),
            title: None,
            copy_calc_sheets: Some(false),
            notes: None,
        },
    )
    .await
    .unwrap();

    let origin_gk: (String,) =
        sqlx::query_as("SELECT group_key FROM engineering_groups WHERE document_id = ?")
            .bind(&origin_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    let derived_gk: (String,) =
        sqlx::query_as("SELECT group_key FROM engineering_groups WHERE document_id = ?")
            .bind(&result.document_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_ne!(origin_gk.0, derived_gk.0);

    // Missing SDB on disk
    let ghost = root.join("P1/02_MODELOS/GHOST.sdb");
    let (ghost_id, _, _) = seed_model_doc(
        &pool,
        &project_id,
        "Ghost",
        "1126-MOD-010",
        "P1/02_MODELOS/GHOST.sdb",
        "GHOST.sdb",
        &ghost,
        b"X",
    )
    .await;
    fs::remove_file(&ghost).unwrap();
    let err2 = create_derived_model_inner(
        &state,
        CreateDerivedModelInput {
            source_document_id: ghost_id,
            title: None,
            copy_calc_sheets: Some(false),
            notes: None,
        },
    )
    .await
    .unwrap_err();
    assert!(
        err2.to_string().to_lowercase().contains("disponible")
            || err2.to_string().to_lowercase().contains("disco")
            || err2.to_string().to_lowercase().contains("sdb"),
        "expected missing SDB guard, got {err2}"
    );
}

#[test]
fn allocate_derived_path_distinct_from_version_suffix() {
    let dir = tempdir().unwrap();
    let src = dir.path().join("MODELO.sdb");
    fs::write(&src, b"x").unwrap();
    let (dest, gk) = allocate_derived_working_path(&src, &["1126".into()]).unwrap();
    assert!(dest
        .file_name()
        .unwrap()
        .to_string_lossy()
        .contains("_derivado"));
    assert!(!dest
        .file_name()
        .unwrap()
        .to_string_lossy()
        .contains("_v0"));
    assert_ne!(gk, normalize_group_key("MODELO", &["1126".into()]));
}
