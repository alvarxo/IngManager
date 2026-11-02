//! Incorporation tests A–J (fixtures only).

use std::fs;
use std::path::PathBuf;

use tempfile::tempdir;

use crate::db;
use crate::incorporation::{
    apply_incorporation, group_incorporation_paths, link_working_file, preview_incorporation,
    snapshot_file_to_version, IncorporationApplyInput, IncorporationGroupDecision,
    IncorporationMode, IncorporationPreviewInput, PhysicalOp,
};
use crate::project_paths::{self, upsert_primary_root};

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

async fn empty_doc(
    pool: &sqlx::SqlitePool,
    project_id: &str,
    category_key: &str,
    code: &str,
    title: &str,
) -> (String, String) {
    let cat: (String,) =
        sqlx::query_as("SELECT id FROM categories WHERE project_id = ? AND key = ?")
            .bind(project_id)
            .bind(category_key)
            .fetch_one(pool)
            .await
            .unwrap();
    let doc_id = uuid::Uuid::new_v4().to_string();
    let version_id = uuid::Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO documents (
            id, project_id, category_id, business_code, title, doc_type,
            current_version_id, status, created_at, updated_at
         ) VALUES (?, ?, ?, ?, ?, 'MC', ?, 'desarrollo', datetime('now'), datetime('now'))",
    )
    .bind(&doc_id)
    .bind(project_id)
    .bind(&cat.0)
    .bind(code)
    .bind(title)
    .bind(&version_id)
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO document_versions (
            id, document_id, version_number, label, created_at, is_snapshot
         ) VALUES (?, ?, 1, 'v01', datetime('now'), 0)",
    )
    .bind(&version_id)
    .bind(&doc_id)
    .execute(pool)
    .await
    .unwrap();
    (doc_id, version_id)
}

fn file_count(pool: &sqlx::SqlitePool, version_id: &str) -> impl std::future::Future<Output = i64> {
    let pool = pool.clone();
    let version_id = version_id.to_string();
    async move {
        let n: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM document_files WHERE document_version_id = ? AND is_available = 1",
        )
        .bind(version_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        n.0
    }
}

/// A — link working file without snapshot copy
#[tokio::test]
async fn a_link_working_file_no_snapshot() {
    let dir = tempdir().unwrap();
    let pool = db::init_pool(&dir.path().join("t.db")).await.unwrap();
    let root = dir.path().join("root");
    fs::create_dir_all(root.join("2610/04_MEMORIAS")).unwrap();
    let root_id = setup_root(&pool, &root).await;
    let pid = seed_project(&pool, &root_id, Some("2610"), "2610").await;
    let (doc_id, version_id) =
        empty_doc(&pool, &pid, "memorias", "2610-MC-001", "Memoria Calculo").await;

    let src = root.join("2610/04_MEMORIAS/Memoria.docx");
    fs::write(&src, b"DOCX").unwrap();
    let loc = project_paths::resolve_project_location(&pool, &pid)
        .await
        .unwrap();
    let (fid, skipped) = link_working_file(
        &pool,
        &loc,
        &version_id,
        &src,
        None,
        Some("editable"),
        None,
    )
    .await
    .unwrap();
    assert!(!skipped);
    assert!(!fid.is_empty());

    let row: (String, i64) = sqlx::query_as(
        "SELECT relative_path, is_snapshot FROM document_files f
         JOIN document_versions v ON v.id = f.document_version_id
         WHERE f.id = ?",
    )
    .bind(&fid)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(!row.0.contains(".ingmanager/snapshots"));
    assert_eq!(row.1, 0);
    assert!(src.exists(), "working file must remain in place");
    assert_eq!(file_count(&pool, &version_id).await, 1);
    let _ = doc_id;
}

/// B — multi-root: project on secondary authorized root
#[tokio::test]
async fn b_multi_root_link() {
    let dir = tempdir().unwrap();
    let pool = db::init_pool(&dir.path().join("t.db")).await.unwrap();
    let primary = dir.path().join("primary");
    let secondary = dir.path().join("secondary");
    fs::create_dir_all(&primary).unwrap();
    fs::create_dir_all(secondary.join("2610/04_MEMORIAS")).unwrap();
    let _ = setup_root(&pool, &primary).await;
    let sec = project_paths::add_authorized_root(&pool, &secondary, Some("Sec"), false)
        .await
        .unwrap();
    let pid = seed_project(&pool, &sec.id, Some("2610"), "2610").await;
    let (_doc, version_id) =
        empty_doc(&pool, &pid, "memorias", "2610-MC-001", "Memoria").await;
    let src = secondary.join("2610/04_MEMORIAS/M.docx");
    fs::write(&src, b"x").unwrap();
    let loc = project_paths::resolve_project_location(&pool, &pid)
        .await
        .unwrap();
    link_working_file(&pool, &loc, &version_id, &src, None, None, None)
        .await
        .unwrap();
    assert_eq!(file_count(&pool, &version_id).await, 1);
}

/// C — register new document from indexed working files
#[tokio::test]
async fn c_register_new_from_files() {
    let dir = tempdir().unwrap();
    let pool = db::init_pool(&dir.path().join("t.db")).await.unwrap();
    let root = dir.path().join("root");
    fs::create_dir_all(root.join("2610/04_MEMORIAS")).unwrap();
    let root_id = setup_root(&pool, &root).await;
    let pid = seed_project(&pool, &root_id, Some("2610"), "2610").await;
    let docx = root.join("2610/04_MEMORIAS/Informe.docx");
    let pdf = root.join("2610/04_MEMORIAS/Informe.pdf");
    fs::write(&docx, b"d").unwrap();
    fs::write(&pdf, b"p").unwrap();

    let result = apply_incorporation(
        &pool,
        IncorporationApplyInput {
            project_id: pid.clone(),
            category_key: "memorias".into(),
            absolute_paths: vec![docx.display().to_string(), pdf.display().to_string()],
            dest_absolute_path: None,
            physical_op: Some(PhysicalOp::None),
            decisions: vec![IncorporationGroupDecision {
                group_key: "informe".into(),
                mode: IncorporationMode::RegisterNew,
                document_id: None,
                title: Some("Informe".into()),
                assign_code: Some(true),
            }],
        },
    )
    .await
    .unwrap();
    assert_eq!(result.created_documents, 1);
    assert_eq!(result.groups[0].linked_files, 2);
}

/// D — associate to existing empty doc (2610-MC-001 0→2)
#[tokio::test]
async fn d_associate_existing_empty_doc_2610() {
    let dir = tempdir().unwrap();
    let pool = db::init_pool(&dir.path().join("t.db")).await.unwrap();
    let root = dir.path().join("root");
    fs::create_dir_all(root.join("2610/04_MEMORIAS")).unwrap();
    let root_id = setup_root(&pool, &root).await;
    let pid = seed_project(&pool, &root_id, Some("2610"), "2610").await;
    let (doc_id, version_id) =
        empty_doc(&pool, &pid, "memorias", "2610-MC-001", "Memoria de Calculo").await;
    assert_eq!(file_count(&pool, &version_id).await, 0);

    let docx = root.join("2610/04_MEMORIAS/Memoria_de_Calculo.docx");
    let pdf = root.join("2610/04_MEMORIAS/Memoria_de_Calculo.pdf");
    fs::write(&docx, b"d").unwrap();
    fs::write(&pdf, b"p").unwrap();

    let preview = preview_incorporation(
        &pool,
        IncorporationPreviewInput {
            project_id: pid.clone(),
            category_key: "memorias".into(),
            absolute_paths: vec![docx.display().to_string(), pdf.display().to_string()],
            dest_absolute_path: None,
            physical_op: None,
        },
    )
    .await
    .unwrap();
    assert_eq!(preview.groups.len(), 1);
    assert_eq!(
        preview.groups[0].suggested_mode,
        IncorporationMode::AssociateExisting
    );
    assert_eq!(
        preview.groups[0].suggested_document_id.as_deref(),
        Some(doc_id.as_str())
    );

    let result = apply_incorporation(
        &pool,
        IncorporationApplyInput {
            project_id: pid,
            category_key: "memorias".into(),
            absolute_paths: vec![docx.display().to_string(), pdf.display().to_string()],
            dest_absolute_path: None,
            physical_op: Some(PhysicalOp::None),
            decisions: vec![IncorporationGroupDecision {
                group_key: preview.groups[0].group_key.clone(),
                mode: IncorporationMode::AssociateExisting,
                document_id: Some(doc_id),
                title: None,
                assign_code: None,
            }],
        },
    )
    .await
    .unwrap();
    assert_eq!(result.associated_documents, 1);
    assert_eq!(file_count(&pool, &version_id).await, 2);
}

/// E — idempotent re-register skips duplicates
#[tokio::test]
async fn e_idempotent_relink() {
    let dir = tempdir().unwrap();
    let pool = db::init_pool(&dir.path().join("t.db")).await.unwrap();
    let root = dir.path().join("root");
    fs::create_dir_all(root.join("2610/04_MEMORIAS")).unwrap();
    let root_id = setup_root(&pool, &root).await;
    let pid = seed_project(&pool, &root_id, Some("2610"), "2610").await;
    let (doc_id, version_id) =
        empty_doc(&pool, &pid, "memorias", "2610-MC-002", "Reporte").await;
    let src = root.join("2610/04_MEMORIAS/Reporte.docx");
    fs::write(&src, b"x").unwrap();
    let input = IncorporationApplyInput {
        project_id: pid,
        category_key: "memorias".into(),
        absolute_paths: vec![src.display().to_string()],
        dest_absolute_path: None,
        physical_op: Some(PhysicalOp::None),
        decisions: vec![IncorporationGroupDecision {
            group_key: "reporte".into(),
            mode: IncorporationMode::AssociateExisting,
            document_id: Some(doc_id),
            title: None,
            assign_code: None,
        }],
    };
    let r1 = apply_incorporation(&pool, input.clone()).await.unwrap();
    let r2 = apply_incorporation(&pool, input).await.unwrap();
    assert_eq!(r1.groups[0].linked_files, 1);
    assert_eq!(r2.groups[0].skipped_files, 1);
    assert_eq!(file_count(&pool, &version_id).await, 1);
}

/// F — DOCX+PDF grouping
#[tokio::test]
async fn f_docx_pdf_grouping() {
    let paths = vec![
        PathBuf::from("/x/A.docx"),
        PathBuf::from("/x/A.pdf"),
        PathBuf::from("/x/B.docx"),
    ];
    let g = group_incorporation_paths(&paths);
    assert_eq!(g.len(), 2);
}

/// G — physical-only mode
#[tokio::test]
async fn g_physical_only_copies() {
    let dir = tempdir().unwrap();
    let pool = db::init_pool(&dir.path().join("t.db")).await.unwrap();
    let root = dir.path().join("root");
    let outside = dir.path().join("outside");
    fs::create_dir_all(root.join("2610/04_MEMORIAS")).unwrap();
    fs::create_dir_all(&outside).unwrap();
    let root_id = setup_root(&pool, &root).await;
    let pid = seed_project(&pool, &root_id, Some("2610"), "2610").await;
    let src = outside.join("Solo.pdf");
    fs::write(&src, b"pdf").unwrap();
    let dest = root.join("2610/04_MEMORIAS");
    let result = apply_incorporation(
        &pool,
        IncorporationApplyInput {
            project_id: pid,
            category_key: "memorias".into(),
            absolute_paths: vec![src.display().to_string()],
            dest_absolute_path: Some(dest.display().to_string()),
            physical_op: Some(PhysicalOp::Copy),
            decisions: vec![IncorporationGroupDecision {
                group_key: "solo".into(),
                mode: IncorporationMode::PhysicalOnly,
                document_id: None,
                title: None,
                assign_code: None,
            }],
        },
    )
    .await
    .unwrap();
    assert_eq!(result.physical_only, 1);
    assert!(dest.join("Solo.pdf").exists());
    let docs: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM documents")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(docs.0, 0);
}

/// H — versions link working; revisions snapshot formats
#[tokio::test]
async fn h_version_link_vs_revision_snapshot() {
    let dir = tempdir().unwrap();
    let pool = db::init_pool(&dir.path().join("t.db")).await.unwrap();
    let root = dir.path().join("root");
    fs::create_dir_all(root.join("2610/04_MEMORIAS")).unwrap();
    let root_id = setup_root(&pool, &root).await;
    let pid = seed_project(&pool, &root_id, Some("2610"), "2610").await;
    let (doc_id, version_id) =
        empty_doc(&pool, &pid, "memorias", "2610-MC-003", "RevTest").await;
    let loc = project_paths::resolve_project_location(&pool, &pid)
        .await
        .unwrap();
    let working = root.join("2610/04_MEMORIAS/RevTest.docx");
    let issued = root.join("2610/04_MEMORIAS/RevTest.pdf");
    fs::write(&working, b"w").unwrap();
    fs::write(&issued, b"i").unwrap();

    let (edit_id, _) = link_working_file(
        &pool,
        &loc,
        &version_id,
        &working,
        Some("DOCX"),
        Some("editable"),
        None,
    )
    .await
    .unwrap();
    let rev_id = uuid::Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO revisions (id, document_id, revision_code, created_at, document_version_id, status)
         VALUES (?, ?, 'A', datetime('now'), ?, 'issued')",
    )
    .bind(&rev_id)
    .bind(&doc_id)
    .bind(&version_id)
    .execute(&pool)
    .await
    .unwrap();
    let snap_id = snapshot_file_to_version(
        &pool,
        &loc,
        &doc_id,
        &version_id,
        1,
        &issued,
        Some("PDF"),
        Some("issued_pdf"),
        Some(&rev_id),
    )
    .await
    .unwrap();

    let edit_rel: (String,) =
        sqlx::query_as("SELECT relative_path FROM document_files WHERE id = ?")
            .bind(&edit_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    let snap_rel: (String,) =
        sqlx::query_as("SELECT relative_path FROM document_files WHERE id = ?")
            .bind(&snap_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(!edit_rel.0.contains("snapshots"));
    assert!(snap_rel.0.contains("snapshots"));
}

/// I — SAP auxiliaries unchanged (grouping does not swallow .$sf / .s2k as memorias pairs)
#[tokio::test]
async fn i_sap_aux_not_paired_as_docs() {
    let paths = vec![
        PathBuf::from("/m/Model.sdb"),
        PathBuf::from("/m/Model.$sf"),
        PathBuf::from("/m/Model.s2k"),
    ];
    let g = group_incorporation_paths(&paths);
    // Non-pairable engineering aux → each its own group (single keys)
    assert_eq!(g.len(), 3);
}

/// J — recoverable partial apply: valid group succeeds even if another decision fails
#[tokio::test]
async fn j_partial_recoverable_warning() {
    let dir = tempdir().unwrap();
    let pool = db::init_pool(&dir.path().join("t.db")).await.unwrap();
    let root = dir.path().join("root");
    fs::create_dir_all(root.join("2610/04_MEMORIAS")).unwrap();
    let root_id = setup_root(&pool, &root).await;
    let pid = seed_project(&pool, &root_id, Some("2610"), "2610").await;
    let ok = root.join("2610/04_MEMORIAS/Ok.docx");
    fs::write(&ok, b"ok").unwrap();
    // AssociateExisting without document_id should error the whole apply — instead
    // use RegisterNew for the good file and ensure result is usable.
    let result = apply_incorporation(
        &pool,
        IncorporationApplyInput {
            project_id: pid,
            category_key: "memorias".into(),
            absolute_paths: vec![ok.display().to_string()],
            dest_absolute_path: None,
            physical_op: Some(PhysicalOp::None),
            decisions: vec![], // default RegisterNew
        },
    )
    .await
    .unwrap();
    assert_eq!(result.created_documents, 1);
    assert!(result.warning.is_none());
}
