//! Create a derived model document from an existing Modelos logical doc.
//!
//! - Copies the current principal `.sdb` to a **new stem** (never `_vNN` / same family).
//! - New UUID + new `XXXX-MOD-NNN` business code.
//! - Optional calc sheet copies to **new paths** (never shared with origin).
//! - Provenance: `document_relationships.relation_type = "deriva_de"`.
//! - Does **not** call `promote_spreadsheets_on_next_version` or next-physical-version.

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::doc_codes::{next_document_code, type_code_for_category, version_label};
use crate::engineering_files::{normalize_group_key, raw_stem, strip_version_suffix};
use crate::error::{AppError, AppResult};
use crate::fs_util::unique_destination;
use crate::incorporation::link_working_file;
use crate::model_spreadsheets::{
    associate_model_spreadsheet_inner, AssociateModelSpreadsheetInput, SHEET_KIND_CALC,
};
use crate::path_security::join_under_root;
use crate::project_paths;
use crate::state::AppState;

pub const RELATION_DERIVA_DE: &str = "deriva_de";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateDerivedModelInput {
    pub source_document_id: String,
    pub title: Option<String>,
    /// When true, copy live calc workbooks to new paths and associate them.
    pub copy_calc_sheets: Option<bool>,
    pub notes: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DerivedModelPreview {
    pub source_document_id: String,
    pub source_title: String,
    pub source_business_code: Option<String>,
    pub source_sdb_absolute_path: String,
    pub proposed_title: String,
    pub proposed_business_code: String,
    /// Inherited software subtype (`sap2000`…).
    pub proposed_model_type: String,
    pub destination_sdb_absolute_path: String,
    pub destination_sdb_file_name: String,
    pub proposed_group_key: String,
    pub source_group_key: String,
    pub calc_sources: Vec<String>,
    pub calc_destinations: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateDerivedModelResult {
    pub document_id: String,
    pub business_code: Option<String>,
    pub title: String,
    pub version_id: String,
    pub copied_sdb_absolute_path: String,
    pub copied_calc_absolute_paths: Vec<String>,
    pub relationship_id: String,
    pub group_key: String,
}

/// Allocate a free destination beside `source` whose stem yields a distinct
/// `normalize_group_key` from the origin (suffix `_derivado`, not `_vNN`).
pub fn allocate_derived_working_path(
    source: &Path,
    project_codes: &[String],
) -> Result<(PathBuf, String), String> {
    let parent = source
        .parent()
        .ok_or_else(|| "Archivo fuente sin directorio padre".to_string())?;
    let file_name = source
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or_else(|| "Nombre de archivo inválido".to_string())?;
    let stem = raw_stem(file_name);
    let (base, _) = strip_version_suffix(&stem);
    let base = if base.is_empty() {
        stem.clone()
    } else {
        base
    };
    let ext = source
        .extension()
        .map(|e| format!(".{}", e.to_string_lossy()))
        .unwrap_or_default();
    let candidate_name = format!("{base}_derivado{ext}");
    let dest = unique_destination(parent, &candidate_name);
    let dest_stem = raw_stem(
        dest.file_name()
            .and_then(|s| s.to_str())
            .unwrap_or(&candidate_name),
    );
    let group_key = normalize_group_key(&dest_stem, project_codes);
    let source_key = normalize_group_key(&stem, project_codes);
    if group_key == source_key {
        return Err(
            "No se pudo generar un stem distinto del origen para el modelo derivado".into(),
        );
    }
    Ok((dest, group_key))
}

async fn resolve_source_principal(
    state: &AppState,
    document_id: &str,
) -> AppResult<(
    crate::commands::documents::DocumentRow,
    project_paths::ProjectLocation,
    PathBuf,
    String,
    Vec<String>,
)> {
    let doc = crate::commands::documents::fetch_document_public(state, document_id).await?;
    if doc.category_key.as_deref() != Some("modelos") {
        return Err(AppError::msg(
            "Solo se pueden derivar documentos de categoría Modelos",
        ));
    }
    let loc = project_paths::resolve_project_location(&state.pool, &doc.project_id).await?;
    let cur = doc
        .current_version_id
        .as_deref()
        .ok_or_else(|| AppError::msg("El documento origen no tiene versión actual"))?;
    let files = crate::commands::documents::list_version_files_inner(state, cur).await?;
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
        return Err(AppError::msg(
            "El .sdb principal del origen no está disponible en disco",
        ));
    }

    let codes = project_codes_for(&loc);
    let stem = raw_stem(&chosen.file_name);
    let source_group_key = normalize_group_key(&stem, &codes);

    Ok((doc, loc, abs, source_group_key, codes))
}

fn project_codes_for(loc: &project_paths::ProjectLocation) -> Vec<String> {
    let mut codes = Vec::new();
    if !loc.business_code_prefix.is_empty() {
        codes.push(loc.business_code_prefix.clone());
    }
    if let Some(code) = loc.code.as_ref().filter(|c| !c.is_empty()) {
        if !codes.iter().any(|c| c == code) {
            codes.push(code.clone());
        }
    }
    codes
}

async fn list_live_calc_paths(
    state: &AppState,
    loc: &project_paths::ProjectLocation,
    version_id: &str,
) -> AppResult<Vec<PathBuf>> {
    let files = crate::commands::documents::list_version_files_inner(state, version_id).await?;
    let mut out = Vec::new();
    for f in files {
        if f.is_available == 0 {
            continue;
        }
        if f.sheet_kind.as_deref() != Some(SHEET_KIND_CALC) {
            continue;
        }
        if f.relative_path.contains(".ingmanager/snapshots") {
            continue;
        }
        let abs = join_under_root(&loc.root_path, &f.relative_path)?;
        if abs.is_file() {
            out.push(abs);
        }
    }
    Ok(out)
}

pub async fn preview_create_derived_model(
    state: &AppState,
    source_document_id: &str,
    title: Option<&str>,
    copy_calc_sheets: bool,
) -> AppResult<DerivedModelPreview> {
    let (doc, loc, source_abs, source_group_key, codes) =
        resolve_source_principal(state, source_document_id).await?;

    let (dest, group_key) = allocate_derived_working_path(&source_abs, &codes).map_err(AppError::msg)?;
    if !dest.starts_with(&loc.root_path) {
        return Err(AppError::msg(
            "Destino fuera del root autorizado del proyecto",
        ));
    }

    let inherited_type = doc
        .model_type
        .as_deref()
        .and_then(crate::model_types::ModelType::parse)
        .unwrap_or_else(|| {
            crate::model_types::detect_model_type_from_file_name(
                source_abs
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("model.sdb"),
            )
        });
    let rows: Vec<(Option<String>,)> =
        sqlx::query_as("SELECT business_code FROM documents WHERE project_id = ?")
            .bind(&doc.project_id)
            .fetch_all(&state.pool)
            .await?;
    let existing: Vec<String> = rows.into_iter().filter_map(|r| r.0).collect();
    let proposed_code = next_document_code(
        &loc.business_code_prefix,
        inherited_type.code_prefix(),
        &existing,
    )?;

    let proposed_title = title
        .map(|t| t.trim().to_string())
        .filter(|t| !t.is_empty())
        .unwrap_or_else(|| format!("{} (derivado)", doc.title.trim()));

    let mut calc_sources = Vec::new();
    let mut calc_destinations = Vec::new();
    if copy_calc_sheets {
        if let Some(cur) = &doc.current_version_id {
            let calcs = list_live_calc_paths(state, &loc, cur).await?;
            let parent = dest
                .parent()
                .unwrap_or_else(|| Path::new("."))
                .to_path_buf();
            for calc in calcs {
                let name = calc
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("calc.xlsx");
                let (base, _) = strip_version_suffix(&raw_stem(name));
                let ext = calc
                    .extension()
                    .map(|e| format!(".{}", e.to_string_lossy()))
                    .unwrap_or_else(|| ".xlsx".into());
                let dest_name = format!("{base}_derivado{ext}");
                let dest_calc = unique_destination(&parent, &dest_name);
                calc_sources.push(calc.display().to_string());
                calc_destinations.push(dest_calc.display().to_string());
            }
        }
    }

    let dest_name = dest
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("modelo_derivado.sdb")
        .to_string();

    Ok(DerivedModelPreview {
        source_document_id: doc.id,
        source_title: doc.title,
        source_business_code: doc.business_code,
        source_sdb_absolute_path: source_abs.display().to_string(),
        proposed_title,
        proposed_business_code: proposed_code,
        proposed_model_type: inherited_type.as_str().to_string(),
        destination_sdb_absolute_path: dest.display().to_string(),
        destination_sdb_file_name: dest_name,
        proposed_group_key: group_key,
        source_group_key,
        calc_sources,
        calc_destinations,
    })
}

pub async fn create_derived_model_inner(
    state: &AppState,
    input: CreateDerivedModelInput,
) -> AppResult<CreateDerivedModelResult> {
    let copy_calc = input.copy_calc_sheets.unwrap_or(false);
    let preview = preview_create_derived_model(
        state,
        &input.source_document_id,
        input.title.as_deref(),
        copy_calc,
    )
    .await?;

    let (doc, loc, source_abs, _source_gk, _codes) =
        resolve_source_principal(state, &input.source_document_id).await?;

    let dest = PathBuf::from(&preview.destination_sdb_absolute_path);
    if dest.exists() {
        return Err(AppError::msg(
            "El destino del .sdb derivado ya existe; reintente",
        ));
    }

    // Filesystem first: copy SDB, then optional calcs.
    fs::copy(&source_abs, &dest).map_err(|e| {
        AppError::msg(format!(
            "No se pudo copiar el .sdb a {}: {e}",
            dest.display()
        ))
    })?;

    let mut copied_calcs: Vec<PathBuf> = Vec::new();
    let cleanup = |sdb: &Path, calcs: &[PathBuf]| {
        let _ = fs::remove_file(sdb);
        for c in calcs {
            let _ = fs::remove_file(c);
        }
    };

    for (src_s, dest_s) in preview
        .calc_sources
        .iter()
        .zip(preview.calc_destinations.iter())
    {
        let src = PathBuf::from(src_s);
        let dest_calc = PathBuf::from(dest_s);
        if let Err(e) = fs::copy(&src, &dest_calc) {
            cleanup(&dest, &copied_calcs);
            return Err(AppError::msg(format!(
                "No se pudo copiar la planilla calc a {}: {e}",
                dest_calc.display()
            )));
        }
        copied_calcs.push(dest_calc);
    }

    let category: (String,) = sqlx::query_as(
        "SELECT id FROM categories WHERE project_id = ? AND key = 'modelos'",
    )
    .bind(&doc.project_id)
    .fetch_optional(&state.pool)
    .await?
    .ok_or_else(|| AppError::msg("Categoría Modelos no encontrada"))?;

    let doc_id = Uuid::new_v4().to_string();
    let version_id = Uuid::new_v4().to_string();
    let rel_id = Uuid::new_v4().to_string();
    let eg_id = Uuid::new_v4().to_string();
    let type_code = type_code_for_category("modelos").map(|s| s.to_string());
    let origin_hint = doc
        .business_code
        .clone()
        .unwrap_or_else(|| doc.title.clone());
    let notes = input.notes.clone().or_else(|| {
        Some(format!(
            "Derivado de {} ({})",
            origin_hint,
            input.source_document_id
        ))
    });

    let insert_result = async {
        let mut tx = state.pool.begin().await?;

        sqlx::query(
            "INSERT INTO documents (
                id, project_id, category_id, business_code, title, doc_type, model_type,
                current_version_id, status, origin, notes,
                created_at, updated_at
             ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, 'active', ?, ?, datetime('now'), datetime('now'))",
        )
        .bind(&doc_id)
        .bind(&doc.project_id)
        .bind(&category.0)
        .bind(&preview.proposed_business_code)
        .bind(&preview.proposed_title)
        .bind(&type_code)
        .bind(&preview.proposed_model_type)
        .bind(&version_id)
        .bind(&origin_hint)
        .bind(&notes)
        .execute(&mut *tx)
        .await?;

        sqlx::query(
            "INSERT INTO document_versions (
                id, document_id, version_number, label, notes, created_at, is_snapshot
             ) VALUES (?, ?, 1, ?, ?, datetime('now'), 0)",
        )
        .bind(&version_id)
        .bind(&doc_id)
        .bind(version_label(1))
        .bind(&notes)
        .execute(&mut *tx)
        .await?;

        sqlx::query(
            "INSERT INTO engineering_groups (
                id, project_id, category_key, group_key, title,
                principal_file_name, document_id, created_at, updated_at
             ) VALUES (?, ?, 'modelos', ?, ?, ?, ?, datetime('now'), datetime('now'))",
        )
        .bind(&eg_id)
        .bind(&doc.project_id)
        .bind(&preview.proposed_group_key)
        .bind(&preview.proposed_title)
        .bind(&preview.destination_sdb_file_name)
        .bind(&doc_id)
        .execute(&mut *tx)
        .await?;

        sqlx::query(
            "INSERT INTO document_relationships (
                id, source_document_id, target_document_id, relation_type, notes, created_at
             ) VALUES (?, ?, ?, ?, ?, datetime('now'))",
        )
        .bind(&rel_id)
        .bind(&doc_id)
        .bind(&input.source_document_id)
        .bind(RELATION_DERIVA_DE)
        .bind(&notes)
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;

        // Link principal after commit (uses own pool ops / path security).
        let (file_id, _) = link_working_file(
            &state.pool,
            &loc,
            &version_id,
            &dest,
            Some("SDB"),
            Some("principal"),
            None,
        )
        .await?;

        sqlx::query(
            "UPDATE document_files SET engineering_group_key = ? WHERE id = ?",
        )
        .bind(&preview.proposed_group_key)
        .bind(&file_id)
        .execute(&state.pool)
        .await?;

        for calc_dest in &copied_calcs {
            associate_model_spreadsheet_inner(
                state,
                AssociateModelSpreadsheetInput {
                    document_version_id: version_id.clone(),
                    absolute_path: calc_dest.display().to_string(),
                    sheet_kind: SHEET_KIND_CALC.to_string(),
                    version_policy: None,
                },
            )
            .await?;
        }

        let _ = crate::auto_version::register_baseline(
            &state.pool,
            &doc.project_id,
            &doc_id,
            Some(&version_id),
            &dest,
        )
        .await;

        crate::audit::log_event(
            &state.pool,
            Some(&doc.project_id),
            "document",
            Some(&doc_id),
            "create_derived",
            Some(&format!(
                "derived from {} → {} ({})",
                input.source_document_id, preview.proposed_business_code, dest.display()
            )),
        )
        .await
        .ok();

        Ok::<(), AppError>(())
    }
    .await;

    if let Err(e) = insert_result {
        cleanup(&dest, &copied_calcs);
        // Best-effort DB rollback of partial rows
        let _ = sqlx::query("DELETE FROM document_relationships WHERE id = ?")
            .bind(&rel_id)
            .execute(&state.pool)
            .await;
        let _ = sqlx::query("DELETE FROM engineering_groups WHERE id = ?")
            .bind(&eg_id)
            .execute(&state.pool)
            .await;
        let _ = sqlx::query("DELETE FROM document_files WHERE document_version_id = ?")
            .bind(&version_id)
            .execute(&state.pool)
            .await;
        let _ = sqlx::query("DELETE FROM document_versions WHERE id = ?")
            .bind(&version_id)
            .execute(&state.pool)
            .await;
        let _ = sqlx::query("DELETE FROM documents WHERE id = ?")
            .bind(&doc_id)
            .execute(&state.pool)
            .await;
        return Err(AppError::msg(format!(
            "Copia creada pero no se registró el modelo derivado ({e}); se eliminaron las copias"
        )));
    }

    // Explicitly do NOT call promote_spreadsheets_on_next_version.

    Ok(CreateDerivedModelResult {
        document_id: doc_id,
        business_code: Some(preview.proposed_business_code),
        title: preview.proposed_title,
        version_id,
        copied_sdb_absolute_path: dest.display().to_string(),
        copied_calc_absolute_paths: copied_calcs
            .iter()
            .map(|p| p.display().to_string())
            .collect(),
        relationship_id: rel_id,
        group_key: preview.proposed_group_key,
    })
}
