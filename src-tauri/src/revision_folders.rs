//! Physical folders per revision (memorias/planos/modelos) and version folders
//! for legacy modelos without `_R`.
//!
//! Scheme (stable, short, aligned with `_RA` / `_R0` filename tokens):
//!
//! ```text
//! 04_MEMORIAS/
//!   RA/
//!     1126-MC-002_RA_v01.docx
//!   RB/
//!     1126-MC-002_RB_v01.docx
//!   R0/
//!     …
//! 02_MODELOS/
//!   RB/                      # when `_R` present (parity with docs)
//!     1126-SAP-001_RB_v01.sdb
//!   v01/                     # legacy: only `_vNN`, no `_R`
//!     Modelo_v01.sdb
//! ```
//!
//! Files without a parseable revision (memorias/planos) or `_R`/`_vNN` (modelos) stay flat.

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use uuid::Uuid;

use crate::doc_revisions::{
    category_uses_revisions, format_revision_token, parse_compound_stem, parse_revision_filename,
};
use crate::error::{AppError, AppResult};
use crate::fs_util::{move_entry, unique_destination};
use crate::path_security::ensure_under_root;
use crate::project_paths::{self, ProjectLocation};

/// Folder name for a revision code: `A` → `RA`, `0` → `R0`.
pub fn revision_folder_name(code: &str) -> String {
    format_revision_token(code)
}

/// Version-only folder for modelos: `1` → `v01`.
pub fn version_folder_name(n: u32) -> String {
    let n = if n < 1 { 1 } else { n };
    format!("v{n:02}")
}

/// True when `name` looks like a revision folder (`RA`, `R0`, `RB`, …).
pub fn is_revision_folder_name(name: &str) -> bool {
    let t = name.trim();
    if t.is_empty() {
        return false;
    }
    match parse_revision_filename(&format!("x_{t}.docx")) {
        Some(p) => format_revision_token(&p.revision_code).eq_ignore_ascii_case(t),
        None => false,
    }
}

/// True when `name` is `v01` / `v12` style (model version folder).
pub fn is_version_folder_name(name: &str) -> bool {
    let t = name.trim();
    let lower = t.to_ascii_lowercase();
    if let Some(num) = lower.strip_prefix('v') {
        !num.is_empty() && num.len() <= 3 && num.chars().all(|c| c.is_ascii_digit())
    } else {
        false
    }
}

/// Category directory for a working file: strip one trailing `RA`/`v01` segment if present.
pub fn category_dir_for_working_file(source: &Path) -> PathBuf {
    let parent = match source.parent() {
        Some(p) => p,
        None => return source.to_path_buf(),
    };
    let name = parent
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("");
    if is_revision_folder_name(name) || is_version_folder_name(name) {
        parent
            .parent()
            .unwrap_or(parent)
            .to_path_buf()
    } else {
        parent.to_path_buf()
    }
}

/// Relative subfolder under the category for this file (or `None` → stay flat).
pub fn target_subfolder_for_file(file_name: &str, category_key: &str) -> Option<String> {
    let stem = Path::new(file_name)
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| file_name.to_string());
    let compound = parse_compound_stem(&stem);

    // Modelos: prefer `_R` folder (RB/…) when present; else legacy `vNN/`.
    if category_key == "modelos" {
        if let Some(r) = &compound.revision {
            return Some(revision_folder_name(&r.revision_code));
        }
        return compound.version_number.map(version_folder_name);
    }

    if category_uses_revisions(category_key) {
        return compound
            .revision
            .as_ref()
            .map(|r| revision_folder_name(&r.revision_code));
    }

    None
}

/// Compute absolute destination for a working file under revision/version folders.
/// Returns `None` when the file should stay flat (no revision/version token).
pub fn compute_dest_path(
    source: &Path,
    category_abs: &Path,
    category_key: &str,
) -> Option<PathBuf> {
    let file_name = source.file_name()?.to_str()?;
    let sub = target_subfolder_for_file(file_name, category_key)?;
    let dest_dir = category_abs.join(&sub);
    Some(dest_dir.join(file_name))
}

/// Allocate `{identity}_R{code}_vNN` under `category/RA/` when `into_revision_folder`,
/// else beside `source` (legacy).
pub fn allocate_revision_working_path(
    source: &Path,
    revision_code: &str,
    start_version: i64,
    into_revision_folder: bool,
) -> Result<PathBuf, String> {
    let file_name = source
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or_else(|| "Nombre de archivo inválido".to_string())?;
    let parent = if into_revision_folder {
        let category = category_dir_for_working_file(source);
        category.join(revision_folder_name(revision_code))
    } else {
        source
            .parent()
            .ok_or_else(|| "Archivo fuente sin directorio padre".to_string())?
            .to_path_buf()
    };
    let mut n = start_version.max(1);
    for _ in 0..1000 {
        let candidate_name =
            crate::doc_revisions::apply_revision_version_filename(file_name, revision_code, n);
        let dest = parent.join(&candidate_name);
        if !dest.exists() {
            if into_revision_folder {
                fs::create_dir_all(&parent).map_err(|e| e.to_string())?;
            }
            return Ok(dest);
        }
        n += 1;
    }
    Err("No hay nombre libre para la revisión física".into())
}

/// Next `_vNN` path: with folders enabled, place under `RA/` or `vNN/` as applicable.
pub fn allocate_next_working_path(
    source: &Path,
    next_version_number: i64,
    category_key: Option<&str>,
    into_folders: bool,
) -> Result<PathBuf, String> {
    let file_name = source
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or_else(|| "Nombre de archivo inválido".to_string())?;
    let mut n = next_version_number.max(1);
    for _ in 0..1000 {
        let candidate_name = crate::engineering_files::next_working_filename(file_name, n);
        let parent = if into_folders {
            let category = category_dir_for_working_file(source);
            let key = category_key.unwrap_or("");
            match target_subfolder_for_file(&candidate_name, key) {
                Some(sub) => {
                    let dir = category.join(sub);
                    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
                    dir
                }
                None => source
                    .parent()
                    .ok_or_else(|| "Archivo fuente sin directorio padre".to_string())?
                    .to_path_buf(),
            }
        } else {
            source
                .parent()
                .ok_or_else(|| "Archivo fuente sin directorio padre".to_string())?
                .to_path_buf()
        };
        let dest = parent.join(&candidate_name);
        if !dest.exists() {
            return Ok(dest);
        }
        n += 1;
    }
    Err("No hay nombre libre para la siguiente versión física".into())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RevisionFolderMoveItem {
    pub document_file_id: String,
    pub category_key: String,
    pub source_relative_path: String,
    pub dest_relative_path: String,
    pub action: String, // move | skip | conflict_rename
    pub detail: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RevisionFolderPreview {
    pub project_id: String,
    pub session_id: String,
    pub items: Vec<RevisionFolderMoveItem>,
    pub to_move: usize,
    pub skipped: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RevisionFolderApplyResult {
    pub session_id: String,
    pub moved: usize,
    pub skipped: usize,
    pub failed: usize,
    pub backup_absolute_path: Option<String>,
}

async fn project_uses_folders(pool: &SqlitePool, project_id: &str) -> AppResult<bool> {
    let flag: i64 = sqlx::query_as(
        "SELECT COALESCE(physical_revision_folders, 0) FROM projects WHERE id = ?",
    )
    .bind(project_id)
    .fetch_optional(pool)
    .await?
    .map(|r: (i64,)| r.0)
    .unwrap_or(0);
    Ok(flag != 0)
}

/// Public helper for emitters: read project flag.
pub async fn project_physical_revision_folders(
    pool: &SqlitePool,
    project_id: &str,
) -> AppResult<bool> {
    project_uses_folders(pool, project_id).await
}

fn category_abs(loc: &ProjectLocation, category_rel: &str) -> AppResult<PathBuf> {
    let rel = category_rel.replace('\\', "/");
    let path = loc.absolute_path.join(rel.trim_start_matches('/'));
    ensure_under_root(&loc.root_path, &path)?;
    Ok(path)
}

async fn build_plan_items(
    pool: &SqlitePool,
    loc: &ProjectLocation,
    project_id: &str,
) -> AppResult<(Vec<RevisionFolderMoveItem>, usize, usize)> {
    let rows: Vec<(String, String, String, String)> = sqlx::query_as(
        "SELECT f.id, f.relative_path, f.file_name, c.key
         FROM document_files f
         INNER JOIN document_versions v ON v.id = f.document_version_id
         INNER JOIN documents d ON d.id = v.document_id
         INNER JOIN categories c ON c.id = d.category_id
         WHERE d.project_id = ?
           AND f.is_available = 1
           AND f.relative_path NOT LIKE '%/.ingmanager/%'
           AND f.relative_path NOT LIKE '%.ingmanager/%'
           AND c.key IN ('memorias', 'planos', 'modelos')
         ORDER BY c.key, f.relative_path",
    )
    .bind(project_id)
    .fetch_all(pool)
    .await?;

    let cats: Vec<(String, String)> = sqlx::query_as(
        "SELECT key, relative_path FROM categories WHERE project_id = ?",
    )
    .bind(project_id)
    .fetch_all(pool)
    .await?;
    let cat_map: std::collections::HashMap<String, String> = cats.into_iter().collect();

    let mut items = Vec::new();
    let mut to_move = 0usize;
    let mut skipped = 0usize;

    for (file_id, rel, file_name, cat_key) in rows {
        let Some(cat_rel) = cat_map.get(&cat_key) else {
            skipped += 1;
            items.push(RevisionFolderMoveItem {
                document_file_id: file_id,
                category_key: cat_key,
                source_relative_path: rel,
                dest_relative_path: String::new(),
                action: "skip".into(),
                detail: Some("Categoría sin carpeta física".into()),
            });
            continue;
        };

        let src_abs = match crate::path_security::join_under_root(&loc.root_path, &rel) {
            Ok(p) if p.is_file() => p,
            _ => {
                skipped += 1;
                items.push(RevisionFolderMoveItem {
                    document_file_id: file_id,
                    category_key: cat_key,
                    source_relative_path: rel,
                    dest_relative_path: String::new(),
                    action: "skip".into(),
                    detail: Some("Archivo no encontrado en disco".into()),
                });
                continue;
            }
        };

        let cat_abs = match category_abs(loc, cat_rel) {
            Ok(p) => p,
            Err(e) => {
                skipped += 1;
                items.push(RevisionFolderMoveItem {
                    document_file_id: file_id,
                    category_key: cat_key,
                    source_relative_path: rel,
                    dest_relative_path: String::new(),
                    action: "skip".into(),
                    detail: Some(e.to_string()),
                });
                continue;
            }
        };

        let Some(mut dest_abs) = compute_dest_path(&src_abs, &cat_abs, &cat_key) else {
            skipped += 1;
            items.push(RevisionFolderMoveItem {
                document_file_id: file_id,
                category_key: cat_key,
                source_relative_path: rel.clone(),
                dest_relative_path: rel,
                action: "skip".into(),
                detail: Some("Sin token de revisión/versión → plano".into()),
            });
            continue;
        };

        // Idempotent: already in place
        if paths_same_file(&src_abs, &dest_abs) {
            skipped += 1;
            items.push(RevisionFolderMoveItem {
                document_file_id: file_id,
                category_key: cat_key,
                source_relative_path: rel.clone(),
                dest_relative_path: rel,
                action: "skip".into(),
                detail: Some("Ya en carpeta correcta".into()),
            });
            continue;
        }

        let mut action = "move".to_string();
        let mut detail = None;
        if dest_abs.exists() && !paths_same_file(&src_abs, &dest_abs) {
            let parent = dest_abs.parent().unwrap_or(&cat_abs).to_path_buf();
            dest_abs = unique_destination(&parent, &file_name);
            action = "conflict_rename".into();
            detail = Some("Conflicto de destino → rename seguro".into());
        }

        let dest_rel = project_paths::rel_from_project_root(loc, &dest_abs)?;
        to_move += 1;
        items.push(RevisionFolderMoveItem {
            document_file_id: file_id,
            category_key: cat_key,
            source_relative_path: rel,
            dest_relative_path: dest_rel,
            action,
            detail,
        });
    }

    Ok((items, to_move, skipped))
}

/// Build preview of moves for a project (does not require the flag).
pub async fn preview_reorg(pool: &SqlitePool, project_id: &str) -> AppResult<RevisionFolderPreview> {
    let loc = project_paths::resolve_project_location(pool, project_id).await?;
    let (items, to_move, skipped) = build_plan_items(pool, &loc, project_id).await?;

    let session_id = Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO revision_folder_sessions (id, project_id, status, notes, created_at)
         VALUES (?, ?, 'previewed', ?, datetime('now'))",
    )
    .bind(&session_id)
    .bind(project_id)
    .bind(format!("preview: {to_move} moves, {skipped} skipped"))
    .execute(pool)
    .await?;

    Ok(RevisionFolderPreview {
        project_id: project_id.to_string(),
        session_id,
        items,
        to_move,
        skipped,
    })
}

fn paths_same_file(a: &Path, b: &Path) -> bool {
    if a == b {
        return true;
    }
    match (fs::canonicalize(a), fs::canonicalize(b)) {
        (Ok(ca), Ok(cb)) => ca == cb,
        _ => false,
    }
}

/// Apply a previewed reorg: move files, update document_files + indexed_files, journal + backup.
pub async fn apply_reorg(
    pool: &SqlitePool,
    project_id: &str,
    session_id: &str,
) -> AppResult<RevisionFolderApplyResult> {
    let loc = project_paths::resolve_project_location(pool, project_id).await?;

    let session: Option<(String, String)> = sqlx::query_as(
        "SELECT id, status FROM revision_folder_sessions WHERE id = ? AND project_id = ?",
    )
    .bind(session_id)
    .bind(project_id)
    .fetch_optional(pool)
    .await?;
    let Some((_, status)) = session else {
        return Err(AppError::msg("Sesión de reorganización no encontrada"));
    };
    if status != "previewed" && status != "draft" {
        return Err(AppError::msg(format!(
            "Sesión en estado inválido para aplicar: {status}"
        )));
    }

    // Rebuild plan (fresh) so we don't trust stale client payloads alone.
    let (items, _to_move, _skipped_preview) = build_plan_items(pool, &loc, project_id).await?;
    let preview_items = items;

    let backup_dir = loc
        .root_path
        .join(".ingmanager")
        .join("revision_folder_backups")
        .join(session_id);
    fs::create_dir_all(&backup_dir)?;
    let backup_str = backup_dir.display().to_string();

    sqlx::query(
        "UPDATE revision_folder_sessions SET backup_absolute_path = ?, status = 'draft'
         WHERE id = ?",
    )
    .bind(&backup_str)
    .bind(session_id)
    .execute(pool)
    .await?;

    let mut moved = 0usize;
    let mut skipped = 0usize;
    let mut failed = 0usize;

    for item in &preview_items {
        if item.action == "skip" {
            skipped += 1;
            write_journal(
                pool,
                session_id,
                "skip",
                Some(&item.source_relative_path),
                Some(&item.dest_relative_path),
                true,
                item.detail.as_deref().unwrap_or("skip"),
            )
            .await?;
            continue;
        }

        let src = match crate::path_security::join_under_root(
            &loc.root_path,
            &item.source_relative_path,
        ) {
            Ok(p) => p,
            Err(e) => {
                failed += 1;
                write_journal(
                    pool,
                    session_id,
                    "move",
                    Some(&item.source_relative_path),
                    Some(&item.dest_relative_path),
                    false,
                    &e.to_string(),
                )
                .await?;
                continue;
            }
        };
        let dest = match crate::path_security::join_under_root(
            &loc.root_path,
            &item.dest_relative_path,
        ) {
            Ok(p) => p,
            Err(e) => {
                failed += 1;
                write_journal(
                    pool,
                    session_id,
                    "move",
                    Some(&item.source_relative_path),
                    Some(&item.dest_relative_path),
                    false,
                    &e.to_string(),
                )
                .await?;
                continue;
            }
        };

        // Light backup: copy source into backup tree preserving relative path.
        let backup_target = backup_dir.join(&item.source_relative_path);
        if let Some(parent) = backup_target.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let _ = fs::copy(&src, &backup_target);

        match move_entry(&src, &dest) {
            Ok(()) => {
                // Update document_files
                if let Err(e) = sqlx::query(
                    "UPDATE document_files SET relative_path = ? WHERE id = ?",
                )
                .bind(&item.dest_relative_path)
                .bind(&item.document_file_id)
                .execute(pool)
                .await
                {
                    failed += 1;
                    write_journal(
                        pool,
                        session_id,
                        "move",
                        Some(&item.source_relative_path),
                        Some(&item.dest_relative_path),
                        false,
                        &format!("FS ok, DB falló: {e}"),
                    )
                    .await?;
                    continue;
                }

                // Also fix source_relative_path on versions that pointed at old path
                let _ = sqlx::query(
                    "UPDATE document_versions SET source_relative_path = ?
                     WHERE source_relative_path = ?",
                )
                .bind(&item.dest_relative_path)
                .bind(&item.source_relative_path)
                .execute(pool)
                .await;

                // indexed_files: drop old, upsert new
                let _ = sqlx::query(
                    "DELETE FROM indexed_files WHERE authorized_root_id = ? AND relative_path = ?",
                )
                .bind(&loc.authorized_root_id)
                .bind(&item.source_relative_path)
                .execute(pool)
                .await;
                let _ = crate::index_sync::upsert_path(pool, &loc.root_path, &dest).await;

                moved += 1;
                write_journal(
                    pool,
                    session_id,
                    "move",
                    Some(&item.source_relative_path),
                    Some(&item.dest_relative_path),
                    true,
                    item.detail.as_deref().unwrap_or("ok"),
                )
                .await?;
            }
            Err(e) => {
                failed += 1;
                write_journal(
                    pool,
                    session_id,
                    "move",
                    Some(&item.source_relative_path),
                    Some(&item.dest_relative_path),
                    false,
                    &e.to_string(),
                )
                .await?;
            }
        }
    }

    let final_status = if failed > 0 { "failed" } else { "applied" };
    sqlx::query(
        "UPDATE revision_folder_sessions SET status = ?, applied_at = datetime('now') WHERE id = ?",
    )
    .bind(final_status)
    .bind(session_id)
    .execute(pool)
    .await?;

    crate::audit::log_event(
        pool,
        Some(project_id),
        "project",
        Some(project_id),
        "revision_folder_reorg",
        Some(&format!(
            "session={session_id} moved={moved} skipped={skipped} failed={failed}"
        )),
    )
    .await
    .ok();

    Ok(RevisionFolderApplyResult {
        session_id: session_id.to_string(),
        moved,
        skipped,
        failed,
        backup_absolute_path: Some(backup_str),
    })
}

async fn write_journal(
    pool: &SqlitePool,
    session_id: &str,
    op: &str,
    source: Option<&str>,
    dest: Option<&str>,
    ok: bool,
    message: &str,
) -> AppResult<()> {
    let id = Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO revision_folder_journal (
            id, session_id, op, source_path, dest_path, ok, message, created_at
         ) VALUES (?, ?, ?, ?, ?, ?, ?, datetime('now'))",
    )
    .bind(&id)
    .bind(session_id)
    .bind(op)
    .bind(source)
    .bind(dest)
    .bind(if ok { 1 } else { 0 })
    .bind(message)
    .execute(pool)
    .await?;
    Ok(())
}

#[cfg(test)]
mod unit_tests {
    use super::*;

    #[test]
    fn folder_names_from_codes() {
        assert_eq!(revision_folder_name("A"), "RA");
        assert_eq!(revision_folder_name("0"), "R0");
        assert_eq!(revision_folder_name("B"), "RB");
        assert_eq!(version_folder_name(1), "v01");
        assert_eq!(version_folder_name(12), "v12");
    }

    #[test]
    fn detects_revision_and_version_folders() {
        assert!(is_revision_folder_name("RA"));
        assert!(is_revision_folder_name("R0"));
        assert!(is_revision_folder_name("RB"));
        assert!(!is_revision_folder_name("MEMORIAS"));
        assert!(!is_revision_folder_name("roof"));
        assert!(is_version_folder_name("v01"));
        assert!(is_version_folder_name("v12"));
        assert!(!is_version_folder_name("v"));
        assert!(!is_version_folder_name("RA"));
    }

    #[test]
    fn target_subfolder_memorias_and_modelos() {
        assert_eq!(
            target_subfolder_for_file("1126-MC-002_RA_v01.docx", "memorias").as_deref(),
            Some("RA")
        );
        assert_eq!(
            target_subfolder_for_file("1126-MC-002_R0.docx", "planos").as_deref(),
            Some("R0")
        );
        assert_eq!(
            target_subfolder_for_file("Memo.docx", "memorias"),
            None
        );
        assert_eq!(
            target_subfolder_for_file("Modelo_v02.sdb", "modelos").as_deref(),
            Some("v02")
        );
        assert_eq!(
            target_subfolder_for_file("1126-SAP-001_RB_v01.sdb", "modelos").as_deref(),
            Some("RB")
        );
        assert_eq!(
            target_subfolder_for_file("1126-SAP-001_RA_v02.sdb", "modelos").as_deref(),
            Some("RA")
        );
        assert_eq!(target_subfolder_for_file("Modelo.sdb", "modelos"), None);
    }

    #[test]
    fn category_dir_strips_rev_folder() {
        let p = PathBuf::from("/root/proj/04_MEMORIAS/RA/file.docx");
        assert_eq!(
            category_dir_for_working_file(&p),
            PathBuf::from("/root/proj/04_MEMORIAS")
        );
        let p2 = PathBuf::from("/root/proj/04_MEMORIAS/file.docx");
        assert_eq!(
            category_dir_for_working_file(&p2),
            PathBuf::from("/root/proj/04_MEMORIAS")
        );
    }
}
