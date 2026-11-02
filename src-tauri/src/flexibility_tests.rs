//! §12 project-flexibility scenarios (unit / integration against temp DB + FS).

use std::fs;

use sqlx::SqlitePool;
use tempfile::tempdir;

use crate::category_mapping::{map_folder_name_to_key, map_project_categories};
use crate::db;
use crate::project_paths::{
    self, business_code_prefix, sanitize_folder_name, upsert_primary_root,
};

async fn pool_in(dir: &std::path::Path) -> SqlitePool {
    let db_path = dir.join("test.db");
    db::init_pool(&db_path).await.expect("pool")
}

#[tokio::test]
async fn migration_005_creates_authorized_roots_and_nullable_code() {
    let dir = tempdir().unwrap();
    let pool = pool_in(dir.path()).await;
    let roots: Vec<(String,)> =
        sqlx::query_as("SELECT name FROM sqlite_master WHERE type='table' AND name='authorized_roots'")
            .fetch_all(&pool)
            .await
            .unwrap();
    assert_eq!(roots.len(), 1);

    // code column accepts NULL
    let root = upsert_primary_root(&pool, dir.path()).await.unwrap();
    sqlx::query(
        "INSERT INTO projects (
            id, code, name, status, authorized_root_id, root_relative_path,
            import_mode, layout_mode, created_at, updated_at
         ) VALUES ('p1', NULL, 'Sin código', 'active', ?, 'FolderA', 'linked', 'mapped',
            datetime('now'), datetime('now'))",
    )
    .bind(&root.id)
    .execute(&pool)
    .await
    .unwrap();

    let row: (Option<String>, String) =
        sqlx::query_as("SELECT code, root_relative_path FROM projects WHERE id = 'p1'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(row.0.is_none());
    assert_eq!(row.1, "FolderA");
}

#[tokio::test]
async fn uuid_name_code_path_are_independent() {
    let dir = tempdir().unwrap();
    let pool = pool_in(dir.path()).await;
    let root = upsert_primary_root(&pool, dir.path()).await.unwrap();

    let folder = dir.path().join("Physical_Name");
    fs::create_dir_all(&folder).unwrap();

    sqlx::query(
        "INSERT INTO projects (
            id, code, name, status, authorized_root_id, root_relative_path,
            import_mode, layout_mode, created_at, updated_at
         ) VALUES ('p2', '2601', 'Display Name', 'active', ?, 'Physical_Name',
            'created', 'standard', datetime('now'), datetime('now'))",
    )
    .bind(&root.id)
    .execute(&pool)
    .await
    .unwrap();

    let loc = project_paths::resolve_project_location(&pool, "p2")
        .await
        .unwrap();
    assert_eq!(loc.code.as_deref(), Some("2601"));
    assert_eq!(loc.root_relative_path, "Physical_Name");
    assert_eq!(loc.business_code_prefix, "2601");
    assert!(loc.absolute_path.ends_with("Physical_Name"));
    // Rename code in DB without moving folder
    sqlx::query("UPDATE projects SET code = '9999' WHERE id = 'p2'")
        .execute(&pool)
        .await
        .unwrap();
    let loc2 = project_paths::resolve_project_location(&pool, "p2")
        .await
        .unwrap();
    assert_eq!(loc2.code.as_deref(), Some("9999"));
    assert!(loc2.absolute_path.exists());
    assert_eq!(loc2.root_relative_path, "Physical_Name");
}

#[tokio::test]
async fn multi_authorized_roots_and_link_in_place() {
    let dir = tempdir().unwrap();
    let pool = pool_in(dir.path()).await;
    let primary = dir.path().join("primary");
    let secondary = dir.path().join("secondary");
    fs::create_dir_all(&primary).unwrap();
    fs::create_dir_all(secondary.join("ExistingProj")).unwrap();
    upsert_primary_root(&pool, &primary).await.unwrap();
    let sec = project_paths::add_authorized_root(&pool, &secondary, Some("Extra"), false)
        .await
        .unwrap();

    let roots = project_paths::list_authorized_roots(&pool).await.unwrap();
    assert_eq!(roots.len(), 2);

    let found = project_paths::find_root_for_path(&pool, &secondary.join("ExistingProj"))
        .await
        .unwrap()
        .expect("under secondary");
    assert_eq!(found.0.id, sec.id);
    assert_eq!(found.2, "ExistingProj");
}

#[test]
fn smart_category_mapping_aliases() {
    assert_eq!(map_folder_name_to_key("Modelos"), Some("modelos"));
    assert_eq!(map_folder_name_to_key("02_MODELOS"), Some("modelos"));
    let dir = tempdir().unwrap();
    fs::create_dir_all(dir.path().join("Planos")).unwrap();
    fs::create_dir_all(dir.path().join("Memorias")).unwrap();
    let mapped = map_project_categories(dir.path());
    assert!(mapped.iter().any(|m| m.key == "planos" && m.matched));
    assert!(mapped.iter().any(|m| m.key == "memorias" && m.matched));
    assert!(mapped.iter().any(|m| m.key == "modelos" && !m.matched));
}

#[test]
fn no_silent_path_helpers() {
    assert!(sanitize_folder_name("../x").is_err());
    assert_eq!(business_code_prefix(None, "A/B"), "B");
}

#[tokio::test]
async fn transmissions_use_physical_rel_not_code() {
    let dir = tempdir().unwrap();
    let pool = pool_in(dir.path()).await;
    let root = upsert_primary_root(&pool, dir.path()).await.unwrap();
    let proj = dir.path().join("MyFolder");
    fs::create_dir_all(proj.join("06_TRANSMISIONES")).unwrap();

    sqlx::query(
        "INSERT INTO projects (
            id, code, name, status, authorized_root_id, root_relative_path,
            import_mode, layout_mode, created_at, updated_at
         ) VALUES ('p3', '2601', 'X', 'active', ?, 'MyFolder',
            'created', 'standard', datetime('now'), datetime('now'))",
    )
    .bind(&root.id)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO categories (id, project_id, key, name, relative_path, sort_order)
         VALUES ('c1', 'p3', 'transmisiones', 'Transmisiones', '06_TRANSMISIONES', 6)",
    )
    .execute(&pool)
    .await
    .unwrap();

    let loc = project_paths::resolve_project_location(&pool, "p3")
        .await
        .unwrap();
    // Physical path uses folder, business prefix can still be code
    assert!(loc.absolute_path.ends_with("MyFolder"));
    assert_ne!(loc.root_relative_path, "2601");
    assert_eq!(loc.business_code_prefix, "2601");
}
