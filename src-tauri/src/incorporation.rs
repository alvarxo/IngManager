//! Documentary incorporation: unify physical files ↔ logical documents.
//!
//! Modes per group: associate existing / register new / physical-only.
//! Working files are linked by default (no forced historical snapshot).
//! DOCX+PDF (and similar) share a stem → one logical document with roles.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use uuid::Uuid;

use crate::doc_codes::{next_document_code, type_code_for_category, version_label};
use crate::error::{AppError, AppResult};
use crate::path_security::ensure_under_root;
use crate::project_paths::{self, ProjectLocation};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IncorporationMode {
    /// Link files into an existing logical document (current version).
    AssociateExisting,
    /// Create a new logical document and link working files.
    RegisterNew,
    /// Only copy/move into the project folder; no logical document.
    PhysicalOnly,
}

impl Default for IncorporationMode {
    fn default() -> Self {
        Self::RegisterNew
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PhysicalOp {
    None,
    Copy,
    Move,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IncorporationFileProposal {
    pub absolute_path: String,
    pub file_name: String,
    pub extension: Option<String>,
    pub size_bytes: Option<i64>,
    pub role: String,
    pub already_linked_document_id: Option<String>,
    pub already_linked_code: Option<String>,
    pub under_project: bool,
    pub relative_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExistingDocMatch {
    pub document_id: String,
    pub business_code: Option<String>,
    pub title: String,
    pub file_count: i64,
    pub score: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IncorporationGroupProposal {
    pub group_key: String,
    pub title: String,
    pub suggested_mode: IncorporationMode,
    pub mode: IncorporationMode,
    pub files: Vec<IncorporationFileProposal>,
    pub suggested_document_id: Option<String>,
    pub document_id: Option<String>,
    pub matches: Vec<ExistingDocMatch>,
    pub notes: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IncorporationPreview {
    pub project_id: String,
    pub category_key: String,
    pub dest_absolute_path: Option<String>,
    pub physical_op: PhysicalOp,
    pub groups: Vec<IncorporationGroupProposal>,
    pub warning: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IncorporationPreviewInput {
    pub project_id: String,
    pub category_key: String,
    pub absolute_paths: Vec<String>,
    /// Destination folder for physical copy/move (required if paths are outside project).
    pub dest_absolute_path: Option<String>,
    pub physical_op: Option<PhysicalOp>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IncorporationGroupDecision {
    pub group_key: String,
    pub mode: IncorporationMode,
    pub document_id: Option<String>,
    pub title: Option<String>,
    pub assign_code: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IncorporationApplyInput {
    pub project_id: String,
    pub category_key: String,
    pub absolute_paths: Vec<String>,
    pub dest_absolute_path: Option<String>,
    pub physical_op: Option<PhysicalOp>,
    pub decisions: Vec<IncorporationGroupDecision>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IncorporationGroupResult {
    pub group_key: String,
    pub mode: IncorporationMode,
    pub document_id: Option<String>,
    pub business_code: Option<String>,
    pub linked_files: usize,
    pub skipped_files: usize,
    pub physical_destinations: Vec<String>,
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IncorporationApplyResult {
    pub groups: Vec<IncorporationGroupResult>,
    pub created_documents: usize,
    pub associated_documents: usize,
    pub physical_only: usize,
    pub warning: Option<String>,
}

fn to_rfc3339(time: SystemTime) -> Option<String> {
    let dt: DateTime<Utc> = time.into();
    Some(dt.to_rfc3339())
}

fn fingerprint(path: &Path) -> AppResult<String> {
    let meta = fs::metadata(path)?;
    let modified = meta
        .modified()
        .ok()
        .and_then(to_rfc3339)
        .unwrap_or_default();
    Ok(format!("{}:{}", meta.len(), modified))
}

fn stem_key(file_name: &str) -> String {
    // Strip `_RA` / `_RB` / `_R0` / `_RevA` so DOCX+PDF across revisions
    // share one document identity (group by id; revisions hang off the doc).
    crate::doc_revisions::revision_identity_key(file_name)
}

fn role_for_extension(ext: Option<&str>) -> &'static str {
    match ext.map(|e| e.to_ascii_lowercase()).as_deref() {
        Some("pdf") => "issued_pdf",
        Some("docx" | "doc" | "odt") => "editable",
        Some("xlsx" | "xls" | "ods" | "csv") => "spreadsheet",
        Some("dwg" | "dxf") => "drawing",
        Some("sdb") => "principal",
        Some("pptx" | "ppt") => "presentation",
        _ => "standalone",
    }
}

fn is_document_pair_ext(ext: &str) -> bool {
    matches!(
        ext.to_ascii_lowercase().as_str(),
        "docx" | "doc" | "odt" | "pdf" | "xlsx" | "xls" | "ods" | "dwg" | "dxf"
    )
}

/// Group paths that should become one logical document (same stem, pairable formats).
pub fn group_incorporation_paths(paths: &[PathBuf]) -> Vec<(String, Vec<PathBuf>)> {
    let mut buckets: HashMap<String, Vec<PathBuf>> = HashMap::new();
    let mut order: Vec<String> = Vec::new();

    for p in paths {
        let name = p
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "file".into());
        let ext = p
            .extension()
            .map(|e| e.to_string_lossy().to_ascii_lowercase())
            .unwrap_or_default();
        let key = if is_document_pair_ext(&ext) {
            stem_key(&name)
        } else {
            // Unique key per non-pairable file
            format!("__single__{}", p.display())
        };
        if !buckets.contains_key(&key) {
            order.push(key.clone());
        }
        buckets.entry(key).or_default().push(p.clone());
    }

    order
        .into_iter()
        .filter_map(|k| buckets.remove(&k).map(|files| (k, files)))
        .collect()
}

fn title_from_group(files: &[PathBuf]) -> String {
    // Prefer editable / principal name; strip revision suffix for logical title
    let mut ranked = files.to_vec();
    ranked.sort_by_key(|p| {
        let ext = p
            .extension()
            .map(|e| e.to_string_lossy().to_ascii_lowercase())
            .unwrap_or_default();
        match ext.as_str() {
            "docx" | "doc" | "odt" => 0,
            "xlsx" | "xls" => 1,
            "dwg" => 2,
            "sdb" => 3,
            "pdf" => 4,
            _ => 9,
        }
    });
    let stem = ranked
        .first()
        .and_then(|p| p.file_stem())
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "Documento".into());
    let (identity, _) = crate::doc_revisions::strip_revision_suffix(&stem);
    if identity.trim().is_empty() {
        stem
    } else {
        identity
    }
}

/// Ensure a revision row exists for `document_id` + code; return its id.
pub async fn ensure_revision_row(
    pool: &SqlitePool,
    document_id: &str,
    version_id: &str,
    revision_code: &str,
) -> AppResult<String> {
    let code = revision_code.trim();
    if code.is_empty() {
        return Err(AppError::msg("Código de revisión vacío"));
    }
    let existing: Option<(String,)> = sqlx::query_as(
        "SELECT id FROM revisions WHERE document_id = ? AND revision_code = ? LIMIT 1",
    )
    .bind(document_id)
    .bind(code)
    .fetch_optional(pool)
    .await?;
    if let Some((id,)) = existing {
        return Ok(id);
    }
    let id = Uuid::new_v4().to_string();
    let status = crate::doc_revisions::status_for_code(code);
    sqlx::query(
        "INSERT INTO revisions (
            id, document_id, revision_code, issued_at, notes, created_at,
            document_version_id, status
         ) VALUES (?, ?, ?, NULL, NULL, datetime('now'), ?, ?)",
    )
    .bind(&id)
    .bind(document_id)
    .bind(code)
    .bind(version_id)
    .bind(status)
    .execute(pool)
    .await?;
    Ok(id)
}

async fn find_linked_document(
    pool: &SqlitePool,
    project_id: &str,
    relative_path: &str,
) -> AppResult<Option<(String, Option<String>)>> {
    let row: Option<(String, Option<String>)> = sqlx::query_as(
        "SELECT d.id, d.business_code
         FROM documents d
         JOIN document_files f ON f.document_version_id = d.current_version_id
         WHERE d.project_id = ? AND f.relative_path = ? AND f.is_available = 1
         LIMIT 1",
    )
    .bind(project_id)
    .bind(relative_path)
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

fn normalize_match_text(s: &str) -> String {
    s.to_ascii_lowercase()
        .chars()
        .map(|c| if c == '_' || c == '-' { ' ' } else { c })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

async fn find_document_matches(
    pool: &SqlitePool,
    project_id: &str,
    category_key: &str,
    title: &str,
    group_key: &str,
) -> AppResult<Vec<ExistingDocMatch>> {
    let rows: Vec<(String, Option<String>, String, i64)> = sqlx::query_as(
        "SELECT d.id, d.business_code, d.title,
                COALESCE((
                    SELECT COUNT(*) FROM document_files f
                    WHERE f.document_version_id = d.current_version_id AND f.is_available = 1
                ), 0) AS file_count
         FROM documents d
         JOIN categories c ON c.id = d.category_id
         WHERE d.project_id = ? AND c.key = ?
         ORDER BY d.updated_at DESC
         LIMIT 80",
    )
    .bind(project_id)
    .bind(category_key)
    .fetch_all(pool)
    .await?;

    let title_n = normalize_match_text(title);
    let key_n = normalize_match_text(group_key);
    let lone_empty = rows.len() == 1;
    let mut matches = Vec::new();
    for (id, code, doc_title, file_count) in rows {
        let mut score = 0i64;
        let t = normalize_match_text(&doc_title);
        if t == title_n || t == key_n {
            score += 100;
        } else if (!title_n.is_empty() && (t.contains(&title_n) || title_n.contains(&t)))
            || (!key_n.is_empty() && (t.contains(&key_n) || key_n.contains(&t)))
        {
            score += 40;
        }
        if let Some(ref c) = code {
            let cl = normalize_match_text(c);
            if cl.contains(&key_n) || key_n.contains(&cl) {
                score += 60;
            }
            // Prefer empty-file docs that look like the target (2610-MC-001 case)
            if file_count == 0 && score > 0 {
                score += 25;
            }
        } else if file_count == 0 && score > 0 {
            score += 15;
        }
        // Lone empty logical doc in category with no better match: mild boost
        if file_count == 0 && score == 0 && lone_empty {
            score = 30;
        }
        if score > 0 {
            matches.push(ExistingDocMatch {
                document_id: id,
                business_code: code,
                title: doc_title,
                file_count,
                score,
            });
        }
    }
    matches.sort_by(|a, b| b.score.cmp(&a.score));
    matches.truncate(8);
    Ok(matches)
}

fn resolve_under_loc(loc: &ProjectLocation, abs: &Path) -> AppResult<(bool, Option<String>)> {
    match ensure_under_root(&loc.root_path, abs) {
        Ok(canon) => {
            let rel = project_paths::rel_from_project_root(loc, &canon)?;
            Ok((true, Some(rel)))
        }
        Err(_) => Ok((false, None)),
    }
}

pub async fn preview_incorporation(
    pool: &SqlitePool,
    input: IncorporationPreviewInput,
) -> AppResult<IncorporationPreview> {
    let loc = project_paths::resolve_project_location(pool, &input.project_id).await?;
    let physical_op = input.physical_op.unwrap_or(PhysicalOp::Copy);

    let mut paths: Vec<PathBuf> = Vec::new();
    for p in &input.absolute_paths {
        let pb = PathBuf::from(p);
        if pb.is_file() {
            paths.push(pb);
        } else if pb.is_dir() {
            // Flatten one level of files for preview
            if let Ok(rd) = fs::read_dir(&pb) {
                for e in rd.flatten() {
                    let ep = e.path();
                    if ep.is_file() {
                        paths.push(ep);
                    }
                }
            }
        }
    }
    if paths.is_empty() {
        return Err(AppError::msg("No hay archivos para incorporar"));
    }

    let grouped = group_incorporation_paths(&paths);
    let mut groups = Vec::new();
    let mut outside = 0usize;

    for (group_key, files) in grouped {
        let title = title_from_group(&files);
        let mut file_props = Vec::new();
        for f in &files {
            let meta = fs::metadata(f).ok();
            let name = f
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();
            let ext = f
                .extension()
                .map(|e| e.to_string_lossy().to_ascii_lowercase());
            let (under, rel) = resolve_under_loc(&loc, f)?;
            if !under {
                outside += 1;
            }
            let linked = if let Some(ref r) = rel {
                find_linked_document(pool, &input.project_id, r).await?
            } else {
                None
            };
            file_props.push(IncorporationFileProposal {
                absolute_path: f.display().to_string(),
                file_name: name,
                extension: ext.clone(),
                size_bytes: meta.as_ref().map(|m| m.len() as i64),
                role: role_for_extension(ext.as_deref()).to_string(),
                already_linked_document_id: linked.as_ref().map(|l| l.0.clone()),
                already_linked_code: linked.and_then(|l| l.1),
                under_project: under,
                relative_path: rel,
            });
        }

        let matches =
            find_document_matches(pool, &input.project_id, &input.category_key, &title, &group_key)
                .await?;

        let all_linked_same = {
            let ids: Vec<_> = file_props
                .iter()
                .filter_map(|f| f.already_linked_document_id.clone())
                .collect();
            !ids.is_empty() && ids.iter().all(|id| Some(id) == ids.first())
        };

        let rev_codes: Vec<String> = {
            let mut codes: Vec<String> = file_props
                .iter()
                .filter_map(|f| {
                    crate::doc_revisions::parse_revision_filename(&f.file_name)
                        .map(|p| p.revision_code)
                })
                .collect();
            codes.sort_by(|a, b| crate::doc_revisions::cmp_revision_codes(a, b));
            codes.dedup();
            codes
        };
        let rev_note = if !rev_codes.is_empty()
            && crate::doc_revisions::category_uses_revisions(&input.category_key)
        {
            Some(format!(
                "Revisiones detectadas: {} (borrador=letra, emitida=número). Un documento lógico.",
                rev_codes.join(", ")
            ))
        } else {
            None
        };

        let (suggested_mode, suggested_doc, notes) = if all_linked_same {
            (
                IncorporationMode::AssociateExisting,
                file_props[0].already_linked_document_id.clone(),
                rev_note.or_else(|| {
                    Some("Ya vinculados al mismo documento (idempotente)".into())
                }),
            )
        } else if let Some(m) = matches.first() {
            if m.file_count == 0 || m.score >= 80 {
                (
                    IncorporationMode::AssociateExisting,
                    Some(m.document_id.clone()),
                    rev_note.or_else(|| {
                        Some("Se propone asociar a un documento existente".into())
                    }),
                )
            } else {
                (IncorporationMode::RegisterNew, None, rev_note)
            }
        } else {
            (IncorporationMode::RegisterNew, None, rev_note)
        };

        groups.push(IncorporationGroupProposal {
            group_key,
            title,
            suggested_mode,
            mode: suggested_mode,
            files: file_props,
            suggested_document_id: suggested_doc.clone(),
            document_id: suggested_doc,
            matches,
            notes,
        });
    }

    let warning = if outside > 0 && input.dest_absolute_path.is_none() {
        Some(format!(
            "{outside} archivo(s) están fuera del proyecto; se copiarán al destino al confirmar (si se indica carpeta)."
        ))
    } else {
        None
    };

    Ok(IncorporationPreview {
        project_id: input.project_id,
        category_key: input.category_key,
        dest_absolute_path: input.dest_absolute_path,
        physical_op,
        groups,
        warning,
    })
}

async fn ensure_category_id(
    pool: &SqlitePool,
    project_id: &str,
    category_key: &str,
) -> AppResult<String> {
    let row: (String,) = sqlx::query_as(
        "SELECT id FROM categories WHERE project_id = ? AND key = ?",
    )
    .bind(project_id)
    .bind(category_key)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| AppError::msg("Categoría no encontrada"))?;
    Ok(row.0)
}

/// Link a working file into a document version without copying to snapshots.
pub async fn link_working_file(
    pool: &SqlitePool,
    loc: &ProjectLocation,
    version_id: &str,
    absolute_path: &Path,
    format_label: Option<&str>,
    file_role: Option<&str>,
    revision_id: Option<&str>,
) -> AppResult<(String, bool)> {
    let src = ensure_under_root(&loc.root_path, absolute_path)?;
    if !src.is_file() {
        return Err(AppError::msg("La ruta no es un archivo"));
    }
    let rel = project_paths::rel_from_project_root(loc, &src)?;

    // Idempotent: same relative path already on this version
    let existing: Option<(String,)> = sqlx::query_as(
        "SELECT id FROM document_files
         WHERE document_version_id = ? AND relative_path = ? AND is_available = 1",
    )
    .bind(version_id)
    .bind(&rel)
    .fetch_optional(pool)
    .await?;
    if let Some((id,)) = existing {
        if let Some(rid) = revision_id {
            sqlx::query("UPDATE document_files SET revision_id = ? WHERE id = ?")
                .bind(rid)
                .bind(&id)
                .execute(pool)
                .await?;
        }
        return Ok((id, true));
    }

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
    let role = file_role
        .map(|s| s.to_string())
        .unwrap_or_else(|| role_for_extension(extension.as_deref()).to_string());
    let label = format_label
        .map(|s| s.to_string())
        .or_else(|| extension.as_ref().map(|e| e.to_ascii_uppercase()));

    let id = Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO document_files (
            id, document_version_id, relative_path, file_name, extension,
            size_bytes, modified_at, content_hash, is_available, format_label,
            revision_id, created_at, file_role
         ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, 1, ?, ?, datetime('now'), ?)",
    )
    .bind(&id)
    .bind(version_id)
    .bind(&rel)
    .bind(&file_name)
    .bind(&extension)
    .bind(meta.len() as i64)
    .bind(&modified)
    .bind(&fp)
    .bind(&label)
    .bind(revision_id)
    .bind(&role)
    .execute(pool)
    .await?;

    // Keep source_relative_path on version for change detection (no snapshot)
    sqlx::query(
        "UPDATE document_versions SET
            source_relative_path = COALESCE(source_relative_path, ?),
            registered_fingerprint = ?,
            is_snapshot = 0
         WHERE id = ?",
    )
    .bind(&rel)
    .bind(&fp)
    .bind(version_id)
    .execute(pool)
    .await?;

    Ok((id, false))
}

/// Copy into `.ingmanager/snapshots/...` for historical / issued revision artifacts.
pub async fn snapshot_file_to_version(
    pool: &SqlitePool,
    loc: &ProjectLocation,
    document_id: &str,
    version_id: &str,
    version_number: i64,
    absolute_path: &Path,
    format_label: Option<&str>,
    file_role: Option<&str>,
    revision_id: Option<&str>,
) -> AppResult<String> {
    let src = ensure_under_root(&loc.root_path, absolute_path)?;
    if !src.is_file() {
        return Err(AppError::msg("La ruta no es un archivo"));
    }
    let file_name = src
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "file".into());
    let snap_dir = loc
        .root_path
        .join(".ingmanager")
        .join("snapshots")
        .join(document_id)
        .join(format!("v{version_number:02}"));
    fs::create_dir_all(&snap_dir)?;
    let dest = {
        let candidate = snap_dir.join(&file_name);
        if candidate.exists() {
            crate::fs_util::unique_destination(&snap_dir, &file_name)
        } else {
            candidate
        }
    };
    fs::copy(&src, &dest)?;
    let snap_rel = project_paths::rel_from_project_root(loc, &dest)?;
    let source_rel = project_paths::rel_from_project_root(loc, &src)?;
    let fp = fingerprint(&src)?;

    sqlx::query(
        "UPDATE document_versions SET
            snapshot_relative_path = ?,
            registered_fingerprint = ?,
            source_relative_path = ?,
            is_snapshot = 1
         WHERE id = ?",
    )
    .bind(project_paths::rel_from_project_root(loc, &snap_dir)?)
    .bind(&fp)
    .bind(&source_rel)
    .bind(version_id)
    .execute(pool)
    .await?;

    let extension = dest
        .extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase());
    let meta = fs::metadata(&dest)?;
    let modified = meta.modified().ok().and_then(to_rfc3339);
    let role = file_role
        .map(|s| s.to_string())
        .unwrap_or_else(|| role_for_extension(extension.as_deref()).to_string());
    let label = format_label
        .map(|s| s.to_string())
        .or_else(|| extension.as_ref().map(|e| e.to_ascii_uppercase()));

    let id = Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO document_files (
            id, document_version_id, relative_path, file_name, extension,
            size_bytes, modified_at, content_hash, is_available, format_label,
            revision_id, created_at, file_role
         ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, 1, ?, ?, datetime('now'), ?)",
    )
    .bind(&id)
    .bind(version_id)
    .bind(&snap_rel)
    .bind(&file_name)
    .bind(&extension)
    .bind(meta.len() as i64)
    .bind(&modified)
    .bind(&fp)
    .bind(&label)
    .bind(revision_id)
    .bind(&role)
    .execute(pool)
    .await?;

    Ok(id)
}

fn physical_place(
    loc: &ProjectLocation,
    src: &Path,
    dest_dir: &Path,
    op: PhysicalOp,
) -> AppResult<PathBuf> {
    let _ = ensure_under_root(&loc.root_path, dest_dir)?;
    if !dest_dir.is_dir() {
        fs::create_dir_all(dest_dir)?;
    }
    let name = src
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .ok_or_else(|| AppError::msg("Nombre de archivo inválido"))?;
    let mut dest = dest_dir.join(&name);
    if dest.exists() {
        dest = crate::fs_util::unique_destination(dest_dir, &name);
    }
    match op {
        PhysicalOp::Copy => {
            fs::copy(src, &dest)?;
        }
        PhysicalOp::Move => {
            if fs::rename(src, &dest).is_err() {
                fs::copy(src, &dest)?;
                let _ = fs::remove_file(src);
            }
        }
        PhysicalOp::None => {
            return Err(AppError::msg(
                "Se requiere copia o movimiento para archivos fuera del proyecto",
            ));
        }
    }
    Ok(dest)
}

async fn create_logical_document(
    pool: &SqlitePool,
    loc: &ProjectLocation,
    category_key: &str,
    category_id: &str,
    title: &str,
    assign_code: bool,
) -> AppResult<(String, String, Option<String>)> {
    let type_code = type_code_for_category(category_key);
    let business_code = if assign_code {
        let Some(tc) = type_code else {
            return Err(AppError::msg("Esta categoría no tiene tipo de código"));
        };
        let rows: Vec<(Option<String>,)> =
            sqlx::query_as("SELECT business_code FROM documents WHERE project_id = ?")
                .bind(&loc.project_id)
                .fetch_all(pool)
                .await?;
        let codes: Vec<String> = rows.into_iter().filter_map(|r| r.0).collect();
        Some(next_document_code(
            &loc.business_code_prefix,
            tc,
            &codes,
        )?)
    } else {
        None
    };

    let status = if category_key == "memorias" {
        "desarrollo"
    } else {
        "active"
    };
    let doc_id = Uuid::new_v4().to_string();
    let version_id = Uuid::new_v4().to_string();

    sqlx::query(
        "INSERT INTO documents (
            id, project_id, category_id, business_code, title, doc_type,
            current_version_id, status, created_at, updated_at
         ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, datetime('now'), datetime('now'))",
    )
    .bind(&doc_id)
    .bind(&loc.project_id)
    .bind(category_id)
    .bind(&business_code)
    .bind(title)
    .bind(type_code.map(|s| s.to_string()))
    .bind(&version_id)
    .bind(status)
    .execute(pool)
    .await?;

    sqlx::query(
        "INSERT INTO document_versions (
            id, document_id, version_number, label, notes, created_at, is_snapshot
         ) VALUES (?, ?, 1, ?, NULL, datetime('now'), 0)",
    )
    .bind(&version_id)
    .bind(&doc_id)
    .bind(version_label(1))
    .execute(pool)
    .await?;

    let _ = crate::search_index::upsert_fts(
        pool,
        "document",
        &doc_id,
        Some(&loc.project_id),
        title,
        business_code.as_deref(),
        None,
        None,
    )
    .await;

    Ok((doc_id, version_id, business_code))
}

pub async fn apply_incorporation(
    pool: &SqlitePool,
    input: IncorporationApplyInput,
) -> AppResult<IncorporationApplyResult> {
    let loc = project_paths::resolve_project_location(pool, &input.project_id).await?;
    let category_id = ensure_category_id(pool, &input.project_id, &input.category_key).await?;
    let physical_op = input.physical_op.unwrap_or(PhysicalOp::Copy);
    let dest_dir = input
        .dest_absolute_path
        .as_ref()
        .map(PathBuf::from);

    let mut paths: Vec<PathBuf> = input
        .absolute_paths
        .iter()
        .map(PathBuf::from)
        .filter(|p| p.is_file())
        .collect();
    // Also expand dirs one level
    for p in input.absolute_paths.iter().map(PathBuf::from) {
        if p.is_dir() {
            if let Ok(rd) = fs::read_dir(&p) {
                for e in rd.flatten() {
                    let ep = e.path();
                    if ep.is_file() {
                        paths.push(ep);
                    }
                }
            }
        }
    }

    let grouped = group_incorporation_paths(&paths);
    let decisions: HashMap<String, IncorporationGroupDecision> = input
        .decisions
        .into_iter()
        .map(|d| (d.group_key.clone(), d))
        .collect();

    let mut results = Vec::new();
    let mut created_documents = 0usize;
    let mut associated_documents = 0usize;
    let mut physical_only = 0usize;
    let mut warnings: Vec<String> = Vec::new();

    for (group_key, files) in grouped {
        let decision = decisions.get(&group_key).cloned().unwrap_or_else(|| {
            IncorporationGroupDecision {
                group_key: group_key.clone(),
                mode: IncorporationMode::RegisterNew,
                document_id: None,
                title: None,
                assign_code: Some(input.category_key != "antecedentes"),
            }
        });

        let title = decision
            .title
            .clone()
            .unwrap_or_else(|| title_from_group(&files));

        // Physical placement for files outside project
        let mut working_paths: Vec<(PathBuf, String)> = Vec::new();
        let mut physical_destinations = Vec::new();
        for f in &files {
            let (under, _) = resolve_under_loc(&loc, f)?;
            let working = if under {
                f.clone()
            } else {
                let dest = dest_dir.as_ref().ok_or_else(|| {
                    AppError::msg(format!(
                        "Archivo fuera del proyecto sin destino: {}",
                        f.display()
                    ))
                })?;
                let placed = physical_place(&loc, f, dest, physical_op)?;
                physical_destinations.push(placed.display().to_string());
                let _ =
                    crate::index_sync::upsert_path(pool, &loc.root_path, &placed).await;
                placed
            };
            let role = role_for_extension(
                working
                    .extension()
                    .map(|e| e.to_string_lossy().to_ascii_lowercase())
                    .as_deref(),
            )
            .to_string();
            working_paths.push((working, role));
        }

        match decision.mode {
            IncorporationMode::PhysicalOnly => {
                physical_only += 1;
                results.push(IncorporationGroupResult {
                    group_key,
                    mode: decision.mode,
                    document_id: None,
                    business_code: None,
                    linked_files: 0,
                    skipped_files: 0,
                    physical_destinations,
                    message: Some("Solo físico (sin documento lógico)".into()),
                });
            }
            IncorporationMode::AssociateExisting | IncorporationMode::RegisterNew => {
                let (doc_id, version_id, business_code) =
                    if decision.mode == IncorporationMode::AssociateExisting {
                        let doc_id = decision.document_id.clone().ok_or_else(|| {
                            AppError::msg(format!(
                                "Falta document_id para asociar el grupo {group_key}"
                            ))
                        })?;
                        let row: (Option<String>,) = sqlx::query_as(
                            "SELECT current_version_id FROM documents WHERE id = ? AND project_id = ?",
                        )
                        .bind(&doc_id)
                        .bind(&input.project_id)
                        .fetch_optional(pool)
                        .await?
                        .ok_or_else(|| AppError::msg("Documento a asociar no encontrado"))?;
                        let version_id = row.0.ok_or_else(|| {
                            AppError::msg("El documento no tiene versión actual")
                        })?;
                        let code: Option<(Option<String>,)> =
                            sqlx::query_as("SELECT business_code FROM documents WHERE id = ?")
                                .bind(&doc_id)
                                .fetch_optional(pool)
                                .await?;
                        associated_documents += 1;
                        (doc_id, version_id, code.and_then(|c| c.0))
                    } else {
                        let assign = decision
                            .assign_code
                            .unwrap_or(input.category_key != "antecedentes");
                        let created = create_logical_document(
                            pool,
                            &loc,
                            &input.category_key,
                            &category_id,
                            &title,
                            assign,
                        )
                        .await?;
                        created_documents += 1;
                        created
                    };

                let mut linked = 0usize;
                let mut skipped = 0usize;
                // Group working files by revision code (id+rev); link without re-copy.
                // Letter codes → draft; numeric → issued. No empty revision rows.
                for (path, role) in &working_paths {
                    let name = path
                        .file_name()
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_default();
                    let rev_id = if crate::doc_revisions::category_uses_revisions(
                        &input.category_key,
                    ) {
                        if let Some(parsed) =
                            crate::doc_revisions::parse_revision_filename(&name)
                        {
                            match ensure_revision_row(
                                pool,
                                &doc_id,
                                &version_id,
                                &parsed.revision_code,
                            )
                            .await
                            {
                                Ok(id) => Some(id),
                                Err(e) => {
                                    warnings.push(format!(
                                        "revisión {}: {e}",
                                        parsed.revision_code
                                    ));
                                    None
                                }
                            }
                        } else {
                            None
                        }
                    } else {
                        None
                    };
                    match link_working_file(
                        pool,
                        &loc,
                        &version_id,
                        path,
                        None,
                        Some(role),
                        rev_id.as_deref(),
                    )
                    .await
                    {
                        Ok((_, was_skip)) => {
                            if was_skip {
                                skipped += 1;
                            } else {
                                linked += 1;
                            }
                        }
                        Err(e) => {
                            warnings.push(format!("{}: {e}", path.display()));
                        }
                    }
                }

                sqlx::query(
                    "UPDATE documents SET updated_at = datetime('now') WHERE id = ?",
                )
                .bind(&doc_id)
                .execute(pool)
                .await?;

                let _ = crate::audit::log_event(
                    pool,
                    Some(&input.project_id),
                    "document",
                    Some(&doc_id),
                    if decision.mode == IncorporationMode::AssociateExisting {
                        "associate_files"
                    } else {
                        "incorporate_register"
                    },
                    Some(&title),
                )
                .await;

                results.push(IncorporationGroupResult {
                    group_key,
                    mode: decision.mode,
                    document_id: Some(doc_id),
                    business_code,
                    linked_files: linked,
                    skipped_files: skipped,
                    physical_destinations,
                    message: None,
                });
            }
        }
    }

    Ok(IncorporationApplyResult {
        groups: results,
        created_documents,
        associated_documents,
        physical_only,
        warning: if warnings.is_empty() {
            None
        } else {
            Some(warnings.join("; "))
        },
    })
}

#[cfg(test)]
mod grouping_tests {
    use super::*;

    #[test]
    fn groups_docx_and_pdf_same_stem() {
        let paths = vec![
            PathBuf::from("/tmp/Memoria_Calculo.docx"),
            PathBuf::from("/tmp/Memoria_Calculo.pdf"),
            PathBuf::from("/tmp/Otro.docx"),
        ];
        let g = group_incorporation_paths(&paths);
        assert_eq!(g.len(), 2);
        let memo = g
            .iter()
            .find(|(k, _)| k == "memoria_calculo")
            .expect("memo group");
        assert_eq!(memo.1.len(), 2);
    }

    #[test]
    fn groups_rb_r0_docx_pdf_as_one_document() {
        let paths = vec![
            PathBuf::from("/p/1126-MC-002_RB.docx"),
            PathBuf::from("/p/1126-MC-002_RB.pdf"),
            PathBuf::from("/p/1126-MC-002_R0.docx"),
            PathBuf::from("/p/1126-MC-002_R0.pdf"),
        ];
        let g = group_incorporation_paths(&paths);
        assert_eq!(g.len(), 1, "four files → one logical identity");
        assert_eq!(g[0].1.len(), 4);
        assert_eq!(g[0].0, "1126-mc-002");
        let title = title_from_group(&g[0].1);
        assert_eq!(title, "1126-MC-002");
    }

    #[test]
    fn roles_for_extensions() {
        assert_eq!(role_for_extension(Some("docx")), "editable");
        assert_eq!(role_for_extension(Some("PDF")), "issued_pdf");
        assert_eq!(role_for_extension(Some("sdb")), "principal");
    }
}
