//! Hotfix: false SAP2000 versions + wrong open.
//!
//! A — `.sbk` never creates a version
//! B — SAP aux (`$2k` / `s2k` / log) never creates a version
//! C — Only `.sdb` Save-As creates a version
//! D — Same-path modify → overwrite; version count stable
//! E — Reconcile twice → idempotent (0 new versions)
//! F — Repair file updates tracking; no duplicate version
//! G — Principal resolve opens `.sdb` not `.sbk`
//! H — Reparar historial preview+apply 6→3 with DB backup

use std::fs;
use std::path::{Path, PathBuf};

use tempfile::tempdir;

use crate::auto_version::{self, process_path, reconcile_project, register_baseline};
use crate::commands::documents::{
    repair_version_file_inner, resolve_document_principal_file_id, RepairVersionFileInput,
};
use crate::db;
use crate::engineering_files::{
    can_auto_version_extension, is_sap_auxiliary_extension, is_sap_principal_extension,
};
use crate::project_paths::upsert_primary_root;
use crate::repair_model_history::{
    apply_repair_model_history_inner, preview_repair_model_history_inner, RepairModelHistoryInput,
};
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

async fn seed_model_doc_with_sdb(
    pool: &sqlx::SqlitePool,
    project_id: &str,
    sdb: &Path,
    title: &str,
) -> (String, String) {
    let cat: (String,) =
        sqlx::query_as("SELECT id FROM categories WHERE project_id = ? AND key = 'modelos'")
            .bind(project_id)
            .fetch_one(pool)
            .await
            .unwrap();
    let doc_id = uuid::Uuid::new_v4().to_string();
    let ver_id = uuid::Uuid::new_v4().to_string();
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

    let loc = crate::project_paths::resolve_project_location(pool, project_id)
        .await
        .unwrap();
    let rel = sdb
        .strip_prefix(&loc.root_path)
        .unwrap()
        .to_string_lossy()
        .replace('\\', "/");
    let fname = sdb.file_name().unwrap().to_string_lossy().to_string();
    sqlx::query(
        "INSERT INTO document_files (
            id, document_version_id, relative_path, file_name, extension,
            size_bytes, is_available, created_at, file_role
         ) VALUES (?, ?, ?, ?, 'sdb', 20, 1, datetime('now'), 'principal')",
    )
    .bind(uuid::Uuid::new_v4().to_string())
    .bind(&ver_id)
    .bind(&rel)
    .bind(&fname)
    .execute(pool)
    .await
    .unwrap();

    register_baseline(pool, project_id, &doc_id, Some(&ver_id), sdb)
        .await
        .unwrap();
    (doc_id, ver_id)
}

async fn version_count(pool: &sqlx::SqlitePool, document_id: &str) -> i64 {
    let n: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM document_versions WHERE document_id = ?")
            .bind(document_id)
            .fetch_one(pool)
            .await
            .unwrap();
    n.0
}

#[test]
fn extension_helpers_sbk_and_sdb() {
    assert!(is_sap_principal_extension("sdb"));
    assert!(is_sap_auxiliary_extension("sbk"));
    assert!(is_sap_auxiliary_extension("$2k"));
    assert!(is_sap_auxiliary_extension("s2k"));
    assert!(is_sap_auxiliary_extension("log"));
    assert!(!can_auto_version_extension(Some("sbk")));
    assert!(!can_auto_version_extension(Some("$2k")));
    assert!(can_auto_version_extension(Some("sdb")));
}

/// A — `.sbk` never creates a version
#[tokio::test]
async fn test_a_sbk_never_creates_version() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("t.db");
    let pool = db::init_pool(&db_path).await.unwrap();
    let root_id = setup_root(&pool, dir.path()).await;
    let pid = seed_project(&pool, &root_id, Some("1126"), "1126").await;
    let modelos = dir.path().join("1126").join("02_MODELOS");
    let sdb = modelos.join("MODELO.sdb");
    write_bytes(&sdb, b"SDB_V1");
    let (doc_id, _) = seed_model_doc_with_sdb(&pool, &pid, &sdb, "MODELO").await;

    let sbk = modelos.join("MODELO.sbk");
    write_bytes(&sbk, b"SBK_BACKUP");
    let r = process_path(&pool, &sbk).await.unwrap();
    assert!(r.is_none(), "sbk must not produce auto-version result");
    assert_eq!(version_count(&pool, &doc_id).await, 1);
}

/// B — SAP aux never creates a version
#[tokio::test]
async fn test_b_aux_never_creates_version() {
    let dir = tempdir().unwrap();
    let pool = db::init_pool(&dir.path().join("t.db")).await.unwrap();
    let root_id = setup_root(&pool, dir.path()).await;
    let pid = seed_project(&pool, &root_id, Some("1126"), "1126").await;
    let modelos = dir.path().join("1126").join("02_MODELOS");
    let sdb = modelos.join("Puente.sdb");
    write_bytes(&sdb, b"SDB");
    let (doc_id, _) = seed_model_doc_with_sdb(&pool, &pid, &sdb, "Puente").await;

    for name in ["Puente.$2k", "Puente.s2k", "Puente.log", "Puente.ebk"] {
        let p = modelos.join(name);
        write_bytes(&p, b"AUX");
        let r = process_path(&pool, &p).await.unwrap();
        assert!(r.is_none(), "{name} must not create a version");
    }
    assert_eq!(version_count(&pool, &doc_id).await, 1);
}

/// C — Only `.sdb` Save-As creates a version
#[tokio::test]
async fn test_c_only_sdb_save_as_creates_version() {
    let dir = tempdir().unwrap();
    let pool = db::init_pool(&dir.path().join("t.db")).await.unwrap();
    let root_id = setup_root(&pool, dir.path()).await;
    let pid = seed_project(&pool, &root_id, Some("1126"), "1126").await;
    let modelos = dir.path().join("1126").join("02_MODELOS");
    let sdb = modelos.join("Columna.sdb");
    write_bytes(&sdb, b"SDB1");
    let (doc_id, _) = seed_model_doc_with_sdb(&pool, &pid, &sdb, "Columna").await;

    let fake = modelos.join("Columna_v02.sbk");
    write_bytes(&fake, b"FAKE");
    assert!(process_path(&pool, &fake).await.unwrap().is_none());
    assert_eq!(version_count(&pool, &doc_id).await, 1);

    let v2 = modelos.join("Columna_v02.sdb");
    write_bytes(&v2, b"SDB2");
    let r = process_path(&pool, &v2).await.unwrap().unwrap();
    assert_eq!(r.action, "version_created");
    assert_eq!(version_count(&pool, &doc_id).await, 2);
}

/// D — Same-path modify → overwrite; count stable
#[tokio::test]
async fn test_d_same_path_modify_no_new_version() {
    let dir = tempdir().unwrap();
    let pool = db::init_pool(&dir.path().join("t.db")).await.unwrap();
    let root_id = setup_root(&pool, dir.path()).await;
    let pid = seed_project(&pool, &root_id, Some("1126"), "1126").await;
    let modelos = dir.path().join("1126").join("02_MODELOS");
    let sdb = modelos.join("Frame.sdb");
    write_bytes(&sdb, b"ORIG");
    let (doc_id, _) = seed_model_doc_with_sdb(&pool, &pid, &sdb, "Frame").await;

    write_bytes(&sdb, b"CHANGED_CONTENT");
    let r = process_path(&pool, &sdb).await.unwrap().unwrap();
    assert_eq!(r.action, "overwrite_update");
    assert_eq!(version_count(&pool, &doc_id).await, 1);
}

/// E — Reconcile twice → idempotent
#[tokio::test]
async fn test_e_reconcile_idempotent() {
    let dir = tempdir().unwrap();
    let pool = db::init_pool(&dir.path().join("t.db")).await.unwrap();
    let root_id = setup_root(&pool, dir.path()).await;
    let pid = seed_project(&pool, &root_id, Some("1126"), "1126").await;
    let modelos = dir.path().join("1126").join("02_MODELOS");
    let sdb = modelos.join("Base.sdb");
    write_bytes(&sdb, b"V1");
    let (doc_id, _) = seed_model_doc_with_sdb(&pool, &pid, &sdb, "Base").await;

    let v2 = modelos.join("Base_v02.sdb");
    write_bytes(&v2, b"V2");
    // Aux siblings that previously caused false versions:
    write_bytes(&modelos.join("Base.sbk"), b"BK");
    write_bytes(&modelos.join("Base_v02.sbk"), b"BK2");
    write_bytes(&modelos.join("Base.$2k"), b"AUX");

    let first = reconcile_project(&pool, &pid).await.unwrap();
    let created: Vec<_> = first
        .iter()
        .filter(|r| r.action == "version_created")
        .collect();
    assert_eq!(created.len(), 1);
    assert_eq!(version_count(&pool, &doc_id).await, 2);

    let second = reconcile_project(&pool, &pid).await.unwrap();
    assert!(
        second.iter().all(|r| r.action != "version_created"),
        "second reconcile must not create versions: {:?}",
        second
    );
    assert_eq!(version_count(&pool, &doc_id).await, 2);

    // Already-linked path is idempotent via create_auto_version
    let again = process_path(&pool, &v2).await.unwrap().unwrap();
    assert!(
        again.action == "unchanged"
            || again.action == "already_linked"
            || again.action == "overwrite_update",
        "got {}",
        again.action
    );
    assert_eq!(version_count(&pool, &doc_id).await, 2);
}

/// F — Repair file updates tracking; no duplicate version
#[tokio::test]
async fn test_f_repair_updates_tracking_no_dup() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("t.db");
    let pool = db::init_pool(&db_path).await.unwrap();
    let root_id = setup_root(&pool, dir.path()).await;
    let pid = seed_project(&pool, &root_id, Some("1126"), "1126").await;
    let modelos = dir.path().join("1126").join("02_MODELOS");
    let sdb = modelos.join("Repair.sdb");
    write_bytes(&sdb, b"OLD");
    let (doc_id, ver_id) = seed_model_doc_with_sdb(&pool, &pid, &sdb, "Repair").await;

    let replacement = modelos.join("Repair_fixed.sdb");
    write_bytes(&replacement, b"NEW");
    let state = AppState::new(pool.clone(), db_path.clone());
    repair_version_file_inner(
        &state,
        RepairVersionFileInput {
            document_version_id: ver_id.clone(),
            absolute_path: replacement.display().to_string(),
            format_label: None,
            replace_file_id: None,
        },
    )
    .await
    .unwrap();

    assert_eq!(version_count(&pool, &doc_id).await, 1);

    // Reconcile / process replacement must not create a new version.
    let r = process_path(&pool, &replacement).await.unwrap().unwrap();
    assert!(
        r.action == "unchanged" || r.action == "overwrite_update" || r.action == "already_linked",
        "got {}",
        r.action
    );
    assert_eq!(version_count(&pool, &doc_id).await, 1);

    let tracked: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM file_version_tracking
         WHERE project_id = ? AND absolute_path = ?",
    )
    .bind(&pid)
    .bind(replacement.display().to_string())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(tracked.0, 1);
}

/// G — Principal resolve opens `.sdb` not `.sbk`
#[tokio::test]
async fn test_g_open_prefers_sdb_not_sbk() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("t.db");
    let pool = db::init_pool(&db_path).await.unwrap();
    let root_id = setup_root(&pool, dir.path()).await;
    let pid = seed_project(&pool, &root_id, Some("1126"), "1126").await;
    let modelos = dir.path().join("1126").join("02_MODELOS");
    let sdb = modelos.join("OpenMe.sdb");
    write_bytes(&sdb, b"SDB");
    let (doc_id, ver_id) = seed_model_doc_with_sdb(&pool, &pid, &sdb, "OpenMe").await;

    // Pollute current version with an .sbk row (historical false version link).
    let sbk = modelos.join("OpenMe.sbk");
    write_bytes(&sbk, b"SBK");
    let loc = crate::project_paths::resolve_project_location(&pool, &pid)
        .await
        .unwrap();
    let rel = sbk
        .strip_prefix(&loc.root_path)
        .unwrap()
        .to_string_lossy()
        .replace('\\', "/");
    sqlx::query(
        "INSERT INTO document_files (
            id, document_version_id, relative_path, file_name, extension,
            size_bytes, is_available, created_at, file_role
         ) VALUES (?, ?, ?, 'OpenMe.sbk', 'sbk', 10, 1, datetime('now'), 'principal')",
    )
    .bind(uuid::Uuid::new_v4().to_string())
    .bind(&ver_id)
    .bind(&rel)
    .execute(&pool)
    .await
    .unwrap();

    let state = AppState::new(pool.clone(), db_path);
    let file_id = resolve_document_principal_file_id(&state, &doc_id)
        .await
        .unwrap();
    let name: (String, Option<String>) =
        sqlx::query_as("SELECT file_name, extension FROM document_files WHERE id = ?")
            .bind(&file_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(
        name.0.to_ascii_lowercase().ends_with(".sdb")
            || name.1.as_deref() == Some("sdb"),
        "resolved {:?} — must be .sdb",
        name
    );
}

/// H — Reparar historial 6→3 with DB backup (case 1126 shape)
#[tokio::test]
async fn test_h_repair_historial_6_to_3_with_backup() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("t.db");
    let pool = db::init_pool(&db_path).await.unwrap();
    let root_id = setup_root(&pool, dir.path()).await;
    let pid = seed_project(&pool, &root_id, Some("1126"), "1126").await;
    let modelos = dir.path().join("1126").join("02_MODELOS");

    let cat: (String,) =
        sqlx::query_as("SELECT id FROM categories WHERE project_id = ? AND key = 'modelos'")
            .bind(&pid)
            .fetch_one(&pool)
            .await
            .unwrap();
    let doc_id = uuid::Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO documents (
            id, project_id, category_id, business_code, title, status,
            created_at, updated_at
         ) VALUES (?, ?, ?, '1126-MOD-001', 'MODELO', 'active', datetime('now'), datetime('now'))",
    )
    .bind(&doc_id)
    .bind(&pid)
    .bind(&cat.0)
    .execute(&pool)
    .await
    .unwrap();

    // 3 real .sdb + 3 false (.sbk / $2k / log) = 6 versions
    let entries: Vec<(&str, &str, &[u8])> = vec![
        ("1126 - MODELO_v01.sdb", "sdb", b"V1"),
        ("1126 - MODELO_v01.sbk", "sbk", b"B1"),
        ("1126 - MODELO_v02.sdb", "sdb", b"V2"),
        ("1126 - MODELO_v02.$2k", "$2k", b"A2"),
        ("1126 - MODELO_v03.sdb", "sdb", b"V3"),
        ("1126 - MODELO_v03.log", "log", b"L3"),
    ];

    let mut last_ver = String::new();
    for (i, (name, ext, bytes)) in entries.iter().enumerate() {
        let path = modelos.join(name);
        write_bytes(&path, bytes);
        let ver_id = uuid::Uuid::new_v4().to_string();
        let n = (i as i64) + 1;
        sqlx::query(
            "INSERT INTO document_versions (
                id, document_id, version_number, label, created_at, is_snapshot
             ) VALUES (?, ?, ?, ?, datetime('now'), 0)",
        )
        .bind(&ver_id)
        .bind(&doc_id)
        .bind(n)
        .bind(format!("v{n:02}"))
        .execute(&pool)
        .await
        .unwrap();
        let rel = format!("1126/02_MODELOS/{name}");
        sqlx::query(
            "INSERT INTO document_files (
                id, document_version_id, relative_path, file_name, extension,
                size_bytes, is_available, created_at, file_role
             ) VALUES (?, ?, ?, ?, ?, ?, 1, datetime('now'), 'principal')",
        )
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(&ver_id)
        .bind(&rel)
        .bind(name)
        .bind(ext)
        .bind(bytes.len() as i64)
        .execute(&pool)
        .await
        .unwrap();
        last_ver = ver_id;
    }
    sqlx::query("UPDATE documents SET current_version_id = ? WHERE id = ?")
        .bind(&last_ver)
        .bind(&doc_id)
        .execute(&pool)
        .await
        .unwrap();

    assert_eq!(version_count(&pool, &doc_id).await, 6);

    let preview = preview_repair_model_history_inner(
        &pool,
        RepairModelHistoryInput {
            project_id: pid.clone(),
            document_id: Some(doc_id.clone()),
        },
    )
    .await
    .unwrap();
    assert_eq!(preview.total_versions_before, 6);
    assert_eq!(preview.total_versions_after, 3);
    assert_eq!(preview.versions_to_drop, 3);

    let result = apply_repair_model_history_inner(
        &pool,
        &db_path,
        RepairModelHistoryInput {
            project_id: pid.clone(),
            document_id: Some(doc_id.clone()),
        },
    )
    .await
    .unwrap();
    assert_eq!(result.versions_removed, 3);
    assert_eq!(result.versions_kept, 3);
    assert!(
        PathBuf::from(&result.db_backup_path).is_file(),
        "backup missing: {}",
        result.db_backup_path
    );
    assert_eq!(version_count(&pool, &doc_id).await, 3);

    // Physical files all remain
    for (name, _, _) in &entries {
        assert!(modelos.join(name).is_file(), "{name} must remain on disk");
    }

    // Current must point at an .sdb version
    let state = AppState::new(pool.clone(), db_path);
    let file_id = resolve_document_principal_file_id(&state, &doc_id)
        .await
        .unwrap();
    let ext: (Option<String>,) =
        sqlx::query_as("SELECT extension FROM document_files WHERE id = ?")
            .bind(&file_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(ext.0.as_deref(), Some("sdb"));

    let _ = auto_version::list_pending(&pool, Some(&pid)).await;
}
