//! Model-associated Excel spreadsheets (calc vs export).
//!
//! - calc → default copy-on-version: on next physical model version, snapshot the
//!   workbook onto the outgoing version and keep the original linked to the new current.
//! - export → default version-bound: stays on the version where it was associated;
//!   the new current shows pending (no export row) until associated again.

use std::fs;
use std::path::{Path, PathBuf};

use sqlx::SqlitePool;
use uuid::Uuid;

use crate::error::{AppError, AppResult};
use crate::incorporation::link_working_file;
use crate::path_security::ensure_under_root;
use crate::project_paths::{self, ProjectLocation};
use crate::state::AppState;

pub const SHEET_KIND_CALC: &str = "calc";
pub const SHEET_KIND_EXPORT: &str = "export";
pub const POLICY_COPY_ON_VERSION: &str = "copy_on_version";
pub const POLICY_VERSION_BOUND: &str = "version_bound";
pub const ROLE_CALC_SHEET: &str = "calc_sheet";
pub const ROLE_EXPORT_SHEET: &str = "export_sheet";

const SPREADSHEET_EXTS: &[&str] = &["xlsx", "xls", "xlsm"];

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AssociateModelSpreadsheetInput {
    pub document_version_id: String,
    pub absolute_path: String,
    /// `calc` | `export`
    pub sheet_kind: String,
    /// Optional override; defaults: calc→copy_on_version, export→version_bound.
    pub version_policy: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ModelSpreadsheetStatus {
    pub document_id: String,
    pub document_version_id: String,
    pub is_current: bool,
    pub has_live_calc: bool,
    pub has_export: bool,
    pub export_pending: bool,
    /// All available calc associations on this version (live and/or historical snapshots).
    pub calc_file_ids: Vec<String>,
    pub export_file_ids: Vec<String>,
}

pub fn is_spreadsheet_ext(ext: Option<&str>) -> bool {
    ext.map(|e| {
        let lower = e.to_ascii_lowercase();
        SPREADSHEET_EXTS.iter().any(|a| *a == lower)
    })
    .unwrap_or(false)
}

pub fn default_policy_for_kind(kind: &str) -> AppResult<&'static str> {
    match kind {
        SHEET_KIND_CALC => Ok(POLICY_COPY_ON_VERSION),
        SHEET_KIND_EXPORT => Ok(POLICY_VERSION_BOUND),
        _ => Err(AppError::msg(
            "sheet_kind debe ser «calc» o «export»",
        )),
    }
}

pub fn role_for_kind(kind: &str) -> &'static str {
    if kind == SHEET_KIND_EXPORT {
        ROLE_EXPORT_SHEET
    } else {
        ROLE_CALC_SHEET
    }
}

pub fn label_for_kind(kind: &str) -> &'static str {
    if kind == SHEET_KIND_EXPORT {
        "EXPORT"
    } else {
        "CALC"
    }
}

/// Associate an XLSX/XLS/XLSM workbook to a model document version.
/// Returns the new/updated `document_files.id`.
pub async fn associate_model_spreadsheet_inner(
    state: &AppState,
    input: AssociateModelSpreadsheetInput,
) -> AppResult<String> {
    let kind = input.sheet_kind.trim().to_ascii_lowercase();
    if kind != SHEET_KIND_CALC && kind != SHEET_KIND_EXPORT {
        return Err(AppError::msg(
            "sheet_kind debe ser «calc» o «export»",
        ));
    }
    let policy = match input.version_policy.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        Some(p) => {
            let p = p.to_ascii_lowercase();
            if p != POLICY_COPY_ON_VERSION && p != POLICY_VERSION_BOUND {
                return Err(AppError::msg(
                    "version_policy debe ser «copy_on_version» o «version_bound»",
                ));
            }
            p
        }
        None => default_policy_for_kind(&kind)?.to_string(),
    };

    let meta: Option<(String, String, Option<String>)> = sqlx::query_as(
        "SELECT v.document_id, d.project_id, c.key
         FROM document_versions v
         JOIN documents d ON d.id = v.document_id
         LEFT JOIN categories c ON c.id = d.category_id
         WHERE v.id = ?",
    )
    .bind(&input.document_version_id)
    .fetch_optional(&state.pool)
    .await?;
    let Some((_document_id, project_id, category_key)) = meta else {
        return Err(AppError::msg("Versión no encontrada"));
    };
    if category_key.as_deref() != Some("modelos") {
        return Err(AppError::msg(
            "Las planillas Excel solo se asocian a documentos de categoría Modelos",
        ));
    }

    let path = PathBuf::from(&input.absolute_path);
    if !path.is_file() {
        return Err(AppError::msg("Archivo de planilla no disponible"));
    }
    let ext = path
        .extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase());
    if !is_spreadsheet_ext(ext.as_deref()) {
        return Err(AppError::msg(
            "Solo se admiten planillas XLSX, XLS o XLSM",
        ));
    }

    let loc = project_paths::resolve_project_location(&state.pool, &project_id).await?;
    // Reject the same working path twice on this version (many calcs OK; duplicate path not).
    let canon = ensure_under_root(&loc.root_path, &path)?;
    let rel = project_paths::rel_from_project_root(&loc, &canon)?;
    let dup: Option<(String,)> = sqlx::query_as(
        "SELECT id FROM document_files
         WHERE document_version_id = ?
           AND relative_path = ?
           AND is_available = 1",
    )
    .bind(&input.document_version_id)
    .bind(&rel)
    .fetch_optional(&state.pool)
    .await?;
    if dup.is_some() {
        return Err(AppError::msg(
            "Esta planilla ya está asociada a esta versión (misma ruta)",
        ));
    }

    let (file_id, already) = link_working_file(
        &state.pool,
        &loc,
        &input.document_version_id,
        &canon,
        Some(label_for_kind(&kind)),
        Some(role_for_kind(&kind)),
        None,
    )
    .await?;
    if already {
        return Err(AppError::msg(
            "Esta planilla ya está asociada a esta versión (misma ruta)",
        ));
    }

    sqlx::query(
        "UPDATE document_files
         SET sheet_kind = ?, version_policy = ?, file_role = ?, format_label = ?, is_available = 1
         WHERE id = ?",
    )
    .bind(&kind)
    .bind(&policy)
    .bind(role_for_kind(&kind))
    .bind(label_for_kind(&kind))
    .bind(&file_id)
    .execute(&state.pool)
    .await?;

    Ok(file_id)
}

/// Detach (mark unavailable) a model spreadsheet association. Does not delete the file on disk.
pub async fn detach_model_spreadsheet_inner(
    state: &AppState,
    file_id: &str,
) -> AppResult<()> {
    let row: Option<(Option<String>,)> = sqlx::query_as(
        "SELECT sheet_kind FROM document_files WHERE id = ?",
    )
    .bind(file_id)
    .fetch_optional(&state.pool)
    .await?;
    let Some((kind,)) = row else {
        return Err(AppError::msg("Archivo no encontrado"));
    };
    if kind.is_none() {
        return Err(AppError::msg(
            "El archivo no es una planilla asociada a modelo",
        ));
    }
    sqlx::query("UPDATE document_files SET is_available = 0 WHERE id = ?")
        .bind(file_id)
        .execute(&state.pool)
        .await?;
    Ok(())
}

pub async fn spreadsheet_status_for_version(
    state: &AppState,
    document_id: &str,
    version_id: &str,
) -> AppResult<ModelSpreadsheetStatus> {
    let current: Option<(Option<String>,)> =
        sqlx::query_as("SELECT current_version_id FROM documents WHERE id = ?")
            .bind(document_id)
            .fetch_optional(&state.pool)
            .await?;
    let is_current = current
        .and_then(|c| c.0)
        .as_deref()
        == Some(version_id);

    let rows: Vec<(String, String, String)> = sqlx::query_as(
        "SELECT id, sheet_kind, relative_path FROM document_files
         WHERE document_version_id = ?
           AND sheet_kind IS NOT NULL
           AND is_available = 1",
    )
    .bind(version_id)
    .fetch_all(&state.pool)
    .await?;

    let mut calc_file_ids = Vec::new();
    let mut has_live_calc = false;
    let mut export_file_ids = Vec::new();
    for (id, kind, rel) in rows {
        if kind == SHEET_KIND_CALC {
            if !rel.contains(".ingmanager/snapshots/") {
                has_live_calc = true;
            }
            calc_file_ids.push(id);
        } else if kind == SHEET_KIND_EXPORT {
            export_file_ids.push(id);
        }
    }
    let has_export = !export_file_ids.is_empty();
    Ok(ModelSpreadsheetStatus {
        document_id: document_id.to_string(),
        document_version_id: version_id.to_string(),
        is_current,
        has_live_calc,
        has_export,
        export_pending: is_current && !has_export,
        calc_file_ids,
        export_file_ids,
    })
}

/// After a successful physical next version: snapshot calc sheets onto the old
/// version and re-link working originals to the new current. Exports stay put.
pub async fn promote_spreadsheets_on_next_version(
    state: &AppState,
    document_id: &str,
    old_version_id: &str,
    new_version_id: &str,
) -> AppResult<()> {
    let project_id: (String,) =
        sqlx::query_as("SELECT project_id FROM documents WHERE id = ?")
            .bind(document_id)
            .fetch_one(&state.pool)
            .await?;
    let loc = project_paths::resolve_project_location(&state.pool, &project_id.0).await?;

    let old_version_number: (i64,) =
        sqlx::query_as("SELECT version_number FROM document_versions WHERE id = ?")
            .bind(old_version_id)
            .fetch_one(&state.pool)
            .await?;

    let sheets: Vec<(String, String, String, Option<String>, Option<String>)> = sqlx::query_as(
        "SELECT id, relative_path, file_name, sheet_kind, version_policy
         FROM document_files
         WHERE document_version_id = ?
           AND sheet_kind IS NOT NULL
           AND is_available = 1",
    )
    .bind(old_version_id)
    .fetch_all(&state.pool)
    .await?;

    for (file_id, rel, file_name, kind, policy) in sheets {
        let Some(kind) = kind else { continue };
        let policy = policy.unwrap_or_else(|| {
            default_policy_for_kind(&kind)
                .unwrap_or(POLICY_VERSION_BOUND)
                .to_string()
        });

        // Exports (and any version_bound sheet) stay on the old version.
        if kind == SHEET_KIND_EXPORT || policy == POLICY_VERSION_BOUND {
            continue;
        }

        // Skip already-frozen snapshot rows.
        if rel.contains(".ingmanager/snapshots/") {
            continue;
        }

        let src = loc.root_path.join(&rel);
        if !src.is_file() {
            sqlx::query("UPDATE document_files SET is_available = 0 WHERE id = ?")
                .bind(&file_id)
                .execute(&state.pool)
                .await?;
            continue;
        }

        // 1) Re-link working path onto the new current first (old row still intact on failure).
        let (new_id, _) = link_working_file(
            &state.pool,
            &loc,
            new_version_id,
            &src,
            Some(label_for_kind(&kind)),
            Some(role_for_kind(&kind)),
            None,
        )
        .await?;
        sqlx::query(
            "UPDATE document_files
             SET sheet_kind = ?, version_policy = ?, file_role = ?, format_label = ?
             WHERE id = ?",
        )
        .bind(&kind)
        .bind(&policy)
        .bind(role_for_kind(&kind))
        .bind(label_for_kind(&kind))
        .bind(&new_id)
        .execute(&state.pool)
        .await?;

        // 2) Freeze a snapshot onto the outgoing version (do NOT flip version is_snapshot).
        snapshot_sheet_onto_version(
            &state.pool,
            &loc,
            document_id,
            old_version_id,
            old_version_number.0,
            &src,
            &file_name,
            &kind,
            &policy,
        )
        .await?;

        // 3) Detach working link from old version (snapshot remains available).
        sqlx::query("UPDATE document_files SET is_available = 0 WHERE id = ?")
            .bind(&file_id)
            .execute(&state.pool)
            .await?;
    }

    Ok(())
}

/// Copy workbook into `.ingmanager/snapshots/.../sheets/` and register on `version_id`.
/// Does **not** set `document_versions.is_snapshot` (model versions stay editable).
async fn snapshot_sheet_onto_version(
    pool: &SqlitePool,
    loc: &ProjectLocation,
    document_id: &str,
    version_id: &str,
    version_number: i64,
    src: &Path,
    file_name: &str,
    kind: &str,
    policy: &str,
) -> AppResult<String> {
    let snap_dir = loc
        .root_path
        .join(".ingmanager")
        .join("snapshots")
        .join(document_id)
        .join(format!("v{version_number:02}"))
        .join("sheets");
    fs::create_dir_all(&snap_dir)?;
    let dest = {
        let candidate = snap_dir.join(file_name);
        if candidate.exists() {
            crate::fs_util::unique_destination(&snap_dir, file_name)
        } else {
            candidate
        }
    };
    fs::copy(src, &dest)?;
    let snap_rel = project_paths::rel_from_project_root(loc, &dest)?;
    let extension = dest
        .extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase());
    let meta = fs::metadata(&dest)?;
    let modified = meta
        .modified()
        .ok()
        .and_then(|t| {
            let dt: chrono::DateTime<chrono::Utc> = t.into();
            Some(dt.to_rfc3339())
        });
    let fp = {
        let modified = modified.clone().unwrap_or_default();
        format!("{}:{}", meta.len(), modified)
    };

    let id = Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO document_files (
            id, document_version_id, relative_path, file_name, extension,
            size_bytes, modified_at, content_hash, is_available, format_label,
            revision_id, created_at, file_role, sheet_kind, version_policy
         ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, 1, ?, NULL, datetime('now'), ?, ?, ?)",
    )
    .bind(&id)
    .bind(version_id)
    .bind(&snap_rel)
    .bind(file_name)
    .bind(&extension)
    .bind(meta.len() as i64)
    .bind(&modified)
    .bind(&fp)
    .bind(label_for_kind(kind))
    .bind(role_for_kind(kind))
    .bind(kind)
    .bind(policy)
    .execute(pool)
    .await?;
    Ok(id)
}
