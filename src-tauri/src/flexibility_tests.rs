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

/// Part B Tests A–C: update label; path/primary unchanged; list reflects label.
#[tokio::test]
async fn test_b_a_c_update_authorized_root_label() {
    let dir = tempdir().unwrap();
    let pool = pool_in(dir.path()).await;
    let primary = dir.path().join("primary");
    let secondary = dir.path().join("secondary");
    fs::create_dir_all(&primary).unwrap();
    fs::create_dir_all(&secondary).unwrap();
    let prim = upsert_primary_root(&pool, &primary).await.unwrap();
    let sec = project_paths::add_authorized_root(&pool, &secondary, Some("Extra"), false)
        .await
        .unwrap();
    let path_before = sec.absolute_path.clone();

    let updated = project_paths::update_authorized_root_label(&pool, &sec.id, "  Oficina Norte  ")
        .await
        .unwrap();
    assert_eq!(updated.label.as_deref(), Some("Oficina Norte"));
    assert_eq!(updated.absolute_path, path_before);
    assert_eq!(updated.is_primary, 0);

    let listed = project_paths::list_authorized_roots(&pool).await.unwrap();
    let listed_sec = listed.iter().find(|r| r.id == sec.id).unwrap();
    assert_eq!(listed_sec.label.as_deref(), Some("Oficina Norte"));
    assert_eq!(listed_sec.absolute_path, path_before);

    // Primary still primary; path untouched on disk
    assert!(primary.is_dir());
    assert!(secondary.is_dir());
    let listed_prim = listed.iter().find(|r| r.id == prim.id).unwrap();
    assert_eq!(listed_prim.is_primary, 1);
}

/// Part B Tests D–E: empty label rejected; unknown id errors.
#[tokio::test]
async fn test_b_d_e_label_guards() {
    let dir = tempdir().unwrap();
    let pool = pool_in(dir.path()).await;
    let primary = dir.path().join("primary");
    fs::create_dir_all(&primary).unwrap();
    let prim = upsert_primary_root(&pool, &primary).await.unwrap();

    let empty = project_paths::update_authorized_root_label(&pool, &prim.id, "   ")
        .await
        .unwrap_err();
    assert!(
        empty.to_string().to_lowercase().contains("vacía")
            || empty.to_string().to_lowercase().contains("vacia"),
        "expected empty-label error, got {empty}"
    );

    let missing = project_paths::update_authorized_root_label(&pool, "no-such-root", "X")
        .await
        .unwrap_err();
    assert!(
        missing.to_string().to_lowercase().contains("no encontrada")
            || missing.to_string().to_lowercase().contains("encontrad"),
        "expected unknown-root error, got {missing}"
    );
}

/// Part B Tests F–G: app_settings filter key round-trip; no FS move after label update.
#[tokio::test]
async fn test_b_f_g_filter_persist_and_no_path_move() {
    let dir = tempdir().unwrap();
    let pool = pool_in(dir.path()).await;
    let primary = dir.path().join("primary");
    fs::create_dir_all(&primary).unwrap();
    let prim = upsert_primary_root(&pool, &primary).await.unwrap();
    let path_before = std::fs::canonicalize(&primary).unwrap();

    db::set_setting(&pool, "project_list_root_filter", &prim.id)
        .await
        .unwrap();
    let got = db::get_setting(&pool, "project_list_root_filter")
        .await
        .unwrap();
    assert_eq!(got.as_deref(), Some(prim.id.as_str()));

    db::set_setting(&pool, "project_list_root_filter", "")
        .await
        .unwrap();
    let cleared = db::get_setting(&pool, "project_list_root_filter")
        .await
        .unwrap();
    assert_eq!(cleared.as_deref(), Some(""));

    let _ = project_paths::update_authorized_root_label(&pool, &prim.id, "Principal renombrada")
        .await
        .unwrap();
    assert_eq!(
        std::fs::canonicalize(&primary).unwrap(),
        path_before,
        "label update must not move the root folder"
    );
    assert!(path_before.is_dir());
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
