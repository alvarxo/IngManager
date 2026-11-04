//! Unify duplicate logical models — Tests A–H.
//!
//! A — Preview remaps `_vNN` across two docs
//! B — Apply keeps principal UUID / code / title
//! C — Apply transfers versions + files; duplicates removed from SQLite
//! D — Physical `.sdb` paths unchanged on disk
//! E — Sheets (calc/export) transfer with version bundles
//! F — Document relationships rewired to principal
//! G — Sync reports unify_suggestions when family is split
//! H — Second sync/unify idempotent; DB backup path returned

use std::fs;
use std::path::{Path, PathBuf};

use tempfile::tempdir;

use crate::commands::engineering::{sync_model_versions_inner, SyncModelVersionsInput};
use crate::db;
use crate::project_paths::upsert_primary_root;
use crate::unify_models::{
    apply_unify_models_inner, detect_unify_suggestions, preview_unify_models_inner,
    UnifyModelsInput,
};

fn write_dummy_sdb(path: &Path) {
    fs::write(path, b"INGMANAGER_FIXTURE_SDB").unwrap();
}

fn write_dummy_xlsx(path: &Path) {
    fs::write(path, b"PK_FAKE_XLSX").unwrap();
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

async fn category_id(pool: &sqlx::SqlitePool, project_id: &str, key: &str) -> String {
    let row: (String,) =
        sqlx::query_as("SELECT id FROM categories WHERE project_id = ? AND key = ?")
            .bind(project_id)
            .bind(key)
            .fetch_one(pool)
            .await
            .unwrap();
    row.0
}

async fn make_model_doc(
    pool: &sqlx::SqlitePool,
    project_id: &str,
    cat_id: &str,
    code: &str,
    title: &str,
    file_rel: &str,
    file_name: &str,
    version_number: i64,
) -> (String, String) {
    let doc_id = uuid::Uuid::new_v4().to_string();
    let ver_id = uuid::Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO documents (
            id, project_id, category_id, business_code, title, doc_type,
            current_version_id, status, created_at, updated_at
         ) VALUES (?, ?, ?, ?, ?, 'MOD', NULL, 'active', datetime('now'), datetime('now'))",
    )
    .bind(&doc_id)
    .bind(project_id)
    .bind(cat_id)
    .bind(code)
    .bind(title)
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO document_versions (
            id, document_id, version_number, label, notes, created_at,
            is_snapshot, source_relative_path
         ) VALUES (?, ?, ?, ?, 'fixture', datetime('now'), 0, ?)",
    )
    .bind(&ver_id)
    .bind(&doc_id)
    .bind(version_number)
    .bind(crate::doc_codes::version_label(version_number))
    .bind(file_rel)
    .execute(pool)
    .await
    .unwrap();
    let file_id = uuid::Uuid::new_v4().to_string();
    let ext = Path::new(file_name)
        .extension()
        .map(|e| e.to_string_lossy().to_string());
    sqlx::query(
        "INSERT INTO document_files (
            id, document_version_id, relative_path, file_name, extension,
            size_bytes, is_available, format_label, created_at, file_role,
            engineering_group_key
         ) VALUES (?, ?, ?, ?, ?, 20, 1, 'principal', datetime('now'), 'principal', ?)",
    )
    .bind(&file_id)
    .bind(&ver_id)
    .bind(file_rel)
    .bind(file_name)
    .bind(&ext)
    .bind("modelo")
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        "UPDATE documents SET current_version_id = ?, updated_at = datetime('now') WHERE id = ?",
    )
    .bind(&ver_id)
    .bind(&doc_id)
    .execute(pool)
    .await
    .unwrap();
    (doc_id, ver_id)
}

async fn attach_sheet(
    pool: &sqlx::SqlitePool,
    version_id: &str,
    rel: &str,
    file_name: &str,
    kind: &str,
) {
    let file_id = uuid::Uuid::new_v4().to_string();
    let role = if kind == "export" {
        "export_sheet"
    } else {
        "calc_sheet"
    };
    let policy = if kind == "export" {
        "version_bound"
    } else {
        "copy_on_version"
    };
    sqlx::query(
        "INSERT INTO document_files (
            id, document_version_id, relative_path, file_name, extension,
            size_bytes, is_available, format_label, created_at, file_role,
            sheet_kind, version_policy
         ) VALUES (?, ?, ?, ?, 'xlsx', 12, 1, ?, datetime('now'), ?, ?, ?)",
    )
    .bind(&file_id)
    .bind(version_id)
    .bind(rel)
    .bind(file_name)
    .bind(kind.to_uppercase())
    .bind(role)
    .bind(kind)
    .bind(policy)
    .execute(pool)
    .await
    .unwrap();
}

struct SplitFixture {
    _tmp: tempfile::TempDir,
    pool: sqlx::SqlitePool,
    #[allow(dead_code)]
    root: PathBuf,
    project_id: String,
    db_path: PathBuf,
    principal_id: String,
    duplicate_id: String,
    #[allow(dead_code)]
    principal_ver: String,
    #[allow(dead_code)]
    duplicate_ver: String,
    v01_abs: PathBuf,
    v02_abs: PathBuf,
    v03_abs: PathBuf,
}

async fn fixture_split_family() -> SplitFixture {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("t.db");
    let pool = db::init_pool(&db_path).await.unwrap();
    let root = dir.path().join("root");
    fs::create_dir_all(&root).unwrap();
    let root_id = setup_root(&pool, &root).await;
    let modelos = root.join("1126").join("02_MODELOS");
    fs::create_dir_all(&modelos).unwrap();
    let project_id = seed_project(&pool, &root_id, Some("1126"), "1126").await;
    let cat = category_id(&pool, &project_id, "modelos").await;

    let names = [
        "1126 - MODELO_v01.sdb",
        "1126 - MODELO_v02.sdb",
        "1126 - MODELO_v03.sdb",
    ];
    for name in &names {
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

    // Principal owns v01+v03 as separate versions incorrectly numbered; duplicate owns v02.
    let (principal_id, principal_ver) = make_model_doc(
        &pool,
        &project_id,
        &cat,
        "1126-MOD-001",
        "Modelo principal custom",
        "1126/02_MODELOS/1126 - MODELO_v01.sdb",
        "1126 - MODELO_v01.sdb",
        1,
    )
    .await;
    // Add v03 as version 2 on principal (wrong order until unify remaps).
    let ver3 = uuid::Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO document_versions (
            id, document_id, version_number, label, notes, created_at,
            is_snapshot, source_relative_path
         ) VALUES (?, ?, 2, 'v02', 'fixture v03', datetime('now'), 0, ?)",
    )
    .bind(&ver3)
    .bind(&principal_id)
    .bind("1126/02_MODELOS/1126 - MODELO_v03.sdb")
    .execute(&pool)
    .await
    .unwrap();
    let f3 = uuid::Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO document_files (
            id, document_version_id, relative_path, file_name, extension,
            size_bytes, is_available, format_label, created_at, file_role,
            engineering_group_key
         ) VALUES (?, ?, ?, ?, 'sdb', 20, 1, 'principal', datetime('now'), 'principal', 'modelo')",
    )
    .bind(&f3)
    .bind(&ver3)
    .bind("1126/02_MODELOS/1126 - MODELO_v03.sdb")
    .bind("1126 - MODELO_v03.sdb")
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query("UPDATE documents SET current_version_id = ? WHERE id = ?")
        .bind(&ver3)
        .bind(&principal_id)
        .execute(&pool)
        .await
        .unwrap();

    let (duplicate_id, duplicate_ver) = make_model_doc(
        &pool,
        &project_id,
        &cat,
        "1126-MOD-099",
        "Duplicado v02",
        "1126/02_MODELOS/1126 - MODELO_v02.sdb",
        "1126 - MODELO_v02.sdb",
        1,
    )
    .await;

    // engineering_groups points only at principal (UNIQUE group_key).
    sqlx::query(
        "INSERT INTO engineering_groups (
            id, project_id, category_key, group_key, title,
            principal_file_name, document_id, created_at, updated_at
         ) VALUES (?, ?, 'modelos', 'modelo', 'Modelo', ?, ?, datetime('now'), datetime('now'))",
    )
    .bind(uuid::Uuid::new_v4().to_string())
    .bind(&project_id)
    .bind("1126 - MODELO_v03.sdb")
    .bind(&principal_id)
    .execute(&pool)
    .await
    .unwrap();

    SplitFixture {
        _tmp: dir,
        pool,
        root: root.clone(),
        project_id,
        db_path,
        principal_id,
        duplicate_id,
        principal_ver,
        duplicate_ver,
        v01_abs: modelos.join("1126 - MODELO_v01.sdb"),
        v02_abs: modelos.join("1126 - MODELO_v02.sdb"),
        v03_abs: modelos.join("1126 - MODELO_v03.sdb"),
    }
}

/// A — Preview lists remapped versions from `_vNN` across two docs.
#[tokio::test]
async fn test_a_preview_remaps_vnn() {
    let fx = fixture_split_family().await;
    let preview = preview_unify_models_inner(
        &fx.pool,
        UnifyModelsInput {
            project_id: fx.project_id.clone(),
            principal_document_id: fx.principal_id.clone(),
            duplicate_document_ids: vec![fx.duplicate_id.clone()],
            category_key: Some("modelos".into()),
        },
    )
    .await
    .unwrap();

    assert_eq!(preview.planned_versions.len(), 3);
    assert_eq!(preview.planned_versions[0].new_version_number, 1);
    assert_eq!(preview.planned_versions[0].file_name, "1126 - MODELO_v01.sdb");
    assert_eq!(
        preview.planned_versions[0].version_hint.as_deref(),
        Some("01")
    );
    assert_eq!(preview.planned_versions[1].file_name, "1126 - MODELO_v02.sdb");
    assert_eq!(preview.planned_versions[1].new_version_number, 2);
    assert_eq!(preview.planned_versions[2].file_name, "1126 - MODELO_v03.sdb");
    assert_eq!(preview.planned_versions[2].new_label, "v03");
    assert_eq!(preview.duplicates_to_delete, vec![fx.duplicate_id.clone()]);
}

/// B — Apply keeps principal UUID / code / title.
#[tokio::test]
async fn test_b_keeps_principal_identity() {
    let fx = fixture_split_family().await;
    let before: (String, Option<String>, String) =
        sqlx::query_as("SELECT id, business_code, title FROM documents WHERE id = ?")
            .bind(&fx.principal_id)
            .fetch_one(&fx.pool)
            .await
            .unwrap();

    let result = apply_unify_models_inner(
        &fx.pool,
        &fx.db_path,
        UnifyModelsInput {
            project_id: fx.project_id.clone(),
            principal_document_id: fx.principal_id.clone(),
            duplicate_document_ids: vec![fx.duplicate_id.clone()],
            category_key: Some("modelos".into()),
        },
    )
    .await
    .unwrap();

    assert_eq!(result.principal_document_id, before.0);
    let after: (String, Option<String>, String) =
        sqlx::query_as("SELECT id, business_code, title FROM documents WHERE id = ?")
            .bind(&fx.principal_id)
            .fetch_one(&fx.pool)
            .await
            .unwrap();
    assert_eq!(after.0, before.0);
    assert_eq!(after.1, before.1);
    assert_eq!(after.2, before.2);
    assert_eq!(after.1.as_deref(), Some("1126-MOD-001"));
    assert_eq!(after.2, "Modelo principal custom");
}

/// C — Apply transfers versions + files; duplicates removed from SQLite.
#[tokio::test]
async fn test_c_transfers_and_deletes_duplicate() {
    let fx = fixture_split_family().await;
    let result = apply_unify_models_inner(
        &fx.pool,
        &fx.db_path,
        UnifyModelsInput {
            project_id: fx.project_id.clone(),
            principal_document_id: fx.principal_id.clone(),
            duplicate_document_ids: vec![fx.duplicate_id.clone()],
            category_key: Some("modelos".into()),
        },
    )
    .await
    .unwrap();

    assert_eq!(result.versions_on_principal, 3);
    assert!(result.duplicates_deleted.contains(&fx.duplicate_id));
    let gone: Option<(String,)> =
        sqlx::query_as("SELECT id FROM documents WHERE id = ?")
            .bind(&fx.duplicate_id)
            .fetch_optional(&fx.pool)
            .await
            .unwrap();
    assert!(gone.is_none());

    let vers: Vec<(i64, String)> = sqlx::query_as(
        "SELECT v.version_number, f.file_name
         FROM document_versions v
         JOIN document_files f ON f.document_version_id = v.id
         WHERE v.document_id = ? AND COALESCE(f.file_role, 'principal') = 'principal'
         ORDER BY v.version_number",
    )
    .bind(&fx.principal_id)
    .fetch_all(&fx.pool)
    .await
    .unwrap();
    assert_eq!(vers.len(), 3);
    assert_eq!(vers[0], (1, "1126 - MODELO_v01.sdb".into()));
    assert_eq!(vers[1], (2, "1126 - MODELO_v02.sdb".into()));
    assert_eq!(vers[2], (3, "1126 - MODELO_v03.sdb".into()));
}

/// D — Physical `.sdb` paths unchanged on disk (never deleted).
#[tokio::test]
async fn test_d_physical_sdb_preserved() {
    let fx = fixture_split_family().await;
    let before = [
        fs::read(&fx.v01_abs).unwrap(),
        fs::read(&fx.v02_abs).unwrap(),
        fs::read(&fx.v03_abs).unwrap(),
    ];
    assert!(fx.v01_abs.is_file());
    assert!(fx.v02_abs.is_file());
    assert!(fx.v03_abs.is_file());

    apply_unify_models_inner(
        &fx.pool,
        &fx.db_path,
        UnifyModelsInput {
            project_id: fx.project_id.clone(),
            principal_document_id: fx.principal_id.clone(),
            duplicate_document_ids: vec![fx.duplicate_id.clone()],
            category_key: Some("modelos".into()),
        },
    )
    .await
    .unwrap();

    assert!(fx.v01_abs.is_file());
    assert!(fx.v02_abs.is_file());
    assert!(fx.v03_abs.is_file());
    assert_eq!(fs::read(&fx.v01_abs).unwrap(), before[0]);
    assert_eq!(fs::read(&fx.v02_abs).unwrap(), before[1]);
    assert_eq!(fs::read(&fx.v03_abs).unwrap(), before[2]);
}

/// E — Sheets (calc/export) transfer with their version bundles.
#[tokio::test]
async fn test_e_sheets_transfer() {
    let fx = fixture_split_family().await;
    let sheets = fx.root.join("1126").join("02_MODELOS");
    let calc = sheets.join("calc_v02.xlsx");
    let export = sheets.join("export_v02.xlsx");
    write_dummy_xlsx(&calc);
    write_dummy_xlsx(&export);
    attach_sheet(
        &fx.pool,
        &fx.duplicate_ver,
        "1126/02_MODELOS/calc_v02.xlsx",
        "calc_v02.xlsx",
        "calc",
    )
    .await;
    attach_sheet(
        &fx.pool,
        &fx.duplicate_ver,
        "1126/02_MODELOS/export_v02.xlsx",
        "export_v02.xlsx",
        "export",
    )
    .await;

    let result = apply_unify_models_inner(
        &fx.pool,
        &fx.db_path,
        UnifyModelsInput {
            project_id: fx.project_id.clone(),
            principal_document_id: fx.principal_id.clone(),
            duplicate_document_ids: vec![fx.duplicate_id.clone()],
            category_key: Some("modelos".into()),
        },
    )
    .await
    .unwrap();
    assert!(result.sheets_transferred >= 2);

    let sheet_rows: Vec<(i64, String, Option<String>)> = sqlx::query_as(
        "SELECT v.version_number, f.file_name, f.sheet_kind
         FROM document_files f
         JOIN document_versions v ON v.id = f.document_version_id
         WHERE v.document_id = ? AND f.sheet_kind IS NOT NULL
         ORDER BY f.file_name",
    )
    .bind(&fx.principal_id)
    .fetch_all(&fx.pool)
    .await
    .unwrap();
    assert_eq!(sheet_rows.len(), 2);
    assert!(sheet_rows.iter().all(|r| r.0 == 2), "sheets stay on v02 bundle");
    assert!(calc.is_file());
    assert!(export.is_file());
}

/// F — Document relationships rewired to principal.
#[tokio::test]
async fn test_f_relations_rewired() {
    let fx = fixture_split_family().await;
    let cat = category_id(&fx.pool, &fx.project_id, "memorias").await;
    let other = uuid::Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO documents (
            id, project_id, category_id, business_code, title, doc_type,
            current_version_id, status, created_at, updated_at
         ) VALUES (?, ?, ?, '1126-MC-001', 'Memoria', 'MC', NULL, 'active', datetime('now'), datetime('now'))",
    )
    .bind(&other)
    .bind(&fx.project_id)
    .bind(&cat)
    .execute(&fx.pool)
    .await
    .unwrap();
    let rel_id = uuid::Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO document_relationships (
            id, source_document_id, target_document_id, relation_type, notes, created_at
         ) VALUES (?, ?, ?, 'related', 'link', datetime('now'))",
    )
    .bind(&rel_id)
    .bind(&fx.duplicate_id)
    .bind(&other)
    .execute(&fx.pool)
    .await
    .unwrap();

    let result = apply_unify_models_inner(
        &fx.pool,
        &fx.db_path,
        UnifyModelsInput {
            project_id: fx.project_id.clone(),
            principal_document_id: fx.principal_id.clone(),
            duplicate_document_ids: vec![fx.duplicate_id.clone()],
            category_key: Some("modelos".into()),
        },
    )
    .await
    .unwrap();
    assert!(result.relations_rewired >= 1);

    let row: (String, String) = sqlx::query_as(
        "SELECT source_document_id, target_document_id FROM document_relationships WHERE id = ?",
    )
    .bind(&rel_id)
    .fetch_one(&fx.pool)
    .await
    .unwrap();
    assert_eq!(row.0, fx.principal_id);
    assert_eq!(row.1, other);
}

/// G — Sync reports unify_suggestions when family is split.
#[tokio::test]
async fn test_g_sync_offers_unify() {
    let fx = fixture_split_family().await;
    let suggestions = detect_unify_suggestions(
        &fx.pool,
        &fx.project_id,
        "modelos",
        Some(&fx.principal_id),
    )
    .await
    .unwrap();
    assert!(
        !suggestions.is_empty(),
        "split family must yield unify suggestion"
    );
    assert!(suggestions[0].document_ids.contains(&fx.principal_id));
    assert!(suggestions[0].document_ids.contains(&fx.duplicate_id));

    let sync = sync_model_versions_inner(
        &fx.pool,
        SyncModelVersionsInput {
            project_id: fx.project_id.clone(),
            category_key: Some("modelos".into()),
            document_id: Some(fx.principal_id.clone()),
        },
    )
    .await
    .unwrap();
    assert!(
        !sync.unify_suggestions.is_empty(),
        "sync must surface unify_suggestions"
    );
    // Cannot steal v02 from duplicate — versions_added stays 0.
    assert_eq!(sync.versions_added, 0);
}

/// H — Second unify/sync idempotent; DB backup path returned.
#[tokio::test]
async fn test_h_idempotent_and_backup() {
    let fx = fixture_split_family().await;
    let first = apply_unify_models_inner(
        &fx.pool,
        &fx.db_path,
        UnifyModelsInput {
            project_id: fx.project_id.clone(),
            principal_document_id: fx.principal_id.clone(),
            duplicate_document_ids: vec![fx.duplicate_id.clone()],
            category_key: Some("modelos".into()),
        },
    )
    .await
    .unwrap();
    assert!(
        Path::new(&first.db_backup_path).is_file(),
        "backup sqlite must exist: {}",
        first.db_backup_path
    );

    let sync = sync_model_versions_inner(
        &fx.pool,
        SyncModelVersionsInput {
            project_id: fx.project_id.clone(),
            category_key: Some("modelos".into()),
            document_id: Some(fx.principal_id.clone()),
        },
    )
    .await
    .unwrap();
    assert_eq!(sync.versions_added, 0);
    assert!(
        sync.unify_suggestions.is_empty(),
        "after unify, family is no longer split"
    );

    let count: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM document_versions WHERE document_id = ?")
            .bind(&fx.principal_id)
            .fetch_one(&fx.pool)
            .await
            .unwrap();
    assert_eq!(count.0, 3);
}
