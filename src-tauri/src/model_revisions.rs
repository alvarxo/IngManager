//! Model revision parity with memorias/planos.
//!
//! - Physical naming: `{identity}_R{code}_vNN.sdb` (initial version always `_v01`).
//! - Emitting a memoria/plano revision can opt-in spawn linked model(s) with the
//!   same `_R` token.
//! - Provenance: `revision_de` (new model → source model) and `modelo_de`
//!   (model → memoria/plano document).

use std::fs;
use std::path::{Path, PathBuf};

use uuid::Uuid;

use crate::commands::documents::{
    attach_file_inner, fetch_document_public, list_version_files_inner, DocumentRow,
};
use crate::doc_codes::version_label;
use crate::error::{AppError, AppResult};
use crate::path_security::join_under_root;
use crate::state::AppState;

/// Relation: model document is the engineering model for a memoria/plano.
pub const RELATION_MODELO_DE: &str = "modelo_de";

#[derive(Debug, Clone)]
pub struct SpawnedModelRevision {
    pub document_id: String,
    pub revision_id: String,
    pub sdb_absolute_path: String,
}

/// Resolve which model documents to spawn from when emitting a memoria/plano revision.
///
/// Priority:
/// 1. Explicit `source_model_document_ids` (must be modelos in the same project).
/// 2. Existing `modelo_de` / `relacionado` links to modelos.
/// 3. Auto if the project has exactly one modelo document.
/// 4. Otherwise error (UI must select) when `require_resolution` is true.
pub async fn resolve_source_model_ids(
    state: &AppState,
    memory_doc: &DocumentRow,
    explicit_ids: Option<&[String]>,
    require_resolution: bool,
) -> AppResult<Vec<String>> {
    if let Some(ids) = explicit_ids {
        let mut out = Vec::new();
        for id in ids {
            let id = id.trim();
            if id.is_empty() {
                continue;
            }
            let doc = fetch_document_public(state, id).await?;
            if doc.project_id != memory_doc.project_id {
                return Err(AppError::msg(format!(
                    "El modelo {id} no pertenece al mismo proyecto"
                )));
            }
            if doc.category_key.as_deref() != Some("modelos") {
                return Err(AppError::msg(format!(
                    "El documento {id} no es un modelo"
                )));
            }
            out.push(doc.id);
        }
        out.sort();
        out.dedup();
        if out.is_empty() && require_resolution {
            return Err(AppError::msg(
                "Indique al menos un modelo origen para la revisión",
            ));
        }
        return Ok(out);
    }

    // Linked models via modelo_de (either direction) or relacionado.
    let linked: Vec<(String,)> = sqlx::query_as(
        "SELECT CASE
            WHEN r.source_document_id = ? THEN r.target_document_id
            ELSE r.source_document_id
         END AS model_id
         FROM document_relationships r
         INNER JOIN documents d ON d.id = CASE
            WHEN r.source_document_id = ? THEN r.target_document_id
            ELSE r.source_document_id
         END
         INNER JOIN categories c ON c.id = d.category_id
         WHERE (r.source_document_id = ? OR r.target_document_id = ?)
           AND r.relation_type IN ('modelo_de', 'relacionado')
           AND c.key = 'modelos'
           AND d.project_id = ?",
    )
    .bind(&memory_doc.id)
    .bind(&memory_doc.id)
    .bind(&memory_doc.id)
    .bind(&memory_doc.id)
    .bind(&memory_doc.project_id)
    .fetch_all(&state.pool)
    .await?;

    let mut ids: Vec<String> = linked.into_iter().map(|r| r.0).collect();
    ids.sort();
    ids.dedup();
    if !ids.is_empty() {
        return Ok(ids);
    }

    let project_models: Vec<(String,)> = sqlx::query_as(
        "SELECT d.id FROM documents d
         INNER JOIN categories c ON c.id = d.category_id
         WHERE d.project_id = ? AND c.key = 'modelos'
         ORDER BY d.created_at, d.id",
    )
    .bind(&memory_doc.project_id)
    .fetch_all(&state.pool)
    .await?;

    if project_models.len() == 1 {
        return Ok(vec![project_models[0].0.clone()]);
    }

    if !require_resolution {
        return Ok(Vec::new());
    }

    if project_models.is_empty() {
        return Err(AppError::msg(
            "No hay modelos en el proyecto para generar la revisión de modelo",
        ));
    }
    Err(AppError::msg(format!(
        "Hay {} modelos en el proyecto; seleccione el origen (source_model_document_ids)",
        project_models.len()
    )))
}

async fn pick_principal_sdb(
    state: &AppState,
    loc: &crate::project_paths::ProjectLocation,
    model_doc: &DocumentRow,
) -> AppResult<(PathBuf, Option<String>)> {
    let cur = model_doc
        .current_version_id
        .as_deref()
        .ok_or_else(|| AppError::msg("El modelo origen no tiene versión actual"))?;
    let files = list_version_files_inner(state, cur).await?;
    let principals: Vec<_> = files
        .iter()
        .filter(|f| f.is_available != 0)
        .filter(|f| !f.relative_path.contains(".ingmanager/snapshots"))
        .filter(|f| {
            f.file_role.as_deref() == Some("principal")
                || f.extension
                    .as_deref()
                    .map(|e| e.eq_ignore_ascii_case("sdb"))
                    .unwrap_or(false)
                || f.file_name.to_ascii_lowercase().ends_with(".sdb")
        })
        .collect();
    if principals.is_empty() {
        return Err(AppError::msg(
            "No hay archivo .sdb principal disponible en el modelo origen",
        ));
    }
    let pick = crate::engineering_files::pick_principal_open_index(
        principals
            .iter()
            .map(|f| (f.file_name.as_str(), f.extension.as_deref())),
    )
    .unwrap_or(0);
    let chosen = principals[pick];
    let abs = join_under_root(&loc.root_path, &chosen.relative_path)?;
    if !abs.is_file() {
        return Err(AppError::msg(format!(
            "El .sdb origen no está disponible en disco: {}",
            abs.display()
        )));
    }
    Ok((abs, chosen.format_label.clone()))
}

/// Create a new modelo document for the given revision code, copying the
/// principal `.sdb` to `_R{code}_v01` (same pattern as memoria/plano emit).
pub async fn spawn_model_revision(
    state: &AppState,
    source_model_id: &str,
    revision_code: &str,
    sibling_doc_id: Option<&str>,
    notes: Option<&str>,
    into_folders: bool,
) -> AppResult<SpawnedModelRevision> {
    let source = fetch_document_public(state, source_model_id).await?;
    if source.category_key.as_deref() != Some("modelos") {
        return Err(AppError::msg("El origen debe ser un modelo"));
    }
    let loc =
        crate::project_paths::resolve_project_location(&state.pool, &source.project_id).await?;
    let (source_sdb, format_label) = pick_principal_sdb(state, &loc, &source).await?;

    let new_doc_id = Uuid::new_v4().to_string();
    let new_ver_id = Uuid::new_v4().to_string();
    let rev_id = Uuid::new_v4().to_string();
    let status = crate::doc_revisions::status_for_code(revision_code);
    let issued_at = if status == "issued" {
        Some(chrono::Utc::now().format("%Y-%m-%d").to_string())
    } else {
        None
    };
    let note = notes
        .map(|s| s.to_string())
        .unwrap_or_else(|| format!("Revisión {revision_code} de modelo"));

    let mut tx = state.pool.begin().await?;

    sqlx::query(
        "INSERT INTO documents (
            id, project_id, category_id, business_code, title, doc_type, model_type,
            current_version_id, status, subcategory, origin, received_at, notes,
            created_at, updated_at
         ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, datetime('now'), datetime('now'))",
    )
    .bind(&new_doc_id)
    .bind(&source.project_id)
    .bind(&source.category_id)
    .bind(&source.business_code)
    .bind(&source.title)
    .bind(&source.doc_type)
    .bind(&source.model_type)
    .bind(&new_ver_id)
    .bind(&source.status)
    .bind(&source.subcategory)
    .bind(&source.origin)
    .bind(&source.received_at)
    .bind(&note)
    .execute(&mut *tx)
    .await?;

    sqlx::query(
        "INSERT INTO document_versions (
            id, document_id, version_number, label, notes, created_at, is_snapshot
         ) VALUES (?, ?, 1, ?, ?, datetime('now'), 0)",
    )
    .bind(&new_ver_id)
    .bind(&new_doc_id)
    .bind(version_label(1))
    .bind(Option::<String>::None)
    .execute(&mut *tx)
    .await?;

    sqlx::query(
        "INSERT INTO revisions (
            id, document_id, revision_code, issued_at, notes, created_at,
            document_version_id, status
         ) VALUES (?, ?, ?, ?, ?, datetime('now'), ?, ?)",
    )
    .bind(&rev_id)
    .bind(&new_doc_id)
    .bind(revision_code)
    .bind(&issued_at)
    .bind(&note)
    .bind(&new_ver_id)
    .bind(status)
    .execute(&mut *tx)
    .await?;

    let rel_rev = Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT OR IGNORE INTO document_relationships (
            id, source_document_id, target_document_id, relation_type, notes, created_at
         ) VALUES (?, ?, ?, 'revision_de', ?, datetime('now'))",
    )
    .bind(&rel_rev)
    .bind(&new_doc_id)
    .bind(source_model_id)
    .bind(format!("Revisión {revision_code}"))
    .execute(&mut *tx)
    .await?;

    if let Some(sib) = sibling_doc_id {
        let rel_mod = Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT OR IGNORE INTO document_relationships (
                id, source_document_id, target_document_id, relation_type, notes, created_at
             ) VALUES (?, ?, ?, ?, ?, datetime('now'))",
        )
        .bind(&rel_mod)
        .bind(&new_doc_id)
        .bind(sib)
        .bind(RELATION_MODELO_DE)
        .bind(format!("Modelo de revisión {revision_code}"))
        .execute(&mut *tx)
        .await?;
    }

    tx.commit().await?;

    let dest = crate::revision_folders::allocate_revision_working_path(
        &source_sdb,
        revision_code,
        1,
        into_folders,
    )
    .map_err(AppError::msg)?;
    let _ = crate::path_security::ensure_under_root(&loc.root_path, &dest)?;

    if let Err(e) = fs::copy(&source_sdb, &dest) {
        let _ = sqlx::query("DELETE FROM documents WHERE id = ?")
            .bind(&new_doc_id)
            .execute(&state.pool)
            .await;
        return Err(AppError::msg(format!(
            "No se pudo copiar el .sdb a {}: {e}",
            dest.display()
        )));
    }

    let attach = attach_file_inner(
        state,
        &new_ver_id,
        &dest.display().to_string(),
        format_label,
        Some(rev_id.clone()),
        false,
        Some("principal".into()),
    )
    .await;

    if let Err(e) = attach {
        let _ = fs::remove_file(&dest);
        let _ = sqlx::query("DELETE FROM documents WHERE id = ?")
            .bind(&new_doc_id)
            .execute(&state.pool)
            .await;
        return Err(e);
    }

    let _ = crate::incorporation::snapshot_file_to_version(
        &state.pool,
        &loc,
        &new_doc_id,
        &new_ver_id,
        1,
        &source_sdb,
        Some("SDB"),
        Some("issued_revision"),
        Some(&rev_id),
    )
    .await;

    let _ = crate::auto_version::register_baseline(
        &state.pool,
        &source.project_id,
        &new_doc_id,
        Some(&new_ver_id),
        &dest,
    )
    .await;

    crate::audit::log_event(
        &state.pool,
        Some(&source.project_id),
        "document",
        Some(&new_doc_id),
        "spawn_model_revision",
        Some(&format!(
            "Revisión {revision_code} desde modelo {source_model_id}"
        )),
    )
    .await
    .ok();

    Ok(SpawnedModelRevision {
        document_id: new_doc_id,
        revision_id: rev_id,
        sdb_absolute_path: dest.display().to_string(),
    })
}

/// Whether a working file should be frozen/copied when emitting a model revision.
/// Only principal `.sdb` — never `.sbk` / SAP aux (avoids false versions).
pub fn is_model_revision_source_file(file_name: &str, file_role: Option<&str>) -> bool {
    let lower = file_name.to_ascii_lowercase();
    if !(lower.ends_with(".sdb") || file_role == Some("principal")) {
        return false;
    }
    if lower.ends_with(".sbk") {
        return false;
    }
    Path::new(&lower)
        .extension()
        .map(|e| e == "sdb")
        .unwrap_or(file_role == Some("principal"))
}
