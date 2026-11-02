//! Engineering file grouping + bulk logical-document registration (link, no copy).

use std::path::Path;

use serde::{Deserialize, Serialize};
use tauri::State;
use uuid::Uuid;

use crate::engineering_files::{
    classify_file_name, group_files, EngineeringGroup, FileRole,
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
    pub document_ids: Vec<String>,
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
#[tauri::command]
pub async fn bulk_register_engineering_docs(
    state: State<'_, AppState>,
    input: BulkRegisterInput,
) -> AppResult<BulkRegisterResult> {
    bulk_register_engineering_docs_inner(&state.pool, input).await
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
    let mut document_ids = Vec::new();

    for g in groups {
        if !filter_keys.is_empty() && !filter_keys.contains(&g.group_key) {
            continue;
        }
        let Some(principal) = g.principal.as_ref() else {
            skipped += 1;
            continue;
        };
        let Some(abs) = path_by_name.get(&principal.file_name) else {
            skipped += 1;
            continue;
        };

        let existing: Option<(String,)> = sqlx::query_as(
            "SELECT id FROM engineering_groups
             WHERE project_id = ? AND category_key = ? AND group_key = ?",
        )
        .bind(&project_id)
        .bind(&category_key)
        .bind(&g.group_key)
        .fetch_optional(pool)
        .await?;
        if existing.is_some() {
            skipped += 1;
            continue;
        }

        let under = ensure_under_root(&root, Path::new(abs))?;
        let rel = under
            .strip_prefix(&root)
            .map(|p| p.to_string_lossy().replace('\\', "/"))
            .unwrap_or_default();
        let linked: Option<(String,)> = sqlx::query_as(
            "SELECT d.id FROM documents d
             JOIN document_files f ON f.document_version_id = d.current_version_id
             WHERE d.project_id = ? AND f.relative_path = ?",
        )
        .bind(&project_id)
        .bind(&rel)
        .fetch_optional(pool)
        .await?;
        if linked.is_some() {
            skipped += 1;
            continue;
        }

        let doc_id = Uuid::new_v4().to_string();
        let version_id = Uuid::new_v4().to_string();
        let title = g.title.clone();
        let business_code = if assign_codes {
            let prefix = loc.business_code_prefix.clone();
            let type_code =
                crate::doc_codes::type_code_for_category(&category_key).unwrap_or("MOD");
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
                id, project_id, category_id, business_code, title, doc_type,
                current_version_id, status, created_at, updated_at
             ) VALUES (?, ?, ?, ?, ?, ?, ?, 'active', datetime('now'), datetime('now'))",
        )
        .bind(&doc_id)
        .bind(&project_id)
        .bind(&category.0)
        .bind(&business_code)
        .bind(&title)
        .bind(crate::doc_codes::type_code_for_category(&category_key))
        .bind(&version_id)
        .execute(&mut *tx)
        .await?;

        sqlx::query(
            "INSERT INTO document_versions (
                id, document_id, version_number, label, notes, created_at,
                is_snapshot, source_relative_path
             ) VALUES (?, ?, 1, ?, NULL, datetime('now'), 0, ?)",
        )
        .bind(&version_id)
        .bind(&doc_id)
        .bind(crate::doc_codes::version_label(1))
        .bind(&rel)
        .execute(&mut *tx)
        .await?;

        let classified = classify_file_name(&principal.file_name, &codes);
        let meta = std::fs::metadata(&under).ok();
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
        .bind(&rel)
        .bind(&principal.file_name)
        .bind(&classified.extension)
        .bind(meta.as_ref().map(|m| m.len() as i64))
        .bind(FileRole::Principal.as_str())
        .bind(&g.group_key)
        .execute(&mut *tx)
        .await?;

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
        .bind(&principal.file_name)
        .bind(&doc_id)
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;

        let _ = crate::auto_version::register_baseline(
            pool,
            &project_id,
            &doc_id,
            Some(&version_id),
            Path::new(abs),
        )
        .await;

        created += 1;
        document_ids.push(doc_id);
    }

    Ok(BulkRegisterResult {
        created,
        skipped,
        document_ids,
    })
}
