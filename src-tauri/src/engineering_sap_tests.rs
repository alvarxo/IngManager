//! Tests 1–10 for organizer SAP grouping / prefix normalization / bulk register.
//! Fixtures only — never mutate real engineering user files.

use std::fs;
use std::path::PathBuf;

use tempfile::tempdir;

use crate::auto_version::{parse_version_filename, process_path, register_baseline};
use crate::category_mapping::{
    map_folder_name_to_key, map_folder_name_to_key_with_codes, map_project_categories,
};
use crate::commands::engineering::{
    bulk_register_engineering_docs_inner, BulkRegisterInput,
};
use crate::db;
use crate::engineering_files::{
    count_principals, filter_visible_names, group_files, normalize_group_key,
    stems_are_distinct_models, strip_project_code_prefix,
};
use crate::organizer::{self, UpdatePlanItemInput};
use crate::project_paths::upsert_primary_root;

async fn setup_root(pool: &sqlx::SqlitePool, root: &std::path::Path) -> String {
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

fn write_dummy_sdb(path: &std::path::Path) {
    fs::write(path, b"INGMANAGER_FIXTURE_SDB").unwrap();
}

async fn index_file(
    pool: &sqlx::SqlitePool,
    project_id: &str,
    root_id: &str,
    rel: &str,
    abs: &str,
    name: &str,
    category: &str,
) {
    let ext = PathBuf::from(name)
        .extension()
        .map(|e| e.to_string_lossy().to_string());
    sqlx::query(
        "INSERT INTO indexed_files (
            id, project_id, authorized_root_id, relative_path, absolute_path,
            file_name, extension, size_bytes, is_directory, is_available,
            category_key, indexed_at
         ) VALUES (?, ?, ?, ?, ?, ?, ?, 10, 0, 1, ?, datetime('now'))",
    )
    .bind(uuid::Uuid::new_v4().to_string())
    .bind(project_id)
    .bind(root_id)
    .bind(rel)
    .bind(abs)
    .bind(name)
    .bind(ext)
    .bind(category)
    .execute(pool)
    .await
    .unwrap();
}

/// 1 — Project-code prefix on folder names maps to category.
#[test]
fn test_01_prefix_strip_folder_aliases() {
    let codes = vec!["1090".into()];
    assert_eq!(
        map_folder_name_to_key_with_codes("1090 - MODELOS", &codes),
        Some("modelos")
    );
    assert_eq!(
        map_folder_name_to_key_with_codes("1090_PLANOS", &codes),
        Some("planos")
    );
    assert_eq!(map_folder_name_to_key("1090 - MODELOS"), Some("modelos"));
    assert_eq!(
        strip_project_code_prefix("1090 - MODELOS", &codes),
        "MODELOS"
    );
}

/// 2 — Prefixed tree under project `1090` is classified.
#[test]
fn test_02_prefixed_project_tree() {
    let dir = tempdir().unwrap();
    let proj = dir.path().join("1090");
    fs::create_dir_all(proj.join("1090 - MODELOS")).unwrap();
    fs::create_dir_all(proj.join("1090 - PLANOS")).unwrap();
    fs::create_dir_all(proj.join("misc")).unwrap();
    let mapped = map_project_categories(&proj);
    assert!(mapped
        .iter()
        .any(|m| m.key == "modelos" && m.matched && m.relative_path == "1090 - MODELOS"));
    assert!(mapped
        .iter()
        .any(|m| m.key == "planos" && m.matched && m.relative_path == "1090 - PLANOS"));
}

/// 3 — .sdb principal groups with auxiliaries.
#[test]
fn test_03_sap_principal_aux_group() {
    let names = vec![
        "Puente.sdb".into(),
        "Puente.$2k".into(),
        "Puente.log".into(),
        "Puente.s2k".into(),
    ];
    let groups = group_files(&names, &[]);
    assert_eq!(groups.len(), 1);
    let g = &groups[0];
    assert_eq!(g.principal.as_ref().unwrap().file_name, "Puente.sdb");
    assert_eq!(g.auxiliaries.len(), 3);
    assert_eq!(count_principals(&names, &[]), 1);
}

/// 4 — SIN CAMIONES ≠ CON CAMIONES.
#[test]
fn test_04_sin_con_camiones_distinct() {
    let codes = vec!["1090".into()];
    let a = "1090 - MODELO SIN CAMIONES.sdb";
    let b = "1090 - MODELO CON CAMIONES.sdb";
    assert!(stems_are_distinct_models(
        PathBuf::from(a)
            .file_stem()
            .unwrap()
            .to_string_lossy()
            .as_ref(),
        PathBuf::from(b)
            .file_stem()
            .unwrap()
            .to_string_lossy()
            .as_ref(),
        &codes
    ));
    let names = vec![
        a.into(),
        b.into(),
        "1090 - MODELO SIN CAMIONES.log".into(),
    ];
    let groups = group_files(&names, &codes);
    assert_eq!(groups.len(), 2);
    assert_eq!(count_principals(&names, &codes), 2);
    let sin = groups
        .iter()
        .find(|g| g.group_key.contains("sin"))
        .unwrap();
    assert_eq!(sin.auxiliaries.len(), 1);
}

/// 5 — Hide auxiliaries filter.
#[test]
fn test_05_hide_auxiliaries() {
    let names = vec![
        "A.sdb".into(),
        "A.log".into(),
        "B.sdb".into(),
        "B.$2k".into(),
        "Nota.pdf".into(),
    ];
    let visible = filter_visible_names(&names, &[], false);
    assert!(visible.contains(&"A.sdb".into()));
    assert!(visible.contains(&"B.sdb".into()));
    assert!(visible.contains(&"Nota.pdf".into()));
    assert!(!visible.iter().any(|n| n.ends_with(".log")));
    assert!(!visible.iter().any(|n| n.ends_with(".$2k")));
}

/// 6 — Counters: models vs physical.
#[test]
fn test_06_model_vs_physical_counts() {
    let names = vec![
        "M1.sdb".into(),
        "M1.log".into(),
        "M1.out".into(),
        "M2.sdb".into(),
        "M2.s2k".into(),
    ];
    assert_eq!(names.len(), 5);
    assert_eq!(count_principals(&names, &[]), 2);
}

/// 7 — Bulk register logical docs without copying (link only).
#[tokio::test]
async fn test_07_bulk_register_without_copy() {
    let dir = tempdir().unwrap();
    let pool = db::init_pool(&dir.path().join("t.db")).await.unwrap();
    let root_id = setup_root(&pool, dir.path()).await;
    let proj_dir = dir.path().join("1090");
    let modelos = proj_dir.join("1090 - MODELOS");
    fs::create_dir_all(&modelos).unwrap();
    let sdb = modelos.join("Estructura.sdb");
    let log = modelos.join("Estructura.log");
    write_dummy_sdb(&sdb);
    fs::write(&log, b"log").unwrap();

    let project_id = seed_project(&pool, &root_id, Some("1090"), "1090").await;
    sqlx::query(
        "UPDATE categories SET relative_path = ? WHERE project_id = ? AND key = 'modelos'",
    )
    .bind("1090 - MODELOS")
    .bind(&project_id)
    .execute(&pool)
    .await
    .unwrap();

    index_file(
        &pool,
        &project_id,
        &root_id,
        "1090/1090 - MODELOS/Estructura.sdb",
        &sdb.display().to_string(),
        "Estructura.sdb",
        "modelos",
    )
    .await;
    index_file(
        &pool,
        &project_id,
        &root_id,
        "1090/1090 - MODELOS/Estructura.log",
        &log.display().to_string(),
        "Estructura.log",
        "modelos",
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
    assert_eq!(fs::read(&sdb).unwrap(), b"INGMANAGER_FIXTURE_SDB");
    // No snapshot copy of the principal
    let snap_root = dir.path().join(".ingmanager").join("snapshots");
    if snap_root.exists() {
        for entry in walkdir::WalkDir::new(&snap_root).into_iter().flatten() {
            if entry.file_type().is_file() {
                let b = fs::read(entry.path()).unwrap_or_default();
                assert_ne!(b, b"INGMANAGER_FIXTURE_SDB");
            }
        }
    }
    let role: (String,) = sqlx::query_as(
        "SELECT file_role FROM document_files WHERE file_name = 'Estructura.sdb'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(role.0, "principal");
    let aux: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM document_files WHERE file_role = 'auxiliary'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(aux.0, 1);
}

/// 8 — Organizer single-project plan + virtual map of prefixed folder.
#[tokio::test]
async fn test_08_organizer_single_project_plan() {
    let dir = tempdir().unwrap();
    let pool = db::init_pool(&dir.path().join("t.db")).await.unwrap();
    let root_id = setup_root(&pool, dir.path()).await;
    let proj = dir.path().join("1090");
    fs::create_dir_all(proj.join("1090 - MODELOS")).unwrap();
    fs::create_dir_all(proj.join("1090 - PLANOS")).unwrap();
    let project_id = seed_project(&pool, &root_id, Some("1090"), "1090").await;

    let scan = organizer::scan_scope(&pool, &dir.path().display().to_string())
        .await
        .unwrap();
    let item = scan
        .iter()
        .find(|s| s.project_id.as_deref() == Some(project_id.as_str()))
        .expect("project scanned");
    assert!(item
        .classified
        .iter()
        .any(|c| c.category_key == "modelos" && c.matched));

    let plan = organizer::build_plan(
        &pool,
        &dir.path().display().to_string(),
        "virtual",
        Some(vec![project_id.clone()]),
    )
    .await
    .unwrap();
    assert!(plan
        .items
        .iter()
        .all(|i| i.project_id.as_deref() == Some(project_id.as_str())));
    assert!(plan.items.iter().any(|i| i.action == "map_virtual"));

    let applied = organizer::apply_plan(&pool, &plan.session.id).await.unwrap();
    assert!(applied.applied > 0);
    let path: (String,) = sqlx::query_as(
        "SELECT relative_path FROM categories WHERE project_id = ? AND key = 'modelos'",
    )
    .bind(&project_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(path.0, "1090 - MODELOS");
}

/// 9 — Editable plan: skip item before apply.
#[tokio::test]
async fn test_09_editable_plan_skip() {
    let dir = tempdir().unwrap();
    let pool = db::init_pool(&dir.path().join("t.db")).await.unwrap();
    let root_id = setup_root(&pool, dir.path()).await;
    let proj = dir.path().join("P1");
    fs::create_dir_all(proj.join("Modelos")).unwrap();
    fs::create_dir_all(proj.join("Planos")).unwrap();
    let project_id = seed_project(&pool, &root_id, Some("2601"), "P1").await;

    let plan = organizer::build_plan(
        &pool,
        &dir.path().display().to_string(),
        "virtual",
        Some(vec![project_id.clone()]),
    )
    .await
    .unwrap();
    let planos = plan
        .items
        .iter()
        .find(|i| i.category_key.as_deref() == Some("planos"))
        .unwrap();
    organizer::update_plan_item(
        &pool,
        UpdatePlanItemInput {
            item_id: planos.id.clone(),
            action: Some("skip".into()),
            category_key: None,
            target_relative_path: None,
            status: Some("skipped".into()),
            detail: Some("omitido en prueba".into()),
        },
    )
    .await
    .unwrap();

    let applied = organizer::apply_plan(&pool, &plan.session.id).await.unwrap();
    assert!(applied.skipped >= 1);
    let planos_path: (String,) = sqlx::query_as(
        "SELECT relative_path FROM categories WHERE project_id = ? AND key = 'planos'",
    )
    .bind(&project_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(planos_path.0, "03_PLANOS");
}

/// 10 — Auto-version with project prefix; SIN/CON stay distinct.
#[tokio::test]
async fn test_10_auto_version_prefix_and_sin_con() {
    let dir = tempdir().unwrap();
    let pool = db::init_pool(&dir.path().join("t.db")).await.unwrap();
    let root_id = setup_root(&pool, dir.path()).await;
    let modelos = dir.path().join("1090").join("02_MODELOS");
    fs::create_dir_all(&modelos).unwrap();
    let project_id = seed_project(&pool, &root_id, Some("1090"), "1090").await;

    let sin = modelos.join("1090 - MODELO SIN CAMIONES.sdb");
    let con = modelos.join("1090 - MODELO CON CAMIONES.sdb");
    write_dummy_sdb(&sin);
    write_dummy_sdb(&con);

    let p_sin = parse_version_filename("1090 - MODELO SIN CAMIONES_v02.sdb");
    let p_con = parse_version_filename("1090 - MODELO CON CAMIONES_v02.sdb");
    assert!(p_sin.is_version_pattern);
    assert_ne!(p_sin.base_stem, p_con.base_stem);
    assert!(p_sin.base_stem.contains("sin"));
    assert!(p_con.base_stem.contains("con"));

    let cat: (String,) =
        sqlx::query_as("SELECT id FROM categories WHERE project_id = ? AND key = 'modelos'")
            .bind(&project_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    for (title, path) in [("SIN", &sin), ("CON", &con)] {
        let doc_id = uuid::Uuid::new_v4().to_string();
        let version_id = uuid::Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT INTO documents (
                id, project_id, category_id, title, doc_type, current_version_id, status,
                created_at, updated_at
             ) VALUES (?, ?, ?, ?, 'MOD', ?, 'active', datetime('now'), datetime('now'))",
        )
        .bind(&doc_id)
        .bind(&project_id)
        .bind(&cat.0)
        .bind(title)
        .bind(&version_id)
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO document_versions (
                id, document_id, version_number, label, created_at, is_snapshot
             ) VALUES (?, ?, 1, 'v01', datetime('now'), 0)",
        )
        .bind(&version_id)
        .bind(&doc_id)
        .execute(&pool)
        .await
        .unwrap();
        register_baseline(&pool, &project_id, &doc_id, Some(&version_id), path)
            .await
            .unwrap();
    }

    let v2 = modelos.join("1090 - MODELO SIN CAMIONES_v02.sdb");
    fs::copy(&sin, &v2).unwrap();
    let result = process_path(&pool, &v2).await.unwrap().unwrap();
    assert_eq!(result.action, "version_created");
    let doc_id = result.document_id.unwrap();
    let title: (String,) = sqlx::query_as("SELECT title FROM documents WHERE id = ?")
        .bind(&doc_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(title.0, "SIN");

    let _: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM engineering_groups")
        .fetch_one(&pool)
        .await
        .unwrap();
    let _ = normalize_group_key("x", &[]);
}
