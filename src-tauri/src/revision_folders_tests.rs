//! Physical revision folders: reorg preview/apply + emit into RA/ when flag on.

use std::fs;
use std::path::Path;

use tempfile::tempdir;

use crate::commands::documents::{
    create_next_physical_version_inner, create_revision_inner, CreateNextPhysicalVersionInput,
    CreateRevisionInput,
};
use crate::db;
use crate::incorporation::{
    apply_incorporation, IncorporationApplyInput, IncorporationGroupDecision, IncorporationMode,
    PhysicalOp,
};
use crate::models::UpdateProjectInput;
use crate::project_paths::upsert_primary_root;
use crate::revision_folders::{
    allocate_revision_working_path, apply_reorg, preview_reorg, revision_folder_name,
};
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

#[test]
fn allocate_into_ra_folder() {
    let dir = tempdir().unwrap();
    let mem = dir.path().join("04_MEMORIAS");
    fs::create_dir_all(&mem).unwrap();
    let src = mem.join("Memo.docx");
    fs::write(&src, b"x").unwrap();

    let dest = allocate_revision_working_path(&src, "A", 1, true).unwrap();
    assert_eq!(
        dest,
        mem.join("RA").join("Memo_RA_v01.docx"),
        "expected under RA/"
    );
    assert!(dest.parent().unwrap().exists());
}

#[tokio::test]
async fn reorg_moves_memorias_into_revision_folders() {
    let dir = tempdir().unwrap();
    let pool = db::init_pool(&dir.path().join("t.db")).await.unwrap();
    let root = dir.path().join("root");
    let mem = root.join("1126/04_MEMORIAS");
    fs::create_dir_all(&mem).unwrap();
    let root_id = setup_root(&pool, &root).await;
    let pid = seed_project(&pool, &root_id, Some("1126"), "1126").await;

    let names = [
        "1126-MC-002_RA_v01.docx",
        "1126-MC-002_RB_v01.docx",
        "1126-MC-002_R0.docx",
    ];
    let mut paths = Vec::new();
    for n in &names {
        let p = mem.join(n);
        fs::write(&p, b"body").unwrap();
        paths.push(p.display().to_string());
    }

    apply_incorporation(
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

    let preview = preview_reorg(&pool, &pid).await.unwrap();
    assert!(
        preview.to_move >= 3,
        "expected moves into RA/RB/R0, got {:?}",
        preview.items
    );

    let result = apply_reorg(&pool, &pid, &preview.session_id).await.unwrap();
    assert_eq!(result.failed, 0);
    assert!(result.moved >= 3);

    assert!(mem.join("RA").join("1126-MC-002_RA_v01.docx").is_file());
    assert!(mem.join("RB").join("1126-MC-002_RB_v01.docx").is_file());
    assert!(mem.join("R0").join("1126-MC-002_R0.docx").is_file());

    let rels: Vec<(String,)> = sqlx::query_as(
        "SELECT f.relative_path FROM document_files f
         INNER JOIN document_versions v ON v.id = f.document_version_id
         INNER JOIN documents d ON d.id = v.document_id
         WHERE d.project_id = ? AND f.is_available = 1
           AND f.relative_path NOT LIKE '%.ingmanager%'
         ORDER BY f.relative_path",
    )
    .bind(&pid)
    .fetch_all(&pool)
    .await
    .unwrap();
    assert!(rels.iter().any(|r| r.0.contains("/RA/")));
    assert!(rels.iter().any(|r| r.0.contains("/RB/")));
    assert!(rels.iter().any(|r| r.0.contains("/R0/")));

    // Idempotent second pass
    let preview2 = preview_reorg(&pool, &pid).await.unwrap();
    assert_eq!(preview2.to_move, 0);
    assert_eq!(revision_folder_name("A"), "RA");
}

#[tokio::test]
async fn create_revision_writes_into_ra_when_flag_on() {
    let dir = tempdir().unwrap();
    let pool = db::init_pool(&dir.path().join("t.db")).await.unwrap();
    let root = dir.path().join("root");
    let mem = root.join("1126/04_MEMORIAS");
    fs::create_dir_all(&mem).unwrap();
    let root_id = setup_root(&pool, &root).await;
    let pid = seed_project(&pool, &root_id, Some("1126"), "1126").await;

    // Avoid bare `…-010.docx`: strip_revision_suffix would treat `-010` as rev code.
    let src = mem.join("1126-MC-010_base.docx");
    fs::write(&src, b"memo").unwrap();

    let state = AppState::new(pool.clone(), dir.path().join("t.db"));
    let result = apply_incorporation(
        &pool,
        IncorporationApplyInput {
            project_id: pid.clone(),
            category_key: "memorias".into(),
            absolute_paths: vec![src.display().to_string()],
            dest_absolute_path: None,
            physical_op: Some(PhysicalOp::None),
            decisions: vec![IncorporationGroupDecision {
                group_key: "1126-mc-010-base".into(),
                mode: IncorporationMode::RegisterNew,
                document_id: None,
                title: Some("Memoria 10".into()),
                assign_code: Some(true),
            }],
        },
    )
    .await
    .unwrap();
    let doc_id = result.groups[0].document_id.clone().unwrap();

    sqlx::query("UPDATE projects SET physical_revision_folders = 1 WHERE id = ?")
        .bind(&pid)
        .execute(&pool)
        .await
        .unwrap();

    let rev = create_revision_inner(
        &state,
        CreateRevisionInput {
            document_id: doc_id,
            revision_code: "A".into(),
            document_version_id: None,
            notes: None,
            issued_at: None,
            freeze_linked: Some(true),
            file_paths: None,
            format_labels: None,
        },
    )
    .await
    .unwrap();

    let expected = mem.join("RA").join("1126-MC-010_base_RA_v01.docx");
    assert!(
        expected.is_file(),
        "expected working copy at {}",
        expected.display()
    );

    let rel: String = sqlx::query_as::<_, (String,)>(
        "SELECT f.relative_path FROM document_files f
         INNER JOIN document_versions v ON v.id = f.document_version_id
         WHERE v.document_id = ? AND f.is_available = 1
           AND f.relative_path NOT LIKE '%.ingmanager%'
         LIMIT 1",
    )
    .bind(&rev.document_id)
    .fetch_one(&pool)
    .await
    .unwrap()
    .0;
    assert!(rel.contains("/RA/"), "sqlite path {rel}");
}

#[tokio::test]
async fn next_version_stays_in_ra_when_flag_on() {
    let dir = tempdir().unwrap();
    let pool = db::init_pool(&dir.path().join("t.db")).await.unwrap();
    let root = dir.path().join("root");
    let mem = root.join("1126/04_MEMORIAS/RA");
    fs::create_dir_all(&mem).unwrap();
    let root_id = setup_root(&pool, &root).await;
    let pid = seed_project(&pool, &root_id, Some("1126"), "1126").await;

    let src = mem.join("1126-MC-011_RA_v01.docx");
    fs::write(&src, b"memo").unwrap();

    let state = AppState::new(pool.clone(), dir.path().join("t.db"));
    let result = apply_incorporation(
        &pool,
        IncorporationApplyInput {
            project_id: pid.clone(),
            category_key: "memorias".into(),
            absolute_paths: vec![src.display().to_string()],
            dest_absolute_path: None,
            physical_op: Some(PhysicalOp::None),
            decisions: vec![IncorporationGroupDecision {
                group_key: "1126-mc-011".into(),
                mode: IncorporationMode::RegisterNew,
                document_id: None,
                title: Some("Memoria 11".into()),
                assign_code: Some(true),
            }],
        },
    )
    .await
    .unwrap();
    let doc_id = result.groups[0].document_id.clone().unwrap();

    sqlx::query("UPDATE projects SET physical_revision_folders = 1 WHERE id = ?")
        .bind(&pid)
        .execute(&pool)
        .await
        .unwrap();

    create_next_physical_version_inner(
        &state,
        CreateNextPhysicalVersionInput {
            document_id: doc_id,
            notes: None,
            source_absolute_path: Some(src.display().to_string()),
        },
    )
    .await
    .unwrap();

    let v02 = mem.join("1126-MC-011_RA_v02.docx");
    assert!(v02.is_file(), "next version should stay in RA/");
}

#[tokio::test]
async fn update_project_flag_column_defaults_off() {
    let dir = tempdir().unwrap();
    let pool = db::init_pool(&dir.path().join("t.db")).await.unwrap();
    let root = dir.path().join("root");
    fs::create_dir_all(&root).unwrap();
    let root_id = setup_root(&pool, &root).await;
    let pid = seed_project(&pool, &root_id, Some("1126"), "1126").await;

    let flag: (i64,) =
        sqlx::query_as("SELECT physical_revision_folders FROM projects WHERE id = ?")
            .bind(&pid)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(flag.0, 0);

    sqlx::query(
        "UPDATE projects SET physical_revision_folders = 1, updated_at = datetime('now') WHERE id = ?",
    )
    .bind(&pid)
    .execute(&pool)
    .await
    .unwrap();

    let flag2: (i64,) =
        sqlx::query_as("SELECT physical_revision_folders FROM projects WHERE id = ?")
            .bind(&pid)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(flag2.0, 1);

    let _ = UpdateProjectInput {
        id: pid,
        name: None,
        description: None,
        client_name: None,
        status: None,
        start_date: None,
        end_date: None,
        is_favorite: None,
        physical_revision_folders: Some(true),
    };
}
