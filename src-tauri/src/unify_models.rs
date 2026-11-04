//! Unify duplicate logical model documents into one principal.
//!
//! Keeps principal UUID / business_code / title. Transfers versions, files
//! (principal + auxiliary), calc/export sheets, and relationships from
//! duplicates. Remaps physical `_vNN` hints → sequential version numbers.
//! Never deletes physical `.sdb` (or any other) files on disk.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use chrono::Local;
use serde::{Deserialize, Serialize};
use sqlx::{Sqlite, Transaction};
use uuid::Uuid;

use crate::engineering_files::{classify_file_name, version_hint_from_name};
use crate::error::{AppError, AppResult};
use crate::project_paths;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnifySuggestion {
    pub group_key: String,
    pub document_ids: Vec<String>,
    pub document_labels: Vec<String>,
    pub file_names: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnifyModelsInput {
    pub project_id: String,
    pub principal_document_id: String,
    pub duplicate_document_ids: Vec<String>,
    pub category_key: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnifyDocSummary {
    pub document_id: String,
    pub business_code: Option<String>,
    pub title: String,
    pub version_count: i64,
    pub file_count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnifyVersionPlan {
    pub source_document_id: String,
    pub source_version_id: String,
    pub file_name: String,
    pub relative_path: String,
    pub version_hint: Option<String>,
    pub new_version_number: i64,
    pub new_label: String,
    pub sheet_count: i64,
    pub auxiliary_count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnifyModelsPreview {
    pub principal: UnifyDocSummary,
    pub duplicates: Vec<UnifyDocSummary>,
    pub planned_versions: Vec<UnifyVersionPlan>,
    pub relations_to_rewire: usize,
    pub sheets_to_transfer: usize,
    pub duplicates_to_delete: Vec<String>,
    pub group_key: Option<String>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnifyModelsResult {
    pub principal_document_id: String,
    pub versions_on_principal: usize,
    pub files_transferred: usize,
    pub sheets_transferred: usize,
    pub relations_rewired: usize,
    pub duplicates_deleted: Vec<String>,
    pub db_backup_path: String,
    pub group_key: Option<String>,
}

#[derive(Debug, Clone)]
struct FileRow {
    id: String,
    document_version_id: String,
    relative_path: String,
    file_name: String,
    file_role: Option<String>,
    sheet_kind: Option<String>,
    is_available: i64,
}

#[derive(Debug, Clone)]
struct VersionBundle {
    source_document_id: String,
    source_version_id: String,
    principal: FileRow,
    attachments: Vec<FileRow>,
    hint_sort: (i64, String),
}

fn parse_hint_sort(file_name: &str) -> (i64, String) {
    let hint = version_hint_from_name(file_name).unwrap_or_default();
    let num = hint
        .chars()
        .filter(|c| c.is_ascii_digit())
        .collect::<String>()
        .parse::<i64>()
        .unwrap_or(0);
    (num, file_name.to_ascii_lowercase())
}

fn is_principal_role(role: Option<&str>) -> bool {
    matches!(role, None | Some("principal") | Some("standalone") | Some(""))
}

async fn load_doc_summary(
    pool: &sqlx::SqlitePool,
    document_id: &str,
) -> AppResult<UnifyDocSummary> {
    let row: Option<(String, Option<String>, String)> = sqlx::query_as(
        "SELECT id, business_code, title FROM documents WHERE id = ?",
    )
    .bind(document_id)
    .fetch_optional(pool)
    .await?;
    let Some((id, code, title)) = row else {
        return Err(AppError::msg("Documento no encontrado"));
    };
    let version_count: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM document_versions WHERE document_id = ?")
            .bind(document_id)
            .fetch_one(pool)
            .await?;
    let file_count: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM document_files f
         JOIN document_versions v ON v.id = f.document_version_id
         WHERE v.document_id = ? AND f.is_available = 1",
    )
    .bind(document_id)
    .fetch_one(pool)
    .await?;
    Ok(UnifyDocSummary {
        document_id: id,
        business_code: code,
        title,
        version_count: version_count.0,
        file_count: file_count.0,
    })
}

async fn load_files_for_docs(
    pool: &sqlx::SqlitePool,
    document_ids: &[String],
) -> AppResult<Vec<(String, FileRow)>> {
    let mut out = Vec::new();
    for doc_id in document_ids {
        let rows: Vec<(
            String,
            String,
            String,
            String,
            Option<String>,
            Option<String>,
            i64,
        )> = sqlx::query_as(
            "SELECT f.id, f.document_version_id, f.relative_path, f.file_name,
                    f.file_role, f.sheet_kind, f.is_available
             FROM document_files f
             JOIN document_versions v ON v.id = f.document_version_id
             WHERE v.document_id = ?
             ORDER BY v.version_number, f.file_name COLLATE NOCASE",
        )
        .bind(doc_id)
        .fetch_all(pool)
        .await?;
        for (id, vid, rel, name, role, sheet, avail) in rows {
            out.push((
                doc_id.clone(),
                FileRow {
                    id,
                    document_version_id: vid,
                    relative_path: rel,
                    file_name: name,
                    file_role: role,
                    sheet_kind: sheet,
                    is_available: avail,
                },
            ));
        }
    }
    Ok(out)
}

fn build_bundles(
    files: &[(String, FileRow)],
) -> AppResult<Vec<VersionBundle>> {
    let mut by_version: HashMap<String, (String, Vec<FileRow>)> = HashMap::new();
    for (doc_id, f) in files {
        by_version
            .entry(f.document_version_id.clone())
            .or_insert_with(|| (doc_id.clone(), Vec::new()))
            .1
            .push(f.clone());
    }

    let mut bundles = Vec::new();
    for (vid, (doc_id, rows)) in by_version {
        let principals: Vec<_> = rows
            .iter()
            .filter(|f| {
                f.is_available == 1
                    && f.sheet_kind.is_none()
                    && is_principal_role(f.file_role.as_deref())
            })
            .cloned()
            .collect();
        if principals.is_empty() {
            // Version with only sheets/aux — attach later if orphaned; skip for remap core.
            continue;
        }
        // Prefer .sdb / engineering principals; otherwise first.
        let principal = principals
            .iter()
            .find(|f| {
                f.file_name
                    .rsplit('.')
                    .next()
                    .map(|e| e.eq_ignore_ascii_case("sdb"))
                    .unwrap_or(false)
            })
            .cloned()
            .unwrap_or_else(|| principals[0].clone());
        let attachments: Vec<_> = rows
            .into_iter()
            .filter(|f| f.id != principal.id)
            .collect();
        let hint_sort = parse_hint_sort(&principal.file_name);
        bundles.push(VersionBundle {
            source_document_id: doc_id,
            source_version_id: vid,
            principal,
            attachments,
            hint_sort,
        });
    }

    bundles.sort_by(|a, b| {
        a.hint_sort
            .0
            .cmp(&b.hint_sort.0)
            .then_with(|| a.hint_sort.1.cmp(&b.hint_sort.1))
            .then_with(|| a.principal.relative_path.cmp(&b.principal.relative_path))
    });

    // Deduplicate by relative_path (keep first / lowest hint).
    let mut seen_paths = HashSet::new();
    let mut deduped = Vec::new();
    for b in bundles {
        if !seen_paths.insert(b.principal.relative_path.clone()) {
            continue;
        }
        deduped.push(b);
    }
    Ok(deduped)
}

async fn count_relations_to_rewire(
    pool: &sqlx::SqlitePool,
    principal_id: &str,
    duplicate_ids: &[String],
) -> AppResult<usize> {
    let mut n = 0usize;
    for dup in duplicate_ids {
        let c: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM document_relationships
             WHERE (source_document_id = ? OR target_document_id = ?)
               AND NOT (source_document_id = ? AND target_document_id = ?)",
        )
        .bind(dup)
        .bind(dup)
        .bind(principal_id)
        .bind(principal_id)
        .fetch_one(pool)
        .await?;
        n += c.0 as usize;
    }
    Ok(n)
}

async fn validate_unify_set(
    pool: &sqlx::SqlitePool,
    input: &UnifyModelsInput,
) -> AppResult<(String, Vec<String>, Option<String>)> {
    let category_key = input
        .category_key
        .clone()
        .unwrap_or_else(|| "modelos".into());
    if input.duplicate_document_ids.is_empty() {
        return Err(AppError::msg(
            "Seleccione al menos un documento duplicado para unificar",
        ));
    }
    if input
        .duplicate_document_ids
        .iter()
        .any(|id| id == &input.principal_document_id)
    {
        return Err(AppError::msg(
            "El documento principal no puede estar en la lista de duplicados",
        ));
    }

    let mut all_ids = vec![input.principal_document_id.clone()];
    all_ids.extend(input.duplicate_document_ids.clone());
    all_ids.sort();
    all_ids.dedup();

    let mut project_id = None;
    for id in &all_ids {
        let row: Option<(String, Option<String>)> = sqlx::query_as(
            "SELECT d.project_id, c.key
             FROM documents d
             LEFT JOIN categories c ON c.id = d.category_id
             WHERE d.id = ?",
        )
        .bind(id)
        .fetch_optional(pool)
        .await?;
        let Some((pid, cat)) = row else {
            return Err(AppError::msg(format!("Documento no encontrado: {id}")));
        };
        if pid != input.project_id {
            return Err(AppError::msg(
                "Todos los documentos deben pertenecer al mismo proyecto",
            ));
        }
        if cat.as_deref() != Some(category_key.as_str()) {
            return Err(AppError::msg(
                "Unificar modelos solo aplica a documentos de la categoría Modelos",
            ));
        }
        project_id = Some(pid);
    }
    let _ = project_id;

    Ok((
        category_key,
        input.duplicate_document_ids.clone(),
        None,
    ))
}

fn infer_group_key(
    codes: &[String],
    bundles: &[VersionBundle],
) -> Option<String> {
    bundles.first().map(|b| {
        classify_file_name(&b.principal.file_name, codes).group_key
    })
}

/// Detect logical model documents that share a technical family key.
pub async fn detect_unify_suggestions(
    pool: &sqlx::SqlitePool,
    project_id: &str,
    category_key: &str,
    focus_document_id: Option<&str>,
) -> AppResult<Vec<UnifySuggestion>> {
    let codes = {
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
        codes
    };

    let docs: Vec<(String, Option<String>, String)> = sqlx::query_as(
        "SELECT d.id, d.business_code, d.title
         FROM documents d
         JOIN categories c ON c.id = d.category_id
         WHERE d.project_id = ? AND c.key = ? AND d.status != 'deleted'",
    )
    .bind(project_id)
    .bind(category_key)
    .fetch_all(pool)
    .await?;

    // group_key → (doc_id → file names)
    let mut by_key: HashMap<String, HashMap<String, Vec<String>>> = HashMap::new();
    let mut labels: HashMap<String, String> = HashMap::new();

    for (id, code, title) in &docs {
        labels.insert(
            id.clone(),
            code.clone().unwrap_or_else(|| title.clone()),
        );
        let files: Vec<(String, Option<String>)> = sqlx::query_as(
            "SELECT f.file_name, f.engineering_group_key
             FROM document_files f
             JOIN document_versions v ON v.id = f.document_version_id
             WHERE v.document_id = ? AND f.is_available = 1
               AND COALESCE(f.sheet_kind, '') = ''
               AND COALESCE(f.file_role, 'principal') IN ('principal', 'standalone', '')",
        )
        .bind(id)
        .fetch_all(pool)
        .await?;
        for (fname, egk) in files {
            let key = egk.unwrap_or_else(|| classify_file_name(&fname, &codes).group_key);
            if key.trim().is_empty() {
                continue;
            }
            by_key
                .entry(key)
                .or_default()
                .entry(id.clone())
                .or_default()
                .push(fname);
        }
    }

    let mut out = Vec::new();
    for (group_key, docs_map) in by_key {
        if docs_map.len() < 2 {
            continue;
        }
        let mut document_ids: Vec<String> = docs_map.keys().cloned().collect();
        document_ids.sort();
        if let Some(focus) = focus_document_id {
            if !document_ids.iter().any(|d| d == focus) {
                continue;
            }
        }
        let document_labels: Vec<String> = document_ids
            .iter()
            .map(|id| labels.get(id).cloned().unwrap_or_else(|| id.clone()))
            .collect();
        let mut file_names = Vec::new();
        for names in docs_map.values() {
            file_names.extend(names.iter().cloned());
        }
        file_names.sort();
        file_names.dedup();
        out.push(UnifySuggestion {
            group_key,
            document_ids,
            document_labels,
            file_names,
        });
    }
    out.sort_by(|a, b| a.group_key.cmp(&b.group_key));
    Ok(out)
}

pub async fn preview_unify_models_inner(
    pool: &sqlx::SqlitePool,
    input: UnifyModelsInput,
) -> AppResult<UnifyModelsPreview> {
    let (_category_key, duplicates, _) = validate_unify_set(pool, &input).await?;
    let principal = load_doc_summary(pool, &input.principal_document_id).await?;
    let mut dup_summaries = Vec::new();
    for id in &duplicates {
        dup_summaries.push(load_doc_summary(pool, id).await?);
    }

    let mut all_ids = vec![input.principal_document_id.clone()];
    all_ids.extend(duplicates.clone());
    let files = load_files_for_docs(pool, &all_ids).await?;
    let bundles = build_bundles(&files)?;

    let codes = {
        let row: Option<(Option<String>, String)> =
            sqlx::query_as("SELECT code, root_relative_path FROM projects WHERE id = ?")
                .bind(&input.project_id)
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
        codes
    };
    let group_key = infer_group_key(&codes, &bundles);

    let mut planned = Vec::new();
    let mut sheets_to_transfer = 0i64;
    for (idx, b) in bundles.iter().enumerate() {
        let new_n = (idx as i64) + 1;
        let sheet_count = b
            .attachments
            .iter()
            .filter(|f| f.sheet_kind.is_some() && f.is_available == 1)
            .count() as i64;
        let aux_count = b
            .attachments
            .iter()
            .filter(|f| {
                f.is_available == 1
                    && f.sheet_kind.is_none()
                    && f.file_role.as_deref() == Some("auxiliary")
            })
            .count() as i64;
        sheets_to_transfer += sheet_count;
        planned.push(UnifyVersionPlan {
            source_document_id: b.source_document_id.clone(),
            source_version_id: b.source_version_id.clone(),
            file_name: b.principal.file_name.clone(),
            relative_path: b.principal.relative_path.clone(),
            version_hint: version_hint_from_name(&b.principal.file_name),
            new_version_number: new_n,
            new_label: crate::doc_codes::version_label(new_n),
            sheet_count,
            auxiliary_count: aux_count,
        });
    }

    let relations_to_rewire =
        count_relations_to_rewire(pool, &input.principal_document_id, &duplicates).await?;

    let mut warnings = Vec::new();
    if planned.is_empty() {
        warnings.push("No hay archivos principales disponibles para unificar.".into());
    }
    let suggestions =
        detect_unify_suggestions(pool, &input.project_id, "modelos", Some(&input.principal_document_id))
            .await
            .unwrap_or_default();
    for s in suggestions {
        for id in &s.document_ids {
            if id != &input.principal_document_id && !duplicates.contains(id) {
                warnings.push(format!(
                    "Documento «{}» también parece de la familia «{}» y no está en duplicados.",
                    labels_fallback(id, &dup_summaries, &principal),
                    s.group_key
                ));
            }
        }
    }

    Ok(UnifyModelsPreview {
        principal,
        duplicates: dup_summaries,
        planned_versions: planned,
        relations_to_rewire,
        sheets_to_transfer: sheets_to_transfer as usize,
        duplicates_to_delete: duplicates,
        group_key,
        warnings,
    })
}

fn labels_fallback(id: &str, dups: &[UnifyDocSummary], principal: &UnifyDocSummary) -> String {
    if principal.document_id == id {
        return principal
            .business_code
            .clone()
            .unwrap_or_else(|| principal.title.clone());
    }
    dups.iter()
        .find(|d| d.document_id == id)
        .map(|d| {
            d.business_code
                .clone()
                .unwrap_or_else(|| d.title.clone())
        })
        .unwrap_or_else(|| id.to_string())
}

async fn snapshot_db(pool: &sqlx::SqlitePool, dest: &Path) -> AppResult<()> {
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

pub async fn backup_db_before_unify(
    pool: &sqlx::SqlitePool,
    db_path: &Path,
    project_id: &str,
) -> AppResult<String> {
    let stamp = Local::now().format("%Y%m%d-%H%M%S").to_string();
    // Prefer project .ingmanager/db_backups; fall back next to app DB.
    let dest = match project_paths::resolve_project_location(pool, project_id).await {
        Ok(loc) => loc
            .root_path
            .join(".ingmanager")
            .join("db_backups")
            .join(format!("unify-{stamp}.sqlite")),
        Err(_) => {
            let parent = db_path
                .parent()
                .map(Path::to_path_buf)
                .unwrap_or_else(|| PathBuf::from("."));
            parent
                .join("db_backups")
                .join(format!("unify-{stamp}.sqlite"))
        }
    };
    snapshot_db(pool, &dest).await?;
    Ok(dest.display().to_string())
}

async fn rewire_relationships(
    tx: &mut Transaction<'_, Sqlite>,
    principal_id: &str,
    duplicate_ids: &[String],
) -> AppResult<usize> {
    let mut rewired = 0usize;
    for dup in duplicate_ids {
        let rows: Vec<(String, String, String, String, Option<String>)> = sqlx::query_as(
            "SELECT id, source_document_id, target_document_id, relation_type, notes
             FROM document_relationships
             WHERE source_document_id = ? OR target_document_id = ?",
        )
        .bind(dup)
        .bind(dup)
        .fetch_all(&mut **tx)
        .await?;

        for (id, src, tgt, rel_type, notes) in rows {
            let mut new_src = src.clone();
            let mut new_tgt = tgt.clone();
            if new_src == *dup {
                new_src = principal_id.to_string();
            }
            if new_tgt == *dup {
                new_tgt = principal_id.to_string();
            }
            if new_src == new_tgt {
                sqlx::query("DELETE FROM document_relationships WHERE id = ?")
                    .bind(&id)
                    .execute(&mut **tx)
                    .await?;
                continue;
            }
            // Conflict with existing unique triple → drop this one.
            let exists: Option<(String,)> = sqlx::query_as(
                "SELECT id FROM document_relationships
                 WHERE source_document_id = ? AND target_document_id = ? AND relation_type = ?
                   AND id != ?",
            )
            .bind(&new_src)
            .bind(&new_tgt)
            .bind(&rel_type)
            .bind(&id)
            .fetch_optional(&mut **tx)
            .await?;
            if exists.is_some() {
                sqlx::query("DELETE FROM document_relationships WHERE id = ?")
                    .bind(&id)
                    .execute(&mut **tx)
                    .await?;
                continue;
            }
            sqlx::query(
                "UPDATE document_relationships
                 SET source_document_id = ?, target_document_id = ?, notes = ?
                 WHERE id = ?",
            )
            .bind(&new_src)
            .bind(&new_tgt)
            .bind(&notes)
            .bind(&id)
            .execute(&mut **tx)
            .await?;
            rewired += 1;
        }
    }
    Ok(rewired)
}

async fn delete_logical_shell(
    tx: &mut Transaction<'_, Sqlite>,
    document_id: &str,
) -> AppResult<()> {
    // Versions/files should already be moved; clean residual metadata.
    let version_ids: Vec<(String,)> =
        sqlx::query_as("SELECT id FROM document_versions WHERE document_id = ?")
            .bind(document_id)
            .fetch_all(&mut **tx)
            .await?;
    for (vid,) in &version_ids {
        sqlx::query("DELETE FROM document_files WHERE document_version_id = ?")
            .bind(vid)
            .execute(&mut **tx)
            .await?;
    }
    sqlx::query("DELETE FROM revisions WHERE document_id = ?")
        .bind(document_id)
        .execute(&mut **tx)
        .await?;
    sqlx::query(
        "DELETE FROM document_relationships
         WHERE source_document_id = ? OR target_document_id = ?",
    )
    .bind(document_id)
    .bind(document_id)
    .execute(&mut **tx)
    .await?;
    sqlx::query("DELETE FROM favorites WHERE entity_type = 'document' AND entity_id = ?")
        .bind(document_id)
        .execute(&mut **tx)
        .await?;
    sqlx::query("DELETE FROM file_version_tracking WHERE document_id = ?")
        .bind(document_id)
        .execute(&mut **tx)
        .await?;
    sqlx::query("DELETE FROM engineering_groups WHERE document_id = ?")
        .bind(document_id)
        .execute(&mut **tx)
        .await?;
    sqlx::query("UPDATE documents SET current_version_id = NULL WHERE id = ?")
        .bind(document_id)
        .execute(&mut **tx)
        .await?;
    sqlx::query("DELETE FROM document_versions WHERE document_id = ?")
        .bind(document_id)
        .execute(&mut **tx)
        .await?;
    sqlx::query("DELETE FROM documents WHERE id = ?")
        .bind(document_id)
        .execute(&mut **tx)
        .await?;
    Ok(())
}

pub async fn apply_unify_models_inner(
    pool: &sqlx::SqlitePool,
    db_path: &Path,
    input: UnifyModelsInput,
) -> AppResult<UnifyModelsResult> {
    let preview = preview_unify_models_inner(pool, input.clone()).await?;
    if preview.planned_versions.is_empty() {
        return Err(AppError::msg(
            "No hay versiones/archivos principales para unificar",
        ));
    }

    let backup_path = backup_db_before_unify(pool, db_path, &input.project_id).await?;

    let principal_id = input.principal_document_id.clone();
    let duplicates = preview.duplicates_to_delete.clone();
    let mut all_ids = vec![principal_id.clone()];
    all_ids.extend(duplicates.clone());

    // Preserve identity fields explicitly (must not change).
    let identity: (String, Option<String>, String) = sqlx::query_as(
        "SELECT id, business_code, title FROM documents WHERE id = ?",
    )
    .bind(&principal_id)
    .fetch_one(pool)
    .await?;

    let files = load_files_for_docs(pool, &all_ids).await?;
    let bundles = build_bundles(&files)?;
    let codes = {
        let row: Option<(Option<String>, String)> =
            sqlx::query_as("SELECT code, root_relative_path FROM projects WHERE id = ?")
                .bind(&input.project_id)
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
        codes
    };
    let group_key = infer_group_key(&codes, &bundles);

    let mut tx = pool.begin().await?;

    // Detach current pointers so we can delete/rebuild versions safely.
    for id in &all_ids {
        sqlx::query("UPDATE documents SET current_version_id = NULL WHERE id = ?")
            .bind(id)
            .execute(&mut *tx)
            .await?;
    }

    // Collect all old version ids we will replace.
    let mut old_version_ids: HashSet<String> = HashSet::new();
    for b in &bundles {
        old_version_ids.insert(b.source_version_id.clone());
    }
    // Also gather any leftover versions on these docs (empty / non-principal).
    for id in &all_ids {
        let vids: Vec<(String,)> =
            sqlx::query_as("SELECT id FROM document_versions WHERE document_id = ?")
                .bind(id)
                .fetch_all(&mut *tx)
                .await?;
        for (vid,) in vids {
            old_version_ids.insert(vid);
        }
    }

    // Park old version numbers so UNIQUE(document_id, version_number) allows rebuild.
    let mut park_n = 100_000i64;
    for vid in &old_version_ids {
        park_n += 1;
        sqlx::query(
            "UPDATE document_versions SET version_number = ?, label = ? WHERE id = ?",
        )
        .bind(park_n)
        .bind(format!("park-{park_n}"))
        .bind(vid)
        .execute(&mut *tx)
        .await?;
    }

    let mut files_transferred = 0usize;
    let mut sheets_transferred = 0usize;
    let mut new_version_ids: Vec<String> = Vec::new();
    let mut file_moves: Vec<(String, String)> = Vec::new(); // file_id → new_version_id
    let mut display_principal = String::new();

    for (idx, b) in bundles.iter().enumerate() {
        let new_n = (idx as i64) + 1;
        let new_vid = Uuid::new_v4().to_string();
        let hint = version_hint_from_name(&b.principal.file_name);
        let notes = match &hint {
            Some(h) => format!("Unificado desde archivo (hint: {h})"),
            None => "Unificado desde archivo".into(),
        };
        sqlx::query(
            "INSERT INTO document_versions (
                id, document_id, version_number, label, notes, created_at,
                is_snapshot, source_relative_path
             ) VALUES (?, ?, ?, ?, ?, datetime('now'), 0, ?)",
        )
        .bind(&new_vid)
        .bind(&principal_id)
        .bind(new_n)
        .bind(crate::doc_codes::version_label(new_n))
        .bind(&notes)
        .bind(&b.principal.relative_path)
        .execute(&mut *tx)
        .await?;

        file_moves.push((b.principal.id.clone(), new_vid.clone()));
        files_transferred += 1;
        for att in &b.attachments {
            file_moves.push((att.id.clone(), new_vid.clone()));
            files_transferred += 1;
            if att.sheet_kind.is_some() {
                sheets_transferred += 1;
            }
        }
        display_principal = b.principal.file_name.clone();
        new_version_ids.push(new_vid);
    }

    // Move file rows onto new versions BEFORE deleting old versions.
    for (file_id, new_vid) in &file_moves {
        sqlx::query("UPDATE document_files SET document_version_id = ? WHERE id = ?")
            .bind(new_vid)
            .bind(file_id)
            .execute(&mut *tx)
            .await?;
        if let Some(ref gk) = group_key {
            sqlx::query(
                "UPDATE document_files SET engineering_group_key = ?
                 WHERE id = ? AND COALESCE(sheet_kind, '') = ''",
            )
            .bind(gk)
            .bind(file_id)
            .execute(&mut *tx)
            .await?;
        }
    }

    // Point revisions that referenced old versions to new ones when possible.
    for b in bundles.iter().zip(new_version_ids.iter()) {
        let (bundle, new_vid) = b;
        sqlx::query(
            "UPDATE revisions SET document_version_id = ?, document_id = ?
             WHERE document_version_id = ?",
        )
        .bind(new_vid)
        .bind(&principal_id)
        .bind(&bundle.source_version_id)
        .execute(&mut *tx)
        .await?;
        sqlx::query(
            "UPDATE file_version_tracking
             SET document_id = ?, document_version_id = ?
             WHERE document_version_id = ?",
        )
        .bind(&principal_id)
        .bind(new_vid)
        .bind(&bundle.source_version_id)
        .execute(&mut *tx)
        .await?;
    }
    for dup in &duplicates {
        sqlx::query("UPDATE revisions SET document_id = ? WHERE document_id = ?")
            .bind(&principal_id)
            .bind(dup)
            .execute(&mut *tx)
            .await?;
        sqlx::query("UPDATE file_version_tracking SET document_id = ? WHERE document_id = ?")
            .bind(&principal_id)
            .bind(dup)
            .execute(&mut *tx)
            .await?;
        sqlx::query(
            "UPDATE transmission_items SET document_id = ? WHERE document_id = ?",
        )
        .bind(&principal_id)
        .bind(dup)
        .execute(&mut *tx)
        .await?;
    }

    let relations_rewired = rewire_relationships(&mut tx, &principal_id, &duplicates).await?;

    // Delete old version shells (files already reassigned).
    for vid in &old_version_ids {
        // Only delete if no files remain pointing here.
        let left: (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM document_files WHERE document_version_id = ?")
                .bind(vid)
                .fetch_one(&mut *tx)
                .await?;
        if left.0 == 0 {
            sqlx::query("DELETE FROM document_versions WHERE id = ?")
                .bind(vid)
                .execute(&mut *tx)
                .await?;
        }
    }

    // Any stray files still on old versions of these docs → move to latest new version.
    if let Some(latest) = new_version_ids.last() {
        for id in &all_ids {
            let stray: Vec<(String, String)> = sqlx::query_as(
                "SELECT f.id, f.document_version_id FROM document_files f
                 JOIN document_versions v ON v.id = f.document_version_id
                 WHERE v.document_id = ?",
            )
            .bind(id)
            .fetch_all(&mut *tx)
            .await?;
            for (fid, vid) in stray {
                if !new_version_ids.contains(&vid) {
                    sqlx::query("UPDATE document_files SET document_version_id = ? WHERE id = ?")
                        .bind(latest)
                        .bind(&fid)
                        .execute(&mut *tx)
                        .await?;
                    files_transferred += 1;
                }
            }
        }
        // Re-delete emptied old versions
        for vid in &old_version_ids {
            let left: (i64,) = sqlx::query_as(
                "SELECT COUNT(*) FROM document_files WHERE document_version_id = ?",
            )
            .bind(vid)
            .fetch_one(&mut *tx)
            .await?;
            if left.0 == 0 {
                sqlx::query("DELETE FROM document_versions WHERE id = ?")
                    .bind(vid)
                    .execute(&mut *tx)
                    .await?;
            }
        }
    }

    let current = new_version_ids
        .last()
        .cloned()
        .ok_or_else(|| AppError::msg("Sin versiones tras unificar"))?;
    sqlx::query(
        "UPDATE documents SET current_version_id = ?, updated_at = datetime('now')
         WHERE id = ?",
    )
    .bind(&current)
    .bind(&principal_id)
    .execute(&mut *tx)
    .await?;

    // Ensure identity unchanged (belt and suspenders).
    sqlx::query(
        "UPDATE documents SET business_code = ?, title = ? WHERE id = ?",
    )
    .bind(&identity.1)
    .bind(&identity.2)
    .bind(&identity.0)
    .execute(&mut *tx)
    .await?;

    // Engineering group → principal only.
    if let Some(ref gk) = group_key {
        let existing: Option<(String,)> = sqlx::query_as(
            "SELECT id FROM engineering_groups
             WHERE project_id = ? AND category_key = 'modelos' AND group_key = ?",
        )
        .bind(&input.project_id)
        .bind(gk)
        .fetch_optional(&mut *tx)
        .await?;
        if let Some((eg_id,)) = existing {
            sqlx::query(
                "UPDATE engineering_groups SET
                    document_id = ?, principal_file_name = ?, title = ?,
                    updated_at = datetime('now')
                 WHERE id = ?",
            )
            .bind(&principal_id)
            .bind(&display_principal)
            .bind(&identity.2)
            .bind(&eg_id)
            .execute(&mut *tx)
            .await?;
        } else {
            let eg_id = Uuid::new_v4().to_string();
            sqlx::query(
                "INSERT INTO engineering_groups (
                    id, project_id, category_key, group_key, title,
                    principal_file_name, document_id, created_at, updated_at
                 ) VALUES (?, ?, 'modelos', ?, ?, ?, ?, datetime('now'), datetime('now'))",
            )
            .bind(&eg_id)
            .bind(&input.project_id)
            .bind(gk)
            .bind(&identity.2)
            .bind(&display_principal)
            .bind(&principal_id)
            .execute(&mut *tx)
            .await?;
        }
    }

    for dup in &duplicates {
        delete_logical_shell(&mut tx, dup).await?;
    }

    tx.commit().await?;

    // Register baselines for principals (best-effort).
    let loc = project_paths::resolve_project_location(pool, &input.project_id).await.ok();
    if let Some(loc) = loc {
        for b in &bundles {
            let abs = loc.root_path.join(&b.principal.relative_path);
            if abs.is_file() {
                let _ = crate::auto_version::register_baseline(
                    pool,
                    &input.project_id,
                    &principal_id,
                    Some(&current),
                    &abs,
                )
                .await;
            }
        }
    }

    crate::audit::log_event(
        pool,
        Some(&input.project_id),
        "document",
        Some(&principal_id),
        "unify_models",
        Some(&format!(
            "Unificados {} duplicados → {}; backup {}",
            duplicates.len(),
            identity.1.as_deref().unwrap_or(&principal_id),
            backup_path
        )),
    )
    .await
    .ok();

    Ok(UnifyModelsResult {
        principal_document_id: principal_id,
        versions_on_principal: new_version_ids.len(),
        files_transferred,
        sheets_transferred,
        relations_rewired,
        duplicates_deleted: duplicates,
        db_backup_path: backup_path,
        group_key,
    })
}
