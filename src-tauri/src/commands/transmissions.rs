use std::fs;
use std::path::{Path, PathBuf};

use chrono::Local;
use serde::{Deserialize, Serialize};
use tauri::State;
use uuid::Uuid;
use walkdir::WalkDir;

use crate::doc_codes::next_transmission_code;
use crate::error::{AppError, AppResult};
use crate::fs_util::copy_recursively;
use crate::path_security::{ensure_under_root, join_under_root};
use crate::project_paths::{self, ProjectLocation};
use crate::state::AppState;
use crate::zip_safe::{create_zip_from_dir, extract_zip_safe, sha256_file};

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct TransmissionRow {
    pub id: String,
    pub project_id: String,
    pub direction: String,
    pub code: Option<String>,
    pub subject: Option<String>,
    pub counterpart: Option<String>,
    pub sent_at: Option<String>,
    pub notes: Option<String>,
    pub created_at: String,
    pub medium: Option<String>,
    pub status: String,
    pub folder_relative_path: Option<String>,
    pub zip_relative_path: Option<String>,
    pub zip_checksum: Option<String>,
    pub linked_transmission_id: Option<String>,
    pub original_import_relative_path: Option<String>,
    pub extracted_relative_path: Option<String>,
    pub antecedentes_relative_path: Option<String>,
    pub item_count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct TransmissionItemRow {
    pub id: String,
    pub transmission_id: String,
    pub document_id: Option<String>,
    pub relative_path: Option<String>,
    pub file_name: Option<String>,
    pub created_at: String,
    pub document_version_id: Option<String>,
    pub checksum: Option<String>,
    pub snapshot_relative_path: Option<String>,
    pub is_available: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct TransmissionLinkRow {
    pub id: String,
    pub sent_transmission_id: String,
    pub received_transmission_id: String,
    pub notes: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SendItemInput {
    pub document_id: Option<String>,
    pub document_version_id: Option<String>,
    pub absolute_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateSentTransmissionInput {
    pub project_id: String,
    pub counterpart: String,
    pub subject: String,
    pub medium: Option<String>,
    pub notes: Option<String>,
    pub sent_at: Option<String>, // ISO date YYYY-MM-DD
    pub items: Vec<SendItemInput>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreviewSentItem {
    pub absolute_path: String,
    pub file_name: String,
    pub available: bool,
    pub document_id: Option<String>,
    pub document_version_id: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreviewSentResult {
    pub code: String,
    pub folder_relative_path: String,
    pub items: Vec<PreviewSentItem>,
    pub all_available: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportReceivedInput {
    pub project_id: String,
    pub counterpart: String,
    pub subject: Option<String>,
    pub notes: Option<String>,
    pub received_at: Option<String>, // ISO
    pub source_path: String,         // zip, file, or folder (may be outside root)
    pub extract_to_antecedentes: Option<bool>,
    pub link_to_sent_id: Option<String>,
}

async fn project_loc(state: &AppState, project_id: &str) -> AppResult<ProjectLocation> {
    project_paths::resolve_project_location(&state.pool, project_id).await
}

async fn tx_folder_base(state: &AppState, loc: &ProjectLocation) -> AppResult<String> {
    let cat: Option<(String,)> = sqlx::query_as(
        "SELECT relative_path FROM categories WHERE project_id = ? AND key = 'transmisiones'",
    )
    .bind(&loc.project_id)
    .fetch_optional(&state.pool)
    .await?;
    let tx_rel = cat
        .map(|c| c.0)
        .unwrap_or_else(|| "06_TRANSMISIONES".into());
    Ok(format!("{}/{}", loc.root_relative_path, tx_rel))
}

async fn antecedentes_base(state: &AppState, loc: &ProjectLocation) -> AppResult<String> {
    let cat: Option<(String,)> = sqlx::query_as(
        "SELECT relative_path FROM categories WHERE project_id = ? AND key = 'antecedentes'",
    )
    .bind(&loc.project_id)
    .fetch_optional(&state.pool)
    .await?;
    let ant_rel = cat
        .map(|c| c.0)
        .unwrap_or_else(|| "01_ANTECEDENTES".into());
    Ok(format!("{}/{}", loc.root_relative_path, ant_rel))
}

async fn existing_tx_codes(state: &AppState, project_id: &str) -> AppResult<Vec<String>> {
    let rows: Vec<(Option<String>,)> =
        sqlx::query_as("SELECT code FROM transmissions WHERE project_id = ?")
            .bind(project_id)
            .fetch_all(&state.pool)
            .await?;
    Ok(rows.into_iter().filter_map(|r| r.0).collect())
}

fn today_iso() -> String {
    Local::now().format("%Y-%m-%d").to_string()
}

fn rel_from_root(root: &Path, abs: &Path) -> AppResult<String> {
    Ok(abs
        .strip_prefix(root)
        .map_err(|_| AppError::msg("Ruta fuera de la raíz"))?
        .to_string_lossy()
        .replace('\\', "/"))
}

fn transmission_select_sql() -> &'static str {
    "SELECT
        t.id, t.project_id, t.direction, t.code, t.subject, t.counterpart, t.sent_at,
        t.notes, t.created_at, t.medium, t.status, t.folder_relative_path,
        t.zip_relative_path, t.zip_checksum, t.linked_transmission_id,
        t.original_import_relative_path, t.extracted_relative_path,
        t.antecedentes_relative_path,
        COALESCE((SELECT COUNT(*) FROM transmission_items i WHERE i.transmission_id = t.id), 0) AS item_count
     FROM transmissions t"
}

async fn fetch_transmission(state: &AppState, id: &str) -> AppResult<TransmissionRow> {
    sqlx::query_as::<_, TransmissionRow>(&format!(
        "{} WHERE t.id = ?",
        transmission_select_sql()
    ))
    .bind(id)
    .fetch_optional(&state.pool)
    .await?
    .ok_or_else(|| AppError::msg("Transmisión no encontrada"))
}

#[tauri::command]
pub async fn list_transmissions(
    state: State<'_, AppState>,
    project_id: String,
    direction: Option<String>,
) -> AppResult<Vec<TransmissionRow>> {
    if let Some(dir) = direction {
        sqlx::query_as::<_, TransmissionRow>(&format!(
            "{} WHERE t.project_id = ? AND t.direction = ? ORDER BY t.created_at DESC",
            transmission_select_sql()
        ))
        .bind(project_id)
        .bind(dir)
        .fetch_all(&state.pool)
        .await
        .map_err(AppError::from)
    } else {
        sqlx::query_as::<_, TransmissionRow>(&format!(
            "{} WHERE t.project_id = ? ORDER BY t.created_at DESC",
            transmission_select_sql()
        ))
        .bind(project_id)
        .fetch_all(&state.pool)
        .await
        .map_err(AppError::from)
    }
}

#[tauri::command]
pub async fn get_transmission(
    state: State<'_, AppState>,
    id: String,
) -> AppResult<TransmissionRow> {
    fetch_transmission(&state, &id).await
}

#[tauri::command]
pub async fn list_transmission_items(
    state: State<'_, AppState>,
    transmission_id: String,
) -> AppResult<Vec<TransmissionItemRow>> {
    sqlx::query_as::<_, TransmissionItemRow>(
        "SELECT id, transmission_id, document_id, relative_path, file_name, created_at,
                document_version_id, checksum, snapshot_relative_path, is_available
         FROM transmission_items
         WHERE transmission_id = ?
         ORDER BY file_name COLLATE NOCASE",
    )
    .bind(transmission_id)
    .fetch_all(&state.pool)
    .await
    .map_err(AppError::from)
}

#[tauri::command]
pub async fn preview_next_transmission_code(
    state: State<'_, AppState>,
    project_id: String,
    kind: String,
) -> AppResult<String> {
    let loc = project_loc(&state, &project_id).await?;
    let existing = existing_tx_codes(&state, &project_id).await?;
    next_transmission_code(&loc.business_code_prefix, &kind, &existing)
}

#[tauri::command]
pub async fn preview_sent_transmission(
    state: State<'_, AppState>,
    input: CreateSentTransmissionInput,
) -> AppResult<PreviewSentResult> {
    let loc = project_loc(&state, &input.project_id).await?;
    let existing = existing_tx_codes(&state, &input.project_id).await?;
    let code = next_transmission_code(&loc.business_code_prefix, "ENV", &existing)?;
    let date = input.sent_at.clone().unwrap_or_else(today_iso);
    let tx_base = tx_folder_base(&state, &loc).await?;
    let folder_rel = format!("{tx_base}/ENVIADOS/{date}/{code}");

    let mut items = Vec::new();
    let mut all_ok = true;
    for it in &input.items {
        let path = PathBuf::from(&it.absolute_path);
        let under = ensure_under_root(&loc.root_path, &path);
        let available = under.as_ref().map(|p| p.is_file()).unwrap_or(false);
        let file_name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| it.absolute_path.clone());
        let error = if available {
            None
        } else {
            all_ok = false;
            Some(
                under
                    .err()
                    .map(|e| e.to_string())
                    .unwrap_or_else(|| "Archivo no disponible".into()),
            )
        };
        items.push(PreviewSentItem {
            absolute_path: it.absolute_path.clone(),
            file_name,
            available,
            document_id: it.document_id.clone(),
            document_version_id: it.document_version_id.clone(),
            error,
        });
    }

    Ok(PreviewSentResult {
        code,
        folder_relative_path: folder_rel,
        items,
        all_available: all_ok && !input.items.is_empty(),
    })
}

#[tauri::command]
pub async fn create_sent_transmission(
    state: State<'_, AppState>,
    input: CreateSentTransmissionInput,
) -> AppResult<TransmissionRow> {
    if input.counterpart.trim().is_empty() {
        return Err(AppError::msg("El destinatario es obligatorio"));
    }
    if input.subject.trim().is_empty() {
        return Err(AppError::msg("El asunto es obligatorio"));
    }
    if input.items.is_empty() {
        return Err(AppError::msg("Debe incluir al menos un archivo/documento"));
    }

    let loc = project_loc(&state, &input.project_id).await?;
    let root = loc.root_path.clone();
    let preview = preview_sent_transmission(state.clone(), input.clone()).await?;
    if !preview.all_available {
        return Err(AppError::msg(
            "No se puede registrar el envío: hay archivos no disponibles. Corrija y reintente.",
        ));
    }

    let date = input.sent_at.clone().unwrap_or_else(today_iso);
    let folder = join_under_root(&root, &preview.folder_relative_path)?;
    let snapshots_dir = folder.join("snapshots");
    let staging = folder.join("_staging");

    // Build everything on disk first; only insert DB row when ZIP is complete.
    if folder.exists() {
        return Err(AppError::msg(format!(
            "Ya existe la carpeta de transmisión {}",
            preview.folder_relative_path
        )));
    }

    let cleanup = |folder: &Path| {
        let _ = fs::remove_dir_all(folder);
    };

    if let Err(e) = fs::create_dir_all(&snapshots_dir) {
        cleanup(&folder);
        return Err(AppError::from(e));
    }
    if let Err(e) = fs::create_dir_all(&staging) {
        cleanup(&folder);
        return Err(e.into());
    }

    let mut staged_items: Vec<(SendItemInput, String, String, String)> = Vec::new();
    // (input, snap_rel, file_name, checksum)

    for it in &input.items {
        let src = match ensure_under_root(&root, Path::new(&it.absolute_path)) {
            Ok(p) if p.is_file() => p,
            _ => {
                cleanup(&folder);
                return Err(AppError::msg(format!(
                    "Archivo no disponible: {}",
                    it.absolute_path
                )));
            }
        };
        let file_name = src
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "file".into());
        let dest = snapshots_dir.join(&file_name);
        let dest = if dest.exists() {
            crate::fs_util::unique_destination(&snapshots_dir, &file_name)
        } else {
            dest
        };
        if let Err(e) = fs::copy(&src, &dest) {
            cleanup(&folder);
            return Err(e.into());
        }
        // Also stage flat copy for ZIP package root content
        let stage_dest = staging.join(dest.file_name().unwrap());
        if let Err(e) = fs::copy(&dest, &stage_dest) {
            cleanup(&folder);
            return Err(e.into());
        }
        let checksum = match sha256_file(&dest) {
            Ok(c) => c,
            Err(e) => {
                cleanup(&folder);
                return Err(e);
            }
        };
        let snap_rel = match rel_from_root(&root, &dest) {
            Ok(r) => r,
            Err(e) => {
                cleanup(&folder);
                return Err(e);
            }
        };
        staged_items.push((it.clone(), snap_rel, file_name, checksum));
    }

    // Write manifest
    let manifest = serde_json::json!({
        "code": preview.code,
        "direction": "sent",
        "counterpart": input.counterpart,
        "subject": input.subject,
        "date": date,
        "medium": input.medium,
        "items": staged_items.iter().map(|(_, rel, name, sum)| {
            serde_json::json!({"file_name": name, "snapshot": rel, "sha256": sum})
        }).collect::<Vec<_>>(),
    });
    if let Err(e) = fs::write(
        staging.join("MANIFEST.json"),
        serde_json::to_string_pretty(&manifest).unwrap_or_default(),
    ) {
        cleanup(&folder);
        return Err(e.into());
    }

    let zip_name = format!("{}.zip", preview.code);
    let zip_path = folder.join(&zip_name);
    if let Err(e) = create_zip_from_dir(&staging, &zip_path) {
        cleanup(&folder);
        return Err(e);
    }
    let zip_checksum = match sha256_file(&zip_path) {
        Ok(c) => c,
        Err(e) => {
            cleanup(&folder);
            return Err(e);
        }
    };
    let zip_rel = rel_from_root(&root, &zip_path)?;
    let folder_rel = preview.folder_relative_path.clone();

    // Persist originals untouched — we only copied.
    let tx_id = Uuid::new_v4().to_string();
    let mut db_tx = state.pool.begin().await?;

    sqlx::query(
        "INSERT INTO transmissions (
            id, project_id, direction, code, subject, counterpart, sent_at, notes,
            created_at, medium, status, folder_relative_path, zip_relative_path, zip_checksum
         ) VALUES (
            ?, ?, 'sent', ?, ?, ?, ?, ?, datetime('now'), ?, 'complete', ?, ?, ?
         )",
    )
    .bind(&tx_id)
    .bind(&input.project_id)
    .bind(&preview.code)
    .bind(input.subject.trim())
    .bind(input.counterpart.trim())
    .bind(&date)
    .bind(&input.notes)
    .bind(&input.medium)
    .bind(&folder_rel)
    .bind(&zip_rel)
    .bind(&zip_checksum)
    .execute(&mut *db_tx)
    .await?;

    for (it, snap_rel, file_name, checksum) in &staged_items {
        sqlx::query(
            "INSERT INTO transmission_items (
                id, transmission_id, document_id, relative_path, file_name, created_at,
                document_version_id, checksum, snapshot_relative_path, is_available
             ) VALUES (?, ?, ?, ?, ?, datetime('now'), ?, ?, ?, 1)",
        )
        .bind(Uuid::new_v4().to_string())
        .bind(&tx_id)
        .bind(&it.document_id)
        .bind(snap_rel)
        .bind(file_name)
        .bind(&it.document_version_id)
        .bind(checksum)
        .bind(snap_rel)
        .execute(&mut *db_tx)
        .await?;
    }

    sqlx::query(
        "INSERT INTO file_operations (id, project_id, operation, source_path, target_path, status, detail, created_at, completed_at)
         VALUES (?, ?, 'transmission_sent', NULL, ?, 'completed', ?, datetime('now'), datetime('now'))",
    )
    .bind(Uuid::new_v4().to_string())
    .bind(&input.project_id)
    .bind(&zip_rel)
    .bind(format!("ZIP sha256={zip_checksum}"))
    .execute(&mut *db_tx)
    .await?;

    db_tx.commit().await?;

    // Remove staging (keep snapshots + zip)
    let _ = fs::remove_dir_all(&staging);

    fetch_transmission(&state, &tx_id).await
}

#[tauri::command]
pub async fn import_received_transmission(
    state: State<'_, AppState>,
    input: ImportReceivedInput,
) -> AppResult<TransmissionRow> {
    if input.counterpart.trim().is_empty() {
        return Err(AppError::msg("El remitente es obligatorio"));
    }
    let loc = project_loc(&state, &input.project_id).await?;
    let root = loc.root_path.clone();
    let source = PathBuf::from(&input.source_path);
    if !source.exists() {
        return Err(AppError::msg("La ruta de origen no existe"));
    }

    let existing = existing_tx_codes(&state, &input.project_id).await?;
    let code = next_transmission_code(&loc.business_code_prefix, "REC", &existing)?;
    let date = input.received_at.clone().unwrap_or_else(today_iso);
    let tx_base = tx_folder_base(&state, &loc).await?;
    let folder_rel = format!("{tx_base}/RECIBIDOS/{date}/{code}");
    let folder = join_under_root(&root, &folder_rel)?;

    if folder.exists() {
        return Err(AppError::msg("Ya existe la carpeta de recepción"));
    }
    fs::create_dir_all(folder.join("original"))?;
    fs::create_dir_all(folder.join("extracted"))?;

    let cleanup = |folder: &Path| {
        let _ = fs::remove_dir_all(folder);
    };

    // Preserve original
    let original_dest = if source.is_dir() {
        let dest = folder.join("original").join(
            source
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| "import".into()),
        );
        if let Err(e) = copy_recursively(&source, &dest) {
            cleanup(&folder);
            return Err(e);
        }
        dest
    } else {
        let name = source
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "import.bin".into());
        let dest = folder.join("original").join(name);
        if let Err(e) = fs::copy(&source, &dest) {
            cleanup(&folder);
            return Err(e.into());
        }
        dest
    };

    let mut extracted_files: Vec<PathBuf> = Vec::new();
    let extracted_dir = folder.join("extracted");
    let is_zip = source
        .extension()
        .map(|e| e.to_string_lossy().eq_ignore_ascii_case("zip"))
        .unwrap_or(false)
        || original_dest
            .extension()
            .map(|e| e.to_string_lossy().eq_ignore_ascii_case("zip"))
            .unwrap_or(false);

    if is_zip {
        match extract_zip_safe(&original_dest, &extracted_dir) {
            Ok(files) => extracted_files = files,
            Err(e) => {
                cleanup(&folder);
                return Err(e);
            }
        }
    } else if source.is_dir() {
        if let Err(e) = copy_recursively(&source, &extracted_dir) {
            cleanup(&folder);
            return Err(e);
        }
        for entry in WalkDir::new(&extracted_dir).into_iter().filter_map(|e| e.ok()) {
            if entry.path().is_file() {
                extracted_files.push(entry.path().to_path_buf());
            }
        }
    } else {
        let name = original_dest.file_name().unwrap();
        let dest = extracted_dir.join(name);
        if let Err(e) = fs::copy(&original_dest, &dest) {
            cleanup(&folder);
            return Err(e.into());
        }
        extracted_files.push(dest);
    }

    let mut antecedentes_rel = None;
    if input.extract_to_antecedentes.unwrap_or(true) && !extracted_files.is_empty() {
        let ant_base = antecedentes_base(&state, &loc).await?;
        let ant = join_under_root(
            &root,
            &format!("{ant_base}/RECIBIDOS/{date}/{code}"),
        )?;
        if let Err(e) = fs::create_dir_all(&ant) {
            cleanup(&folder);
            return Err(e.into());
        }
        for f in &extracted_files {
            if !f.is_file() {
                continue;
            }
            let name = f.file_name().unwrap();
            let dest = ant.join(name);
            let dest = if dest.exists() {
                crate::fs_util::unique_destination(&ant, &name.to_string_lossy())
            } else {
                dest
            };
            if let Err(e) = fs::copy(f, &dest) {
                cleanup(&folder);
                return Err(e.into());
            }
        }
        antecedentes_rel = Some(rel_from_root(&root, &ant)?);
    }

    let zip_rel;
    let zip_checksum;
    if is_zip {
        zip_rel = Some(rel_from_root(&root, &original_dest)?);
        zip_checksum = Some(sha256_file(&original_dest)?);
    } else {
        // Package extracted content into a registry ZIP for consistency
        let zip_path = folder.join(format!("{code}.zip"));
        if let Err(e) = create_zip_from_dir(&extracted_dir, &zip_path) {
            cleanup(&folder);
            return Err(e);
        }
        zip_rel = Some(rel_from_root(&root, &zip_path)?);
        zip_checksum = Some(sha256_file(&zip_path)?);
    }

    let original_rel = rel_from_root(&root, &original_dest)?;
    let extracted_rel = rel_from_root(&root, &extracted_dir)?;
    let tx_id = Uuid::new_v4().to_string();

    let mut db_tx = state.pool.begin().await?;
    sqlx::query(
        "INSERT INTO transmissions (
            id, project_id, direction, code, subject, counterpart, sent_at, notes,
            created_at, medium, status, folder_relative_path, zip_relative_path, zip_checksum,
            original_import_relative_path, extracted_relative_path, antecedentes_relative_path,
            linked_transmission_id
         ) VALUES (
            ?, ?, 'received', ?, ?, ?, ?, ?, datetime('now'), ?, 'complete', ?, ?, ?,
            ?, ?, ?, ?
         )",
    )
    .bind(&tx_id)
    .bind(&input.project_id)
    .bind(&code)
    .bind(
        input
            .subject
            .as_deref()
            .unwrap_or("Recepción de documentación")
            .trim(),
    )
    .bind(input.counterpart.trim())
    .bind(&date)
    .bind(&input.notes)
    .bind(Option::<String>::None)
    .bind(&folder_rel)
    .bind(&zip_rel)
    .bind(&zip_checksum)
    .bind(&original_rel)
    .bind(&extracted_rel)
    .bind(&antecedentes_rel)
    .bind(&input.link_to_sent_id)
    .execute(&mut *db_tx)
    .await?;

    for f in &extracted_files {
        if !f.is_file() {
            continue;
        }
        let name = f
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        let rel = rel_from_root(&root, f)?;
        let checksum = sha256_file(f).ok();
        sqlx::query(
            "INSERT INTO transmission_items (
                id, transmission_id, document_id, relative_path, file_name, created_at,
                document_version_id, checksum, snapshot_relative_path, is_available
             ) VALUES (?, ?, NULL, ?, ?, datetime('now'), NULL, ?, ?, 1)",
        )
        .bind(Uuid::new_v4().to_string())
        .bind(&tx_id)
        .bind(&rel)
        .bind(&name)
        .bind(&checksum)
        .bind(&rel)
        .execute(&mut *db_tx)
        .await?;
    }

    if let Some(sent_id) = &input.link_to_sent_id {
        sqlx::query(
            "INSERT INTO transmission_links (id, sent_transmission_id, received_transmission_id, notes)
             VALUES (?, ?, ?, ?)",
        )
        .bind(Uuid::new_v4().to_string())
        .bind(sent_id)
        .bind(&tx_id)
        .bind("Vinculación al importar recepción")
        .execute(&mut *db_tx)
        .await?;
    }

    db_tx.commit().await?;
    fetch_transmission(&state, &tx_id).await
}

#[tauri::command]
pub async fn link_transmissions(
    state: State<'_, AppState>,
    sent_id: String,
    received_id: String,
    notes: Option<String>,
) -> AppResult<TransmissionLinkRow> {
    let sent = fetch_transmission(&state, &sent_id).await?;
    let received = fetch_transmission(&state, &received_id).await?;
    if sent.direction != "sent" || received.direction != "received" {
        return Err(AppError::msg(
            "Solo se pueden vincular ENV (sent) con REC (received)",
        ));
    }
    let id = Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO transmission_links (id, sent_transmission_id, received_transmission_id, notes)
         VALUES (?, ?, ?, ?)
         ON CONFLICT(sent_transmission_id, received_transmission_id) DO UPDATE SET notes = excluded.notes",
    )
    .bind(&id)
    .bind(&sent_id)
    .bind(&received_id)
    .bind(&notes)
    .execute(&state.pool)
    .await?;

    // Keep denormalized pointer on received
    sqlx::query("UPDATE transmissions SET linked_transmission_id = ? WHERE id = ?")
        .bind(&sent_id)
        .bind(&received_id)
        .execute(&state.pool)
        .await?;

    sqlx::query_as::<_, TransmissionLinkRow>(
        "SELECT id, sent_transmission_id, received_transmission_id, notes, created_at
         FROM transmission_links
         WHERE sent_transmission_id = ? AND received_transmission_id = ?",
    )
    .bind(sent_id)
    .bind(received_id)
    .fetch_one(&state.pool)
    .await
    .map_err(AppError::from)
}

#[tauri::command]
pub async fn list_transmission_links(
    state: State<'_, AppState>,
    transmission_id: String,
) -> AppResult<Vec<TransmissionLinkRow>> {
    sqlx::query_as::<_, TransmissionLinkRow>(
        "SELECT id, sent_transmission_id, received_transmission_id, notes, created_at
         FROM transmission_links
         WHERE sent_transmission_id = ? OR received_transmission_id = ?
         ORDER BY created_at DESC",
    )
    .bind(&transmission_id)
    .bind(&transmission_id)
    .fetch_all(&state.pool)
    .await
    .map_err(AppError::from)
}

#[tauri::command]
pub async fn open_transmission_folder(
    state: State<'_, AppState>,
    transmission_id: String,
) -> AppResult<()> {
    let tx = fetch_transmission(&state, &transmission_id).await?;
    let loc = project_loc(&state, &tx.project_id).await?;
    let rel = tx
        .folder_relative_path
        .ok_or_else(|| AppError::msg("La transmisión no tiene carpeta registrada"))?;
    let path = join_under_root(&loc.root_path, &rel)?;
    if !path.is_dir() {
        return Err(AppError::msg("La carpeta de transmisión no existe"));
    }
    open::that(&path).map_err(|e| AppError::msg(format!("No se pudo abrir: {e}")))?;
    Ok(())
}
