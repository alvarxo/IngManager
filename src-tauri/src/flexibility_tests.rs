//! §12 project-flexibility scenarios (unit / integration against temp DB + FS).

use std::fs;

use sqlx::SqlitePool;
use tempfile::tempdir;

use crate::category_mapping::{map_folder_name_to_key, map_project_categories};
use crate::commands::projects::{
    assert_code_unique, normalize_optional_code, reprefix_business_code, rewrite_root_rel_path,
    update_project_inner,
};
use crate::db;
use crate::models::UpdateProjectInput;
use crate::project_paths::{
    self, business_code_prefix, sanitize_folder_name, upsert_primary_root,
};
use crate::state::AppState;

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

/// update_project can change logical `code` without touching `root_relative_path`.
/// Uniqueness is global (UNIQUE on projects.code), not per authorized root.
#[tokio::test]
async fn update_project_code_keeps_physical_path() {
    let dir = tempdir().unwrap();
    let pool = pool_in(dir.path()).await;
    let root = upsert_primary_root(&pool, dir.path()).await.unwrap();
    let folder = dir.path().join("Physical_Name");
    fs::create_dir_all(&folder).unwrap();

    sqlx::query(
        "INSERT INTO projects (
            id, code, name, status, authorized_root_id, root_relative_path,
            import_mode, layout_mode, created_at, updated_at
         ) VALUES ('p-code', '1090', 'Proyecto', 'active', ?, 'Physical_Name',
            'linked', 'mapped', datetime('now'), datetime('now'))",
    )
    .bind(&root.id)
    .execute(&pool)
    .await
    .unwrap();

    // Second project occupies another code (global uniqueness).
    sqlx::query(
        "INSERT INTO projects (
            id, code, name, status, authorized_root_id, root_relative_path,
            import_mode, layout_mode, created_at, updated_at
         ) VALUES ('p-other', '2601', 'Otro', 'active', ?, 'Other',
            'linked', 'mapped', datetime('now'), datetime('now'))",
    )
    .bind(&root.id)
    .execute(&pool)
    .await
    .unwrap();

    let input = UpdateProjectInput {
        id: "p-code".into(),
        name: None,
        description: None,
        client_name: None,
        code: Some("9999".into()),
        status: None,
        start_date: None,
        end_date: None,
        is_favorite: None,
        physical_revision_folders: None,
        rename_root_folder: None,
    };
    let new_code = normalize_optional_code(input.code.as_deref())
        .unwrap()
        .expect("code set");
    assert_code_unique(&pool, &new_code, Some(&input.id))
        .await
        .expect("9999 free");
    assert!(assert_code_unique(&pool, "2601", Some(&input.id))
        .await
        .is_err());

    // Same columns as update_project (code only; never root_relative_path).
    sqlx::query(
        "UPDATE projects SET code = ?, updated_at = datetime('now') WHERE id = ?",
    )
    .bind(&new_code)
    .bind(&input.id)
    .execute(&pool)
    .await
    .unwrap();

    let row: (Option<String>, String) =
        sqlx::query_as("SELECT code, root_relative_path FROM projects WHERE id = 'p-code'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(row.0.as_deref(), Some("9999"));
    assert_eq!(row.1, "Physical_Name");
    assert!(folder.is_dir());

    // Empty code clears logical code; path still untouched.
    let cleared = normalize_optional_code(Some("  ")).unwrap();
    assert!(cleared.is_none());
    sqlx::query("UPDATE projects SET code = NULL WHERE id = 'p-code'")
        .execute(&pool)
        .await
        .unwrap();
    let row2: (Option<String>, String) =
        sqlx::query_as("SELECT code, root_relative_path FROM projects WHERE id = 'p-code'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(row2.0.is_none());
    assert_eq!(row2.1, "Physical_Name");

    assert!(normalize_optional_code(Some("a/b")).is_err());
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

#[test]
fn reprefix_and_path_helpers() {
    assert_eq!(
        reprefix_business_code("1133_3-MC-001", "1133_3", "1133_2").as_deref(),
        Some("1133_2-MC-001")
    );
    assert_eq!(
        reprefix_business_code("1133_3-SAP-002", "1133_3", "1133_2").as_deref(),
        Some("1133_2-SAP-002")
    );
    assert!(reprefix_business_code("OTHER-MC-001", "1133_3", "1133_2").is_none());
    assert_eq!(
        rewrite_root_rel_path("1133_3/01_MEMORIAS/a.docx", "1133_3", "1133_2").as_deref(),
        Some("1133_2/01_MEMORIAS/a.docx")
    );
    assert_eq!(
        rewrite_root_rel_path("1133_3", "1133_3", "1133_2").as_deref(),
        Some("1133_2")
    );
    assert!(rewrite_root_rel_path("other/x", "1133_3", "1133_2").is_none());
}

#[tokio::test]
async fn update_project_syncs_document_business_codes() {
    let dir = tempdir().unwrap();
    let pool = pool_in(dir.path()).await;
    let root = upsert_primary_root(&pool, dir.path()).await.unwrap();
    let folder = dir.path().join("1133_3");
    fs::create_dir_all(&folder).unwrap();

    sqlx::query(
        "INSERT INTO projects (
            id, code, name, status, authorized_root_id, root_relative_path,
            import_mode, layout_mode, created_at, updated_at
         ) VALUES ('p-sync', '1133_3', 'Andamios', 'active', ?, '1133_3',
            'created', 'standard', datetime('now'), datetime('now'))",
    )
    .bind(&root.id)
    .execute(&pool)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO documents (
            id, project_id, category_id, business_code, title, doc_type,
            status, created_at, updated_at
         ) VALUES
           ('d1', 'p-sync', NULL, '1133_3-MC-001', 'Memoria 1', 'memory',
            'desarrollo', datetime('now'), datetime('now')),
           ('d2', 'p-sync', NULL, '1133_3-MC-002', 'Memoria 2', 'memory',
            'desarrollo', datetime('now'), datetime('now')),
           ('d3', 'p-sync', NULL, 'OTHER-MC-009', 'Alien', 'memory',
            'desarrollo', datetime('now'), datetime('now'))",
    )
    .execute(&pool)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO transmissions (
            id, project_id, direction, code, status, created_at
         ) VALUES ('t1', 'p-sync', 'sent', '1133_3-ENV-001', 'complete', datetime('now'))",
    )
    .execute(&pool)
    .await
    .unwrap();

    let state = AppState::new(pool.clone(), dir.path().join("test.db"));
    let updated = update_project_inner(
        &state,
        UpdateProjectInput {
            id: "p-sync".into(),
            name: None,
            description: None,
            client_name: None,
            code: Some("1133_2".into()),
            status: None,
            start_date: None,
            end_date: None,
            is_favorite: None,
            physical_revision_folders: None,
            rename_root_folder: Some(false),
        },
    )
    .await
    .unwrap();

    assert_eq!(updated.code.as_deref(), Some("1133_2"));
    assert_eq!(updated.root_relative_path, "1133_3");
    assert!(folder.is_dir());

    let codes: Vec<(String, Option<String>)> =
        sqlx::query_as("SELECT id, business_code FROM documents WHERE project_id = 'p-sync' ORDER BY id")
            .fetch_all(&pool)
            .await
            .unwrap();
    assert_eq!(codes[0].1.as_deref(), Some("1133_2-MC-001"));
    assert_eq!(codes[1].1.as_deref(), Some("1133_2-MC-002"));
    assert_eq!(codes[2].1.as_deref(), Some("OTHER-MC-009"));

    let tx_code: (Option<String>,) =
        sqlx::query_as("SELECT code FROM transmissions WHERE id = 't1'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(tx_code.0.as_deref(), Some("1133_2-ENV-001"));
}

#[tokio::test]
async fn update_project_renames_root_folder_when_requested() {
    let dir = tempdir().unwrap();
    let pool = pool_in(dir.path()).await;
    let root = upsert_primary_root(&pool, dir.path()).await.unwrap();
    let old_folder = dir.path().join("1133_3");
    fs::create_dir_all(old_folder.join("01_MEMORIAS")).unwrap();
    fs::write(old_folder.join("01_MEMORIAS").join("a.docx"), b"x").unwrap();

    sqlx::query(
        "INSERT INTO projects (
            id, code, name, status, authorized_root_id, root_relative_path,
            import_mode, layout_mode, created_at, updated_at
         ) VALUES ('p-ren', '1133_3', 'Andamios', 'active', ?, '1133_3',
            'created', 'standard', datetime('now'), datetime('now'))",
    )
    .bind(&root.id)
    .execute(&pool)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO documents (
            id, project_id, category_id, business_code, title, doc_type,
            status, created_at, updated_at
         ) VALUES ('d-ren', 'p-ren', NULL, '1133_3-MC-001', 'M1', 'memory',
            'desarrollo', datetime('now'), datetime('now'))",
    )
    .execute(&pool)
    .await
    .unwrap();

    let ver_id = "v-ren";
    sqlx::query(
        "INSERT INTO document_versions (
            id, document_id, version_number, label, created_at, is_snapshot
         ) VALUES (?, 'd-ren', 1, 'v01', datetime('now'), 0)",
    )
    .bind(ver_id)
    .execute(&pool)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO document_files (
            id, document_version_id, relative_path, file_name, extension,
            size_bytes, is_available, created_at
         ) VALUES ('f-ren', ?, '1133_3/01_MEMORIAS/a.docx', 'a.docx', 'docx',
            1, 1, datetime('now'))",
    )
    .bind(ver_id)
    .execute(&pool)
    .await
    .unwrap();

    sqlx::query(
        "UPDATE document_versions SET source_relative_path = '1133_3/01_MEMORIAS/a.docx'
         WHERE id = ?",
    )
    .bind(ver_id)
    .execute(&pool)
    .await
    .unwrap();

    let state = AppState::new(pool.clone(), dir.path().join("test.db"));
    let updated = update_project_inner(
        &state,
        UpdateProjectInput {
            id: "p-ren".into(),
            name: None,
            description: None,
            client_name: None,
            code: Some("1133_2".into()),
            status: None,
            start_date: None,
            end_date: None,
            is_favorite: None,
            physical_revision_folders: None,
            rename_root_folder: Some(true),
        },
    )
    .await
    .unwrap();

    assert_eq!(updated.code.as_deref(), Some("1133_2"));
    assert_eq!(updated.root_relative_path, "1133_2");
    assert!(!old_folder.exists());
    assert!(dir.path().join("1133_2").join("01_MEMORIAS").join("a.docx").is_file());

    let doc_code: (Option<String>,) =
        sqlx::query_as("SELECT business_code FROM documents WHERE id = 'd-ren'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(doc_code.0.as_deref(), Some("1133_2-MC-001"));

    let file_rel: (String,) =
        sqlx::query_as("SELECT relative_path FROM document_files WHERE id = 'f-ren'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(file_rel.0, "1133_2/01_MEMORIAS/a.docx");

    let src_rel: (Option<String>,) =
        sqlx::query_as("SELECT source_relative_path FROM document_versions WHERE id = ?")
            .bind(ver_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(src_rel.0.as_deref(), Some("1133_2/01_MEMORIAS/a.docx"));
}

#[tokio::test]
async fn update_project_rename_conflicts_with_existing_folder() {
    let dir = tempdir().unwrap();
    let pool = pool_in(dir.path()).await;
    let root = upsert_primary_root(&pool, dir.path()).await.unwrap();
    fs::create_dir_all(dir.path().join("1133_3")).unwrap();
    fs::create_dir_all(dir.path().join("1133_2")).unwrap(); // collision

    sqlx::query(
        "INSERT INTO projects (
            id, code, name, status, authorized_root_id, root_relative_path,
            import_mode, layout_mode, created_at, updated_at
         ) VALUES ('p-col', '1133_3', 'X', 'active', ?, '1133_3',
            'created', 'standard', datetime('now'), datetime('now'))",
    )
    .bind(&root.id)
    .execute(&pool)
    .await
    .unwrap();

    let state = AppState::new(pool.clone(), dir.path().join("test.db"));
    let err = update_project_inner(
        &state,
        UpdateProjectInput {
            id: "p-col".into(),
            name: None,
            description: None,
            client_name: None,
            code: Some("1133_2".into()),
            status: None,
            start_date: None,
            end_date: None,
            is_favorite: None,
            physical_revision_folders: None,
            rename_root_folder: Some(true),
        },
    )
    .await
    .expect_err("folder collision");
    let msg = err.to_string();
    assert!(
        msg.contains("1133_2") || msg.contains("existe"),
        "unexpected error: {msg}"
    );

    // Code and path unchanged after failed rename
    let row: (Option<String>, String) =
        sqlx::query_as("SELECT code, root_relative_path FROM projects WHERE id = 'p-col'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(row.0.as_deref(), Some("1133_3"));
    assert_eq!(row.1, "1133_3");
    assert!(dir.path().join("1133_3").is_dir());
}
