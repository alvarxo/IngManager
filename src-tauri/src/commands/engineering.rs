//! Engineering file grouping + bulk logical-document registration (link, no copy).
//!
//! Model identity axes are independent:
//! - `documents.id` (UUID)
//! - `documents.business_code` (optional catalog code)
//! - `documents.title` (custom display; never used as technical identity)
//! - physical `document_files` links
//! - `engineering_groups.group_key` (technical family; ≠ title)
//!
//! Explicit associations win over filename/title inference. Re-running
//! bulk/sync is idempotent: already-linked principals are skipped; missing
//! disk versions are added as linked document_versions.

use std::collections::HashSet;
use std::path::Path;

use serde::{Deserialize, Serialize};
use tauri::State;
use uuid::Uuid;

use crate::engineering_files::{
    classify_file_name, group_files, version_hint_from_name, ClassifiedFile, EngineeringGroup,
    FileRole,
};
use crate::error::{AppError, AppResult};
use crate::path_security::ensure_under_root;
use crate::project_paths;
use crate::state::AppState;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineeringGroupDto {
    pub group_key: String,
    pub title: String,
    pub principal_file_name: Option<String>,
    pub principal_absolute_path: Option<String>,
    pub version_file_names: Vec<String>,
    pub auxiliary_count: usize,
    pub auxiliary_names: Vec<String>,
    pub standalone_names: Vec<String>,
    pub already_registered: bool,
    pub document_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BulkRegisterInput {
    pub project_id: String,
    pub category_key: String,
    /// When None/empty, register all unregistered principals.
    pub group_keys: Option<Vec<String>>,
    pub assign_codes: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BulkRegisterResult {
    pub created: usize,
    pub skipped: usize,
    /// Principals linked as new versions onto already-registered documents.
    pub versions_added: usize,
    pub document_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncModelVersionsInput {
    pub project_id: String,
    pub category_key: Option<String>,
    /// When set, sync only this logical document (e.g. 1126-MOD-001).
    pub document_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncModelVersionsResult {
    pub documents_touched: usize,
    pub versions_added: usize,
    pub already_linked: usize,
    pub document_ids: Vec<String>,
    /// Same technical family split across multiple logical docs — offer unify.
    pub unify_suggestions: Vec<crate::unify_models::UnifySuggestion>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssociateModelVersionInput {
    pub document_id: String,
    pub absolute_path: String,
    pub notes: Option<String>,
}

async fn project_codes(pool: &sqlx::SqlitePool, project_id: &str) -> AppResult<Vec<String>> {
    let row: Option<(Option<String>, String)> =
        sqlx::query_as("SELECT code, root_relative_path FROM projects WHERE id = ?")
            .bind(project_id)
            .fetch_optional(pool)
            .await?;
    let mut codes = Vec::new();
    if let Some((code, rel)) = row {
        if let Some(c) = code.filter(|s| !s.trim().is_empty()) {
            codes.push(c.trim().to_string());
        }
        if let Some(base) = Path::new(&rel)
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
        {
            let digitish: String = base
                .chars()
                .take_while(|ch| ch.is_ascii_alphanumeric())
                .collect();
            if digitish.len() >= 2 && !codes.iter().any(|c| c == &digitish) {
                codes.push(digitish);
            }
        }
    }
    Ok(codes)
}

async fn load_indexed_names(
    pool: &sqlx::SqlitePool,
    project_id: &str,
    category_key: &str,
) -> AppResult<Vec<(String, String)>> {
    // (file_name, absolute_path)
    let rows: Vec<(String, String)> = sqlx::query_as(
        "SELECT file_name, absolute_path FROM indexed_files
         WHERE project_id = ? AND category_key = ?
           AND is_directory = 0 AND is_available = 1
         ORDER BY file_name COLLATE NOCASE",
    )
    .bind(project_id)
    .bind(category_key)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

#[tauri::command]
pub async fn list_engineering_groups(
    state: State<'_, AppState>,
    project_id: String,
    category_key: String,
    include_auxiliaries: Option<bool>,
) -> AppResult<Vec<EngineeringGroupDto>> {
    let _include_aux = include_auxiliaries.unwrap_or(true);
    let codes = project_codes(&state.pool, &project_id).await?;
    let rows = load_indexed_names(&state.pool, &project_id, &category_key).await?;
    let names: Vec<String> = rows.iter().map(|r| r.0.clone()).collect();
    let path_by_name: std::collections::HashMap<String, String> =
        rows.into_iter().collect();

    let registered: Vec<(String, Option<String>)> = sqlx::query_as(
        "SELECT group_key, document_id FROM engineering_groups
         WHERE project_id = ? AND category_key = ?",
    )
    .bind(&project_id)
    .bind(&category_key)
    .fetch_all(&state.pool)
    .await
    .unwrap_or_default();
    let reg_map: std::collections::HashMap<String, Option<String>> =
        registered.into_iter().collect();

    let groups = group_files(&names, &codes);
    Ok(groups
        .into_iter()
        .map(|g| to_dto(g, &path_by_name, &reg_map))
        .collect())
}

fn to_dto(
    g: EngineeringGroup,
    path_by_name: &std::collections::HashMap<String, String>,
    reg_map: &std::collections::HashMap<String, Option<String>>,
) -> EngineeringGroupDto {
    let principal_file_name = g.principal.as_ref().map(|p| p.file_name.clone());
    let principal_absolute_path = principal_file_name
        .as_ref()
        .and_then(|n| path_by_name.get(n).cloned());
    let doc = reg_map.get(&g.group_key).cloned().flatten();
    EngineeringGroupDto {
        group_key: g.group_key.clone(),
        title: g.title,
        principal_file_name,
        principal_absolute_path,
        version_file_names: g
            .version_principals
            .iter()
            .map(|p| p.file_name.clone())
            .collect(),
        auxiliary_count: g.auxiliaries.len(),
        auxiliary_names: g.auxiliaries.iter().map(|a| a.file_name.clone()).collect(),
        standalone_names: g.standalone.iter().map(|s| s.file_name.clone()).collect(),
        already_registered: reg_map.contains_key(&g.group_key),
        document_id: doc,
    }
}

#[tauri::command]
pub async fn list_category_files_filtered(
    state: State<'_, AppState>,
    project_id: String,
    category_key: String,
    show_auxiliaries: Option<bool>,
) -> AppResult<Vec<crate::models::IndexedFile>> {
    let show_aux = show_auxiliaries.unwrap_or(false);
    let files = sqlx::query_as::<_, crate::models::IndexedFile>(
        "SELECT * FROM indexed_files
         WHERE project_id = ? AND category_key = ? AND is_directory = 0
         ORDER BY file_name COLLATE NOCASE",
    )
    .bind(&project_id)
    .bind(&category_key)
    .fetch_all(&state.pool)
    .await?;

    if show_aux || category_key != "modelos" {
        return Ok(files);
    }

    let codes = project_codes(&state.pool, &project_id).await?;
    let names: Vec<String> = files.iter().map(|f| f.file_name.clone()).collect();
    let visible = crate::engineering_files::filter_visible_names(&names, &codes, false);
    let visible_set: std::collections::HashSet<_> = visible.into_iter().collect();
    Ok(files
        .into_iter()
        .filter(|f| visible_set.contains(&f.file_name))
        .collect())
}

/// Bulk-register logical documents for principal models without copying files.
/// Already-registered families sync missing disk versions onto the existing doc.
#[tauri::command]
pub async fn bulk_register_engineering_docs(
    state: State<'_, AppState>,
    input: BulkRegisterInput,
) -> AppResult<BulkRegisterResult> {
    bulk_register_engineering_docs_inner(&state.pool, input).await
}

/// Sync missing physical principal versions onto registered model documents.
#[tauri::command]
pub async fn sync_model_versions(
    state: State<'_, AppState>,
    input: SyncModelVersionsInput,
) -> AppResult<SyncModelVersionsResult> {
    sync_model_versions_inner(&state.pool, input).await
}

/// Manually associate an existing working file as a new version of a model doc.
#[tauri::command]
pub async fn associate_model_version(
    state: State<'_, AppState>,
    input: AssociateModelVersionInput,
) -> AppResult<crate::commands::documents::DocumentVersionRow> {
    associate_model_version_inner(&state.pool, input).await
}

/// List model families that are split across multiple logical documents.
#[tauri::command]
pub async fn list_unify_model_suggestions(
    state: State<'_, AppState>,
    project_id: String,
    category_key: Option<String>,
    document_id: Option<String>,
) -> AppResult<Vec<crate::unify_models::UnifySuggestion>> {
    let category_key = category_key.unwrap_or_else(|| "modelos".into());
    crate::unify_models::detect_unify_suggestions(
        &state.pool,
        &project_id,
        &category_key,
        document_id.as_deref(),
    )
    .await
}

/// Preview unifying duplicate logical model documents into a principal.
#[tauri::command]
pub async fn preview_unify_models(
    state: State<'_, AppState>,
    input: crate::unify_models::UnifyModelsInput,
) -> AppResult<crate::unify_models::UnifyModelsPreview> {
    crate::unify_models::preview_unify_models_inner(&state.pool, input).await
}

/// Apply unify: DB backup + transactional transfer; never deletes physical files.
#[tauri::command]
pub async fn apply_unify_models(
    state: State<'_, AppState>,
    input: crate::unify_models::UnifyModelsInput,
) -> AppResult<crate::unify_models::UnifyModelsResult> {
    crate::unify_models::apply_unify_models_inner(&state.pool, &state.db_path, input).await
}

/// Preview «Reparar historial»: drop false aux/.sbk versions; keep `.sdb` only.
#[tauri::command]
pub async fn preview_repair_model_history(
    state: State<'_, AppState>,
    input: crate::repair_model_history::RepairModelHistoryInput,
) -> AppResult<crate::repair_model_history::RepairModelHistoryPreview> {
    crate::repair_model_history::preview_repair_model_history_inner(&state.pool, input).await
}

/// Apply historial repair with SQLite backup; never deletes physical files.
#[tauri::command]
pub async fn apply_repair_model_history(
    state: State<'_, AppState>,
    input: crate::repair_model_history::RepairModelHistoryInput,
) -> AppResult<crate::repair_model_history::RepairModelHistoryResult> {
    crate::repair_model_history::apply_repair_model_history_inner(
        &state.pool,
        &state.db_path,
        input,
    )
    .await
}

type ResolvedPrincipal = (ClassifiedFile, String, String);

fn resolve_principals_on_disk(
    principals: &[ClassifiedFile],
    path_by_name: &std::collections::HashMap<String, String>,
    root: &Path,
) -> Vec<ResolvedPrincipal> {
    let mut resolved = Vec::new();
    for p in principals {
        let Some(abs) = path_by_name.get(&p.file_name) else {
            continue;
        };
        let Ok(under) = ensure_under_root(root, Path::new(abs)) else {
            continue;
        };
        if !under.is_file() {
            continue;
        }
        let rel = under
            .strip_prefix(root)
            .map(|p| p.to_string_lossy().replace('\\', "/"))
            .unwrap_or_default();
        resolved.push((p.clone(), abs.clone(), rel));
    }
    resolved
}

async fn linked_relative_paths_for_document(
    pool: &sqlx::SqlitePool,
    document_id: &str,
) -> AppResult<HashSet<String>> {
    let rows: Vec<(String,)> = sqlx::query_as(
        "SELECT f.relative_path
         FROM document_files f
         JOIN document_versions v ON v.id = f.document_version_id
         WHERE v.document_id = ? AND f.is_available = 1",
    )
    .bind(document_id)
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(|r| r.0).collect())
}

async fn document_owning_relative_path(
    pool: &sqlx::SqlitePool,
    project_id: &str,
    rel: &str,
) -> AppResult<Option<String>> {
    let row: Option<(String,)> = sqlx::query_as(
        "SELECT d.id FROM documents d
         JOIN document_versions v ON v.document_id = d.id
         JOIN document_files f ON f.document_version_id = v.id
         WHERE d.project_id = ? AND f.relative_path = ? AND f.is_available = 1
         LIMIT 1",
    )
    .bind(project_id)
    .bind(rel)
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|r| r.0))
}

/// Append missing principals as new linked versions on an existing document.
/// Idempotent: already-linked relative paths are skipped. Does not copy files.
async fn append_missing_versions(
    pool: &sqlx::SqlitePool,
    project_id: &str,
    document_id: &str,
    group_key: &str,
    codes: &[String],
    resolved: &[ResolvedPrincipal],
    auxiliaries: &[ClassifiedFile],
    path_by_name: &std::collections::HashMap<String, String>,
    root: &Path,
) -> AppResult<(usize, usize)> {
    let linked = linked_relative_paths_for_document(pool, document_id).await?;
    let mut already = 0usize;
    let mut to_add: Vec<&ResolvedPrincipal> = Vec::new();
    for item in resolved {
        if linked.contains(&item.2) {
            already += 1;
            continue;
        }
        // Explicit association elsewhere wins: do not steal another document's file.
        if let Some(owner) = document_owning_relative_path(pool, project_id, &item.2).await? {
            if owner != document_id {
                already += 1;
                continue;
            }
        }
        to_add.push(item);
    }
    if to_add.is_empty() {
        return Ok((0, already));
    }

    let max_n: (i64,) = sqlx::query_as(
        "SELECT COALESCE(MAX(version_number), 0) FROM document_versions WHERE document_id = ?",
    )
    .bind(document_id)
    .fetch_one(pool)
    .await?;
    let mut next_n = max_n.0;
    let mut current_version_id = String::new();
    let mut current_abs = String::new();
    let mut display_name = String::new();
    let attach_aux = linked.is_empty();

    let mut tx = pool.begin().await?;
    for (idx, (principal, abs, rel)) in to_add.iter().enumerate() {
        next_n += 1;
        let version_id = Uuid::new_v4().to_string();
        let hint = version_hint_from_name(&principal.file_name);
        let notes = hint
            .as_ref()
            .map(|h| format!("Sincronizado desde archivo (hint: {h})"))
            .unwrap_or_else(|| "Sincronizado desde archivo".into());

        sqlx::query(
            "INSERT INTO document_versions (
                id, document_id, version_number, label, notes, created_at,
                is_snapshot, source_relative_path
             ) VALUES (?, ?, ?, ?, ?, datetime('now'), 0, ?)",
        )
        .bind(&version_id)
        .bind(document_id)
        .bind(next_n)
        .bind(crate::doc_codes::version_label(next_n))
        .bind(&notes)
        .bind(rel)
        .execute(&mut *tx)
        .await?;

        let classified = classify_file_name(&principal.file_name, codes);
        let meta = std::fs::metadata(Path::new(abs)).ok();
        let file_id = Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT INTO document_files (
                id, document_version_id, relative_path, file_name, extension,
                size_bytes, modified_at, content_hash, is_available, format_label,
                created_at, file_role, engineering_group_key
             ) VALUES (?, ?, ?, ?, ?, ?, NULL, NULL, 1, 'principal', datetime('now'), ?, ?)",
        )
        .bind(&file_id)
        .bind(&version_id)
        .bind(rel)
        .bind(&principal.file_name)
        .bind(&classified.extension)
        .bind(meta.as_ref().map(|m| m.len() as i64))
        .bind(FileRole::Principal.as_str())
        .bind(group_key)
        .execute(&mut *tx)
        .await?;

        if attach_aux && idx == 0 {
            for aux in auxiliaries {
                if let Some(aux_abs) = path_by_name.get(&aux.file_name) {
                    if let Ok(aux_path) = ensure_under_root(root, Path::new(aux_abs)) {
                        let aux_rel = aux_path
                            .strip_prefix(root)
                            .map(|p| p.to_string_lossy().replace('\\', "/"))
                            .unwrap_or_default();
                        let aux_id = Uuid::new_v4().to_string();
                        let aux_meta = std::fs::metadata(&aux_path).ok();
                        let aux_c = classify_file_name(&aux.file_name, codes);
                        sqlx::query(
                            "INSERT INTO document_files (
                                id, document_version_id, relative_path, file_name, extension,
                                size_bytes, modified_at, content_hash, is_available, format_label,
                                created_at, file_role, engineering_group_key
                             ) VALUES (?, ?, ?, ?, ?, ?, NULL, NULL, 1, 'auxiliary', datetime('now'), ?, ?)",
                        )
                        .bind(&aux_id)
                        .bind(&version_id)
                        .bind(&aux_rel)
                        .bind(&aux.file_name)
                        .bind(&aux_c.extension)
                        .bind(aux_meta.as_ref().map(|m| m.len() as i64))
                        .bind(FileRole::Auxiliary.as_str())
                        .bind(group_key)
                        .execute(&mut *tx)
                        .await?;
                    }
                }
            }
        }

        current_version_id = version_id;
        current_abs = (*abs).clone();
        display_name = principal.file_name.clone();
    }

    sqlx::query(
        "UPDATE documents SET current_version_id = ?, updated_at = datetime('now') WHERE id = ?",
    )
    .bind(&current_version_id)
    .bind(document_id)
    .execute(&mut *tx)
    .await?;

    sqlx::query(
        "UPDATE engineering_groups SET
            principal_file_name = ?, updated_at = datetime('now')
         WHERE project_id = ? AND group_key = ? AND document_id = ?",
    )
    .bind(&display_name)
    .bind(project_id)
    .bind(group_key)
    .bind(document_id)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;

    for item in &to_add {
        let _ = crate::auto_version::register_baseline(
            pool,
            project_id,
            document_id,
            Some(&current_version_id),
            Path::new(&item.1),
        )
        .await;
    }
    let _ = current_abs;

    Ok((to_add.len(), already))
}

async fn ensure_engineering_group(
    pool: &sqlx::SqlitePool,
    project_id: &str,
    category_key: &str,
    group_key: &str,
    title: &str,
    principal_file_name: &str,
    document_id: &str,
) -> AppResult<()> {
    let existing: Option<(String, Option<String>)> = sqlx::query_as(
        "SELECT id, document_id FROM engineering_groups
         WHERE project_id = ? AND category_key = ? AND group_key = ?",
    )
    .bind(project_id)
    .bind(category_key)
    .bind(group_key)
    .fetch_optional(pool)
    .await?;

    if let Some((id, doc)) = existing {
        if doc.as_deref() != Some(document_id) {
            sqlx::query(
                "UPDATE engineering_groups SET
                    document_id = ?, principal_file_name = ?, updated_at = datetime('now')
                 WHERE id = ?",
            )
            .bind(document_id)
            .bind(principal_file_name)
            .bind(&id)
            .execute(pool)
            .await?;
        }
        return Ok(());
    }

    let eg_id = Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO engineering_groups (
            id, project_id, category_key, group_key, title,
            principal_file_name, document_id, created_at, updated_at
         ) VALUES (?, ?, ?, ?, ?, ?, ?, datetime('now'), datetime('now'))",
    )
    .bind(&eg_id)
    .bind(project_id)
    .bind(category_key)
    .bind(group_key)
    .bind(title)
    .bind(principal_file_name)
    .bind(document_id)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn bulk_register_engineering_docs_inner(
    pool: &sqlx::SqlitePool,
    input: BulkRegisterInput,
) -> AppResult<BulkRegisterResult> {
    let category_key = input.category_key;
    let project_id = input.project_id;
    let assign_codes = input.assign_codes.unwrap_or(true);
    let filter_keys = input.group_keys.unwrap_or_default();

    let loc = project_paths::resolve_project_location(pool, &project_id).await?;
    let root = loc.root_path.clone();
    let codes = project_codes(pool, &project_id).await?;
    let rows = load_indexed_names(pool, &project_id, &category_key).await?;
    let names: Vec<String> = rows.iter().map(|r| r.0.clone()).collect();
    let path_by_name: std::collections::HashMap<String, String> =
        rows.into_iter().collect();
    let groups = group_files(&names, &codes);

    let category: (String,) = sqlx::query_as(
        "SELECT id FROM categories WHERE project_id = ? AND key = ?",
    )
    .bind(&project_id)
    .bind(&category_key)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| AppError::msg("Categoría no encontrada"))?;

    let mut created = 0usize;
    let mut skipped = 0usize;
    let mut versions_added = 0usize;
    let mut document_ids = Vec::new();

    for g in groups {
        if !filter_keys.is_empty() && !filter_keys.contains(&g.group_key) {
            continue;
        }
        let principals = if g.version_principals.is_empty() {
            g.principal.iter().cloned().collect::<Vec<_>>()
        } else {
            g.version_principals.clone()
        };
        if principals.is_empty() {
            skipped += 1;
            continue;
        }

        let resolved = resolve_principals_on_disk(&principals, &path_by_name, &root);
        if resolved.is_empty() {
            skipped += 1;
            continue;
        }

        // Prefer explicit engineering_groups → document association.
        let eg_doc: Option<(String, Option<String>)> = sqlx::query_as(
            "SELECT id, document_id FROM engineering_groups
             WHERE project_id = ? AND category_key = ? AND group_key = ?",
        )
        .bind(&project_id)
        .bind(&category_key)
        .bind(&g.group_key)
        .fetch_optional(pool)
        .await?;

        let mut existing_doc_id: Option<String> = eg_doc.and_then(|(_, d)| d);

        // Fall back to any explicit file→document link for this family.
        if existing_doc_id.is_none() {
            for (_, _, rel) in &resolved {
                if let Some(owner) =
                    document_owning_relative_path(pool, &project_id, rel).await?
                {
                    existing_doc_id = Some(owner);
                    break;
                }
            }
        }

        if let Some(doc_id) = existing_doc_id {
            let display = resolved
                .last()
                .map(|(p, _, _)| p.file_name.clone())
                .unwrap_or_default();
            ensure_engineering_group(
                pool,
                &project_id,
                &category_key,
                &g.group_key,
                &g.title,
                &display,
                &doc_id,
            )
            .await?;
            let (added, already) = append_missing_versions(
                pool,
                &project_id,
                &doc_id,
                &g.group_key,
                &codes,
                &resolved,
                &g.auxiliaries,
                &path_by_name,
                &root,
            )
            .await?;
            versions_added += added;
            if added == 0 {
                skipped += 1;
            }
            let _ = already;
            if !document_ids.contains(&doc_id) {
                document_ids.push(doc_id);
            }
            continue;
        }

        let doc_id = Uuid::new_v4().to_string();
        // Default display title from stem — independent of technical group_key.
        let title = g.title.clone();
        let detected_type = {
            let principal_name = resolved
                .last()
                .map(|(p, _, _)| p.file_name.as_str())
                .unwrap_or("");
            crate::model_types::detect_model_type_from_file_name(principal_name)
        };
        let model_type_str = if category_key == "modelos" {
            Some(detected_type.as_str().to_string())
        } else {
            None
        };
        let business_code = if assign_codes {
            let prefix = loc.business_code_prefix.clone();
            let type_code = if category_key == "modelos" {
                detected_type.code_prefix()
            } else {
                crate::doc_codes::type_code_for_category(&category_key).unwrap_or("MOD")
            };
            let rows: Vec<(Option<String>,)> =
                sqlx::query_as("SELECT business_code FROM documents WHERE project_id = ?")
                    .bind(&project_id)
                    .fetch_all(pool)
                    .await?;
            let codes_existing: Vec<String> = rows.into_iter().filter_map(|r| r.0).collect();
            Some(crate::doc_codes::next_document_code(
                &prefix,
                type_code,
                &codes_existing,
            )?)
        } else {
            None
        };

        let mut tx = pool.begin().await?;
        sqlx::query(
            "INSERT INTO documents (
                id, project_id, category_id, business_code, title, doc_type, model_type,
                current_version_id, status, created_at, updated_at
             ) VALUES (?, ?, ?, ?, ?, ?, ?, NULL, 'active', datetime('now'), datetime('now'))",
        )
        .bind(&doc_id)
        .bind(&project_id)
        .bind(&category.0)
        .bind(&business_code)
        .bind(&title)
        .bind(crate::doc_codes::type_code_for_category(&category_key))
        .bind(&model_type_str)
        .execute(&mut *tx)
        .await?;

        let mut current_version_id = String::new();
        let mut current_abs = String::new();
        let mut display_principal_name = resolved
            .last()
            .map(|(p, _, _)| p.file_name.clone())
            .unwrap_or_default();

        for (idx, (principal, abs, rel)) in resolved.iter().enumerate() {
            let version_number = (idx as i64) + 1;
            let version_id = Uuid::new_v4().to_string();
            let hint = version_hint_from_name(&principal.file_name);
            let notes = hint
                .as_ref()
                .map(|h| format!("Registrado desde archivo (hint: {h})"));

            sqlx::query(
                "INSERT INTO document_versions (
                    id, document_id, version_number, label, notes, created_at,
                    is_snapshot, source_relative_path
                 ) VALUES (?, ?, ?, ?, ?, datetime('now'), 0, ?)",
            )
            .bind(&version_id)
            .bind(&doc_id)
            .bind(version_number)
            .bind(crate::doc_codes::version_label(version_number))
            .bind(&notes)
            .bind(rel)
            .execute(&mut *tx)
            .await?;

            let classified = classify_file_name(&principal.file_name, &codes);
            let under = Path::new(abs);
            let meta = std::fs::metadata(under).ok();
            let file_id = Uuid::new_v4().to_string();
            sqlx::query(
                "INSERT INTO document_files (
                    id, document_version_id, relative_path, file_name, extension,
                    size_bytes, modified_at, content_hash, is_available, format_label,
                    created_at, file_role, engineering_group_key
                 ) VALUES (?, ?, ?, ?, ?, ?, NULL, NULL, 1, 'principal', datetime('now'), ?, ?)",
            )
            .bind(&file_id)
            .bind(&version_id)
            .bind(rel)
            .bind(&principal.file_name)
            .bind(&classified.extension)
            .bind(meta.as_ref().map(|m| m.len() as i64))
            .bind(FileRole::Principal.as_str())
            .bind(&g.group_key)
            .execute(&mut *tx)
            .await?;

            if idx == 0 {
                for aux in &g.auxiliaries {
                    if let Some(aux_abs) = path_by_name.get(&aux.file_name) {
                        if let Ok(aux_path) = ensure_under_root(&root, Path::new(aux_abs)) {
                            let aux_rel = aux_path
                                .strip_prefix(&root)
                                .map(|p| p.to_string_lossy().replace('\\', "/"))
                                .unwrap_or_default();
                            let aux_id = Uuid::new_v4().to_string();
                            let aux_meta = std::fs::metadata(&aux_path).ok();
                            let aux_c = classify_file_name(&aux.file_name, &codes);
                            sqlx::query(
                                "INSERT INTO document_files (
                                    id, document_version_id, relative_path, file_name, extension,
                                    size_bytes, modified_at, content_hash, is_available, format_label,
                                    created_at, file_role, engineering_group_key
                                 ) VALUES (?, ?, ?, ?, ?, ?, NULL, NULL, 1, 'auxiliary', datetime('now'), ?, ?)",
                            )
                            .bind(&aux_id)
                            .bind(&version_id)
                            .bind(&aux_rel)
                            .bind(&aux.file_name)
                            .bind(&aux_c.extension)
                            .bind(aux_meta.as_ref().map(|m| m.len() as i64))
                            .bind(FileRole::Auxiliary.as_str())
                            .bind(&g.group_key)
                            .execute(&mut *tx)
                            .await?;
                        }
                    }
                }
            }

            current_version_id = version_id;
            current_abs = abs.clone();
            display_principal_name = principal.file_name.clone();
        }

        sqlx::query(
            "UPDATE documents SET current_version_id = ?, updated_at = datetime('now') WHERE id = ?",
        )
        .bind(&current_version_id)
        .bind(&doc_id)
        .execute(&mut *tx)
        .await?;

        let eg_id = Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT INTO engineering_groups (
                id, project_id, category_key, group_key, title,
                principal_file_name, document_id, created_at, updated_at
             ) VALUES (?, ?, ?, ?, ?, ?, ?, datetime('now'), datetime('now'))",
        )
        .bind(&eg_id)
        .bind(&project_id)
        .bind(&category_key)
        .bind(&g.group_key)
        .bind(&title)
        .bind(&display_principal_name)
        .bind(&doc_id)
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;

        for (_, abs, _) in &resolved {
            let _ = crate::auto_version::register_baseline(
                pool,
                &project_id,
                &doc_id,
                Some(&current_version_id),
                Path::new(abs),
            )
            .await;
        }
        let _ = current_abs;

        created += 1;
        document_ids.push(doc_id);
    }

    Ok(BulkRegisterResult {
        created,
        skipped,
        versions_added,
        document_ids,
    })
}

pub async fn sync_model_versions_inner(
    pool: &sqlx::SqlitePool,
    input: SyncModelVersionsInput,
) -> AppResult<SyncModelVersionsResult> {
    let category_key = input
        .category_key
        .unwrap_or_else(|| "modelos".to_string());
    let project_id = input.project_id;

    let loc = project_paths::resolve_project_location(pool, &project_id).await?;
    let root = loc.root_path.clone();
    let codes = project_codes(pool, &project_id).await?;
    let rows = load_indexed_names(pool, &project_id, &category_key).await?;
    let names: Vec<String> = rows.iter().map(|r| r.0.clone()).collect();
    let path_by_name: std::collections::HashMap<String, String> =
        rows.into_iter().collect();
    let groups = group_files(&names, &codes);

    let mut documents_touched = 0usize;
    let mut versions_added = 0usize;
    let mut already_linked = 0usize;
    let mut document_ids = Vec::new();

    // Map group_key → document via explicit associations only.
    let registered: Vec<(String, String)> = if let Some(doc_id) = &input.document_id {
        sqlx::query_as(
            "SELECT group_key, document_id FROM engineering_groups
             WHERE project_id = ? AND category_key = ? AND document_id = ?",
        )
        .bind(&project_id)
        .bind(&category_key)
        .bind(doc_id)
        .fetch_all(pool)
        .await?
    } else {
        sqlx::query_as(
            "SELECT group_key, document_id FROM engineering_groups
             WHERE project_id = ? AND category_key = ? AND document_id IS NOT NULL",
        )
        .bind(&project_id)
        .bind(&category_key)
        .fetch_all(pool)
        .await?
    };
    let reg_map: std::collections::HashMap<String, String> = registered.into_iter().collect();

    for g in &groups {
        let Some(doc_id) = reg_map.get(&g.group_key).cloned() else {
            continue;
        };
        if let Some(filter_doc) = &input.document_id {
            if filter_doc != &doc_id {
                continue;
            }
        }
        let principals = if g.version_principals.is_empty() {
            g.principal.iter().cloned().collect::<Vec<_>>()
        } else {
            g.version_principals.clone()
        };
        let resolved = resolve_principals_on_disk(&principals, &path_by_name, &root);
        if resolved.is_empty() {
            continue;
        }
        let (added, already) = append_missing_versions(
            pool,
            &project_id,
            &doc_id,
            &g.group_key,
            &codes,
            &resolved,
            &g.auxiliaries,
            &path_by_name,
            &root,
        )
        .await?;
        versions_added += added;
        already_linked += already;
        if added > 0 {
            documents_touched += 1;
        }
        if !document_ids.contains(&doc_id) {
            document_ids.push(doc_id);
        }
    }

    // Document filter with no engineering_groups row: recover family from linked files.
    if let Some(doc_id) = &input.document_id {
        if document_ids.is_empty() {
            let file_names: Vec<(String,)> = sqlx::query_as(
                "SELECT f.file_name FROM document_files f
                 JOIN document_versions v ON v.id = f.document_version_id
                 WHERE v.document_id = ? AND f.is_available = 1
                   AND COALESCE(f.file_role, 'principal') = 'principal'",
            )
            .bind(doc_id)
            .fetch_all(pool)
            .await?;
            if let Some((fname,)) = file_names.first() {
                let classified = classify_file_name(fname, &codes);
                let group_key = classified.group_key.clone();
                if let Some(g) = groups.iter().find(|g| g.group_key == group_key) {
                    let principals = if g.version_principals.is_empty() {
                        g.principal.iter().cloned().collect::<Vec<_>>()
                    } else {
                        g.version_principals.clone()
                    };
                    let resolved =
                        resolve_principals_on_disk(&principals, &path_by_name, &root);
                    let display = resolved
                        .last()
                        .map(|(p, _, _)| p.file_name.clone())
                        .unwrap_or_else(|| fname.clone());
                    ensure_engineering_group(
                        pool,
                        &project_id,
                        &category_key,
                        &group_key,
                        &g.title,
                        &display,
                        doc_id,
                    )
                    .await?;
                    let (added, already) = append_missing_versions(
                        pool,
                        &project_id,
                        doc_id,
                        &group_key,
                        &codes,
                        &resolved,
                        &g.auxiliaries,
                        &path_by_name,
                        &root,
                    )
                    .await?;
                    versions_added += added;
                    already_linked += already;
                    if added > 0 {
                        documents_touched += 1;
                    }
                    document_ids.push(doc_id.clone());
                }
            }
        }
    }

    let unify_suggestions = crate::unify_models::detect_unify_suggestions(
        pool,
        &project_id,
        &category_key,
        input.document_id.as_deref(),
    )
    .await
    .unwrap_or_default();

    Ok(SyncModelVersionsResult {
        documents_touched,
        versions_added,
        already_linked,
        document_ids,
        unify_suggestions,
    })
}

pub async fn associate_model_version_inner(
    pool: &sqlx::SqlitePool,
    input: AssociateModelVersionInput,
) -> AppResult<crate::commands::documents::DocumentVersionRow> {
    let doc: (String, Option<String>, String) = sqlx::query_as(
        "SELECT d.project_id, c.key, d.title
         FROM documents d
         LEFT JOIN categories c ON c.id = d.category_id
         WHERE d.id = ?",
    )
    .bind(&input.document_id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| AppError::msg("Documento no encontrado"))?;
    let (project_id, category_key, _title) = doc;
    let category_key = category_key.unwrap_or_else(|| "modelos".into());

    let loc = project_paths::resolve_project_location(pool, &project_id).await?;
    let abs = Path::new(&input.absolute_path);
    let under = ensure_under_root(&loc.root_path, abs)?;
    if !under.is_file() {
        return Err(AppError::msg("Archivo fuente no disponible"));
    }

    let codes = project_codes(pool, &project_id).await?;
    let file_name = under
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "file".into());
    let classified = classify_file_name(&file_name, &codes);
    let group_key = classified.group_key.clone();
    let rel = under
        .strip_prefix(&loc.root_path)
        .map(|p| p.to_string_lossy().replace('\\', "/"))
        .unwrap_or_default();

    // Refuse stealing another document's explicit link.
    if let Some(owner) = document_owning_relative_path(pool, &project_id, &rel).await? {
        if owner != input.document_id {
            return Err(AppError::msg(
                "El archivo ya está asociado explícitamente a otro documento",
            ));
        }
        // Already on this document — idempotent: return current version that holds it.
        let ver: Option<(String,)> = sqlx::query_as(
            "SELECT v.id FROM document_versions v
             JOIN document_files f ON f.document_version_id = v.id
             WHERE v.document_id = ? AND f.relative_path = ? AND f.is_available = 1
             ORDER BY v.version_number DESC LIMIT 1",
        )
        .bind(&input.document_id)
        .bind(&rel)
        .fetch_optional(pool)
        .await?;
        if let Some((vid,)) = ver {
            let row = sqlx::query_as::<_, crate::commands::documents::DocumentVersionRow>(
                "SELECT id, document_id, version_number, label, notes, created_at,
                        snapshot_relative_path, registered_fingerprint, source_relative_path,
                        is_snapshot, 1 AS is_current
                 FROM document_versions WHERE id = ?",
            )
            .bind(&vid)
            .fetch_one(pool)
            .await?;
            return Ok(row);
        }
    }

    ensure_engineering_group(
        pool,
        &project_id,
        &category_key,
        &group_key,
        &classified.stem,
        &file_name,
        &input.document_id,
    )
    .await?;

    let resolved = vec![(
        classified.clone(),
        under.display().to_string(),
        rel.clone(),
    )];
    let (added, _) = append_missing_versions(
        pool,
        &project_id,
        &input.document_id,
        &group_key,
        &codes,
        &resolved,
        &[],
        &std::collections::HashMap::new(),
        &loc.root_path,
    )
    .await?;
    if added == 0 {
        return Err(AppError::msg(
            "No se asoció versión (archivo ya vinculado o no disponible)",
        ));
    }

    // Optional notes override on the newest version
    if let Some(notes) = &input.notes {
        sqlx::query(
            "UPDATE document_versions SET notes = ?
             WHERE id = (SELECT current_version_id FROM documents WHERE id = ?)",
        )
        .bind(notes)
        .bind(&input.document_id)
        .execute(pool)
        .await?;
    }

    let current: (String,) =
        sqlx::query_as("SELECT current_version_id FROM documents WHERE id = ?")
            .bind(&input.document_id)
            .fetch_one(pool)
            .await?;
    let row = sqlx::query_as::<_, crate::commands::documents::DocumentVersionRow>(
        "SELECT id, document_id, version_number, label, notes, created_at,
                snapshot_relative_path, registered_fingerprint, source_relative_path,
                is_snapshot, 1 AS is_current
         FROM document_versions WHERE id = ?",
    )
    .bind(&current.0)
    .fetch_one(pool)
    .await?;
    Ok(row)
}
