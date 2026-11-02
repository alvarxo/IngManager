use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tauri::State;
use uuid::Uuid;

use crate::db;
use crate::doc_codes::{next_document_code, type_code_for_category, version_label};
use crate::error::{AppError, AppResult};
use crate::path_security::{ensure_under_root, join_under_root};
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
    pub source_absolute_path: Option<String>,
    /// If true, copy current version files into the new version snapshot.
    pub from_current: Option<bool>,
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
}

async fn require_root(state: &AppState) -> AppResult<PathBuf> {
    let root = db::get_setting(&state.pool, "root_path")
        .await?
        .ok_or_else(|| AppError::msg("Configure la carpeta raíz antes de continuar"))?;
    let path = PathBuf::from(&root);
    if !path.is_dir() {
        return Err(AppError::msg(
            "La carpeta raíz configurada no es accesible",
        ));
    }
    Ok(path)
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

fn rel_from_root(root: &Path, abs: &Path) -> AppResult<String> {
    Ok(abs
        .strip_prefix(root)
        .map_err(|_| AppError::msg("Ruta fuera de la raíz"))?
        .to_string_lossy()
        .replace('\\', "/"))
}

fn snapshots_dir(root: &Path, document_id: &str, version_number: i64) -> PathBuf {
    root.join(".ingmanager")
        .join("snapshots")
        .join(document_id)
        .join(format!("v{version_number:02}"))
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
    root: &Path,
    mut docs: Vec<DocumentRow>,
) -> AppResult<Vec<DocumentRow>> {
    for doc in &mut docs {
        if let Some(vid) = &doc.current_version_id {
            let files: Vec<(String, Option<String>)> = sqlx::query_as(
                "SELECT relative_path, content_hash FROM document_files
                 WHERE document_version_id = ? AND is_available = 1",
            )
            .bind(vid)
            .fetch_all(&state.pool)
            .await?;
            let mut changed = false;
            for (rel, hash) in files {
                let abs = root.join(&rel);
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
    let root = require_root(&state).await?;

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

    enrich_change_flags(&state, &root, docs).await
}

#[tauri::command]
pub async fn get_document(state: State<'_, AppState>, id: String) -> AppResult<DocumentRow> {
    let root = require_root(&state).await?;
    let doc = fetch_document(&state, &id).await?;
    Ok(enrich_change_flags(&state, &root, vec![doc])
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
    let project: (String,) = sqlx::query_as("SELECT code FROM projects WHERE id = ?")
        .bind(&project_id)
        .fetch_optional(&state.pool)
        .await?
        .ok_or_else(|| AppError::msg("Proyecto no encontrado"))?;
    let rows: Vec<(Option<String>,)> =
        sqlx::query_as("SELECT business_code FROM documents WHERE project_id = ?")
            .bind(&project_id)
            .fetch_all(&state.pool)
            .await?;
    let codes: Vec<String> = rows.into_iter().filter_map(|r| r.0).collect();
    Ok(Some(next_document_code(&project.0, type_code, &codes)?))
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

    let root = require_root(&state).await?;
    let project: (String,) = sqlx::query_as("SELECT code FROM projects WHERE id = ?")
        .bind(&input.project_id)
        .fetch_optional(&state.pool)
        .await?
        .ok_or_else(|| AppError::msg("Proyecto no encontrado"))?;

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
        Some(next_document_code(&project.0, tc, &codes)?)
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
            &root,
            &version_id,
            &src,
            None,
            None,
            true,
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

async fn attach_file_inner(
    state: &AppState,
    root: &Path,
    version_id: &str,
    absolute_path: &str,
    format_label: Option<String>,
    revision_id: Option<String>,
    take_snapshot: bool,
) -> AppResult<DocumentFileRow> {
    let src = ensure_under_root(root, Path::new(absolute_path))?;
    if !src.is_file() {
        return Err(AppError::msg("La ruta no es un archivo"));
    }

    let version: (String, i64, String) = sqlx::query_as(
        "SELECT document_id, version_number, COALESCE(snapshot_relative_path, '')
         FROM document_versions WHERE id = ?",
    )
    .bind(version_id)
    .fetch_optional(&state.pool)
    .await?
    .ok_or_else(|| AppError::msg("Versión no encontrada"))?;

    let (document_id, version_number, existing_snap) = version;
    let file_name = src
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "file".into());
    let extension = src
        .extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase());
    let meta = fs::metadata(&src)?;
    let modified = meta.modified().ok().and_then(to_rfc3339);
    let fp = fingerprint(&src)?;

    let stored_rel = if take_snapshot {
        let snap_dir = snapshots_dir(root, &document_id, version_number);
        fs::create_dir_all(&snap_dir)?;
        let dest = snap_dir.join(&file_name);
        let dest = if dest.exists() {
            crate::fs_util::unique_destination(&snap_dir, &file_name)
        } else {
            dest
        };
        fs::copy(&src, &dest)?;
        let rel = rel_from_root(root, &dest)?;
        let snap_rel = rel_from_root(root, &snap_dir)?;
        sqlx::query(
            "UPDATE document_versions SET
                snapshot_relative_path = ?,
                registered_fingerprint = ?,
                source_relative_path = ?,
                is_snapshot = 1
             WHERE id = ?",
        )
        .bind(&snap_rel)
        .bind(&fp)
        .bind(rel_from_root(root, &src)?)
        .bind(version_id)
        .execute(&state.pool)
        .await?;
        let _ = existing_snap;
        rel
    } else {
        rel_from_root(root, &src)?
    };

    let id = Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO document_files (
            id, document_version_id, relative_path, file_name, extension,
            size_bytes, modified_at, content_hash, is_available, format_label,
            revision_id, created_at
         ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, 1, ?, ?, datetime('now'))",
    )
    .bind(&id)
    .bind(version_id)
    .bind(&stored_rel)
    .bind(&file_name)
    .bind(&extension)
    .bind(meta.len() as i64)
    .bind(&modified)
    .bind(&fp)
    .bind(&format_label)
    .bind(&revision_id)
    .execute(&state.pool)
    .await?;

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

#[tauri::command]
pub async fn attach_file_to_version(
    state: State<'_, AppState>,
    input: AttachFileInput,
) -> AppResult<DocumentFileRow> {
    let root = require_root(&state).await?;
    attach_file_inner(
        &state,
        &root,
        &input.document_version_id,
        &input.absolute_path,
        input.format_label,
        input.revision_id,
        true,
    )
    .await
}

#[tauri::command]
pub async fn list_document_versions(
    state: State<'_, AppState>,
    document_id: String,
) -> AppResult<Vec<DocumentVersionRow>> {
    let current_id: Option<String> =
        sqlx::query_as::<_, (Option<String>,)>("SELECT current_version_id FROM documents WHERE id = ?")
            .bind(&document_id)
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
    .bind(&document_id)
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
    let root = require_root(&state).await?;
    let doc = fetch_document(&state, &input.document_id).await?;

    let max_n: (i64,) = sqlx::query_as(
        "SELECT COALESCE(MAX(version_number), 0) FROM document_versions WHERE document_id = ?",
    )
    .bind(&input.document_id)
    .fetch_one(&state.pool)
    .await?;
    let next_n = max_n.0 + 1;
    let version_id = Uuid::new_v4().to_string();

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

    if let Some(src) = input.source_absolute_path {
        let _ = attach_file_inner(&state, &root, &version_id, &src, None, None, true).await?;
    } else if input.from_current.unwrap_or(false) {
        if let Some(cur) = doc.current_version_id {
            // copy files from previous current (before update we already pointed new — use listed files of old)
            // We updated current already; fetch files that were on old version `cur` if cur != version_id
            if cur != version_id {
                let files = list_version_files(state.clone(), cur).await?;
                for f in files {
                    let abs = join_under_root(&root, &f.relative_path)?;
                    if abs.is_file() {
                        let _ = attach_file_inner(
                            &state,
                            &root,
                            &version_id,
                            &abs.display().to_string(),
                            f.format_label,
                            None,
                            true,
                        )
                        .await?;
                    }
                }
            }
        }
    }

    let versions = list_document_versions(state, input.document_id).await?;
    versions
        .into_iter()
        .find(|v| v.id == version_id)
        .ok_or_else(|| AppError::msg("Versión creada no encontrada"))
}

/// Restore a historical version by creating a NEW current version (does not rewrite history).
#[tauri::command]
pub async fn restore_document_version(
    state: State<'_, AppState>,
    document_id: String,
    version_id: String,
    notes: Option<String>,
) -> AppResult<DocumentVersionRow> {
    let root = require_root(&state).await?;
    let source_files = list_version_files(state.clone(), version_id.clone()).await?;
    if source_files.is_empty() {
        return Err(AppError::msg(
            "La versión a restaurar no tiene archivos asociados",
        ));
    }

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

    for f in source_files {
        let abs = join_under_root(&root, &f.relative_path)?;
        if abs.is_file() {
            let _ = attach_file_inner(
                &state,
                &root,
                &new_id,
                &abs.display().to_string(),
                f.format_label,
                None,
                true,
            )
            .await?;
        }
    }

    let versions = list_document_versions(state, document_id).await?;
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
    let root = require_root(&state).await?;
    let row: (String, String, i64) = sqlx::query_as(
        "SELECT f.relative_path, f.file_name, COALESCE(v.is_snapshot, 0)
         FROM document_files f
         JOIN document_versions v ON v.id = f.document_version_id
         WHERE f.id = ?",
    )
    .bind(&file_id)
    .fetch_optional(&state.pool)
    .await?
    .ok_or_else(|| AppError::msg("Archivo de documento no encontrado"))?;

    let (rel, file_name, is_snapshot) = row;
    let src = join_under_root(&root, &rel)?;
    if !src.is_file() {
        return Err(AppError::msg("El archivo físico no está disponible"));
    }

    let force_temp = use_temp_copy.unwrap_or(is_snapshot == 1);
    let open_path = if force_temp {
        let tmp = temp_open_dir(&root).join(Uuid::new_v4().to_string());
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
    let root = require_root(&state).await?;
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

    // Attach multiple formats to the linked version (or current) without duplicating document rows
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
                &root,
                &version_id,
                path,
                label,
                Some(id.clone()),
                true,
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
    let root = require_root(&state).await?;
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
    enrich_change_flags(&state, &root, docs).await
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
