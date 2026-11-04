//! «Reparar historial» — collapse false SAP model versions (aux/.sbk).
//!
//! Keeps versions whose principal is an available `.sdb`. Drops versions that
//! only have `.sbk`/aux or no principal. Preview required; SQLite backup via
//! `VACUUM INTO` before apply. Never deletes physical files on disk.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use chrono::Local;
use serde::{Deserialize, Serialize};
use sqlx::{Sqlite, SqlitePool, Transaction};
use uuid::Uuid;

use crate::doc_codes::version_label;
use crate::engineering_files::{is_sap_auxiliary_extension, is_sap_principal_extension};
use crate::error::{AppError, AppResult};
use crate::project_paths;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepairModelHistoryInput {
    pub project_id: String,
    /// When set, repair only this document; otherwise all modelos docs in project.
    pub document_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepairVersionDecision {
    pub version_id: String,
    pub version_number: i64,
    pub label: Option<String>,
    pub keep: bool,
    pub reason: String,
    pub principal_file_name: Option<String>,
    pub principal_extension: Option<String>,
    pub relative_path: Option<String>,
    pub new_version_number: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepairModelHistoryDocPreview {
    pub document_id: String,
    pub business_code: Option<String>,
    pub title: String,
    pub versions_before: i64,
    pub versions_after: i64,
    pub decisions: Vec<RepairVersionDecision>,
    pub new_current_version_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepairModelHistoryPreview {
    pub project_id: String,
    pub documents: Vec<RepairModelHistoryDocPreview>,
    pub total_versions_before: i64,
    pub total_versions_after: i64,
    pub versions_to_drop: i64,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepairModelHistoryResult {
    pub project_id: String,
    pub documents_repaired: usize,
    pub versions_kept: usize,
    pub versions_removed: usize,
    pub db_backup_path: String,
    pub documents: Vec<RepairModelHistoryDocPreview>,
}

#[derive(Debug, Clone)]
struct VersionRow {
    id: String,
    version_number: i64,
    label: Option<String>,
}

#[derive(Debug, Clone)]
struct FileRow {
    id: String,
    document_version_id: String,
    relative_path: String,
    file_name: String,
    extension: Option<String>,
    file_role: Option<String>,
    is_available: i64,
    sheet_kind: Option<String>,
}

fn ext_of(f: &FileRow) -> String {
    f.extension
        .as_deref()
        .map(|e| e.trim_start_matches('.').to_ascii_lowercase())
        .filter(|e| !e.is_empty())
        .unwrap_or_else(|| {
            Path::new(&f.file_name)
                .extension()
                .map(|e| e.to_string_lossy().to_ascii_lowercase())
                .unwrap_or_default()
        })
}

fn is_sdb_file(f: &FileRow) -> bool {
    if f.is_available == 0 {
        return false;
    }
    let ext = ext_of(f);
    is_sap_principal_extension(&ext) || f.file_name.to_ascii_lowercase().ends_with(".sdb")
}

fn is_aux_or_sbk(f: &FileRow) -> bool {
    let ext = ext_of(f);
    is_sap_auxiliary_extension(&ext)
}

fn pick_principal<'a>(files: &'a [FileRow]) -> Option<&'a FileRow> {
    let available: Vec<_> = files.iter().filter(|f| f.is_available != 0).collect();
    if available.is_empty() {
        return None;
    }
    // Prefer principal-role .sdb, then any .sdb, then any principal-role, else first.
    available
        .iter()
        .find(|f| {
            is_sdb_file(f)
                && f.file_role
                    .as_deref()
                    .map(|r| r == "principal")
                    .unwrap_or(false)
        })
        .or_else(|| available.iter().find(|f| is_sdb_file(f)))
        .or_else(|| {
            available.iter().find(|f| {
                f.file_role
                    .as_deref()
                    .map(|r| r == "principal" || r == "standalone")
                    .unwrap_or(true)
                    && f.sheet_kind.is_none()
            })
        })
        .copied()
}

async fn list_modelos_docs(
    pool: &SqlitePool,
    project_id: &str,
    document_id: Option<&str>,
) -> AppResult<Vec<(String, Option<String>, String)>> {
    if let Some(did) = document_id {
        let row: Option<(String, Option<String>, String, Option<String>)> = sqlx::query_as(
            "SELECT d.id, d.business_code, d.title, c.key
             FROM documents d
             LEFT JOIN categories c ON c.id = d.category_id
             WHERE d.id = ? AND d.project_id = ?",
        )
        .bind(did)
        .bind(project_id)
        .fetch_optional(pool)
        .await?;
        let Some((id, code, title, key)) = row else {
            return Err(AppError::msg("Documento no encontrado en el proyecto"));
        };
        if key.as_deref() != Some("modelos") {
            return Err(AppError::msg(
                "«Reparar historial» solo aplica a documentos de categoría Modelos",
            ));
        }
        return Ok(vec![(id, code, title)]);
    }

    let rows: Vec<(String, Option<String>, String)> = sqlx::query_as(
        "SELECT d.id, d.business_code, d.title
         FROM documents d
         JOIN categories c ON c.id = d.category_id
         WHERE d.project_id = ? AND c.key = 'modelos'
         ORDER BY COALESCE(d.business_code, d.title)",
    )
    .bind(project_id)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

async fn load_versions(pool: &SqlitePool, document_id: &str) -> AppResult<Vec<VersionRow>> {
    let rows: Vec<(String, i64, Option<String>)> = sqlx::query_as(
        "SELECT id, version_number, label FROM document_versions
         WHERE document_id = ? ORDER BY version_number ASC",
    )
    .bind(document_id)
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|(id, version_number, label)| VersionRow {
            id,
            version_number,
            label,
        })
        .collect())
}

async fn load_files_for_versions(
    pool: &SqlitePool,
    version_ids: &[String],
) -> AppResult<Vec<FileRow>> {
    if version_ids.is_empty() {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    for vid in version_ids {
        let rows: Vec<(
            String,
            String,
            String,
            String,
            Option<String>,
            Option<String>,
            i64,
            Option<String>,
        )> = sqlx::query_as(
            "SELECT id, document_version_id, relative_path, file_name, extension,
                    file_role, is_available, sheet_kind
             FROM document_files WHERE document_version_id = ?",
        )
        .bind(vid)
        .fetch_all(pool)
        .await?;
        for (id, document_version_id, relative_path, file_name, extension, file_role, is_available, sheet_kind) in
            rows
        {
            out.push(FileRow {
                id,
                document_version_id,
                relative_path,
                file_name,
                extension,
                file_role,
                is_available,
                sheet_kind,
            });
        }
    }
    Ok(out)
}

fn decide_versions(
    versions: &[VersionRow],
    files: &[FileRow],
) -> (Vec<RepairVersionDecision>, Option<String>) {
    let mut decisions = Vec::new();
    let mut seen_sdb_paths: HashSet<String> = HashSet::new();
    let mut keep_ids: Vec<String> = Vec::new();
    let mut next_n = 1i64;

    for v in versions {
        let vfiles: Vec<_> = files
            .iter()
            .filter(|f| f.document_version_id == v.id)
            .cloned()
            .collect();
        let principal = pick_principal(&vfiles);
        let (keep, reason, pname, pext, prel) = match principal {
            Some(p) if is_sdb_file(p) => {
                let norm = p.relative_path.replace('\\', "/").to_ascii_lowercase();
                if seen_sdb_paths.contains(&norm) {
                    (
                        false,
                        "duplicate_sdb_path".into(),
                        Some(p.file_name.clone()),
                        Some(ext_of(p)),
                        Some(p.relative_path.clone()),
                    )
                } else {
                    seen_sdb_paths.insert(norm);
                    (
                        true,
                        "principal_sdb".into(),
                        Some(p.file_name.clone()),
                        Some(ext_of(p)),
                        Some(p.relative_path.clone()),
                    )
                }
            }
            Some(p) if is_aux_or_sbk(p) || !is_sdb_file(p) => (
                false,
                "false_aux_or_non_sdb".into(),
                Some(p.file_name.clone()),
                Some(ext_of(p)),
                Some(p.relative_path.clone()),
            ),
            Some(p) => (
                false,
                "no_sdb_principal".into(),
                Some(p.file_name.clone()),
                Some(ext_of(p)),
                Some(p.relative_path.clone()),
            ),
            None => (false, "empty_or_unavailable".into(), None, None, None),
        };

        let new_n = if keep {
            let n = next_n;
            next_n += 1;
            keep_ids.push(v.id.clone());
            Some(n)
        } else {
            None
        };

        decisions.push(RepairVersionDecision {
            version_id: v.id.clone(),
            version_number: v.version_number,
            label: v.label.clone(),
            keep,
            reason,
            principal_file_name: pname,
            principal_extension: pext,
            relative_path: prel,
            new_version_number: new_n,
        });
    }

    let new_current = keep_ids.last().cloned();
    (decisions, new_current)
}

pub async fn preview_repair_model_history_inner(
    pool: &SqlitePool,
    input: RepairModelHistoryInput,
) -> AppResult<RepairModelHistoryPreview> {
    let docs = list_modelos_docs(pool, &input.project_id, input.document_id.as_deref()).await?;
    let mut documents = Vec::new();
    let mut total_before = 0i64;
    let mut total_after = 0i64;
    let mut warnings = Vec::new();

    if docs.is_empty() {
        warnings.push("No hay documentos de Modelos en el proyecto".into());
    }

    for (id, code, title) in docs {
        let versions = load_versions(pool, &id).await?;
        let vids: Vec<String> = versions.iter().map(|v| v.id.clone()).collect();
        let files = load_files_for_versions(pool, &vids).await?;
        let (decisions, new_current) = decide_versions(&versions, &files);
        let before = versions.len() as i64;
        let after = decisions.iter().filter(|d| d.keep).count() as i64;
        total_before += before;
        total_after += after;
        if after == 0 && before > 0 {
            warnings.push(format!(
                "{}: ninguna versión .sdb conservable — no se aplicará borrado total automático",
                code.clone().unwrap_or_else(|| title.clone())
            ));
        }
        documents.push(RepairModelHistoryDocPreview {
            document_id: id,
            business_code: code,
            title,
            versions_before: before,
            versions_after: after,
            decisions,
            new_current_version_id: new_current,
        });
    }

    Ok(RepairModelHistoryPreview {
        project_id: input.project_id,
        documents,
        total_versions_before: total_before,
        total_versions_after: total_after,
        versions_to_drop: (total_before - total_after).max(0),
        warnings,
    })
}

async fn snapshot_db(pool: &SqlitePool, dest: &Path) -> AppResult<()> {
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent)?;
    }
    if dest.exists() {
        fs::remove_file(dest)?;
    }
    let dest_str = dest.display().to_string().replace('\'', "''");
    sqlx::query(&format!("VACUUM INTO '{dest_str}'"))
        .execute(pool)
        .await
        .map_err(|e| AppError::msg(format!("VACUUM INTO falló: {e}")))?;
    Ok(())
}

pub async fn backup_db_before_repair(
    pool: &SqlitePool,
    db_path: &Path,
    project_id: &str,
) -> AppResult<String> {
    let stamp = Local::now().format("%Y%m%d-%H%M%S").to_string();
    let dest = match project_paths::resolve_project_location(pool, project_id).await {
        Ok(loc) => loc
            .root_path
            .join(".ingmanager")
            .join("db_backups")
            .join(format!("repair-history-{stamp}.sqlite")),
        Err(_) => {
            let parent = db_path
                .parent()
                .map(Path::to_path_buf)
                .unwrap_or_else(|| PathBuf::from("."));
            parent
                .join("db_backups")
                .join(format!("repair-history-{stamp}.sqlite"))
        }
    };
    snapshot_db(pool, &dest).await?;
    Ok(dest.display().to_string())
}

async fn delete_version_cascade(
    tx: &mut Transaction<'_, Sqlite>,
    version_id: &str,
) -> AppResult<()> {
    // Sheets / files / tracking / pending references — SQLite rows only.
    sqlx::query("DELETE FROM file_version_tracking WHERE document_version_id = ?")
        .bind(version_id)
        .execute(&mut **tx)
        .await?;
    sqlx::query("UPDATE revisions SET document_version_id = NULL WHERE document_version_id = ?")
        .bind(version_id)
        .execute(&mut **tx)
        .await
        .ok();
    sqlx::query("DELETE FROM document_files WHERE document_version_id = ?")
        .bind(version_id)
        .execute(&mut **tx)
        .await?;
    sqlx::query("DELETE FROM document_versions WHERE id = ?")
        .bind(version_id)
        .execute(&mut **tx)
        .await?;
    Ok(())
}

async fn apply_doc_repair(
    tx: &mut Transaction<'_, Sqlite>,
    pool: &SqlitePool,
    project_id: &str,
    doc: &RepairModelHistoryDocPreview,
) -> AppResult<(usize, usize)> {
    if doc.versions_after == 0 {
        // Refuse to wipe all versions of a document.
        return Ok((0, 0));
    }

    let drop_ids: Vec<String> = doc
        .decisions
        .iter()
        .filter(|d| !d.keep)
        .map(|d| d.version_id.clone())
        .collect();
    let keep: Vec<&RepairVersionDecision> = doc.decisions.iter().filter(|d| d.keep).collect();

    // Clear current pointer before deletes / renumber.
    sqlx::query("UPDATE documents SET current_version_id = NULL WHERE id = ?")
        .bind(&doc.document_id)
        .execute(&mut **tx)
        .await?;

    for id in &drop_ids {
        delete_version_cascade(tx, id).await?;
    }

    // Renumber keepers with temp ids to avoid UNIQUE (document_id, version_number) clashes.
    for (i, d) in keep.iter().enumerate() {
        let temp_n = -1_000_000 - (i as i64);
        sqlx::query("UPDATE document_versions SET version_number = ? WHERE id = ?")
            .bind(temp_n)
            .bind(&d.version_id)
            .execute(&mut **tx)
            .await?;
    }
    for d in &keep {
        let new_n = d.new_version_number.unwrap_or(d.version_number);
        sqlx::query(
            "UPDATE document_versions SET version_number = ?, label = ? WHERE id = ?",
        )
        .bind(new_n)
        .bind(version_label(new_n))
        .bind(&d.version_id)
        .execute(&mut **tx)
        .await?;
    }

    let new_current = doc
        .new_current_version_id
        .clone()
        .or_else(|| keep.last().map(|d| d.version_id.clone()));
    if let Some(ref cid) = new_current {
        sqlx::query(
            "UPDATE documents SET current_version_id = ?, updated_at = datetime('now') WHERE id = ?",
        )
        .bind(cid)
        .bind(&doc.document_id)
        .execute(&mut **tx)
        .await?;
    }

    // Refresh tracking baselines for kept .sdb principals (best-effort; needs absolute paths).
    if let Ok(loc) = project_paths::resolve_project_location(pool, project_id).await {
        for d in &keep {
            if let Some(rel) = &d.relative_path {
                let abs = loc.root_path.join(rel.replace('\\', "/"));
                if abs.is_file() {
                    if let Ok((fp, size, modified)) =
                        crate::auto_version::fingerprint_meta(&abs)
                    {
                        let parsed = crate::auto_version::parse_version_filename(
                            &abs.file_name()
                                .map(|n| n.to_string_lossy().to_string())
                                .unwrap_or_default(),
                        );
                        let track_id = Uuid::new_v4().to_string();
                        let abs_str = abs.display().to_string();
                        sqlx::query(
                            "INSERT INTO file_version_tracking (
                                id, project_id, document_id, document_version_id, absolute_path,
                                base_stem, last_fingerprint, last_size_bytes, last_modified_at, updated_at
                             ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, datetime('now'))
                             ON CONFLICT(project_id, absolute_path) DO UPDATE SET
                                document_id = excluded.document_id,
                                document_version_id = excluded.document_version_id,
                                base_stem = excluded.base_stem,
                                last_fingerprint = excluded.last_fingerprint,
                                last_size_bytes = excluded.last_size_bytes,
                                last_modified_at = excluded.last_modified_at,
                                updated_at = datetime('now')",
                        )
                        .bind(&track_id)
                        .bind(project_id)
                        .bind(&doc.document_id)
                        .bind(&d.version_id)
                        .bind(&abs_str)
                        .bind(&parsed.base_stem)
                        .bind(&fp)
                        .bind(size as i64)
                        .bind(&modified)
                        .execute(&mut **tx)
                        .await
                        .ok();
                    }
                }
            }
        }
    }

    // Drop leftover tracking rows that pointed at deleted aux paths for this document.
    sqlx::query(
        "DELETE FROM file_version_tracking
         WHERE document_id = ?
           AND (document_version_id IS NULL
                OR document_version_id NOT IN (
                    SELECT id FROM document_versions WHERE document_id = ?
                ))",
    )
    .bind(&doc.document_id)
    .bind(&doc.document_id)
    .execute(&mut **tx)
    .await?;

    Ok((keep.len(), drop_ids.len()))
}

pub async fn apply_repair_model_history_inner(
    pool: &SqlitePool,
    db_path: &Path,
    input: RepairModelHistoryInput,
) -> AppResult<RepairModelHistoryResult> {
    let preview = preview_repair_model_history_inner(pool, input.clone()).await?;
    if preview.versions_to_drop == 0 {
        return Err(AppError::msg(
            "No hay versiones falsas que reparar (historial ya limpio)",
        ));
    }
    if preview
        .documents
        .iter()
        .all(|d| d.versions_after == 0 || d.versions_before == d.versions_after)
    {
        return Err(AppError::msg(
            "Ningún documento tiene un plan de reparación seguro",
        ));
    }

    let backup_path = backup_db_before_repair(pool, db_path, &input.project_id).await?;

    let mut tx = pool.begin().await?;
    let mut kept = 0usize;
    let mut removed = 0usize;
    let mut repaired = 0usize;

    for doc in &preview.documents {
        if doc.versions_after == 0 || doc.versions_before == doc.versions_after {
            continue;
        }
        let (k, r) = apply_doc_repair(&mut tx, pool, &input.project_id, doc).await?;
        if r > 0 {
            repaired += 1;
            kept += k;
            removed += r;
        }
    }

    tx.commit().await?;

    // Re-preview after apply for accurate post-state.
    let after = preview_repair_model_history_inner(pool, input.clone()).await?;

    Ok(RepairModelHistoryResult {
        project_id: input.project_id,
        documents_repaired: repaired,
        versions_kept: kept,
        versions_removed: removed,
        db_backup_path: backup_path,
        documents: after.documents,
    })
}
