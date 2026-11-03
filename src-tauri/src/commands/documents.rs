use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tauri::State;
use uuid::Uuid;

use crate::doc_codes::{next_document_code, type_code_for_category, version_label};
use crate::error::{AppError, AppResult};
use crate::path_security::join_under_root;
use crate::state::AppState;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct DocumentRow {
    pub id: String,
    pub project_id: String,
    pub category_id: Option<String>,
    pub business_code: Option<String>,
    pub title: String,
    pub doc_type: Option<String>,
    pub current_version_id: Option<String>,
    pub status: String,
    pub subcategory: Option<String>,
    pub origin: Option<String>,
    pub received_at: Option<String>,
    pub notes: Option<String>,
    pub metadata_json: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub category_key: Option<String>,
    pub current_version_number: Option<i64>,
    pub current_version_label: Option<String>,
    pub file_count: i64,
    pub has_unregistered_change: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct DocumentVersionRow {
    pub id: String,
    pub document_id: String,
    pub version_number: i64,
    pub label: Option<String>,
    pub notes: Option<String>,
    pub created_at: String,
    pub snapshot_relative_path: Option<String>,
    pub registered_fingerprint: Option<String>,
    pub source_relative_path: Option<String>,
    pub is_snapshot: i64,
    pub is_current: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct DocumentFileRow {
    pub id: String,
    pub document_version_id: String,
    pub relative_path: String,
    pub file_name: String,
    pub extension: Option<String>,
    pub size_bytes: Option<i64>,
    pub modified_at: Option<String>,
    pub content_hash: Option<String>,
    pub is_available: i64,
    pub format_label: Option<String>,
    pub revision_id: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct RevisionRow {
    pub id: String,
    pub document_id: String,
    pub revision_code: String,
    pub issued_at: Option<String>,
    pub notes: Option<String>,
    pub created_at: String,
    pub document_version_id: Option<String>,
    pub status: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct SubcategoryRow {
    pub id: String,
    pub project_id: String,
    pub category_key: String,
    pub name: String,
    pub sort_order: i64,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateDocumentInput {
    pub project_id: String,
    pub category_key: String,
    pub title: String,
    pub status: Option<String>,
    pub subcategory: Option<String>,
    pub origin: Option<String>,
    pub received_at: Option<String>,
    pub notes: Option<String>,
    pub assign_code: Option<bool>,
    pub source_absolute_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateDocumentInput {
    pub id: String,
    pub title: Option<String>,
    pub status: Option<String>,
    pub subcategory: Option<String>,
    pub origin: Option<String>,
    pub received_at: Option<String>,
    pub notes: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateVersionInput {
    pub document_id: String,
    pub notes: Option<String>,
    /// Link an existing working file as the new version (no FS copy).
    pub source_absolute_path: Option<String>,
    /// Legacy: re-link current version paths (no physical next-name copy).
    pub from_current: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateNextPhysicalVersionInput {
    pub document_id: String,
    pub notes: Option<String>,
    /// Optional override source absolute path (must exist). Default: current principal.
    pub source_absolute_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NextPhysicalVersionPreview {
    pub source_absolute_path: String,
    pub destination_absolute_path: String,
    pub destination_file_name: String,
    pub next_version_number: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepairVersionFileInput {
    pub document_version_id: String,
    pub absolute_path: String,
    /// When set, mark this document_files row unavailable before linking the new path.
    pub replace_file_id: Option<String>,
    pub format_label: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateRevisionInput {
    pub document_id: String,
    pub revision_code: String,
    pub issued_at: Option<String>,
    pub notes: Option<String>,
    pub document_version_id: Option<String>,
    pub file_paths: Option<Vec<String>>,
    pub format_labels: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttachFileInput {
    pub document_version_id: String,
    pub absolute_path: String,
    pub format_label: Option<String>,
    pub revision_id: Option<String>,
    /// When true, copy into `.ingmanager/snapshots` (historical freeze).
    /// Default **false**: link the working file in place (no copy).
    pub as_snapshot: Option<bool>,
    pub file_role: Option<String>,
}

async fn loc_for_version(
    state: &AppState,
    version_id: &str,
) -> AppResult<crate::project_paths::ProjectLocation> {
    let project_id: (String,) = sqlx::query_as(
        "SELECT d.project_id
         FROM document_versions v
         JOIN documents d ON d.id = v.document_id
         WHERE v.id = ?",
    )
    .bind(version_id)
    .fetch_optional(&state.pool)
    .await?
    .ok_or_else(|| AppError::msg("Versión no encontrada"))?;
    crate::project_paths::resolve_project_location(&state.pool, &project_id.0).await
}

async fn loc_for_document(
    state: &AppState,
    document_id: &str,
) -> AppResult<crate::project_paths::ProjectLocation> {
    let project_id: (String,) =
        sqlx::query_as("SELECT project_id FROM documents WHERE id = ?")
            .bind(document_id)
            .fetch_optional(&state.pool)
            .await?
            .ok_or_else(|| AppError::msg("Documento no encontrado"))?;
    crate::project_paths::resolve_project_location(&state.pool, &project_id.0).await
}

async fn project_business_prefix(state: &AppState, project_id: &str) -> AppResult<String> {
    let loc = crate::project_paths::resolve_project_location(&state.pool, project_id).await?;
    Ok(loc.business_code_prefix)
}

fn fingerprint(path: &Path) -> AppResult<String> {
    let meta = fs::metadata(path)?;
    let modified = meta
        .modified()
        .ok()
        .and_then(|t| {
            let dt: DateTime<Utc> = t.into();
            Some(dt.to_rfc3339())
        })
        .unwrap_or_default();
    Ok(format!("{}:{}", meta.len(), modified))
}

fn to_rfc3339(time: SystemTime) -> Option<String> {
    let dt: DateTime<Utc> = time.into();
    Some(dt.to_rfc3339())
}

fn temp_open_dir(root: &Path) -> PathBuf {
    root.join(".ingmanager").join("temp_open")
}

fn validate_doc_status(category_key: &str, status: &str) -> AppResult<()> {
    let allowed = if category_key == "memorias" {
        ["desarrollo", "revision", "aprobada", "obsoleta", "active"].as_slice()
    } else {
        ["active", "archived", "obsoleta", "desarrollo", "revision", "aprobada"].as_slice()
    };
    if !allowed.contains(&status) {
        return Err(AppError::msg(format!("Estado de documento inválido: {status}")));
    }
    Ok(())
}

async fn fetch_document(state: &AppState, id: &str) -> AppResult<DocumentRow> {
    sqlx::query_as::<_, DocumentRow>(
        "SELECT
            d.id, d.project_id, d.category_id, d.business_code, d.title, d.doc_type,
            d.current_version_id, d.status, d.subcategory, d.origin, d.received_at,
            d.notes, d.metadata_json, d.created_at, d.updated_at,
            c.key AS category_key,
            v.version_number AS current_version_number,
            v.label AS current_version_label,
            COALESCE((
                SELECT COUNT(*) FROM document_files f
                WHERE f.document_version_id = d.current_version_id AND f.is_available = 1
            ), 0) AS file_count,
            0 AS has_unregistered_change
         FROM documents d
         LEFT JOIN categories c ON c.id = d.category_id
         LEFT JOIN document_versions v ON v.id = d.current_version_id
         WHERE d.id = ?",
    )
    .bind(id)
    .fetch_optional(&state.pool)
    .await?
    .ok_or_else(|| AppError::msg("Documento no encontrado"))
}

async fn enrich_change_flags(
    state: &AppState,
    mut docs: Vec<DocumentRow>,
) -> AppResult<Vec<DocumentRow>> {
    for doc in &mut docs {
        if let Some(vid) = &doc.current_version_id {
            let loc = match loc_for_document(state, &doc.id).await {
                Ok(l) => l,
                Err(_) => continue,
            };
            let files: Vec<(String, Option<String>)> = sqlx::query_as(
                "SELECT relative_path, content_hash FROM document_files
                 WHERE document_version_id = ? AND is_available = 1",
            )
            .bind(vid)
            .fetch_all(&state.pool)
            .await?;
            let mut changed = false;
            for (rel, hash) in files {
                let abs = match crate::path_security::join_under_root(&loc.root_path, &rel) {
                    Ok(p) => p,
                    Err(_) => {
                        changed = true;
                        break;
                    }
                };
                if !abs.exists() {
                    changed = true;
                    break;
                }
                if let Some(registered) = hash {
                    if fingerprint(&abs).unwrap_or_default() != registered {
                        changed = true;
                        break;
                    }
                }
            }
            doc.has_unregistered_change = if changed { 1 } else { 0 };
        }
    }
    Ok(docs)
}

#[tauri::command]
pub async fn list_documents(
    state: State<'_, AppState>,
    project_id: String,
    category_key: Option<String>,
    current_only: Option<bool>,
) -> AppResult<Vec<DocumentRow>> {
    let _current_only = current_only.unwrap_or(true); // listing is always logical docs; versions filtered elsewhere

    let docs = if let Some(key) = category_key {
        sqlx::query_as::<_, DocumentRow>(
            "SELECT
                d.id, d.project_id, d.category_id, d.business_code, d.title, d.doc_type,
                d.current_version_id, d.status, d.subcategory, d.origin, d.received_at,
                d.notes, d.metadata_json, d.created_at, d.updated_at,
                c.key AS category_key,
                v.version_number AS current_version_number,
                v.label AS current_version_label,
                COALESCE((
                    SELECT COUNT(*) FROM document_files f
                    WHERE f.document_version_id = d.current_version_id AND f.is_available = 1
                ), 0) AS file_count,
                0 AS has_unregistered_change
             FROM documents d
             LEFT JOIN categories c ON c.id = d.category_id
             LEFT JOIN document_versions v ON v.id = d.current_version_id
             WHERE d.project_id = ? AND c.key = ?
             ORDER BY d.business_code IS NULL, d.business_code, d.title",
        )
        .bind(&project_id)
        .bind(&key)
        .fetch_all(&state.pool)
        .await?
    } else {
        sqlx::query_as::<_, DocumentRow>(
            "SELECT
                d.id, d.project_id, d.category_id, d.business_code, d.title, d.doc_type,
                d.current_version_id, d.status, d.subcategory, d.origin, d.received_at,
                d.notes, d.metadata_json, d.created_at, d.updated_at,
                c.key AS category_key,
                v.version_number AS current_version_number,
                v.label AS current_version_label,
                COALESCE((
                    SELECT COUNT(*) FROM document_files f
                    WHERE f.document_version_id = d.current_version_id AND f.is_available = 1
                ), 0) AS file_count,
                0 AS has_unregistered_change
             FROM documents d
             LEFT JOIN categories c ON c.id = d.category_id
             LEFT JOIN document_versions v ON v.id = d.current_version_id
             WHERE d.project_id = ?
             ORDER BY d.business_code IS NULL, d.business_code, d.title",
        )
        .bind(&project_id)
        .fetch_all(&state.pool)
        .await?
    };

    enrich_change_flags(&state, docs).await
}

#[tauri::command]
pub async fn get_document(state: State<'_, AppState>, id: String) -> AppResult<DocumentRow> {
    let doc = fetch_document(&state, &id).await?;
    Ok(enrich_change_flags(&state, vec![doc])
        .await?
        .into_iter()
        .next()
        .unwrap())
}

#[tauri::command]
pub async fn preview_next_document_code(
    state: State<'_, AppState>,
    project_id: String,
    category_key: String,
) -> AppResult<Option<String>> {
    let type_code = match type_code_for_category(&category_key) {
        Some(t) => t,
        None => return Ok(None),
    };
    let prefix = project_business_prefix(&state, &project_id).await?;
    let rows: Vec<(Option<String>,)> =
        sqlx::query_as("SELECT business_code FROM documents WHERE project_id = ?")
            .bind(&project_id)
            .fetch_all(&state.pool)
            .await?;
    let codes: Vec<String> = rows.into_iter().filter_map(|r| r.0).collect();
    Ok(Some(next_document_code(&prefix, type_code, &codes)?))
}

#[tauri::command]
pub async fn create_document(
    state: State<'_, AppState>,
    input: CreateDocumentInput,
) -> AppResult<DocumentRow> {
    let title = input.title.trim().to_string();
    if title.is_empty() {
        return Err(AppError::msg("El título es obligatorio"));
    }

    let prefix = project_business_prefix(&state, &input.project_id).await?;

    let category: (String, String) = sqlx::query_as(
        "SELECT id, key FROM categories WHERE project_id = ? AND key = ?",
    )
    .bind(&input.project_id)
    .bind(&input.category_key)
    .fetch_optional(&state.pool)
    .await?
    .ok_or_else(|| AppError::msg("Categoría no encontrada"))?;

    let status = input
        .status
        .unwrap_or_else(|| {
            if input.category_key == "memorias" {
                "desarrollo".into()
            } else {
                "active".into()
            }
        });
    validate_doc_status(&input.category_key, &status)?;

    let type_code = type_code_for_category(&input.category_key);
    let assign_code = input.assign_code.unwrap_or(type_code.is_some() && input.category_key != "antecedentes");
    // Antecedentes: code optional (default off unless assign_code true)
    let assign_code = if input.category_key == "antecedentes" {
        input.assign_code.unwrap_or(false)
    } else {
        assign_code
    };

    let business_code = if assign_code {
        let Some(tc) = type_code else {
            return Err(AppError::msg("Esta categoría no tiene tipo de código"));
        };
        let rows: Vec<(Option<String>,)> =
            sqlx::query_as("SELECT business_code FROM documents WHERE project_id = ?")
                .bind(&input.project_id)
                .fetch_all(&state.pool)
                .await?;
        let codes: Vec<String> = rows.into_iter().filter_map(|r| r.0).collect();
        Some(next_document_code(&prefix, tc, &codes)?)
    } else {
        None
    };

    let doc_id = Uuid::new_v4().to_string();
    let version_id = Uuid::new_v4().to_string();
    let doc_type = type_code.map(|s| s.to_string());

    let mut tx = state.pool.begin().await?;

    sqlx::query(
        "INSERT INTO documents (
            id, project_id, category_id, business_code, title, doc_type,
            current_version_id, status, subcategory, origin, received_at, notes,
            created_at, updated_at
         ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, datetime('now'), datetime('now'))",
    )
    .bind(&doc_id)
    .bind(&input.project_id)
    .bind(&category.0)
    .bind(&business_code)
    .bind(&title)
    .bind(&doc_type)
    .bind(&version_id)
    .bind(&status)
    .bind(&input.subcategory)
    .bind(&input.origin)
    .bind(&input.received_at)
    .bind(&input.notes)
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
    .bind(Option::<String>::None)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;

    if let Some(src) = input.source_absolute_path {
        let _ = attach_file_inner(
            &state,
            &version_id,
            &src,
            None,
            None,
            false, // link working file
            None,
        )
        .await?;
    }

    crate::audit::log_event(
        &state.pool,
        Some(&input.project_id),
        "document",
        Some(&doc_id),
        "create",
        Some(&title),
    )
    .await
    .ok();
    crate::search_index::upsert_fts(
        &state.pool,
        "document",
        &doc_id,
        Some(&input.project_id),
        &title,
        business_code.as_deref(),
        input.notes.as_deref(),
        None,
    )
    .await
    .ok();

    get_document(state, doc_id).await
}

#[tauri::command]
pub async fn update_document(
    state: State<'_, AppState>,
    input: UpdateDocumentInput,
) -> AppResult<DocumentRow> {
    let existing = fetch_document(&state, &input.id).await?;
    let title = input
        .title
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or(existing.title);
    let status = input.status.unwrap_or(existing.status);
    let cat_key = existing.category_key.unwrap_or_default();
    validate_doc_status(&cat_key, &status)?;

    let subcategory = input.subcategory.or(existing.subcategory);
    let origin = input.origin.or(existing.origin);
    let received_at = input.received_at.or(existing.received_at);
    let notes = input.notes.or(existing.notes);

    sqlx::query(
        "UPDATE documents SET
            title = ?, status = ?, subcategory = ?, origin = ?, received_at = ?,
            notes = ?, updated_at = datetime('now')
         WHERE id = ?",
    )
    .bind(&title)
    .bind(&status)
    .bind(&subcategory)
    .bind(&origin)
    .bind(&received_at)
    .bind(&notes)
    .bind(&input.id)
    .execute(&state.pool)
    .await?;

    crate::search_index::upsert_fts(
        &state.pool,
        "document",
        &input.id,
        Some(&existing.project_id),
        &title,
        existing.business_code.as_deref(),
        notes.as_deref(),
        None,
    )
    .await
    .ok();

    get_document(state, input.id).await
}

async fn fetch_document_file(state: &AppState, id: &str) -> AppResult<DocumentFileRow> {
    sqlx::query_as::<_, DocumentFileRow>(
        "SELECT id, document_version_id, relative_path, file_name, extension,
                size_bytes, modified_at, content_hash, is_available, format_label,
                revision_id, created_at
         FROM document_files WHERE id = ?",
    )
    .bind(id)
    .fetch_one(&state.pool)
    .await
    .map_err(AppError::from)
}

/// Attach a file to a version.
/// Default: **link** the working path under the project's authorized root.
/// Set `take_snapshot` to copy into `.ingmanager/snapshots` (issued revisions / freeze).
async fn attach_file_inner(
    state: &AppState,
    version_id: &str,
    absolute_path: &str,
    format_label: Option<String>,
    revision_id: Option<String>,
    take_snapshot: bool,
    file_role: Option<String>,
) -> AppResult<DocumentFileRow> {
    let loc = loc_for_version(state, version_id).await?;
    let version: (String, i64) = sqlx::query_as(
        "SELECT document_id, version_number FROM document_versions WHERE id = ?",
    )
    .bind(version_id)
    .fetch_optional(&state.pool)
    .await?
    .ok_or_else(|| AppError::msg("Versión no encontrada"))?;
    let (document_id, version_number) = version;

    let id = if take_snapshot {
        crate::incorporation::snapshot_file_to_version(
            &state.pool,
            &loc,
            &document_id,
            version_id,
            version_number,
            Path::new(absolute_path),
            format_label.as_deref(),
            file_role.as_deref(),
            revision_id.as_deref(),
        )
        .await?
    } else {
        let (id, _) = crate::incorporation::link_working_file(
            &state.pool,
            &loc,
            version_id,
            Path::new(absolute_path),
            format_label.as_deref(),
            file_role.as_deref(),
            revision_id.as_deref(),
        )
        .await?;
        id
    };
    fetch_document_file(state, &id).await
}

#[tauri::command]
pub async fn attach_file_to_version(
    state: State<'_, AppState>,
    input: AttachFileInput,
) -> AppResult<DocumentFileRow> {
    // Default: link working file. Snapshots only when explicitly requested
    // (or when attaching formats to an issued revision).
    let take_snapshot = input.as_snapshot.unwrap_or(input.revision_id.is_some());
    attach_file_inner(
        &state,
        &input.document_version_id,
        &input.absolute_path,
        input.format_label,
        input.revision_id,
        take_snapshot,
        input.file_role,
    )
    .await
}

#[tauri::command]
pub async fn list_document_versions(
    state: State<'_, AppState>,
    document_id: String,
) -> AppResult<Vec<DocumentVersionRow>> {
    list_document_versions_inner(&state, &document_id).await
}

pub async fn list_document_versions_inner(
    state: &AppState,
    document_id: &str,
) -> AppResult<Vec<DocumentVersionRow>> {
    let current_id: Option<String> =
        sqlx::query_as::<_, (Option<String>,)>("SELECT current_version_id FROM documents WHERE id = ?")
            .bind(document_id)
            .fetch_optional(&state.pool)
            .await?
            .and_then(|r| r.0);
    let rows = sqlx::query_as::<_, DocumentVersionRow>(
        "SELECT id, document_id, version_number, label, notes, created_at,
                snapshot_relative_path, registered_fingerprint, source_relative_path,
                is_snapshot, 0 AS is_current
         FROM document_versions
         WHERE document_id = ?
         ORDER BY version_number DESC",
    )
    .bind(document_id)
    .fetch_all(&state.pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|mut r| {
            r.is_current = if current_id.as_ref() == Some(&r.id) {
                1
            } else {
                0
            };
            r
        })
        .collect())
}

#[tauri::command]
pub async fn list_version_files(
    state: State<'_, AppState>,
    version_id: String,
) -> AppResult<Vec<DocumentFileRow>> {
    list_version_files_inner(&state, &version_id).await
}

pub async fn list_version_files_inner(
    state: &AppState,
    version_id: &str,
) -> AppResult<Vec<DocumentFileRow>> {
    sqlx::query_as::<_, DocumentFileRow>(
        "SELECT id, document_version_id, relative_path, file_name, extension,
                size_bytes, modified_at, content_hash, is_available, format_label,
                revision_id, created_at
         FROM document_files
         WHERE document_version_id = ?
         ORDER BY file_name COLLATE NOCASE",
    )
    .bind(version_id)
    .fetch_all(&state.pool)
    .await
    .map_err(AppError::from)
}

#[tauri::command]
pub async fn create_document_version(
    state: State<'_, AppState>,
    input: CreateVersionInput,
) -> AppResult<DocumentVersionRow> {
    create_document_version_inner(&state, input).await
}

pub async fn create_document_version_inner(
    state: &AppState,
    input: CreateVersionInput,
) -> AppResult<DocumentVersionRow> {
    let loc = loc_for_document(state, &input.document_id).await?;
    let doc = fetch_document(state, &input.document_id).await?;
    let from_current = input.from_current.unwrap_or(false);
    if input.source_absolute_path.is_none() && !from_current {
        return Err(AppError::msg(
            "Indique un archivo fuente o use from_current para Nueva versión",
        ));
    }

    let max_n: (i64,) = sqlx::query_as(
        "SELECT COALESCE(MAX(version_number), 0) FROM document_versions WHERE document_id = ?",
    )
    .bind(&input.document_id)
    .fetch_one(&state.pool)
    .await?;
    let next_n = max_n.0 + 1;
    let version_id = Uuid::new_v4().to_string();
    let prev_current = doc.current_version_id.clone();

    sqlx::query(
        "INSERT INTO document_versions (
            id, document_id, version_number, label, notes, created_at, is_snapshot
         ) VALUES (?, ?, ?, ?, ?, datetime('now'), 0)",
    )
    .bind(&version_id)
    .bind(&input.document_id)
    .bind(next_n)
    .bind(version_label(next_n))
    .bind(&input.notes)
    .execute(&state.pool)
    .await?;

    sqlx::query(
        "UPDATE documents SET current_version_id = ?, updated_at = datetime('now') WHERE id = ?",
    )
    .bind(&version_id)
    .bind(&input.document_id)
    .execute(&state.pool)
    .await?;

    let mut linked_ok = 0usize;
    let mut missing = 0usize;
    let mut last_err: Option<String> = None;

    if let Some(src) = &input.source_absolute_path {
        match attach_file_inner(state, &version_id, src, None, None, false, None).await {
            Ok(_) => linked_ok += 1,
            Err(e) => last_err = Some(e.to_string()),
        }
    } else if from_current {
        if let Some(cur) = prev_current.clone() {
            if cur != version_id {
                let files = list_version_files_inner(state, &cur).await?;
                if files.is_empty() {
                    rollback_empty_version(state, &input.document_id, &version_id, cur.as_str())
                        .await?;
                    return Err(AppError::msg(
                        "La versión actual no tiene archivos; no se creó vacio",
                    ));
                }
                for f in files {
                    let abs = join_under_root(&loc.root_path, &f.relative_path)?;
                    if abs.is_file() {
                        let is_snap_path = f.relative_path.contains(".ingmanager/snapshots");
                        match attach_file_inner(
                            state,
                            &version_id,
                            &abs.display().to_string(),
                            f.format_label.clone(),
                            None,
                            is_snap_path,
                            None,
                        )
                        .await
                        {
                            Ok(_) => linked_ok += 1,
                            Err(e) => last_err = Some(e.to_string()),
                        }
                    } else {
                        insert_unavailable_file_row(state, &version_id, &f).await?;
                        missing += 1;
                    }
                }
            }
        }
    }

    if linked_ok == 0 {
        if let Some(prev) = prev_current {
            let _ = rollback_empty_version(state, &input.document_id, &version_id, &prev).await;
        } else {
            let _ = sqlx::query("DELETE FROM document_files WHERE document_version_id = ?")
                .bind(&version_id)
                .execute(&state.pool)
                .await;
            let _ = sqlx::query("DELETE FROM document_versions WHERE id = ?")
                .bind(&version_id)
                .execute(&state.pool)
                .await;
        }
        let detail = last_err.unwrap_or_else(|| {
            if missing > 0 {
                format!("{missing} archivo(s) no disponible(s) en disco")
            } else {
                "sin archivos vinculados".into()
            }
        });
        return Err(AppError::msg(format!(
            "No se creó la versión: {detail}. Elija un archivo existente para vincular."
        )));
    }

    let versions = list_document_versions_inner(state, &input.document_id).await?;
    versions
        .into_iter()
        .find(|v| v.id == version_id)
        .ok_or_else(|| AppError::msg("Versión creada no encontrada"))
}

async fn rollback_empty_version(
    state: &AppState,
    document_id: &str,
    new_version_id: &str,
    previous_version_id: &str,
) -> AppResult<()> {
    sqlx::query("DELETE FROM document_files WHERE document_version_id = ?")
        .bind(new_version_id)
        .execute(&state.pool)
        .await?;
    sqlx::query("DELETE FROM document_versions WHERE id = ?")
        .bind(new_version_id)
        .execute(&state.pool)
        .await?;
    sqlx::query(
        "UPDATE documents SET current_version_id = ?, updated_at = datetime('now') WHERE id = ?",
    )
    .bind(previous_version_id)
    .bind(document_id)
    .execute(&state.pool)
    .await?;
    Ok(())
}

/// Resolve the current principal working file for physical next-version copy.
async fn resolve_current_principal_source(
    state: &AppState,
    document_id: &str,
    override_absolute: Option<&str>,
) -> AppResult<(PathBuf, i64)> {
    let loc = loc_for_document(state, document_id).await?;
    let doc = fetch_document(state, document_id).await?;
    let max_n: (i64,) = sqlx::query_as(
        "SELECT COALESCE(MAX(version_number), 0) FROM document_versions WHERE document_id = ?",
    )
    .bind(document_id)
    .fetch_one(&state.pool)
    .await?;
    let next_n = max_n.0 + 1;

    if let Some(src) = override_absolute {
        let path = PathBuf::from(src);
        if !path.is_file() {
            return Err(AppError::msg(
                "Archivo fuente no disponible; no se creó versión ni copia",
            ));
        }
        return Ok((path, next_n));
    }

    let cur = doc
        .current_version_id
        .ok_or_else(|| AppError::msg("El documento no tiene versión actual"))?;
    let files = list_version_files_inner(state, &cur).await?;
    let candidates: Vec<_> = files
        .into_iter()
        .filter(|f| f.is_available != 0)
        .filter(|f| !f.relative_path.contains(".ingmanager/snapshots"))
        .collect();
    if candidates.is_empty() {
        return Err(AppError::msg(
            "No hay archivo principal disponible para copiar; no se creó versión",
        ));
    }
    let pick = crate::engineering_files::pick_principal_open_index(
        candidates
            .iter()
            .map(|f| (f.file_name.as_str(), f.extension.as_deref())),
    )
    .unwrap_or(0);
    let chosen = &candidates[pick];
    let abs = join_under_root(&loc.root_path, &chosen.relative_path)?;
    if !abs.is_file() {
        return Err(AppError::msg(
            "Archivo fuente no disponible en disco; no se creó versión ni copia",
        ));
    }
    Ok((abs, next_n))
}

#[tauri::command]
pub async fn preview_next_physical_version(
    state: State<'_, AppState>,
    document_id: String,
    source_absolute_path: Option<String>,
) -> AppResult<NextPhysicalVersionPreview> {
    preview_next_physical_version_inner(&state, &document_id, source_absolute_path.as_deref()).await
}

pub async fn preview_next_physical_version_inner(
    state: &AppState,
    document_id: &str,
    source_absolute_path: Option<&str>,
) -> AppResult<NextPhysicalVersionPreview> {
    let (source, next_n) =
        resolve_current_principal_source(state, document_id, source_absolute_path).await?;
    let dest = crate::engineering_files::allocate_next_working_path(&source, next_n)
        .map_err(AppError::msg)?;
    let dest_name = dest
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("archivo")
        .to_string();
    Ok(NextPhysicalVersionPreview {
        source_absolute_path: source.display().to_string(),
        destination_absolute_path: dest.display().to_string(),
        destination_file_name: dest_name,
        next_version_number: next_n,
    })
}

#[tauri::command]
pub async fn create_next_physical_version(
    state: State<'_, AppState>,
    input: CreateNextPhysicalVersionInput,
) -> AppResult<DocumentVersionRow> {
    create_next_physical_version_inner(&state, input).await
}

/// «Crear siguiente versión»: FS copy to next `_vNN` name first, then SQLite register.
pub async fn create_next_physical_version_inner(
    state: &AppState,
    input: CreateNextPhysicalVersionInput,
) -> AppResult<DocumentVersionRow> {
    let loc = loc_for_document(state, &input.document_id).await?;
    let doc = fetch_document(state, &input.document_id).await?;
    let prev_current = doc.current_version_id.clone();

    let (source, next_n) = resolve_current_principal_source(
        state,
        &input.document_id,
        input.source_absolute_path.as_deref(),
    )
    .await?;

    // Authorize destination under project root.
    let dest = crate::engineering_files::allocate_next_working_path(&source, next_n)
        .map_err(AppError::msg)?;
    let dest_rel = dest
        .strip_prefix(&loc.root_path)
        .map_err(|_| AppError::msg("Destino fuera del root autorizado del proyecto"))?;
    let _ = dest_rel; // path_security via join later; ensure under root
    if !dest.starts_with(&loc.root_path) {
        return Err(AppError::msg(
            "Destino fuera del root autorizado del proyecto",
        ));
    }

    // Atomic preference: filesystem first.
    fs::copy(&source, &dest).map_err(|e| {
        AppError::msg(format!(
            "No se pudo copiar a {}: {e}",
            dest.display()
        ))
    })?;

    let version_id = Uuid::new_v4().to_string();
    let insert_result = async {
        sqlx::query(
            "INSERT INTO document_versions (
                id, document_id, version_number, label, notes, created_at, is_snapshot
             ) VALUES (?, ?, ?, ?, ?, datetime('now'), 0)",
        )
        .bind(&version_id)
        .bind(&input.document_id)
        .bind(next_n)
        .bind(version_label(next_n))
        .bind(&input.notes)
        .execute(&state.pool)
        .await?;

        sqlx::query(
            "UPDATE documents SET current_version_id = ?, updated_at = datetime('now') WHERE id = ?",
        )
        .bind(&version_id)
        .bind(&input.document_id)
        .execute(&state.pool)
        .await?;

        attach_file_inner(
            state,
            &version_id,
            &dest.display().to_string(),
            None,
            None,
            false,
            Some("principal".into()),
        )
        .await?;
        Ok::<(), AppError>(())
    }
    .await;

    if let Err(e) = insert_result {
        let _ = fs::remove_file(&dest);
        if let Some(prev) = &prev_current {
            let _ = rollback_empty_version(state, &input.document_id, &version_id, prev).await;
        } else {
            let _ = sqlx::query("DELETE FROM document_files WHERE document_version_id = ?")
                .bind(&version_id)
                .execute(&state.pool)
                .await;
            let _ = sqlx::query("DELETE FROM document_versions WHERE id = ?")
                .bind(&version_id)
                .execute(&state.pool)
                .await;
        }
        return Err(AppError::msg(format!(
            "Copia creada pero no se registró la versión ({e}); se eliminó la copia"
        )));
    }

    let versions = list_document_versions_inner(state, &input.document_id).await?;
    versions
        .into_iter()
        .find(|v| v.id == version_id)
        .ok_or_else(|| AppError::msg("Versión creada no encontrada"))
}

#[tauri::command]
pub async fn repair_version_file(
    state: State<'_, AppState>,
    input: RepairVersionFileInput,
) -> AppResult<DocumentFileRow> {
    repair_version_file_inner(&state, input).await
}

/// «Reparar archivo de esta versión»: link/replace path on existing version (no bump).
pub async fn repair_version_file_inner(
    state: &AppState,
    input: RepairVersionFileInput,
) -> AppResult<DocumentFileRow> {
    let path = PathBuf::from(&input.absolute_path);
    if !path.is_file() {
        return Err(AppError::msg(
            "Archivo de reparación no disponible; no se modificó la versión",
        ));
    }

    let version_exists: Option<(String,)> =
        sqlx::query_as("SELECT id FROM document_versions WHERE id = ?")
            .bind(&input.document_version_id)
            .fetch_optional(&state.pool)
            .await?;
    if version_exists.is_none() {
        return Err(AppError::msg("Versión no encontrada"));
    }

    if let Some(old_id) = &input.replace_file_id {
        sqlx::query("UPDATE document_files SET is_available = 0 WHERE id = ? AND document_version_id = ?")
            .bind(old_id)
            .bind(&input.document_version_id)
            .execute(&state.pool)
            .await?;
    } else {
        // Mark missing/unavailable principal rows on this version so repair replaces them.
        let files = list_version_files_inner(state, &input.document_version_id).await?;
        let loc = loc_for_version(state, &input.document_version_id).await?;
        for f in files {
            if f.is_available == 0 {
                continue;
            }
            let abs = join_under_root(&loc.root_path, &f.relative_path)?;
            if !abs.is_file() {
                sqlx::query("UPDATE document_files SET is_available = 0 WHERE id = ?")
                    .bind(&f.id)
                    .execute(&state.pool)
                    .await?;
            }
        }
    }

    attach_file_inner(
        state,
        &input.document_version_id,
        &input.absolute_path,
        input.format_label,
        None,
        false,
        Some("principal".into()),
    )
    .await
}

#[tauri::command]
pub async fn open_document_principal(
    state: State<'_, AppState>,
    document_id: String,
    use_temp_copy: Option<bool>,
) -> AppResult<String> {
    open_document_principal_inner(&state, &document_id, use_temp_copy).await
}

pub async fn open_document_principal_inner(
    state: &AppState,
    document_id: &str,
    use_temp_copy: Option<bool>,
) -> AppResult<String> {
    let doc = fetch_document(state, document_id).await?;
    let version_id = doc
        .current_version_id
        .ok_or_else(|| AppError::msg("El documento no tiene versión actual"))?;
    let files = list_version_files_inner(state, &version_id).await?;
    let available: Vec<_> = files
        .into_iter()
        .filter(|f| f.is_available != 0)
        .collect();
    if available.is_empty() {
        return Err(AppError::msg("No hay archivos disponibles para abrir"));
    }
    let pick = crate::engineering_files::pick_principal_open_index(
        available
            .iter()
            .map(|f| (f.file_name.as_str(), f.extension.as_deref())),
    )
    .unwrap_or(0);
    let file_id = available[pick].id.clone();
    open_document_file_by_id(state, &file_id, use_temp_copy).await
}

async fn open_document_file_by_id(
    state: &AppState,
    file_id: &str,
    use_temp_copy: Option<bool>,
) -> AppResult<String> {
    let row: (String, String, i64, String) = sqlx::query_as(
        "SELECT f.relative_path, f.file_name, COALESCE(v.is_snapshot, 0), d.project_id
         FROM document_files f
         JOIN document_versions v ON v.id = f.document_version_id
         JOIN documents d ON d.id = v.document_id
         WHERE f.id = ?",
    )
    .bind(file_id)
    .fetch_optional(&state.pool)
    .await?
    .ok_or_else(|| AppError::msg("Archivo de documento no encontrado"))?;

    let (rel, file_name, is_snapshot, project_id) = row;
    let loc = crate::project_paths::resolve_project_location(&state.pool, &project_id).await?;
    let src = join_under_root(&loc.root_path, &rel)?;
    if !src.is_file() {
        sqlx::query("UPDATE document_files SET is_available = 0 WHERE id = ?")
            .bind(file_id)
            .execute(&state.pool)
            .await
            .ok();
        return Err(AppError::msg("Archivo no disponible"));
    }

    let force_temp = use_temp_copy.unwrap_or(is_snapshot == 1);
    let open_path = if force_temp {
        let tmp = temp_open_dir(&loc.root_path).join(Uuid::new_v4().to_string());
        fs::create_dir_all(&tmp)?;
        let dest = tmp.join(&file_name);
        fs::copy(&src, &dest)?;
        dest
    } else {
        src
    };

    open::that(&open_path).map_err(|e| AppError::msg(format!("No se pudo abrir: {e}")))?;
    Ok(open_path.display().to_string())
}

/// Resolve principal file id without opening (for tests / UI previews).
pub async fn resolve_document_principal_file_id(
    state: &AppState,
    document_id: &str,
) -> AppResult<String> {
    let doc = fetch_document(state, document_id).await?;
    let version_id = doc
        .current_version_id
        .ok_or_else(|| AppError::msg("El documento no tiene versión actual"))?;
    let files = list_version_files_inner(state, &version_id).await?;
    let available: Vec<_> = files
        .into_iter()
        .filter(|f| f.is_available != 0)
        .collect();
    if available.is_empty() {
        return Err(AppError::msg("No hay archivos disponibles"));
    }
    let pick = crate::engineering_files::pick_principal_open_index(
        available
            .iter()
            .map(|f| (f.file_name.as_str(), f.extension.as_deref())),
    )
    .unwrap_or(0);
    Ok(available[pick].id.clone())
}

async fn insert_unavailable_file_row(
    state: &AppState,
    version_id: &str,
    source: &DocumentFileRow,
) -> AppResult<()> {
    let id = Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO document_files (
            id, document_version_id, relative_path, file_name, extension,
            size_bytes, modified_at, content_hash, is_available, format_label,
            revision_id, created_at
         ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, 0, ?, ?, datetime('now'))",
    )
    .bind(&id)
    .bind(version_id)
    .bind(&source.relative_path)
    .bind(&source.file_name)
    .bind(&source.extension)
    .bind(source.size_bytes)
    .bind(&source.modified_at)
    .bind(&source.content_hash)
    .bind(&source.format_label)
    .bind(&source.revision_id)
    .execute(&state.pool)
    .await?;
    Ok(())
}

/// Restore a historical version by creating a NEW current version (does not rewrite history).
#[tauri::command]
pub async fn restore_document_version(
    state: State<'_, AppState>,
    document_id: String,
    version_id: String,
    notes: Option<String>,
) -> AppResult<DocumentVersionRow> {
    let loc = loc_for_document(&state, &document_id).await?;
    let source_files = list_version_files_inner(&state, &version_id).await?;
    if source_files.is_empty() {
        return Err(AppError::msg(
            "La versión a restaurar no tiene archivos asociados",
        ));
    }

    let prev_doc = fetch_document(&state, &document_id).await?;
    let prev_current = prev_doc.current_version_id.clone();

    let max_n: (i64,) = sqlx::query_as(
        "SELECT COALESCE(MAX(version_number), 0) FROM document_versions WHERE document_id = ?",
    )
    .bind(&document_id)
    .fetch_one(&state.pool)
    .await?;
    let next_n = max_n.0 + 1;
    let new_id = Uuid::new_v4().to_string();
    let note = notes.unwrap_or_else(|| format!("Restaurado desde versión {version_id}"));

    sqlx::query(
        "INSERT INTO document_versions (
            id, document_id, version_number, label, notes, created_at, is_snapshot
         ) VALUES (?, ?, ?, ?, ?, datetime('now'), 0)",
    )
    .bind(&new_id)
    .bind(&document_id)
    .bind(next_n)
    .bind(version_label(next_n))
    .bind(&note)
    .execute(&state.pool)
    .await?;

    sqlx::query(
        "UPDATE documents SET current_version_id = ?, updated_at = datetime('now') WHERE id = ?",
    )
    .bind(&new_id)
    .bind(&document_id)
    .execute(&state.pool)
    .await?;

    let mut linked_ok = 0usize;
    for f in source_files {
        let abs = join_under_root(&loc.root_path, &f.relative_path)?;
        if abs.is_file() {
            let is_snap_path = f.relative_path.contains(".ingmanager/snapshots");
            attach_file_inner(
                &state,
                &new_id,
                &abs.display().to_string(),
                f.format_label.clone(),
                None,
                is_snap_path,
                None,
            )
            .await?;
            linked_ok += 1;
        } else {
            insert_unavailable_file_row(&state, &new_id, &f).await?;
        }
    }
    if linked_ok == 0 {
        if let Some(prev) = prev_current {
            let _ = rollback_empty_version(&state, &document_id, &new_id, &prev).await;
        }
        return Err(AppError::msg(
            "No se pudo restaurar: ningún archivo físico está disponible",
        ));
    }

    let versions = list_document_versions_inner(&state, &document_id).await?;
    versions
        .into_iter()
        .find(|v| v.id == new_id)
        .ok_or_else(|| AppError::msg("Versión restaurada no encontrada"))
}

#[tauri::command]
pub async fn open_document_file(
    state: State<'_, AppState>,
    file_id: String,
    use_temp_copy: Option<bool>,
) -> AppResult<String> {
    open_document_file_by_id(&state, &file_id, use_temp_copy).await
}

#[tauri::command]
pub async fn list_revisions(
    state: State<'_, AppState>,
    document_id: String,
) -> AppResult<Vec<RevisionRow>> {
    sqlx::query_as::<_, RevisionRow>(
        "SELECT id, document_id, revision_code, issued_at, notes, created_at,
                document_version_id, status
         FROM revisions
         WHERE document_id = ?
         ORDER BY created_at DESC",
    )
    .bind(document_id)
    .fetch_all(&state.pool)
    .await
    .map_err(AppError::from)
}

#[tauri::command]
pub async fn create_revision(
    state: State<'_, AppState>,
    input: CreateRevisionInput,
) -> AppResult<RevisionRow> {
    let code = input.revision_code.trim().to_string();
    if code.is_empty() {
        return Err(AppError::msg("El código de revisión es obligatorio"));
    }

    let id = Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO revisions (
            id, document_id, revision_code, issued_at, notes, created_at,
            document_version_id, status
         ) VALUES (?, ?, ?, ?, ?, datetime('now'), ?, 'issued')",
    )
    .bind(&id)
    .bind(&input.document_id)
    .bind(&code)
    .bind(&input.issued_at)
    .bind(&input.notes)
    .bind(&input.document_version_id)
    .execute(&state.pool)
    .await?;

    // Issued revision formats are snapshotted (historical freeze). Working
    // editable files on the version remain linked separately.
    if let Some(paths) = input.file_paths {
        let version_id = if let Some(v) = input.document_version_id.clone() {
            v
        } else {
            let doc = fetch_document(&state, &input.document_id).await?;
            doc.current_version_id
                .ok_or_else(|| AppError::msg("El documento no tiene versión actual"))?
        };
        let labels = input.format_labels.unwrap_or_default();
        for (i, path) in paths.iter().enumerate() {
            let label = labels.get(i).cloned();
            let _ = attach_file_inner(
                &state,
                &version_id,
                path,
                label,
                Some(id.clone()),
                true, // snapshot issued formats
                Some("issued_revision".into()),
            )
            .await?;
        }
    }

    sqlx::query_as::<_, RevisionRow>(
        "SELECT id, document_id, revision_code, issued_at, notes, created_at,
                document_version_id, status
         FROM revisions WHERE id = ?",
    )
    .bind(id)
    .fetch_one(&state.pool)
    .await
    .map_err(AppError::from)
}

#[tauri::command]
pub async fn list_subcategories(
    state: State<'_, AppState>,
    project_id: String,
    category_key: String,
) -> AppResult<Vec<SubcategoryRow>> {
    sqlx::query_as::<_, SubcategoryRow>(
        "SELECT id, project_id, category_key, name, sort_order, created_at
         FROM document_subcategories
         WHERE project_id = ? AND category_key = ?
         ORDER BY sort_order, name",
    )
    .bind(project_id)
    .bind(category_key)
    .fetch_all(&state.pool)
    .await
    .map_err(AppError::from)
}

#[tauri::command]
pub async fn create_subcategory(
    state: State<'_, AppState>,
    project_id: String,
    category_key: String,
    name: String,
) -> AppResult<SubcategoryRow> {
    let name = name.trim().to_string();
    if name.is_empty() {
        return Err(AppError::msg("Nombre de subcategoría vacío"));
    }
    let id = Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO document_subcategories (id, project_id, category_key, name, sort_order)
         VALUES (?, ?, ?, ?, 0)",
    )
    .bind(&id)
    .bind(&project_id)
    .bind(&category_key)
    .bind(&name)
    .execute(&state.pool)
    .await?;

    sqlx::query_as::<_, SubcategoryRow>(
        "SELECT id, project_id, category_key, name, sort_order, created_at
         FROM document_subcategories WHERE id = ?",
    )
    .bind(id)
    .fetch_one(&state.pool)
    .await
    .map_err(AppError::from)
}

#[tauri::command]
pub async fn update_subcategory(
    state: State<'_, AppState>,
    id: String,
    name: String,
) -> AppResult<SubcategoryRow> {
    update_subcategory_inner(&state, &id, &name).await
}

pub async fn update_subcategory_inner(
    state: &AppState,
    id: &str,
    name: &str,
) -> AppResult<SubcategoryRow> {
    let name = name.trim().to_string();
    if name.is_empty() {
        return Err(AppError::msg("Nombre de subcategoría vacío"));
    }
    let existing = sqlx::query_as::<_, SubcategoryRow>(
        "SELECT id, project_id, category_key, name, sort_order, created_at
         FROM document_subcategories WHERE id = ?",
    )
    .bind(id)
    .fetch_optional(&state.pool)
    .await?
    .ok_or_else(|| AppError::msg("Subcategoría no encontrada"))?;

    let old_name = existing.name.clone();
    sqlx::query("UPDATE document_subcategories SET name = ? WHERE id = ?")
        .bind(&name)
        .bind(id)
        .execute(&state.pool)
        .await?;

    // Keep document subcategory strings in sync when they matched the old name.
    // Metadata only — never touches file paths.
    sqlx::query(
        "UPDATE documents SET subcategory = ?, updated_at = datetime('now')
         WHERE project_id = ? AND subcategory = ?",
    )
    .bind(&name)
    .bind(&existing.project_id)
    .bind(&old_name)
    .execute(&state.pool)
    .await?;

    sqlx::query_as::<_, SubcategoryRow>(
        "SELECT id, project_id, category_key, name, sort_order, created_at
         FROM document_subcategories WHERE id = ?",
    )
    .bind(id)
    .fetch_one(&state.pool)
    .await
    .map_err(AppError::from)
}

#[tauri::command]
pub async fn delete_subcategory(
    state: State<'_, AppState>,
    id: String,
) -> AppResult<()> {
    delete_subcategory_inner(&state, &id).await
}

pub async fn delete_subcategory_inner(state: &AppState, id: &str) -> AppResult<()> {
    let res = sqlx::query("DELETE FROM document_subcategories WHERE id = ?")
        .bind(id)
        .execute(&state.pool)
        .await?;
    if res.rows_affected() == 0 {
        return Err(AppError::msg("Subcategoría no encontrada"));
    }
    // Document.subcategory strings may remain as orphan labels (no path renames).
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct RelationshipRow {
    pub id: String,
    pub source_document_id: String,
    pub target_document_id: String,
    pub relation_type: String,
    pub notes: Option<String>,
    pub source_title: Option<String>,
    pub source_code: Option<String>,
    pub target_title: Option<String>,
    pub target_code: Option<String>,
    pub source_version_label: Option<String>,
    pub target_version_label: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateRelationshipInput {
    pub source_document_id: String,
    pub target_document_id: String,
    pub relation_type: String,
    pub notes: Option<String>,
}

#[tauri::command]
pub async fn list_document_relationships(
    state: State<'_, AppState>,
    document_id: String,
) -> AppResult<Vec<RelationshipRow>> {
    sqlx::query_as::<_, RelationshipRow>(
        "SELECT
            r.id,
            r.source_document_id,
            r.target_document_id,
            r.relation_type,
            r.notes,
            s.title AS source_title,
            s.business_code AS source_code,
            t.title AS target_title,
            t.business_code AS target_code,
            sv.label AS source_version_label,
            tv.label AS target_version_label
         FROM document_relationships r
         JOIN documents s ON s.id = r.source_document_id
         JOIN documents t ON t.id = r.target_document_id
         LEFT JOIN document_versions sv ON sv.id = s.current_version_id
         LEFT JOIN document_versions tv ON tv.id = t.current_version_id
         WHERE r.source_document_id = ? OR r.target_document_id = ?
         ORDER BY r.created_at DESC",
    )
    .bind(&document_id)
    .bind(&document_id)
    .fetch_all(&state.pool)
    .await
    .map_err(AppError::from)
}

#[tauri::command]
pub async fn create_document_relationship(
    state: State<'_, AppState>,
    input: CreateRelationshipInput,
) -> AppResult<RelationshipRow> {
    if input.source_document_id == input.target_document_id {
        return Err(AppError::msg("No se puede relacionar un documento consigo mismo"));
    }
    let rel_type = input.relation_type.trim().to_string();
    if rel_type.is_empty() {
        return Err(AppError::msg("Tipo de relación obligatorio"));
    }
    let id = Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO document_relationships (id, source_document_id, target_document_id, relation_type, notes, created_at)
         VALUES (?, ?, ?, ?, ?, datetime('now'))",
    )
    .bind(&id)
    .bind(&input.source_document_id)
    .bind(&input.target_document_id)
    .bind(&rel_type)
    .bind(&input.notes)
    .execute(&state.pool)
    .await
    .map_err(|e| {
        if e.to_string().contains("UNIQUE") {
            AppError::msg("La relación ya existe")
        } else {
            AppError::from(e)
        }
    })?;

    let src = fetch_document(&state, &input.source_document_id).await?;
    crate::audit::log_event(
        &state.pool,
        Some(&src.project_id),
        "document_relationship",
        Some(&id),
        "create",
        Some(&format!(
            "{} -> {} ({})",
            input.source_document_id, input.target_document_id, rel_type
        )),
    )
    .await
    .ok();

    let rows = list_document_relationships(state, input.source_document_id).await?;
    rows.into_iter()
        .find(|r| r.id == id)
        .ok_or_else(|| AppError::msg("Relación creada no encontrada"))
}

#[tauri::command]
pub async fn delete_document_relationship(
    state: State<'_, AppState>,
    relationship_id: String,
) -> AppResult<()> {
    sqlx::query("DELETE FROM document_relationships WHERE id = ?")
        .bind(&relationship_id)
        .execute(&state.pool)
        .await?;
    Ok(())
}

#[tauri::command]
pub async fn set_document_favorite(
    state: State<'_, AppState>,
    document_id: String,
    favorite: bool,
) -> AppResult<()> {
    if favorite {
        sqlx::query(
            "INSERT INTO favorites (id, entity_type, entity_id, created_at)
             VALUES (?, 'document', ?, datetime('now'))
             ON CONFLICT(entity_type, entity_id) DO NOTHING",
        )
        .bind(Uuid::new_v4().to_string())
        .bind(&document_id)
        .execute(&state.pool)
        .await?;
    } else {
        sqlx::query("DELETE FROM favorites WHERE entity_type = 'document' AND entity_id = ?")
            .bind(&document_id)
            .execute(&state.pool)
            .await?;
    }
    Ok(())
}

#[tauri::command]
pub async fn list_favorite_documents(
    state: State<'_, AppState>,
    project_id: Option<String>,
) -> AppResult<Vec<DocumentRow>> {
    let docs = if let Some(pid) = project_id {
        sqlx::query_as::<_, DocumentRow>(
            "SELECT
                d.id, d.project_id, d.category_id, d.business_code, d.title, d.doc_type,
                d.current_version_id, d.status, d.subcategory, d.origin, d.received_at,
                d.notes, d.metadata_json, d.created_at, d.updated_at,
                c.key AS category_key,
                v.version_number AS current_version_number,
                v.label AS current_version_label,
                COALESCE((
                    SELECT COUNT(*) FROM document_files f
                    WHERE f.document_version_id = d.current_version_id AND f.is_available = 1
                ), 0) AS file_count,
                0 AS has_unregistered_change
             FROM favorites fav
             JOIN documents d ON d.id = fav.entity_id
             LEFT JOIN categories c ON c.id = d.category_id
             LEFT JOIN document_versions v ON v.id = d.current_version_id
             WHERE fav.entity_type = 'document' AND d.project_id = ?
             ORDER BY fav.created_at DESC",
        )
        .bind(pid)
        .fetch_all(&state.pool)
        .await?
    } else {
        sqlx::query_as::<_, DocumentRow>(
            "SELECT
                d.id, d.project_id, d.category_id, d.business_code, d.title, d.doc_type,
                d.current_version_id, d.status, d.subcategory, d.origin, d.received_at,
                d.notes, d.metadata_json, d.created_at, d.updated_at,
                c.key AS category_key,
                v.version_number AS current_version_number,
                v.label AS current_version_label,
                COALESCE((
                    SELECT COUNT(*) FROM document_files f
                    WHERE f.document_version_id = d.current_version_id AND f.is_available = 1
                ), 0) AS file_count,
                0 AS has_unregistered_change
             FROM favorites fav
             JOIN documents d ON d.id = fav.entity_id
             LEFT JOIN categories c ON c.id = d.category_id
             LEFT JOIN document_versions v ON v.id = d.current_version_id
             WHERE fav.entity_type = 'document'
             ORDER BY fav.created_at DESC",
        )
        .fetch_all(&state.pool)
        .await?
    };
    enrich_change_flags(&state, docs).await
}

#[tauri::command]
pub async fn is_document_favorite(
    state: State<'_, AppState>,
    document_id: String,
) -> AppResult<bool> {
    let row: Option<(String,)> = sqlx::query_as(
        "SELECT id FROM favorites WHERE entity_type = 'document' AND entity_id = ?",
    )
    .bind(document_id)
    .fetch_optional(&state.pool)
    .await?;
    Ok(row.is_some())
}

/// Delete a logical document from SQLite only. Physical files on disk are kept.
#[tauri::command]
pub async fn delete_logical_document(
    state: State<'_, AppState>,
    document_id: String,
) -> AppResult<()> {
    delete_logical_document_inner(&state.pool, &document_id).await
}

pub async fn delete_logical_document_inner(
    pool: &sqlx::SqlitePool,
    document_id: &str,
) -> AppResult<()> {
    let doc: Option<(String, Option<String>)> =
        sqlx::query_as("SELECT project_id, business_code FROM documents WHERE id = ?")
            .bind(document_id)
            .fetch_optional(pool)
            .await?;
    let Some((project_id, code)) = doc else {
        return Err(AppError::msg("Documento lógico no encontrado"));
    };

    let mut tx = pool.begin().await?;

    let version_ids: Vec<(String,)> =
        sqlx::query_as("SELECT id FROM document_versions WHERE document_id = ?")
            .bind(document_id)
            .fetch_all(&mut *tx)
            .await?;

    for (vid,) in &version_ids {
        sqlx::query("DELETE FROM document_files WHERE document_version_id = ?")
            .bind(vid)
            .execute(&mut *tx)
            .await?;
    }

    sqlx::query("DELETE FROM revisions WHERE document_id = ?")
        .bind(document_id)
        .execute(&mut *tx)
        .await?;
    sqlx::query(
        "DELETE FROM document_relationships
         WHERE source_document_id = ? OR target_document_id = ?",
    )
    .bind(document_id)
    .bind(document_id)
    .execute(&mut *tx)
    .await?;
    sqlx::query("DELETE FROM favorites WHERE entity_type = 'document' AND entity_id = ?")
        .bind(document_id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM file_version_tracking WHERE document_id = ?")
        .bind(document_id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM engineering_groups WHERE document_id = ?")
        .bind(document_id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("UPDATE documents SET current_version_id = NULL WHERE id = ?")
        .bind(document_id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM document_versions WHERE document_id = ?")
        .bind(document_id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM documents WHERE id = ?")
        .bind(document_id)
        .execute(&mut *tx)
        .await?;

    tx.commit().await?;

    crate::audit::log_event(
        pool,
        Some(&project_id),
        "document",
        Some(document_id),
        "delete_logical",
        Some(&format!(
            "Eliminado documento lógico {} (archivos físicos conservados)",
            code.unwrap_or_else(|| document_id.to_string())
        )),
    )
    .await
    .ok();

    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AvailabilityCleanupResult {
    pub marked_unavailable: usize,
    pub deleted_empty_documents: usize,
    pub document_ids_removed: Vec<String>,
}

/// Mark missing physical files as unavailable; optionally remove empty logical docs
/// (no available files) without opening/touching files on disk beyond existence checks.
#[tauri::command]
pub async fn cleanup_document_availability(
    state: State<'_, AppState>,
    project_id: String,
    delete_empty_logicals: Option<bool>,
) -> AppResult<AvailabilityCleanupResult> {
    cleanup_document_availability_inner(
        &state.pool,
        &project_id,
        delete_empty_logicals.unwrap_or(true),
    )
    .await
}

pub async fn cleanup_document_availability_inner(
    pool: &sqlx::SqlitePool,
    project_id: &str,
    delete_empty_logicals: bool,
) -> AppResult<AvailabilityCleanupResult> {
    let loc = crate::project_paths::resolve_project_location(pool, project_id).await?;
    let rows: Vec<(String, String)> = sqlx::query_as(
        "SELECT f.id, f.relative_path
         FROM document_files f
         JOIN document_versions v ON v.id = f.document_version_id
         JOIN documents d ON d.id = v.document_id
         WHERE d.project_id = ? AND f.is_available = 1",
    )
    .bind(project_id)
    .fetch_all(pool)
    .await?;

    let mut marked = 0usize;
    for (fid, rel) in rows {
        let abs = join_under_root(&loc.root_path, &rel)?;
        if !abs.is_file() {
            sqlx::query("UPDATE document_files SET is_available = 0 WHERE id = ?")
                .bind(&fid)
                .execute(pool)
                .await?;
            marked += 1;
        }
    }

    let mut removed = Vec::new();
    if delete_empty_logicals {
        let empty_docs: Vec<(String,)> = sqlx::query_as(
            "SELECT d.id FROM documents d
             WHERE d.project_id = ?
               AND NOT EXISTS (
                 SELECT 1 FROM document_versions v
                 JOIN document_files f ON f.document_version_id = v.id
                 WHERE v.document_id = d.id AND f.is_available = 1
               )",
        )
        .bind(project_id)
        .fetch_all(pool)
        .await?;
        for (id,) in empty_docs {
            delete_logical_document_inner(pool, &id).await?;
            removed.push(id);
        }
    }

    Ok(AvailabilityCleanupResult {
        marked_unavailable: marked,
        deleted_empty_documents: removed.len(),
        document_ids_removed: removed,
    })
}

/// Mark document_files unavailable when a relative path under a project root is removed.
pub async fn mark_document_files_unavailable_for_path(
    pool: &sqlx::SqlitePool,
    relative_path: &str,
) -> AppResult<u64> {
    let res = sqlx::query(
        "UPDATE document_files SET is_available = 0
         WHERE is_available = 1 AND (relative_path = ? OR relative_path LIKE ?)",
    )
    .bind(relative_path)
    .bind(format!("{relative_path}/%"))
    .execute(pool)
    .await?;
    Ok(res.rows_affected())
}
