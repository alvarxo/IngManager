//! Memory/drawing revisions hotfix — mandatory case 1126-MC-002 + supporting checks.
//!
//! Requirements (brief 1–11):
//! 1. Four files RB/R0 docx+pdf → one logical doc, two revisions (not internal v03)
//! 2. Recognize `_RA`/`_RB`/`_R0`/`_RevA`
//! 3. Revisión actual on logical docs
//! 4. Draft (letter) vs emitted (numeric)
//! 5. Group by id+rev
//! 6. Link existing without re-copy
//! 7. Emitir revisión freezes linked formats
//! 8. Repair mis-assigned versions→revisions
//! 9. Sequence A→B→0→1→2
//! 10. Dbl-click / principal opens draft DOCX
//! 11. No empty revision rows

use std::fs;
use std::path::Path;

use tempfile::tempdir;

use crate::commands::documents::{
    create_revision_inner, repair_document_revisions_inner, resolve_document_principal_file_id,
    CreateRevisionInput, RepairRevisionsInput,
};
use crate::db;
use crate::doc_revisions::{cmp_revision_codes, parse_revision_filename, strip_revision_suffix};
use crate::incorporation::{
    apply_incorporation, group_incorporation_paths, IncorporationApplyInput,
    IncorporationGroupDecision, IncorporationMode, PhysicalOp,
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

/// Mandatory: 1126-MC-002 four files → 1 doc, 2 revisions (B draft, 0 issued), not v03.
#[tokio::test]
async fn mandatory_1126_mc_002_one_doc_two_revisions() {
    let dir = tempdir().unwrap();
    let pool = db::init_pool(&dir.path().join("t.db")).await.unwrap();
    let root = dir.path().join("root");
    let mem = root.join("1126/04_MEMORIAS");
    fs::create_dir_all(&mem).unwrap();
    let root_id = setup_root(&pool, &root).await;
    let pid = seed_project(&pool, &root_id, Some("1126"), "1126").await;

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

    // Grouping: one identity
    let grouped = group_incorporation_paths(
        &paths
            .iter()
            .map(std::path::PathBuf::from)
            .collect::<Vec<_>>(),
    );
    assert_eq!(grouped.len(), 1);
    assert_eq!(grouped[0].0, "1126-mc-002");
    assert_eq!(grouped[0].1.len(), 4);

    let result = apply_incorporation(
        &pool,
        IncorporationApplyInput {
            project_id: pid.clone(),
            category_key: "memorias".into(),
            absolute_paths: paths,
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

    assert_eq!(result.created_documents, 1);
    let doc_id = result.groups[0].document_id.clone().unwrap();

    let docs: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM documents WHERE project_id = ?")
        .bind(&pid)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(docs.0, 1, "one logical document");

    let versions: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM document_versions WHERE document_id = ?")
            .bind(&doc_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(versions.0, 1, "not internal v03 — single working version");

    let revs: Vec<(String, Option<String>)> = sqlx::query_as(
        "SELECT revision_code, status FROM revisions WHERE document_id = ?",
    )
    .bind(&doc_id)
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(revs.len(), 2, "two revisions (RB + R0)");
    let mut codes: Vec<String> = revs.iter().map(|r| r.0.clone()).collect();
    codes.sort_by(|a, b| cmp_revision_codes(a, b));
    assert_eq!(codes, vec!["B", "0"]);

    let b = revs.iter().find(|r| r.0 == "B").unwrap();
    let zero = revs.iter().find(|r| r.0 == "0").unwrap();
    assert_eq!(b.1.as_deref(), Some("draft"));
    assert_eq!(zero.1.as_deref(), Some("issued"));

    let files: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM document_files f
         JOIN document_versions v ON v.id = f.document_version_id
         WHERE v.document_id = ? AND f.is_available = 1",
    )
    .bind(&doc_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(files.0, 4);

    // Link without re-copy: no snapshot paths for the four working files
    let snap: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM document_files f
         JOIN document_versions v ON v.id = f.document_version_id
         WHERE v.document_id = ? AND f.relative_path LIKE '%snapshots%'",
    )
    .bind(&doc_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(snap.0, 0, "incorporation links without re-copy/snapshot");

    // Original files still in place
    for n in &names {
        assert!(mem.join(n).is_file());
    }

    // Revisión actual = latest in sequence = 0
    assert_eq!(codes.last().map(|s| s.as_str()), Some("0"));

    // Principal prefers DOCX of current revision (0) when present
    let state = AppState::new(pool.clone(), dir.path().join("t.db"));
    let file_id = resolve_document_principal_file_id(&state, &doc_id)
        .await
        .unwrap();
    let fname: (String,) =
        sqlx::query_as("SELECT file_name FROM document_files WHERE id = ?")
            .bind(&file_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        fname.0, "1126-MC-002_R0.docx",
        "dbl-click opens draft DOCX of current revision"
    );
}

#[test]
fn recognizes_ra_rb_r0_reva_patterns() {
    for (name, code, draft) in [
        ("x_RA.docx", "A", true),
        ("x_RB.pdf", "B", true),
        ("x_R0.docx", "0", false),
        ("x_RevA.docx", "A", true),
        ("plano_Rev.B.dwg", "B", true),
    ] {
        let p = parse_revision_filename(name).expect(name);
        assert_eq!(p.revision_code, code);
        assert_eq!(p.is_draft, draft);
    }
    assert!(strip_revision_suffix("MODELO_v01").1.is_none());
}

#[test]
fn revision_sequence_a_b_0_1_2() {
    let mut c = vec!["2", "0", "B", "1", "A"];
    c.sort_by(|a, b| cmp_revision_codes(a, b));
    assert_eq!(c, vec!["A", "B", "0", "1", "2"]);
}

/// Emitir revisión: nuevo documento + freeze + archivos `_R0_v01`; refuses empty.
#[tokio::test]
async fn emitir_freezes_linked_formats_no_empty() {
    let dir = tempdir().unwrap();
    let pool = db::init_pool(&dir.path().join("t.db")).await.unwrap();
    let root = dir.path().join("root");
    let mem = root.join("1126/04_MEMORIAS");
    fs::create_dir_all(&mem).unwrap();
    let root_id = setup_root(&pool, &root).await;
    let pid = seed_project(&pool, &root_id, Some("1126"), "1126").await;

    let docx = mem.join("Memo_RA.docx");
    let pdf = mem.join("Memo_RA.pdf");
    fs::write(&docx, b"docx").unwrap();
    fs::write(&pdf, b"pdf").unwrap();

    let result = apply_incorporation(
        &pool,
        IncorporationApplyInput {
            project_id: pid,
            category_key: "memorias".into(),
            absolute_paths: vec![docx.display().to_string(), pdf.display().to_string()],
            dest_absolute_path: None,
            physical_op: Some(PhysicalOp::None),
            decisions: vec![IncorporationGroupDecision {
                group_key: "memo".into(),
                mode: IncorporationMode::RegisterNew,
                document_id: None,
                title: Some("Memo".into()),
                assign_code: Some(true),
            }],
        },
    )
    .await
    .unwrap();
    let doc_id = result.groups[0].document_id.clone().unwrap();
    let state = AppState::new(pool.clone(), dir.path().join("t.db"));

    // Empty emit without linked freeze and without paths → error (use freeze false + no paths)
    let err = create_revision_inner(
        &state,
        CreateRevisionInput {
            document_id: doc_id.clone(),
            revision_code: "1".into(),
            issued_at: None,
            notes: None,
            document_version_id: None,
            file_paths: Some(vec![]),
            format_labels: None,
            freeze_linked: Some(false),
        },
    )
    .await;
    assert!(err.is_err(), "no empty revision rows");

    let rev = create_revision_inner(
        &state,
        CreateRevisionInput {
            document_id: doc_id.clone(),
            revision_code: "0".into(),
            issued_at: Some("2026-11-03".into()),
            notes: Some("emisión".into()),
            document_version_id: None,
            file_paths: None,
            format_labels: None,
            freeze_linked: Some(true),
        },
    )
    .await
    .unwrap();
    assert_eq!(rev.revision_code, "0");
    assert_eq!(rev.status.as_deref(), Some("issued"));
    // Nueva revisión ⇒ otro documento (mismo código base).
    assert_ne!(rev.document_id, doc_id, "emit spawns a new document");

    let snaps: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM document_files
         WHERE revision_id = ? AND relative_path LIKE '%snapshots%'",
    )
    .bind(&rev.id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(snaps.0 >= 2, "freeze linked formats as snapshots");

    let working: Vec<(String,)> = sqlx::query_as(
        "SELECT file_name FROM document_files
         WHERE document_version_id = ? AND is_available = 1
           AND relative_path NOT LIKE '%snapshots%'",
    )
    .bind(rev.document_version_id.as_ref().unwrap())
    .fetch_all(&pool)
    .await
    .unwrap();
    let names: Vec<&str> = working.iter().map(|(n,)| n.as_str()).collect();
    assert!(
        names.iter().any(|n| n.contains("_R0_v01")),
        "working copy must carry _R0_v01, got {names:?}"
    );
    assert!(mem.join("Memo_R0_v01.docx").is_file());
    assert!(mem.join("Memo_R0_v01.pdf").is_file());

    // Same business_code on source and new revision document.
    let codes: Vec<(Option<String>,)> = sqlx::query_as(
        "SELECT business_code FROM documents WHERE id IN (?, ?)",
    )
    .bind(&doc_id)
    .bind(&rev.document_id)
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(codes.len(), 2);
    assert_eq!(codes[0].0, codes[1].0);

    let rel: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM document_relationships
         WHERE source_document_id = ? AND target_document_id = ?
           AND relation_type = 'revision_de'",
    )
    .bind(&rev.document_id)
    .bind(&doc_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(rel.0, 1);
}

/// Repair: mis-assigned v01/v02/v03 from RB/R0 files → revisions.
#[tokio::test]
async fn repair_versions_to_revisions() {
    let dir = tempdir().unwrap();
    let pool = db::init_pool(&dir.path().join("t.db")).await.unwrap();
    let root = dir.path().join("root");
    let mem = root.join("1126/04_MEMORIAS");
    fs::create_dir_all(&mem).unwrap();
    let root_id = setup_root(&pool, &root).await;
    let pid = seed_project(&pool, &root_id, Some("1126"), "1126").await;

    let cat: (String,) =
        sqlx::query_as("SELECT id FROM categories WHERE project_id = ? AND key = 'memorias'")
            .bind(&pid)
            .fetch_one(&pool)
            .await
            .unwrap();
    let doc_id = uuid::Uuid::new_v4().to_string();
    let v1 = uuid::Uuid::new_v4().to_string();
    let v2 = uuid::Uuid::new_v4().to_string();
    let v3 = uuid::Uuid::new_v4().to_string();

    sqlx::query(
        "INSERT INTO documents (
            id, project_id, category_id, business_code, title, doc_type,
            current_version_id, status, created_at, updated_at
         ) VALUES (?, ?, ?, '1126-MC-002', '1126-MC-002', 'MC', ?, 'desarrollo',
                   datetime('now'), datetime('now'))",
    )
    .bind(&doc_id)
    .bind(&pid)
    .bind(&cat.0)
    .bind(&v3)
    .execute(&pool)
    .await
    .unwrap();

    for (vid, num, label) in [(&v1, 1, "v01"), (&v2, 2, "v02"), (&v3, 3, "v03")] {
        sqlx::query(
            "INSERT INTO document_versions (
                id, document_id, version_number, label, created_at, is_snapshot
             ) VALUES (?, ?, ?, ?, datetime('now'), 0)",
        )
        .bind(vid)
        .bind(&doc_id)
        .bind(num)
        .bind(label)
        .execute(&pool)
        .await
        .unwrap();
    }

    let files = [
        (&v1, "1126-MC-002_RB.docx", "docx"),
        (&v2, "1126-MC-002_RB.pdf", "pdf"),
        (&v3, "1126-MC-002_R0.docx", "docx"),
    ];
    for (vid, name, ext) in files {
        let abs = mem.join(name);
        fs::write(&abs, b"x").unwrap();
        let rel = format!("1126/04_MEMORIAS/{name}");
        sqlx::query(
            "INSERT INTO document_files (
                id, document_version_id, relative_path, file_name, extension,
                is_available, created_at
             ) VALUES (?, ?, ?, ?, ?, 1, datetime('now'))",
        )
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(vid)
        .bind(&rel)
        .bind(name)
        .bind(ext)
        .execute(&pool)
        .await
        .unwrap();
    }

    let state = AppState::new(pool.clone(), dir.path().join("t.db"));
    let repaired = repair_document_revisions_inner(
        &state,
        RepairRevisionsInput {
            document_id: doc_id.clone(),
        },
    )
    .await
    .unwrap();

    assert!(repaired.revisions_created >= 2);
    let revs: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM revisions WHERE document_id = ?")
            .bind(&doc_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(revs.0, 2);

    let versions: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM document_versions WHERE document_id = ?")
            .bind(&doc_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(versions.0, 1, "empty mis-assigned versions removed");
    assert!(repaired.empty_versions_removed >= 1);
}
