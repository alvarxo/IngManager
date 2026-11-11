//! Model revision parity with memorias/planos (`_R{token}_vNN.sdb`).

use std::fs;
use std::path::Path;

use tempfile::tempdir;

use crate::commands::documents::{
    create_next_physical_version_inner, create_revision_inner, get_document_inner,
    CreateNextPhysicalVersionInput, CreateRevisionInput,
};
use crate::db;
use crate::doc_revisions::{
    apply_revision_version_filename, category_supports_revisions, category_uses_revisions,
    parse_revision_filename,
};
use crate::incorporation::{
    apply_incorporation, IncorporationApplyInput, IncorporationGroupDecision, IncorporationMode,
    PhysicalOp,
};
use crate::model_revisions::{is_model_revision_source_file, RELATION_MODELO_DE};
use crate::project_paths::upsert_primary_root;
use crate::revision_folders::{preview_reorg, target_subfolder_for_file};
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
fn naming_and_category_helpers() {
    assert!(category_supports_revisions("modelos"));
    assert!(!category_uses_revisions("modelos"));
    assert_eq!(
        parse_revision_filename("1126-SAP-001_RB_v01.sdb")
            .map(|p| p.revision_code),
        Some("B".into())
    );
    assert_eq!(
        apply_revision_version_filename("Modelo_v03.sdb", "B", 1),
        "Modelo_RB_v01.sdb"
    );
    assert_eq!(
        target_subfolder_for_file("1126-SAP-001_RB_v01.sdb", "modelos").as_deref(),
        Some("RB")
    );
    assert_eq!(
        target_subfolder_for_file("Modelo_v02.sdb", "modelos").as_deref(),
        Some("v02")
    );
    assert!(is_model_revision_source_file("x.sdb", Some("principal")));
    assert!(!is_model_revision_source_file("x.sbk", Some("aux")));
}

#[tokio::test]
async fn emit_memory_revision_spawns_model_rb_v01() {
    let dir = tempdir().unwrap();
    let pool = db::init_pool(&dir.path().join("t.db")).await.unwrap();
    let root = dir.path().join("root");
    let mem = root.join("1126/04_MEMORIAS");
    let modelos = root.join("1126/02_MODELOS");
    fs::create_dir_all(&mem).unwrap();
    fs::create_dir_all(&modelos).unwrap();
    let root_id = setup_root(&pool, &root).await;
    let pid = seed_project(&pool, &root_id, Some("1126"), "1126").await;
    let state = AppState::new(pool.clone(), dir.path().join("t.db"));

    let memo = mem.join("1126-MC-010_base.docx");
    fs::write(&memo, b"memo").unwrap();
    let sdb = modelos.join("1126-SAP-001.sdb");
    fs::write(&sdb, b"sdb-bytes").unwrap();

    let memo_res = apply_incorporation(
        &pool,
        IncorporationApplyInput {
            project_id: pid.clone(),
            category_key: "memorias".into(),
            absolute_paths: vec![memo.display().to_string()],
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
    let memo_id = memo_res.groups[0].document_id.clone().unwrap();

    let model_res = apply_incorporation(
        &pool,
        IncorporationApplyInput {
            project_id: pid.clone(),
            category_key: "modelos".into(),
            absolute_paths: vec![sdb.display().to_string()],
            dest_absolute_path: None,
            physical_op: Some(PhysicalOp::None),
            decisions: vec![IncorporationGroupDecision {
                group_key: "1126-sap-001".into(),
                mode: IncorporationMode::RegisterNew,
                document_id: None,
                title: Some("Modelo SAP".into()),
                assign_code: Some(true),
            }],
        },
    )
    .await
    .unwrap();
    let model_id = model_res.groups[0].document_id.clone().unwrap();

    // Mark principal role for SDB
    sqlx::query(
        "UPDATE document_files SET file_role = 'principal'
         WHERE file_name LIKE '%.sdb'",
    )
    .execute(&pool)
    .await
    .unwrap();

    let rev = create_revision_inner(
        &state,
        CreateRevisionInput {
            document_id: memo_id.clone(),
            revision_code: "B".into(),
            issued_at: None,
            notes: Some("rev B".into()),
            document_version_id: None,
            file_paths: None,
            format_labels: None,
            freeze_linked: Some(true),
            also_create_model_revision: Some(true),
            source_model_document_ids: Some(vec![model_id.clone()]),
        },
    )
    .await
    .unwrap();

    assert_ne!(rev.document_id, memo_id);
    assert_eq!(rev.revision_code, "B");
    assert_eq!(rev.spawned_model_document_ids.len(), 1);

    let spawned_id = &rev.spawned_model_document_ids[0];
    let expected = modelos.join("1126-SAP-001_RB_v01.sdb");
    assert!(
        expected.is_file(),
        "expected {} on disk",
        expected.display()
    );

    let names: Vec<(String,)> = sqlx::query_as(
        "SELECT f.file_name FROM document_files f
         INNER JOIN document_versions v ON v.id = f.document_version_id
         WHERE v.document_id = ? AND f.is_available = 1
           AND f.relative_path NOT LIKE '%.ingmanager%'
         ORDER BY f.file_name",
    )
    .bind(spawned_id)
    .fetch_all(&pool)
    .await
    .unwrap();
    assert!(
        names.iter().any(|n| n.0 == "1126-SAP-001_RB_v01.sdb"),
        "got {names:?}"
    );

    let rel: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM document_relationships
         WHERE source_document_id = ? AND target_document_id = ?
           AND relation_type = ?",
    )
    .bind(spawned_id)
    .bind(&rev.document_id)
    .bind(RELATION_MODELO_DE)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(rel.0, 1);

    let rev_de: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM document_relationships
         WHERE source_document_id = ? AND target_document_id = ?
           AND relation_type = 'revision_de'",
    )
    .bind(spawned_id)
    .bind(&model_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(rev_de.0, 1);

    // Enrichment: current_revision_code from filename for model.
    let spawned_doc = get_document_inner(&state, spawned_id).await.unwrap();
    assert_eq!(spawned_doc.current_revision_code.as_deref(), Some("B"));
}

#[tokio::test]
async fn emit_model_revision_direct_and_next_version() {
    let dir = tempdir().unwrap();
    let pool = db::init_pool(&dir.path().join("t.db")).await.unwrap();
    let root = dir.path().join("root");
    let modelos = root.join("1126/02_MODELOS");
    fs::create_dir_all(&modelos).unwrap();
    let root_id = setup_root(&pool, &root).await;
    let pid = seed_project(&pool, &root_id, Some("1126"), "1126").await;
    let state = AppState::new(pool.clone(), dir.path().join("t.db"));

    let sdb = modelos.join("PUENTE.sdb");
    fs::write(&sdb, b"model").unwrap();

    let model_res = apply_incorporation(
        &pool,
        IncorporationApplyInput {
            project_id: pid.clone(),
            category_key: "modelos".into(),
            absolute_paths: vec![sdb.display().to_string()],
            dest_absolute_path: None,
            physical_op: Some(PhysicalOp::None),
            decisions: vec![IncorporationGroupDecision {
                group_key: "puente".into(),
                mode: IncorporationMode::RegisterNew,
                document_id: None,
                title: Some("Puente".into()),
                assign_code: Some(true),
            }],
        },
    )
    .await
    .unwrap();
    let model_id = model_res.groups[0].document_id.clone().unwrap();
    sqlx::query(
        "UPDATE document_files SET file_role = 'principal' WHERE file_name LIKE '%.sdb'",
    )
    .execute(&pool)
    .await
    .unwrap();

    let rev = create_revision_inner(
        &state,
        CreateRevisionInput {
            document_id: model_id.clone(),
            revision_code: "A".into(),
            issued_at: None,
            notes: None,
            document_version_id: None,
            file_paths: None,
            format_labels: None,
            freeze_linked: Some(true),
            also_create_model_revision: None,
            source_model_document_ids: None,
        },
    )
    .await
    .unwrap();
    assert_ne!(rev.document_id, model_id);
    assert!(modelos.join("PUENTE_RA_v01.sdb").is_file());
    assert!(rev.spawned_model_document_ids.is_empty());

    let next = create_next_physical_version_inner(
        &state,
        CreateNextPhysicalVersionInput {
            document_id: rev.document_id.clone(),
            notes: None,
            source_absolute_path: None,
        },
    )
    .await
    .unwrap();
    assert_eq!(next.version_number, 2);
    assert!(
        modelos.join("PUENTE_RA_v02.sdb").is_file(),
        "next version keeps _RA"
    );
}

#[tokio::test]
async fn reorg_preview_model_with_r_goes_to_rb_folder() {
    let dir = tempdir().unwrap();
    let pool = db::init_pool(&dir.path().join("t.db")).await.unwrap();
    let root = dir.path().join("root");
    let modelos = root.join("1126/02_MODELOS");
    fs::create_dir_all(&modelos).unwrap();
    let root_id = setup_root(&pool, &root).await;
    let pid = seed_project(&pool, &root_id, Some("1126"), "1126").await;

    let sdb = modelos.join("1126-SAP-001_RB_v01.sdb");
    fs::write(&sdb, b"x").unwrap();

    apply_incorporation(
        &pool,
        IncorporationApplyInput {
            project_id: pid.clone(),
            category_key: "modelos".into(),
            absolute_paths: vec![sdb.display().to_string()],
            dest_absolute_path: None,
            physical_op: Some(PhysicalOp::None),
            decisions: vec![IncorporationGroupDecision {
                group_key: "1126-sap-001".into(),
                mode: IncorporationMode::RegisterNew,
                document_id: None,
                title: Some("SAP".into()),
                assign_code: Some(true),
            }],
        },
    )
    .await
    .unwrap();

    let preview = preview_reorg(&pool, &pid).await.unwrap();
    let item = preview
        .items
        .iter()
        .find(|i| i.source_relative_path.contains("1126-SAP-001_RB_v01.sdb"))
        .expect("model file in preview");
    assert!(
        item.dest_relative_path.contains("/RB/"),
        "dest={} action={}",
        item.dest_relative_path,
        item.action
    );
}

#[tokio::test]
async fn auto_pick_single_project_model_when_spawning() {
    let dir = tempdir().unwrap();
    let pool = db::init_pool(&dir.path().join("t.db")).await.unwrap();
    let root = dir.path().join("root");
    let mem = root.join("1126/04_MEMORIAS");
    let modelos = root.join("1126/02_MODELOS");
    fs::create_dir_all(&mem).unwrap();
    fs::create_dir_all(&modelos).unwrap();
    let root_id = setup_root(&pool, &root).await;
    let pid = seed_project(&pool, &root_id, Some("1126"), "1126").await;
    let state = AppState::new(pool.clone(), dir.path().join("t.db"));

    let memo = mem.join("Memo_base.docx");
    fs::write(&memo, b"m").unwrap();
    let sdb = modelos.join("ONLY.sdb");
    fs::write(&sdb, b"s").unwrap();

    let memo_id = apply_incorporation(
        &pool,
        IncorporationApplyInput {
            project_id: pid.clone(),
            category_key: "memorias".into(),
            absolute_paths: vec![memo.display().to_string()],
            dest_absolute_path: None,
            physical_op: Some(PhysicalOp::None),
            decisions: vec![IncorporationGroupDecision {
                group_key: "memo-base".into(),
                mode: IncorporationMode::RegisterNew,
                document_id: None,
                title: Some("Memo".into()),
                assign_code: Some(true),
            }],
        },
    )
    .await
    .unwrap()
    .groups[0]
    .document_id
    .clone()
    .unwrap();

    apply_incorporation(
        &pool,
        IncorporationApplyInput {
            project_id: pid.clone(),
            category_key: "modelos".into(),
            absolute_paths: vec![sdb.display().to_string()],
            dest_absolute_path: None,
            physical_op: Some(PhysicalOp::None),
            decisions: vec![IncorporationGroupDecision {
                group_key: "only".into(),
                mode: IncorporationMode::RegisterNew,
                document_id: None,
                title: Some("Only".into()),
                assign_code: Some(true),
            }],
        },
    )
    .await
    .unwrap();
    sqlx::query(
        "UPDATE document_files SET file_role = 'principal' WHERE file_name LIKE '%.sdb'",
    )
    .execute(&pool)
    .await
    .unwrap();

    let rev = create_revision_inner(
        &state,
        CreateRevisionInput {
            document_id: memo_id,
            revision_code: "A".into(),
            issued_at: None,
            notes: None,
            document_version_id: None,
            file_paths: None,
            format_labels: None,
            freeze_linked: Some(true),
            also_create_model_revision: Some(true),
            source_model_document_ids: None, // auto single model
        },
    )
    .await
    .unwrap();
    assert_eq!(rev.spawned_model_document_ids.len(), 1);
    assert!(modelos.join("ONLY_RA_v01.sdb").is_file());
}
