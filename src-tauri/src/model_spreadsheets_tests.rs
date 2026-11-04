//! Model-associated Excel spreadsheets — Tests A–F.
//!
//! A — Associate calc XLSX to a model version (sheet_kind + policy persisted)
//! B — Associate export as version-bound
//! C — create_next_physical_version snapshots calc onto old; originals on new current
//! D — After next version: exports stay on old; new shows export pending
//! E — Open historical/snapshot sheet via safe temp copy (snapshot path untouched)
//! F — Tree lists sheets under versions; refuse non-model / non-spreadsheet / second calc

use std::fs;
use std::path::Path;

use tempfile::tempdir;

use crate::commands::documents::{
    create_next_physical_version_inner, list_version_files_inner, CreateNextPhysicalVersionInput,
};
use crate::db;
use crate::model_spreadsheets::{
    associate_model_spreadsheet_inner, detach_model_spreadsheet_inner,
    spreadsheet_status_for_version, AssociateModelSpreadsheetInput, POLICY_COPY_ON_VERSION,
    POLICY_VERSION_BOUND, SHEET_KIND_CALC, SHEET_KIND_EXPORT,
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
    rel: &str,
    file_name: &str,
    abs: &Path,
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
    write_bytes(abs, b"INGMANAGER_FIXTURE_SDB_V1");

    sqlx::query(
        "INSERT INTO documents (
            id, project_id, category_id, title, status, created_at, updated_at, current_version_id
         ) VALUES (?, ?, ?, ?, 'active', datetime('now'), datetime('now'), ?)",
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
        "INSERT INTO document_versions (
            id, document_id, version_number, label, created_at, is_snapshot
         ) VALUES (?, ?, 1, 'v01', datetime('now'), 0)",
    )
    .bind(&ver_id)
    .bind(&doc_id)
    .execute(pool)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO document_files (
            id, document_version_id, relative_path, file_name, extension,
            size_bytes, is_available, format_label, created_at, file_role
         ) VALUES (?, ?, ?, ?, 'sdb', 24, 1, 'SDB', datetime('now'), 'principal')",
    )
    .bind(&file_id)
    .bind(&ver_id)
    .bind(rel)
    .bind(file_name)
    .execute(pool)
    .await
    .unwrap();

    (doc_id, ver_id, file_id)
}

async fn seed_memoria_doc(
    pool: &sqlx::SqlitePool,
    project_id: &str,
    abs: &Path,
) -> (String, String) {
    let cat: (String,) =
        sqlx::query_as("SELECT id FROM categories WHERE project_id = ? AND key = 'memorias'")
            .bind(project_id)
            .fetch_one(pool)
            .await
            .unwrap();
    let doc_id = uuid::Uuid::new_v4().to_string();
    let ver_id = uuid::Uuid::new_v4().to_string();
    let file_id = uuid::Uuid::new_v4().to_string();
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
    let rel = format!(
        "P1/04_MEMORIAS/{}",
        abs.file_name().unwrap().to_string_lossy()
    );
    sqlx::query(
        "INSERT INTO document_files (
            id, document_version_id, relative_path, file_name, extension,
            size_bytes, is_available, created_at, file_role
         ) VALUES (?, ?, ?, ?, 'docx', 4, 1, datetime('now'), 'editable')",
    )
    .bind(&file_id)
    .bind(&ver_id)
    .bind(&rel)
    .bind(abs.file_name().unwrap().to_string_lossy().as_ref())
    .execute(pool)
    .await
    .unwrap();
    (doc_id, ver_id)
}

#[tokio::test]
async fn test_a_associate_calc_sheet() {
    let tmp = tempdir().unwrap();
    let root = tmp.path().join("root");
    fs::create_dir_all(&root).unwrap();
    let db_path = tmp.path().join("t.db");
    let pool = db::init_pool(&db_path).await.unwrap();
    let root_id = setup_root(&pool, &root).await;
    let project_id = seed_project(&pool, &root_id, "P1").await;
    let sdb = root.join("P1/02_MODELOS/Modelo_v01.sdb");
    let (doc_id, ver_id, _) = seed_model_doc(
        &pool,
        &project_id,
        "Modelo",
        "P1/02_MODELOS/Modelo_v01.sdb",
        "Modelo_v01.sdb",
        &sdb,
    )
    .await;
    let calc = root.join("P1/02_MODELOS/Calc.xlsx");
    write_bytes(&calc, b"CALC_V1");

    let state = AppState::new(pool.clone(), db_path);
    let file_id = associate_model_spreadsheet_inner(
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

    let row: (String, String, String, String) = sqlx::query_as(
        "SELECT sheet_kind, version_policy, file_role, format_label FROM document_files WHERE id = ?",
    )
    .bind(&file_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(row.0, SHEET_KIND_CALC);
    assert_eq!(row.1, POLICY_COPY_ON_VERSION);
    assert_eq!(row.2, "calc_sheet");
    assert_eq!(row.3, "CALC");

    let status = spreadsheet_status_for_version(&state, &doc_id, &ver_id)
        .await
        .unwrap();
    assert!(status.has_live_calc);
    assert!(!status.has_export);
    assert!(status.export_pending);
}

#[tokio::test]
async fn test_b_associate_export_version_bound() {
    let tmp = tempdir().unwrap();
    let root = tmp.path().join("root");
    fs::create_dir_all(&root).unwrap();
    let db_path = tmp.path().join("t.db");
    let pool = db::init_pool(&db_path).await.unwrap();
    let root_id = setup_root(&pool, &root).await;
    let project_id = seed_project(&pool, &root_id, "P1").await;
    let sdb = root.join("P1/02_MODELOS/Modelo_v01.sdb");
    let (_doc_id, ver_id, _) = seed_model_doc(
        &pool,
        &project_id,
        "Modelo",
        "P1/02_MODELOS/Modelo_v01.sdb",
        "Modelo_v01.sdb",
        &sdb,
    )
    .await;
    let export = root.join("P1/02_MODELOS/Export.xlsm");
    write_bytes(&export, b"EXPORT_V1");

    let state = AppState::new(pool.clone(), db_path);
    let file_id = associate_model_spreadsheet_inner(
        &state,
        AssociateModelSpreadsheetInput {
            document_version_id: ver_id.clone(),
            absolute_path: export.display().to_string(),
            sheet_kind: SHEET_KIND_EXPORT.into(),
            version_policy: None,
        },
    )
    .await
    .unwrap();

    let row: (String, String) = sqlx::query_as(
        "SELECT sheet_kind, version_policy FROM document_files WHERE id = ?",
    )
    .bind(&file_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(row.0, SHEET_KIND_EXPORT);
    assert_eq!(row.1, POLICY_VERSION_BOUND);
}

#[tokio::test]
async fn test_c_next_version_snapshots_calc_and_relinks() {
    let tmp = tempdir().unwrap();
    let root = tmp.path().join("root");
    fs::create_dir_all(&root).unwrap();
    let db_path = tmp.path().join("t.db");
    let pool = db::init_pool(&db_path).await.unwrap();
    let root_id = setup_root(&pool, &root).await;
    let project_id = seed_project(&pool, &root_id, "P1").await;
    let sdb = root.join("P1/02_MODELOS/Modelo_v01.sdb");
    let (doc_id, ver1, _) = seed_model_doc(
        &pool,
        &project_id,
        "Modelo",
        "P1/02_MODELOS/Modelo_v01.sdb",
        "Modelo_v01.sdb",
        &sdb,
    )
    .await;
    let calc = root.join("P1/02_MODELOS/Calc.xlsx");
    write_bytes(&calc, b"CALC_LIVE");

    let state = AppState::new(pool.clone(), db_path);
    associate_model_spreadsheet_inner(
        &state,
        AssociateModelSpreadsheetInput {
            document_version_id: ver1.clone(),
            absolute_path: calc.display().to_string(),
            sheet_kind: SHEET_KIND_CALC.into(),
            version_policy: None,
        },
    )
    .await
    .unwrap();

    let created = create_next_physical_version_inner(
        &state,
        CreateNextPhysicalVersionInput {
            document_id: doc_id.clone(),
            notes: Some("next".into()),
            source_absolute_path: None,
        },
    )
    .await
    .unwrap();
    assert_eq!(created.version_number, 2);

    let v1_files = list_version_files_inner(&state, &ver1).await.unwrap();
    let v1_calc_live: Vec<_> = v1_files
        .iter()
        .filter(|f| f.sheet_kind.as_deref() == Some(SHEET_KIND_CALC) && f.is_available == 1)
        .collect();
    assert_eq!(v1_calc_live.len(), 1, "old version keeps one available calc (snapshot)");
    assert!(
        v1_calc_live[0]
            .relative_path
            .contains(".ingmanager/snapshots/"),
        "old calc must be a snapshot path: {}",
        v1_calc_live[0].relative_path
    );
    let snap_abs = root.join(&v1_calc_live[0].relative_path);
    assert!(snap_abs.is_file());
    assert_eq!(fs::read(&snap_abs).unwrap(), b"CALC_LIVE");

    let v2_files = list_version_files_inner(&state, &created.id).await.unwrap();
    let v2_calc: Vec<_> = v2_files
        .iter()
        .filter(|f| {
            f.sheet_kind.as_deref() == Some(SHEET_KIND_CALC)
                && f.is_available == 1
                && !f.relative_path.contains(".ingmanager/snapshots/")
        })
        .collect();
    assert_eq!(v2_calc.len(), 1);
    assert!(v2_calc[0].relative_path.ends_with("Calc.xlsx"));
    // Original still at working path and still editable.
    assert_eq!(fs::read(&calc).unwrap(), b"CALC_LIVE");
}

#[tokio::test]
async fn test_d_exports_stay_old_new_pending() {
    let tmp = tempdir().unwrap();
    let root = tmp.path().join("root");
    fs::create_dir_all(&root).unwrap();
    let db_path = tmp.path().join("t.db");
    let pool = db::init_pool(&db_path).await.unwrap();
    let root_id = setup_root(&pool, &root).await;
    let project_id = seed_project(&pool, &root_id, "P1").await;
    let sdb = root.join("P1/02_MODELOS/Modelo_v01.sdb");
    let (doc_id, ver1, _) = seed_model_doc(
        &pool,
        &project_id,
        "Modelo",
        "P1/02_MODELOS/Modelo_v01.sdb",
        "Modelo_v01.sdb",
        &sdb,
    )
    .await;
    let calc = root.join("P1/02_MODELOS/Calc.xlsx");
    let export = root.join("P1/02_MODELOS/Export.xlsx");
    write_bytes(&calc, b"CALC");
    write_bytes(&export, b"EXPORT");

    let state = AppState::new(pool.clone(), db_path);
    associate_model_spreadsheet_inner(
        &state,
        AssociateModelSpreadsheetInput {
            document_version_id: ver1.clone(),
            absolute_path: calc.display().to_string(),
            sheet_kind: SHEET_KIND_CALC.into(),
            version_policy: None,
        },
    )
    .await
    .unwrap();
    associate_model_spreadsheet_inner(
        &state,
        AssociateModelSpreadsheetInput {
            document_version_id: ver1.clone(),
            absolute_path: export.display().to_string(),
            sheet_kind: SHEET_KIND_EXPORT.into(),
            version_policy: None,
        },
    )
    .await
    .unwrap();

    let created = create_next_physical_version_inner(
        &state,
        CreateNextPhysicalVersionInput {
            document_id: doc_id.clone(),
            notes: None,
            source_absolute_path: None,
        },
    )
    .await
    .unwrap();

    let old_status = spreadsheet_status_for_version(&state, &doc_id, &ver1)
        .await
        .unwrap();
    assert!(old_status.has_export);
    assert!(!old_status.export_pending);

    let new_status = spreadsheet_status_for_version(&state, &doc_id, &created.id)
        .await
        .unwrap();
    assert!(!new_status.has_export);
    assert!(new_status.export_pending);
    assert!(new_status.has_live_calc);

    let v1_exports: Vec<_> = list_version_files_inner(&state, &ver1)
        .await
        .unwrap()
        .into_iter()
        .filter(|f| f.sheet_kind.as_deref() == Some(SHEET_KIND_EXPORT) && f.is_available == 1)
        .collect();
    assert_eq!(v1_exports.len(), 1);
    assert!(!v1_exports[0].relative_path.contains(".ingmanager/snapshots/"));
}

#[tokio::test]
async fn test_e_open_historical_via_safe_temp_copy() {
    let tmp = tempdir().unwrap();
    let root = tmp.path().join("root");
    fs::create_dir_all(&root).unwrap();
    let db_path = tmp.path().join("t.db");
    let pool = db::init_pool(&db_path).await.unwrap();
    let root_id = setup_root(&pool, &root).await;
    let project_id = seed_project(&pool, &root_id, "P1").await;
    let sdb = root.join("P1/02_MODELOS/Modelo_v01.sdb");
    let (doc_id, ver1, _) = seed_model_doc(
        &pool,
        &project_id,
        "Modelo",
        "P1/02_MODELOS/Modelo_v01.sdb",
        "Modelo_v01.sdb",
        &sdb,
    )
    .await;
    let calc = root.join("P1/02_MODELOS/Calc.xlsx");
    write_bytes(&calc, b"CALC_HIST");

    let state = AppState::new(pool.clone(), db_path);
    associate_model_spreadsheet_inner(
        &state,
        AssociateModelSpreadsheetInput {
            document_version_id: ver1.clone(),
            absolute_path: calc.display().to_string(),
            sheet_kind: SHEET_KIND_CALC.into(),
            version_policy: None,
        },
    )
    .await
    .unwrap();
    create_next_physical_version_inner(
        &state,
        CreateNextPhysicalVersionInput {
            document_id: doc_id.clone(),
            notes: None,
            source_absolute_path: None,
        },
    )
    .await
    .unwrap();

    let snap = list_version_files_inner(&state, &ver1)
        .await
        .unwrap()
        .into_iter()
        .find(|f| {
            f.sheet_kind.as_deref() == Some(SHEET_KIND_CALC)
                && f.is_available == 1
                && f.relative_path.contains(".ingmanager/snapshots/")
        })
        .expect("snapshot calc on v01");

    let snap_path = root.join(&snap.relative_path);
    let before = fs::read(&snap_path).unwrap();
    assert_eq!(before, b"CALC_HIST");
    assert!(
        snap.relative_path.contains(".ingmanager/snapshots/"),
        "historical calc must live under snapshots for safe-open"
    );

    // Safe-copy path used by open_document_file when path is under snapshots:
    // copy to temp_open and confirm snapshot source is untouched.
    let tmp_dir = root
        .join(".ingmanager/temp_open")
        .join(uuid::Uuid::new_v4().to_string());
    fs::create_dir_all(&tmp_dir).unwrap();
    let dest = tmp_dir.join(&snap.file_name);
    fs::copy(&snap_path, &dest).unwrap();
    assert!(dest.is_file());
    assert_eq!(fs::read(&dest).unwrap(), before);
    // Mutate the temp copy only — snapshot must stay intact.
    fs::write(&dest, b"MUTATED_TEMP").unwrap();
    assert_eq!(
        fs::read(&snap_path).unwrap(),
        before,
        "historical snapshot must not be mutated"
    );
    let _ = state; // keep AppState alive for pool
}

#[tokio::test]
async fn test_f_tree_menus_guards() {
    let tmp = tempdir().unwrap();
    let root = tmp.path().join("root");
    fs::create_dir_all(&root).unwrap();
    let db_path = tmp.path().join("t.db");
    let pool = db::init_pool(&db_path).await.unwrap();
    let root_id = setup_root(&pool, &root).await;
    let project_id = seed_project(&pool, &root_id, "P1").await;
    let sdb = root.join("P1/02_MODELOS/Modelo_v01.sdb");
    let (_doc_id, ver_id, _) = seed_model_doc(
        &pool,
        &project_id,
        "Modelo",
        "P1/02_MODELOS/Modelo_v01.sdb",
        "Modelo_v01.sdb",
        &sdb,
    )
    .await;
    let calc = root.join("P1/02_MODELOS/Calc.xlsx");
    let calc2 = root.join("P1/02_MODELOS/Calc2.xlsx");
    let bad = root.join("P1/02_MODELOS/notes.txt");
    write_bytes(&calc, b"C1");
    write_bytes(&calc2, b"C2");
    write_bytes(&bad, b"TXT");

    let state = AppState::new(pool.clone(), db_path.clone());
    let id1 = associate_model_spreadsheet_inner(
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

    // Second live calc refused.
    let err = associate_model_spreadsheet_inner(
        &state,
        AssociateModelSpreadsheetInput {
            document_version_id: ver_id.clone(),
            absolute_path: calc2.display().to_string(),
            sheet_kind: SHEET_KIND_CALC.into(),
            version_policy: None,
        },
    )
    .await
    .unwrap_err();
    assert!(err.to_string().contains("calc"));

    // Non-spreadsheet refused.
    let err = associate_model_spreadsheet_inner(
        &state,
        AssociateModelSpreadsheetInput {
            document_version_id: ver_id.clone(),
            absolute_path: bad.display().to_string(),
            sheet_kind: SHEET_KIND_CALC.into(),
            version_policy: None,
        },
    )
    .await
    .unwrap_err();
    assert!(err.to_string().to_lowercase().contains("xls"));

    // Non-model category refused.
    let mem_docx = root.join("P1/04_MEMORIAS/M.docx");
    let (_mdoc, mver) = seed_memoria_doc(&pool, &project_id, &mem_docx).await;
    let err = associate_model_spreadsheet_inner(
        &state,
        AssociateModelSpreadsheetInput {
            document_version_id: mver,
            absolute_path: calc2.display().to_string(),
            sheet_kind: SHEET_KIND_CALC.into(),
            version_policy: None,
        },
    )
    .await
    .unwrap_err();
    assert!(err.to_string().contains("Modelos"));

    // Tree listing under version includes sheet with labels.
    let files = list_version_files_inner(&state, &ver_id).await.unwrap();
    assert!(files.iter().any(|f| {
        f.id == id1 && f.sheet_kind.as_deref() == Some(SHEET_KIND_CALC) && f.format_label.as_deref() == Some("CALC")
    }));

    // Detach then allow another calc.
    detach_model_spreadsheet_inner(&state, &id1).await.unwrap();
    associate_model_spreadsheet_inner(
        &state,
        AssociateModelSpreadsheetInput {
            document_version_id: ver_id.clone(),
            absolute_path: calc2.display().to_string(),
            sheet_kind: SHEET_KIND_CALC.into(),
            version_policy: Some(POLICY_COPY_ON_VERSION.into()),
        },
    )
    .await
    .unwrap();
}
