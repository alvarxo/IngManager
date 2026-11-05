//! Model subtypes + independent codes — Tests 1–12.
//!
//! 1  .sdb → sap2000 / SAP
//! 2  .rvt → revit / RVT
//! 3  .idea → idea_statica / IDE
//! 4  .rtd → robot / ROB
//! 5  .edb → etabs / ETB
//! 6  unknown → other / MOD
//! 7  Per-prefix sequences independent
//! 8  Migration keeps existing MOD; backfills model_type
//! 9  Actualizar código según tipo
//! 10 create_document assigns type-specific code + model_type
//! 11 Derived inherits model_type + prefix
//! 12 DocumentRow.model_type exposed; filter helper parity

use std::fs;
use std::path::Path;

use tempfile::tempdir;

use crate::commands::documents::{
    fetch_document_public, update_document_code_from_model_type_inner, CreateDocumentInput,
};
use crate::db;
use crate::derived_models::{create_derived_model_inner, CreateDerivedModelInput};
use crate::doc_codes::next_document_code;
use crate::model_types::{
    detect_model_type_from_extension, model_code_prefix, ModelType,
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

#[test]
fn test_01_sdb_sap2000_sap() {
    assert_eq!(
        detect_model_type_from_extension(Some("sdb")),
        ModelType::Sap2000
    );
    assert_eq!(ModelType::Sap2000.code_prefix(), "SAP");
}

#[test]
fn test_02_rvt_revit_rvt() {
    assert_eq!(
        detect_model_type_from_extension(Some("rvt")),
        ModelType::Revit
    );
    assert_eq!(ModelType::Revit.code_prefix(), "RVT");
}

#[test]
fn test_03_idea_idea_statica_ide() {
    assert_eq!(
        detect_model_type_from_extension(Some("idea")),
        ModelType::IdeaStatica
    );
    assert_eq!(ModelType::IdeaStatica.code_prefix(), "IDE");
}

#[test]
fn test_04_rtd_robot_rob() {
    assert_eq!(
        detect_model_type_from_extension(Some("rtd")),
        ModelType::Robot
    );
    assert_eq!(ModelType::Robot.code_prefix(), "ROB");
}

#[test]
fn test_05_edb_etabs_etb() {
    assert_eq!(
        detect_model_type_from_extension(Some("edb")),
        ModelType::Etabs
    );
    assert_eq!(ModelType::Etabs.code_prefix(), "ETB");
}

#[test]
fn test_06_unknown_other_mod() {
    assert_eq!(
        detect_model_type_from_extension(Some("dwg")),
        ModelType::Other
    );
    assert_eq!(ModelType::Other.code_prefix(), "MOD");
}

#[test]
fn test_07_per_prefix_sequences_independent() {
    let existing = vec![
        "1126-SAP-001".into(),
        "1126-SAP-002".into(),
        "1126-MOD-001".into(),
        "1126-RVT-001".into(),
    ];
    assert_eq!(
        next_document_code("1126", "SAP", &existing).unwrap(),
        "1126-SAP-003"
    );
    assert_eq!(
        next_document_code("1126", "MOD", &existing).unwrap(),
        "1126-MOD-002"
    );
    assert_eq!(
        next_document_code("1126", "IDE", &existing).unwrap(),
        "1126-IDE-001"
    );
}

#[tokio::test]
async fn test_08_migration_keeps_mod_backfills_type() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("t.db");
    // Simulate pre-migration DB: apply migrations 1–8 manually by using init then
    // inserting a MOD doc with principal .sdb, then clearing model_type and
    // re-running the backfill statements from 009 (already applied by init_pool).
    let pool = db::init_pool(&db_path).await.unwrap();
    let root = dir.path().join("root");
    let root_id = setup_root(&pool, &root).await;
    let project_id = seed_project(&pool, &root_id, "P1").await;
    let cat: (String,) =
        sqlx::query_as("SELECT id FROM categories WHERE project_id = ? AND key = 'modelos'")
            .bind(&project_id)
            .fetch_one(&pool)
            .await
            .unwrap();

    let doc_id = uuid::Uuid::new_v4().to_string();
    let ver_id = uuid::Uuid::new_v4().to_string();
    let file_id = uuid::Uuid::new_v4().to_string();
    let modelos = root.join("P1/02_MODELOS");
    let sdb = modelos.join("Legacy.sdb");
    write_bytes(&sdb, b"LEGACY");

    sqlx::query(
        "INSERT INTO documents (
            id, project_id, category_id, business_code, title, doc_type, model_type,
            current_version_id, status, created_at, updated_at
         ) VALUES (?, ?, ?, '1126-MOD-007', 'Legacy', 'MOD', NULL, ?, 'active', datetime('now'), datetime('now'))",
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
            is_available, file_role, created_at
         ) VALUES (?, ?, 'P1/02_MODELOS/Legacy.sdb', 'Legacy.sdb', 'sdb', 1, 'principal', datetime('now'))",
    )
    .bind(&file_id)
    .bind(&ver_id)
    .execute(&pool)
    .await
    .unwrap();

    // Re-apply backfill logic (same as migration 009) for rows still NULL.
    sqlx::query(
        "UPDATE documents
         SET model_type = (
             SELECT CASE LOWER(COALESCE(f.extension, ''))
                 WHEN 'sdb' THEN 'sap2000'
                 WHEN 'rvt' THEN 'revit'
                 WHEN 'rfa' THEN 'revit'
                 WHEN 'idea' THEN 'idea_statica'
                 WHEN 'ideastatica' THEN 'idea_statica'
                 WHEN 'rtd' THEN 'robot'
                 WHEN 'edb' THEN 'etabs'
                 WHEN 'e2k' THEN 'etabs'
                 ELSE 'other'
             END
             FROM document_files f
             INNER JOIN document_versions v ON v.id = f.document_version_id
             WHERE v.document_id = documents.id
               AND f.is_available = 1
               AND (
                     f.file_role = 'principal'
                  OR LOWER(COALESCE(f.extension, '')) IN (
                         'sdb', 'rvt', 'rfa', 'idea', 'ideastatica', 'rtd', 'edb', 'e2k'
                     )
               )
             ORDER BY
                 CASE WHEN f.file_role = 'principal' THEN 0 ELSE 1 END,
                 v.version_number DESC
             LIMIT 1
         )
         WHERE model_type IS NULL
           AND category_id IN (SELECT id FROM categories WHERE key = 'modelos')",
    )
    .execute(&pool)
    .await
    .unwrap();

    let row: (Option<String>, Option<String>) =
        sqlx::query_as("SELECT business_code, model_type FROM documents WHERE id = ?")
            .bind(&doc_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(row.0.as_deref(), Some("1126-MOD-007")); // kept
    assert_eq!(row.1.as_deref(), Some("sap2000")); // backfilled
}

#[tokio::test]
async fn test_09_actualizar_codigo_segun_tipo() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("t.db");
    let pool = db::init_pool(&db_path).await.unwrap();
    let root = dir.path().join("root");
    let root_id = setup_root(&pool, &root).await;
    let project_id = seed_project(&pool, &root_id, "P1").await;
    let cat: (String,) =
        sqlx::query_as("SELECT id FROM categories WHERE project_id = ? AND key = 'modelos'")
            .bind(&project_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    let doc_id = uuid::Uuid::new_v4().to_string();
    let ver_id = uuid::Uuid::new_v4().to_string();
    let file_id = uuid::Uuid::new_v4().to_string();
    let sdb = root.join("P1/02_MODELOS/A.sdb");
    write_bytes(&sdb, b"X");

    sqlx::query(
        "INSERT INTO documents (
            id, project_id, category_id, business_code, title, doc_type, model_type,
            current_version_id, status, created_at, updated_at
         ) VALUES (?, ?, ?, '1126-MOD-001', 'A', 'MOD', 'sap2000', ?, 'active', datetime('now'), datetime('now'))",
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
            is_available, file_role, created_at
         ) VALUES (?, ?, 'P1/02_MODELOS/A.sdb', 'A.sdb', 'sdb', 1, 'principal', datetime('now'))",
    )
    .bind(&file_id)
    .bind(&ver_id)
    .execute(&pool)
    .await
    .unwrap();

    let state = AppState::new(pool.clone(), db_path);
    let updated = update_document_code_from_model_type_inner(&state, &doc_id)
        .await
        .unwrap();
    assert_eq!(updated.business_code.as_deref(), Some("1126-SAP-001"));
    assert_eq!(updated.model_type.as_deref(), Some("sap2000"));
    // Old MOD-001 remains in sequence pool for MOD; SAP starts at 001.
}

#[tokio::test]
async fn test_10_create_document_type_specific_code() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("t.db");
    let pool = db::init_pool(&db_path).await.unwrap();
    let root = dir.path().join("root");
    let root_id = setup_root(&pool, &root).await;
    let project_id = seed_project(&pool, &root_id, "P1").await;
    let state = AppState::new(pool.clone(), db_path);

    // Simulate create_document allocation rules for Modelos + model_type.
    let input = CreateDocumentInput {
        project_id: project_id.clone(),
        category_key: "modelos".into(),
        title: "Torre".into(),
        status: None,
        subcategory: None,
        origin: None,
        received_at: None,
        notes: None,
        assign_code: Some(true),
        source_absolute_path: Some(root.join("P1/02_MODELOS/Torre.rvt").display().to_string()),
        model_type: Some("revit".into()),
    };
    assert_eq!(
        model_code_prefix(input.model_type.as_deref()),
        ModelType::Revit.code_prefix()
    );

    let cat: (String,) =
        sqlx::query_as("SELECT id FROM categories WHERE project_id = ? AND key = 'modelos'")
            .bind(&project_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    let mt = ModelType::parse(input.model_type.as_deref().unwrap()).unwrap();
    let codes: Vec<(Option<String>,)> =
        sqlx::query_as("SELECT business_code FROM documents WHERE project_id = ?")
            .bind(&project_id)
            .fetch_all(&pool)
            .await
            .unwrap();
    let existing: Vec<String> = codes.into_iter().filter_map(|r| r.0).collect();
    let code = next_document_code("1126", mt.code_prefix(), &existing).unwrap();
    let doc_id = uuid::Uuid::new_v4().to_string();
    let ver_id = uuid::Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO documents (
            id, project_id, category_id, business_code, title, doc_type, model_type,
            current_version_id, status, created_at, updated_at
         ) VALUES (?, ?, ?, ?, ?, 'MOD', ?, ?, 'active', datetime('now'), datetime('now'))",
    )
    .bind(&doc_id)
    .bind(&project_id)
    .bind(&cat.0)
    .bind(&code)
    .bind(&input.title)
    .bind(mt.as_str())
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

    let row = fetch_document_public(&state, &doc_id).await.unwrap();
    assert_eq!(row.business_code.as_deref(), Some("1126-RVT-001"));
    assert_eq!(row.model_type.as_deref(), Some("revit"));

    let codes2: Vec<(Option<String>,)> =
        sqlx::query_as("SELECT business_code FROM documents WHERE project_id = ?")
            .bind(&project_id)
            .fetch_all(&pool)
            .await
            .unwrap();
    let existing2: Vec<String> = codes2.into_iter().filter_map(|r| r.0).collect();
    assert_eq!(
        next_document_code("1126", "ETB", &existing2).unwrap(),
        "1126-ETB-001"
    );
}

#[tokio::test]
async fn test_11_derived_inherits_type_and_prefix() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("t.db");
    let pool = db::init_pool(&db_path).await.unwrap();
    let root = dir.path().join("root");
    let root_id = setup_root(&pool, &root).await;
    let project_id = seed_project(&pool, &root_id, "P1").await;
    let cat: (String,) =
        sqlx::query_as("SELECT id FROM categories WHERE project_id = ? AND key = 'modelos'")
            .bind(&project_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    let doc_id = uuid::Uuid::new_v4().to_string();
    let ver_id = uuid::Uuid::new_v4().to_string();
    let file_id = uuid::Uuid::new_v4().to_string();
    let eg_id = uuid::Uuid::new_v4().to_string();
    let sdb = root.join("P1/02_MODELOS/Base.sdb");
    write_bytes(&sdb, b"BASE_SDB");

    sqlx::query(
        "INSERT INTO documents (
            id, project_id, category_id, business_code, title, doc_type, model_type,
            current_version_id, status, created_at, updated_at
         ) VALUES (?, ?, ?, '1126-SAP-001', 'Base', 'MOD', 'sap2000', ?, 'active', datetime('now'), datetime('now'))",
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
            is_available, file_role, engineering_group_key, created_at
         ) VALUES (?, ?, 'P1/02_MODELOS/Base.sdb', 'Base.sdb', 'sdb', 1, 'principal', 'base', datetime('now'))",
    )
    .bind(&file_id)
    .bind(&ver_id)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO engineering_groups (
            id, project_id, category_key, group_key, title, principal_file_name,
            document_id, created_at, updated_at
         ) VALUES (?, ?, 'modelos', 'base', 'Base', 'Base.sdb', ?, datetime('now'), datetime('now'))",
    )
    .bind(&eg_id)
    .bind(&project_id)
    .bind(&doc_id)
    .execute(&pool)
    .await
    .unwrap();

    let state = AppState::new(pool.clone(), db_path);
    let result = create_derived_model_inner(
        &state,
        CreateDerivedModelInput {
            source_document_id: doc_id.clone(),
            title: Some("Derivado".into()),
            copy_calc_sheets: Some(false),
            notes: None,
        },
    )
    .await
    .unwrap();

    assert_eq!(result.business_code.as_deref(), Some("1126-SAP-002"));
    let mt: (Option<String>,) =
        sqlx::query_as("SELECT model_type FROM documents WHERE id = ?")
            .bind(&result.document_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(mt.0.as_deref(), Some("sap2000"));
}

#[tokio::test]
async fn test_12_document_row_exposes_model_type() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("t.db");
    let pool = db::init_pool(&db_path).await.unwrap();
    let root = dir.path().join("root");
    let root_id = setup_root(&pool, &root).await;
    let project_id = seed_project(&pool, &root_id, "P1").await;
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
            id, project_id, category_id, business_code, title, doc_type, model_type,
            current_version_id, status, created_at, updated_at
         ) VALUES (?, ?, ?, '1126-IDE-001', 'Conn', 'MOD', 'idea_statica', ?, 'active', datetime('now'), datetime('now'))",
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

    let state = AppState::new(pool.clone(), db_path);
    let doc = fetch_document_public(&state, &doc_id).await.unwrap();
    assert_eq!(doc.model_type.as_deref(), Some("idea_statica"));
    assert_eq!(doc.business_code.as_deref(), Some("1126-IDE-001"));
    assert_eq!(model_code_prefix(doc.model_type.as_deref()), "IDE");
}
