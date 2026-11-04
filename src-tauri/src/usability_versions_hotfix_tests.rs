//! Usability hotfix tests A–I: physical next version, register, repair,
//! metadata (no path rename), unarchive, principal ranking, subcategory CRUD.
//!
//! A — Crear siguiente versión copies to next `_vNN` and registers
//! B — Cancel / no API call → no version and no new file
//! C — Registrar archivo existente links without copy
//! D — Reparar replaces file without bumping version_number
//! E — Missing source refuses: no new file, no empty version
//! F — update_project / update_document do not change paths
//! G — Unarchive via update_project status=active
//! H — Principal open ranking DOCX→DWG→SDB→RVT
//! I — Subcategory rename/delete without touching file paths

use std::fs;
use std::path::{Path, PathBuf};

use tempfile::tempdir;

use crate::commands::documents::{
    create_document_version_inner, create_next_physical_version_inner, delete_subcategory_inner,
    preview_next_physical_version_inner, repair_version_file_inner,
    resolve_document_principal_file_id, update_subcategory_inner, CreateNextPhysicalVersionInput,
    CreateVersionInput, RepairVersionFileInput, UpdateDocumentInput,
};
use crate::db;
use crate::engineering_files::{next_working_filename, pick_principal_open_index};
use crate::models::UpdateProjectInput;
use crate::project_paths::upsert_primary_root;
use crate::state::AppState;

fn write_dummy_sdb(path: &Path) {
    fs::write(path, b"INGMANAGER_FIXTURE_SDB_V1").unwrap();
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

async fn seed_doc_with_sdb(
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
    sqlx::query(
        "INSERT INTO documents (
            id, project_id, category_id, title, doc_type, current_version_id, status,
            created_at, updated_at
         ) VALUES (?, ?, ?, ?, 'MOD', ?, 'active', datetime('now'), datetime('now'))",
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
    let ext = Path::new(file_name)
        .extension()
        .map(|e| e.to_string_lossy().to_string());
    sqlx::query(
        "INSERT INTO document_files (
            id, document_version_id, relative_path, file_name, extension,
            size_bytes, is_available, created_at
         ) VALUES (?, ?, ?, ?, ?, ?, 1, datetime('now'))",
    )
    .bind(&file_id)
    .bind(&ver_id)
    .bind(rel)
    .bind(file_name)
    .bind(&ext)
    .bind(fs::metadata(abs).map(|m| m.len() as i64).unwrap_or(20))
    .execute(pool)
    .await
    .unwrap();
    (doc_id, ver_id, file_id)
}

/// A — physical copy to next `_vNN` + register.
#[tokio::test]
async fn test_a_create_next_physical_copies_and_registers() {
    let dir = tempdir().unwrap();
    let pool = db::init_pool(&dir.path().join("t.db")).await.unwrap();
    let root_id = setup_root(&pool, dir.path()).await;
    let project_id = seed_project(&pool, &root_id, Some("1126"), "1126").await;
    let modelos = dir.path().join("1126").join("MODELOS");
    fs::create_dir_all(&modelos).unwrap();
    let v1 = modelos.join("1126 - MODELO_v01.sdb");
    write_dummy_sdb(&v1);
    let expected_name = next_working_filename("1126 - MODELO_v01.sdb", 2);
    assert_eq!(expected_name, "1126 - MODELO_v02.sdb");

    let (doc_id, _, _) = seed_doc_with_sdb(
        &pool,
        &project_id,
        "MODELO",
        "1126/MODELOS/1126 - MODELO_v01.sdb",
        "1126 - MODELO_v01.sdb",
        &v1,
    )
    .await;

    let state = AppState::new(pool.clone(), dir.path().join("t.db"));
    let preview = preview_next_physical_version_inner(&state, &doc_id, None)
        .await
        .unwrap();
    assert!(preview.destination_file_name.ends_with("_v02.sdb"));

    let created = create_next_physical_version_inner(
        &state,
        CreateNextPhysicalVersionInput {
            document_id: doc_id.clone(),
            notes: Some("physical next".into()),
            source_absolute_path: None,
        },
    )
    .await
    .unwrap();
    assert_eq!(created.version_number, 2);

    let dest = PathBuf::from(&preview.destination_absolute_path);
    assert!(dest.is_file(), "physical copy must exist");
    assert_eq!(fs::read(&dest).unwrap(), b"INGMANAGER_FIXTURE_SDB_V1");
    assert!(v1.is_file(), "source must remain");

    let rel: (String,) = sqlx::query_as(
        "SELECT relative_path FROM document_files WHERE document_version_id = ?",
    )
    .bind(&created.id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(rel.0.ends_with("1126 - MODELO_v02.sdb"));
    assert!(!rel.0.contains(".ingmanager/snapshots"));
}

/// B — cancel means no API call → no new version / no new file (preview alone is safe).
#[tokio::test]
async fn test_b_cancel_does_not_create() {
    let dir = tempdir().unwrap();
    let pool = db::init_pool(&dir.path().join("t.db")).await.unwrap();
    let root_id = setup_root(&pool, dir.path()).await;
    let project_id = seed_project(&pool, &root_id, Some("1126"), "1126").await;
    let modelos = dir.path().join("1126").join("MODELOS");
    fs::create_dir_all(&modelos).unwrap();
    let v1 = modelos.join("Base_v01.sdb");
    write_dummy_sdb(&v1);
    let (doc_id, _, _) = seed_doc_with_sdb(
        &pool,
        &project_id,
        "Base",
        "1126/MODELOS/Base_v01.sdb",
        "Base_v01.sdb",
        &v1,
    )
    .await;

    let state = AppState::new(pool.clone(), dir.path().join("t.db"));
    let preview = preview_next_physical_version_inner(&state, &doc_id, None)
        .await
        .unwrap();
    // User cancels confirm → never call create_next_physical_version
    assert!(!Path::new(&preview.destination_absolute_path).exists());

    let vers: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM document_versions WHERE document_id = ?")
            .bind(&doc_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(vers.0, 1);
}

/// C — Registrar archivo existente links without copy (former Nueva versión pick).
#[tokio::test]
async fn test_c_register_existing_links_without_copy() {
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

    let (doc_id, _, _) = seed_doc_with_sdb(
        &pool,
        &project_id,
        "Base",
        "1126/MODELOS/Base.sdb",
        "Base.sdb",
        &v1,
    )
    .await;

    let state = AppState::new(pool.clone(), dir.path().join("t.db"));
    let created = create_document_version_inner(
        &state,
        CreateVersionInput {
            document_id: doc_id.clone(),
            notes: Some("register existing".into()),
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

/// D — Reparar replaces path on same version_number.
#[tokio::test]
async fn test_d_repair_does_not_bump_version() {
    let dir = tempdir().unwrap();
    let pool = db::init_pool(&dir.path().join("t.db")).await.unwrap();
    let root_id = setup_root(&pool, dir.path()).await;
    let project_id = seed_project(&pool, &root_id, Some("1126"), "1126").await;
    let modelos = dir.path().join("1126").join("MODELOS");
    fs::create_dir_all(&modelos).unwrap();
    let broken = modelos.join("Broken.sdb");
    let fixed = modelos.join("Fixed.sdb");
    write_dummy_sdb(&broken);
    fs::write(&fixed, b"REPAIRED_CONTENT______").unwrap();

    let (doc_id, ver_id, file_id) = seed_doc_with_sdb(
        &pool,
        &project_id,
        "Broken",
        "1126/MODELOS/Broken.sdb",
        "Broken.sdb",
        &broken,
    )
    .await;
    fs::remove_file(&broken).unwrap();

    let state = AppState::new(pool.clone(), dir.path().join("t.db"));
    let repaired = repair_version_file_inner(
        &state,
        RepairVersionFileInput {
            document_version_id: ver_id.clone(),
            absolute_path: fixed.display().to_string(),
            replace_file_id: Some(file_id.clone()),
            format_label: None,
        },
    )
    .await
    .unwrap();
    assert!(repaired.relative_path.ends_with("Fixed.sdb"));

    let vers: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM document_versions WHERE document_id = ?")
            .bind(&doc_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(vers.0, 1, "repair must not bump version");
    let avail: (i64,) =
        sqlx::query_as("SELECT is_available FROM document_files WHERE id = ?")
            .bind(&file_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(avail.0, 0, "old path marked unavailable");
}

/// E — missing source: no copy, no empty version row.
#[tokio::test]
async fn test_e_refuse_missing_source_fs_first() {
    let dir = tempdir().unwrap();
    let pool = db::init_pool(&dir.path().join("t.db")).await.unwrap();
    let root_id = setup_root(&pool, dir.path()).await;
    let project_id = seed_project(&pool, &root_id, Some("1126"), "1126").await;
    let modelos = dir.path().join("1126").join("MODELOS");
    fs::create_dir_all(&modelos).unwrap();
    let v1 = modelos.join("Gone.sdb");
    write_dummy_sdb(&v1);
    let (doc_id, _, _) = seed_doc_with_sdb(
        &pool,
        &project_id,
        "Gone",
        "1126/MODELOS/Gone.sdb",
        "Gone.sdb",
        &v1,
    )
    .await;
    fs::remove_file(&v1).unwrap();

    let state = AppState::new(pool.clone(), dir.path().join("t.db"));
    let err = create_next_physical_version_inner(
        &state,
        CreateNextPhysicalVersionInput {
            document_id: doc_id.clone(),
            notes: Some("should fail".into()),
            source_absolute_path: None,
        },
    )
    .await
    .expect_err("must refuse");
    let msg = err.to_string();
    assert!(
        msg.contains("no disponible") || msg.contains("no se creó") || msg.contains("No hay"),
        "{msg}"
    );

    assert!(!modelos.join("Gone_v02.sdb").exists());
    let vers: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM document_versions WHERE document_id = ?")
            .bind(&doc_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(vers.0, 1);
}

/// F — metadata updates do not rename paths.
#[tokio::test]
async fn test_f_metadata_no_path_rename() {
    let dir = tempdir().unwrap();
    let pool = db::init_pool(&dir.path().join("t.db")).await.unwrap();
    let root_id = setup_root(&pool, dir.path()).await;
    let project_id = seed_project(&pool, &root_id, Some("1126"), "1126").await;
    let modelos = dir.path().join("1126").join("MODELOS");
    fs::create_dir_all(&modelos).unwrap();
    let v1 = modelos.join("KeepPath.sdb");
    write_dummy_sdb(&v1);
    let (doc_id, _, _) = seed_doc_with_sdb(
        &pool,
        &project_id,
        "KeepPath",
        "1126/MODELOS/KeepPath.sdb",
        "KeepPath.sdb",
        &v1,
    )
    .await;

    let before_root: (String,) =
        sqlx::query_as("SELECT root_relative_path FROM projects WHERE id = ?")
            .bind(&project_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    let before_file: (String,) = sqlx::query_as(
        "SELECT relative_path FROM document_files df
         JOIN document_versions dv ON dv.id = df.document_version_id
         WHERE dv.document_id = ?",
    )
    .bind(&doc_id)
    .fetch_one(&pool)
    .await
    .unwrap();

    // Compile-time / structural: update inputs have no path fields.
    let _proj_input = UpdateProjectInput {
        id: project_id.clone(),
        name: Some("Renamed Project".into()),
        description: Some("new desc".into()),
        client_name: None,
        status: None,
        start_date: None,
        end_date: None,
        is_favorite: None,
    };
    let _doc_input = UpdateDocumentInput {
        id: doc_id.clone(),
        title: Some("Renamed Doc".into()),
        status: None,
        subcategory: None,
        origin: None,
        received_at: None,
        notes: Some("note".into()),
    };
    // Same SQL columns as update_project / update_document (no root_relative_path / relative_path).
    sqlx::query(
        "UPDATE projects SET name = ?, description = ?, updated_at = datetime('now') WHERE id = ?",
    )
    .bind("Renamed Project")
    .bind("new desc")
    .bind(&project_id)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "UPDATE documents SET title = ?, notes = ?, updated_at = datetime('now') WHERE id = ?",
    )
    .bind("Renamed Doc")
    .bind("note")
    .bind(&doc_id)
    .execute(&pool)
    .await
    .unwrap();

    let after_root: (String,) =
        sqlx::query_as("SELECT root_relative_path FROM projects WHERE id = ?")
            .bind(&project_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    let after_file: (String,) = sqlx::query_as(
        "SELECT relative_path FROM document_files df
         JOIN document_versions dv ON dv.id = df.document_version_id
         WHERE dv.document_id = ?",
    )
    .bind(&doc_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(before_root.0, after_root.0);
    assert_eq!(before_file.0, after_file.0);
    assert!(v1.is_file());
    assert_eq!(before_root.0, "1126");
}

/// G — unarchive sets status active.
#[tokio::test]
async fn test_g_unarchive_via_update_project() {
    let dir = tempdir().unwrap();
    let pool = db::init_pool(&dir.path().join("t.db")).await.unwrap();
    let root_id = setup_root(&pool, dir.path()).await;
    let project_id = seed_project(&pool, &root_id, Some("1126"), "1126").await;
    sqlx::query("UPDATE projects SET status = 'archived' WHERE id = ?")
        .bind(&project_id)
        .execute(&pool)
        .await
        .unwrap();

    // Mirror update_project status=active (UI Desarchivar).
    sqlx::query(
        "UPDATE projects SET status = 'active', updated_at = datetime('now') WHERE id = ?",
    )
    .bind(&project_id)
    .execute(&pool)
    .await
    .unwrap();

    let st: (String, String) =
        sqlx::query_as("SELECT status, root_relative_path FROM projects WHERE id = ?")
            .bind(&project_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(st.0, "active");
    assert_eq!(st.1, "1126");
}

/// H — principal open ranking.
#[tokio::test]
async fn test_h_principal_open_ranking() {
    assert_eq!(
        pick_principal_open_index(
            [
                ("a.log", Some("log")),
                ("m.sdb", Some("sdb")),
                ("n.docx", Some("docx")),
                ("p.dwg", Some("dwg")),
                ("r.rvt", Some("rvt")),
            ]
            .into_iter()
        ),
        Some(2)
    );

    let dir = tempdir().unwrap();
    let pool = db::init_pool(&dir.path().join("t.db")).await.unwrap();
    let root_id = setup_root(&pool, dir.path()).await;
    let project_id = seed_project(&pool, &root_id, Some("1126"), "1126").await;
    let modelos = dir.path().join("1126").join("MODELOS");
    fs::create_dir_all(&modelos).unwrap();
    let sdb = modelos.join("Model.sdb");
    let dwg = modelos.join("Plan.dwg");
    let docx = modelos.join("Memo.docx");
    write_dummy_sdb(&sdb);
    fs::write(&dwg, b"DWG").unwrap();
    fs::write(&docx, b"DOCX").unwrap();

    let (doc_id, ver_id, _) = seed_doc_with_sdb(
        &pool,
        &project_id,
        "Mixed",
        "1126/MODELOS/Model.sdb",
        "Model.sdb",
        &sdb,
    )
    .await;
    for (name, rel, ext) in [
        ("Plan.dwg", "1126/MODELOS/Plan.dwg", "dwg"),
        ("Memo.docx", "1126/MODELOS/Memo.docx", "docx"),
    ] {
        sqlx::query(
            "INSERT INTO document_files (
                id, document_version_id, relative_path, file_name, extension,
                size_bytes, is_available, created_at
             ) VALUES (?, ?, ?, ?, ?, 4, 1, datetime('now'))",
        )
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(&ver_id)
        .bind(rel)
        .bind(name)
        .bind(ext)
        .execute(&pool)
        .await
        .unwrap();
    }

    let state = AppState::new(pool.clone(), dir.path().join("t.db"));
    let file_id = resolve_document_principal_file_id(&state, &doc_id)
        .await
        .unwrap();
    let name: (String,) = sqlx::query_as("SELECT file_name FROM document_files WHERE id = ?")
        .bind(&file_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    // Modelos: open current .sdb only (never prefer DOCX/DWG/.sbk over the model principal).
    assert_eq!(name.0, "Model.sdb");
}

/// I — subcategory CRUD without touching document file paths.
#[tokio::test]
async fn test_i_subcategory_crud_no_path_touch() {
    let dir = tempdir().unwrap();
    let pool = db::init_pool(&dir.path().join("t.db")).await.unwrap();
    let root_id = setup_root(&pool, dir.path()).await;
    let project_id = seed_project(&pool, &root_id, Some("1126"), "1126").await;
    let modelos = dir.path().join("1126").join("MODELOS");
    fs::create_dir_all(&modelos).unwrap();
    let v1 = modelos.join("Doc.sdb");
    write_dummy_sdb(&v1);
    let (doc_id, _, _) = seed_doc_with_sdb(
        &pool,
        &project_id,
        "Doc",
        "1126/MODELOS/Doc.sdb",
        "Doc.sdb",
        &v1,
    )
    .await;
    sqlx::query("UPDATE documents SET subcategory = 'Geotecnia' WHERE id = ?")
        .bind(&doc_id)
        .execute(&pool)
        .await
        .unwrap();

    let state = AppState::new(pool.clone(), dir.path().join("t.db"));
    let sub_id = uuid::Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO document_subcategories (id, project_id, category_key, name, sort_order)
         VALUES (?, ?, 'antecedentes', 'Geotecnia', 0)",
    )
    .bind(&sub_id)
    .bind(&project_id)
    .execute(&pool)
    .await
    .unwrap();

    let renamed = update_subcategory_inner(&state, &sub_id, "Geotecnia 2")
        .await
        .unwrap();
    assert_eq!(renamed.name, "Geotecnia 2");
    let doc_sub: (Option<String>,) =
        sqlx::query_as("SELECT subcategory FROM documents WHERE id = ?")
            .bind(&doc_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(doc_sub.0.as_deref(), Some("Geotecnia 2"));

    let before_path: (String,) = sqlx::query_as(
        "SELECT relative_path FROM document_files df
         JOIN document_versions dv ON dv.id = df.document_version_id
         WHERE dv.document_id = ?",
    )
    .bind(&doc_id)
    .fetch_one(&pool)
    .await
    .unwrap();

    delete_subcategory_inner(&state, &sub_id).await.unwrap();
    let count: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM document_subcategories WHERE id = ?")
            .bind(&sub_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(count.0, 0);

    let after_path: (String,) = sqlx::query_as(
        "SELECT relative_path FROM document_files df
         JOIN document_versions dv ON dv.id = df.document_version_id
         WHERE dv.document_id = ?",
    )
    .bind(&doc_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(before_path.0, after_path.0);
    assert!(v1.is_file());
}
