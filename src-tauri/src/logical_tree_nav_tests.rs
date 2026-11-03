//! Logical tree navigation — Tests A–G (navigation/open only).
//!
//! A — Tree hierarchy from SQLite: doc → rev/version → files
//! B — Expand/collapse is state-only (no open-file side effect helpers)
//! C — Memorias/planos middle = revisions; modelos middle = versions
//! D — Resolve preferred principal vs specific file id
//! E — Multi-format preference / open-without-default / change-default
//! F — Expand state + settings policy remember/collapsed/expanded
//! G — Lazy-load: listing children does not require loading sibling trees

use std::fs;
use std::path::Path;

use serde_json::json;
use tempfile::tempdir;

use crate::commands::documents::{
    list_current_openable_files, list_document_open_formats_inner, list_document_versions_inner,
    list_revision_files_inner, list_version_files_inner, resolve_document_file_for_format,
    resolve_document_principal_file_id, set_document_open_preference_inner,
};
use crate::db;
use crate::doc_revisions::category_uses_revisions;
use crate::incorporation::{
    apply_incorporation, IncorporationApplyInput, IncorporationGroupDecision, IncorporationMode,
    PhysicalOp,
};
use crate::logical_tree::{
    self, expand_state_key, ExpandPolicy, EXPAND_POLICY_KEY,
};
use crate::project_paths::upsert_primary_root;
use crate::state::AppState;

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

async fn seed_memoria_multi_format(
    pool: &sqlx::SqlitePool,
    project_id: &str,
    root: &Path,
) -> (String, Vec<String>) {
    let mem = root.join("1126/04_MEMORIAS");
    fs::create_dir_all(&mem).unwrap();
    let names = [
        "1126-MC-002_RB.docx",
        "1126-MC-002_RB.pdf",
        "1126-MC-002_R0.docx",
        "1126-MC-002_R0.pdf",
    ];
    let mut paths = Vec::new();
    for n in &names {
        let p = mem.join(n);
        fs::write(&p, format!("fixture-{n}").as_bytes()).unwrap();
        paths.push(p.display().to_string());
    }
    let result = apply_incorporation(
        pool,
        IncorporationApplyInput {
            project_id: project_id.to_string(),
            category_key: "memorias".into(),
            absolute_paths: paths.clone(),
            dest_absolute_path: None,
            physical_op: Some(PhysicalOp::None),
            decisions: vec![IncorporationGroupDecision {
                group_key: "1126-mc-002".into(),
                mode: IncorporationMode::RegisterNew,
                document_id: None,
                title: Some("1126-MC-002".into()),
                assign_code: Some(true),
            }],
        },
    )
    .await
    .unwrap();
    let doc_id = result.groups[0].document_id.clone().unwrap();
    (doc_id, paths)
}

async fn seed_model_two_versions(
    pool: &sqlx::SqlitePool,
    project_id: &str,
    root: &Path,
) -> (String, String, String) {
    let modelos = root.join("1126/02_MODELOS");
    fs::create_dir_all(&modelos).unwrap();
    let v1 = modelos.join("1126 - MODELO_v01.sdb");
    let v2 = modelos.join("1126 - MODELO_v02.sdb");
    fs::write(&v1, b"SDB_V01").unwrap();
    fs::write(&v2, b"SDB_V02").unwrap();

    let cat: (String,) =
        sqlx::query_as("SELECT id FROM categories WHERE project_id = ? AND key = 'modelos'")
            .bind(project_id)
            .fetch_one(pool)
            .await
            .unwrap();
    let doc_id = uuid::Uuid::new_v4().to_string();
    let ver1 = uuid::Uuid::new_v4().to_string();
    let ver2 = uuid::Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO documents (
            id, project_id, category_id, business_code, title, status,
            current_version_id, created_at, updated_at
         ) VALUES (?, ?, ?, '1126-MO-001', 'MODELO', 'active', ?, datetime('now'), datetime('now'))",
    )
    .bind(&doc_id)
    .bind(project_id)
    .bind(&cat.0)
    .bind(&ver2)
    .execute(pool)
    .await
    .unwrap();

    for (vid, n, label, rel, name) in [
        (
            &ver1,
            1i64,
            "v01",
            "1126/02_MODELOS/1126 - MODELO_v01.sdb",
            "1126 - MODELO_v01.sdb",
        ),
        (
            &ver2,
            2i64,
            "v02",
            "1126/02_MODELOS/1126 - MODELO_v02.sdb",
            "1126 - MODELO_v02.sdb",
        ),
    ] {
        sqlx::query(
            "INSERT INTO document_versions (
                id, document_id, version_number, label, created_at, is_snapshot,
                source_relative_path
             ) VALUES (?, ?, ?, ?, datetime('now'), 0, ?)",
        )
        .bind(vid)
        .bind(&doc_id)
        .bind(n)
        .bind(label)
        .bind(rel)
        .execute(pool)
        .await
        .unwrap();
        let fid = uuid::Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT INTO document_files (
                id, document_version_id, relative_path, file_name, extension,
                is_available, format_label, created_at
             ) VALUES (?, ?, ?, ?, 'sdb', 1, 'SDB', datetime('now'))",
        )
        .bind(&fid)
        .bind(vid)
        .bind(rel)
        .bind(name)
        .execute(pool)
        .await
        .unwrap();
    }
    (doc_id, ver1, ver2)
}

/// A — Hierarchy reflects SQLite (not placeholders).
#[tokio::test]
async fn test_a_tree_hierarchy_from_sqlite() {
    let dir = tempdir().unwrap();
    let pool = db::init_pool(&dir.path().join("t.db")).await.unwrap();
    let root = dir.path().join("root");
    let root_id = setup_root(&pool, &root).await;
    let pid = seed_project(&pool, &root_id, Some("1126"), "1126").await;
    let (doc_id, _) = seed_memoria_multi_format(&pool, &pid, &root).await;
    let state = AppState::new(pool.clone(), dir.path().join("t.db"));

    let revs: Vec<(String, String)> =
        sqlx::query_as("SELECT id, revision_code FROM revisions WHERE document_id = ?")
            .bind(&doc_id)
            .fetch_all(&pool)
            .await
            .unwrap();
    assert_eq!(revs.len(), 2);

    let mut total_files = 0usize;
    for (rev_id, _code) in &revs {
        let files = list_revision_files_inner(&state, rev_id).await.unwrap();
        assert!(
            !files.is_empty(),
            "revision {rev_id} must have real file leaves"
        );
        total_files += files.len();
    }
    assert!(total_files >= 4, "all four linked files reachable via tree");
}

/// B — Expand state changes do not invoke open resolvers (policy/state only).
#[tokio::test]
async fn test_b_expand_is_state_only() {
    let dir = tempdir().unwrap();
    let pool = db::init_pool(&dir.path().join("t.db")).await.unwrap();
    let root = dir.path().join("root");
    let root_id = setup_root(&pool, &root).await;
    let pid = seed_project(&pool, &root_id, Some("1126"), "1126").await;

    let before = db::get_setting(&pool, EXPAND_POLICY_KEY).await.unwrap();
    assert!(before.is_none() || before.as_deref() != Some("opened"));

    logical_tree::set_expand_policy(&pool, ExpandPolicy::Collapsed)
        .await
        .unwrap();
    let map = json!({ format!("doc:{pid}"): true });
    logical_tree::set_expand_state_json(&pool, &pid, &map)
        .await
        .unwrap();

    // No open preference was set by expand helpers
    let (doc_id, _) = seed_memoria_multi_format(&pool, &pid, &root).await;
    let pref = crate::logical_tree::preferred_open_ext_from_metadata(
        sqlx::query_as::<_, (Option<String>,)>("SELECT metadata_json FROM documents WHERE id = ?")
            .bind(&doc_id)
            .fetch_one(&pool)
            .await
            .unwrap()
            .0
            .as_deref(),
    );
    assert!(pref.is_none(), "expand must not set open preference");
    assert_eq!(
        logical_tree::get_expand_policy(&pool).await.unwrap(),
        ExpandPolicy::Collapsed
    );
}

/// C — Category middle level: revisions vs versions.
#[tokio::test]
async fn test_c_middle_level_rev_vs_version() {
    assert!(category_uses_revisions("memorias"));
    assert!(category_uses_revisions("planos"));
    assert!(!category_uses_revisions("modelos"));

    let dir = tempdir().unwrap();
    let pool = db::init_pool(&dir.path().join("t.db")).await.unwrap();
    let root = dir.path().join("root");
    let root_id = setup_root(&pool, &root).await;
    let pid = seed_project(&pool, &root_id, Some("1126"), "1126").await;
    let state = AppState::new(pool.clone(), dir.path().join("t.db"));

    let (mem_id, _) = seed_memoria_multi_format(&pool, &pid, &root).await;
    let revs: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM revisions WHERE document_id = ?")
        .bind(&mem_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert!(revs.0 >= 2, "memorias expose revision middle nodes");

    let (model_id, ver1, ver2) = seed_model_two_versions(&pool, &pid, &root).await;
    let versions = list_document_versions_inner(&state, &model_id).await.unwrap();
    assert_eq!(versions.len(), 2);
    assert!(versions.iter().any(|v| v.id == ver1));
    assert!(versions.iter().any(|v| v.id == ver2 && v.is_current == 1));
    let model_revs: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM revisions WHERE document_id = ?")
            .bind(&model_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(model_revs.0, 0, "modelos middle nodes are versions, not revisions");
}

/// D — Doc principal resolve vs specific file leaf.
#[tokio::test]
async fn test_d_dblclick_resolve_paths() {
    let dir = tempdir().unwrap();
    let pool = db::init_pool(&dir.path().join("t.db")).await.unwrap();
    let root = dir.path().join("root");
    let root_id = setup_root(&pool, &root).await;
    let pid = seed_project(&pool, &root_id, Some("1126"), "1126").await;
    let (doc_id, _) = seed_memoria_multi_format(&pool, &pid, &root).await;
    let state = AppState::new(pool.clone(), dir.path().join("t.db"));

    let principal = resolve_document_principal_file_id(&state, &doc_id)
        .await
        .unwrap();
    let row: (String, String) =
        sqlx::query_as("SELECT file_name, extension FROM document_files WHERE id = ?")
            .bind(&principal)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(row.1.to_ascii_lowercase(), "docx");

    let pdf_id = resolve_document_file_for_format(&state, &doc_id, "pdf")
        .await
        .unwrap();
    assert_ne!(principal, pdf_id, "file leaf resolve differs from preferred principal");
}

/// E — Picker need, remember preference, open-without-default, change-default.
#[tokio::test]
async fn test_e_format_preference_and_context_open() {
    let dir = tempdir().unwrap();
    let pool = db::init_pool(&dir.path().join("t.db")).await.unwrap();
    let root = dir.path().join("root");
    let root_id = setup_root(&pool, &root).await;
    let pid = seed_project(&pool, &root_id, Some("1126"), "1126").await;
    let (doc_id, _) = seed_memoria_multi_format(&pool, &pid, &root).await;
    let state = AppState::new(pool.clone(), dir.path().join("t.db"));

    let formats = list_document_open_formats_inner(&state, &doc_id)
        .await
        .unwrap();
    assert!(formats.formats.contains(&"docx".into()));
    assert!(formats.formats.contains(&"pdf".into()));
    assert!(formats.needs_picker, "first multi-format needs picker");

    // Context one-shot: resolve PDF without remembering
    let pdf_before = resolve_document_file_for_format(&state, &doc_id, "pdf")
        .await
        .unwrap();
    let still = list_document_open_formats_inner(&state, &doc_id)
        .await
        .unwrap();
    assert!(still.needs_picker, "one-shot open must not set default");
    assert!(still.preferred_format.is_none());
    let _ = pdf_before;

    // Remember PDF as default
    set_document_open_preference_inner(&state, &doc_id, Some("pdf"))
        .await
        .unwrap();
    let after = list_document_open_formats_inner(&state, &doc_id)
        .await
        .unwrap();
    assert!(!after.needs_picker);
    assert_eq!(after.preferred_format.as_deref(), Some("pdf"));
    let principal = resolve_document_principal_file_id(&state, &doc_id)
        .await
        .unwrap();
    let ext: (String,) = sqlx::query_as("SELECT extension FROM document_files WHERE id = ?")
        .bind(&principal)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(ext.0.to_ascii_lowercase(), "pdf");

    // Change default to docx
    set_document_open_preference_inner(&state, &doc_id, Some("docx"))
        .await
        .unwrap();
    let principal2 = resolve_document_principal_file_id(&state, &doc_id)
        .await
        .unwrap();
    let ext2: (String,) = sqlx::query_as("SELECT extension FROM document_files WHERE id = ?")
        .bind(&principal2)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(ext2.0.to_ascii_lowercase(), "docx");
}

/// F — Expand persistence + policy modes.
#[tokio::test]
async fn test_f_expand_persist_and_policy() {
    let dir = tempdir().unwrap();
    let pool = db::init_pool(&dir.path().join("t.db")).await.unwrap();

    logical_tree::set_expand_policy(&pool, ExpandPolicy::Remember)
        .await
        .unwrap();
    assert_eq!(
        logical_tree::get_expand_policy(&pool).await.unwrap(),
        ExpandPolicy::Remember
    );

    let project = "proj-expand";
    let state = json!({
        "doc:abc": true,
        "rev:r1": true,
        "ver:v1": false
    });
    logical_tree::set_expand_state_json(&pool, project, &state)
        .await
        .unwrap();
    let loaded = logical_tree::get_expand_state_json(&pool, project)
        .await
        .unwrap();
    assert_eq!(loaded.get("doc:abc"), Some(&json!(true)));
    assert_eq!(loaded.get("rev:r1"), Some(&json!(true)));

    let key = expand_state_key(project);
    let raw = db::get_setting(&pool, &key).await.unwrap().unwrap();
    assert!(raw.contains("doc:abc"));

    logical_tree::set_expand_policy(&pool, ExpandPolicy::Expanded)
        .await
        .unwrap();
    assert_eq!(
        logical_tree::get_expand_policy(&pool).await.unwrap().as_str(),
        "expanded"
    );
    logical_tree::set_expand_policy(&pool, ExpandPolicy::Collapsed)
        .await
        .unwrap();
    assert_eq!(
        logical_tree::get_expand_policy(&pool).await.unwrap().as_str(),
        "collapsed"
    );
}

/// G — Lazy load smoke: listing one revision's files does not require loading all versions' peers eagerly.
#[tokio::test]
async fn test_g_lazy_load_scoped_queries() {
    let dir = tempdir().unwrap();
    let pool = db::init_pool(&dir.path().join("t.db")).await.unwrap();
    let root = dir.path().join("root");
    let root_id = setup_root(&pool, &root).await;
    let pid = seed_project(&pool, &root_id, Some("1126"), "1126").await;
    let (doc_id, _) = seed_memoria_multi_format(&pool, &pid, &root).await;
    let (model_id, ver1, ver2) = seed_model_two_versions(&pool, &pid, &root).await;
    let state = AppState::new(pool.clone(), dir.path().join("t.db"));

    // Documents list exists independently of children
    let docs: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM documents WHERE project_id = ?")
        .bind(&pid)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(docs.0, 2);

    // Expand only one revision → scoped file list
    let rev_id: (String,) = sqlx::query_as(
        "SELECT id FROM revisions WHERE document_id = ? ORDER BY revision_code LIMIT 1",
    )
    .bind(&doc_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    let rev_files = list_revision_files_inner(&state, &rev_id.0).await.unwrap();
    assert!(!rev_files.is_empty());
    assert!(
        rev_files.len() < 4 || rev_files.len() == 2,
        "one revision should not dump the entire document file set unless identical"
    );

    // Expand one model version only
    let v1_files = list_version_files_inner(&state, &ver1).await.unwrap();
    let v2_files = list_version_files_inner(&state, &ver2).await.unwrap();
    assert_eq!(v1_files.len(), 1);
    assert_eq!(v2_files.len(), 1);
    assert_ne!(v1_files[0].id, v2_files[0].id);

    // Current openable for model does not pull v01 when v02 is current
    let openable = list_current_openable_files(&state, &model_id).await.unwrap();
    assert_eq!(openable.len(), 1);
    assert_eq!(openable[0].document_version_id, ver2);
}
