//! Auto-versioning from Save As filename patterns + watcher/reconcile hooks.
//!
//! Rules:
//! - Same path overwrite → update fingerprint only (no new version).
//! - New file whose stem matches exactly one tracked document → create version + snapshot.
//! - Ambiguous matches → pending_version_events.
//! - Debounce/stability checked by caller before invoking.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::SqlitePool;
use uuid::Uuid;

use crate::doc_codes::version_label;
use crate::error::{AppError, AppResult};
use crate::path_security::{ensure_under_root, join_under_root};
use crate::project_paths;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedVersionName {
    pub base_stem: String,
    pub version_hint: Option<String>,
    pub extension: Option<String>,
    /// True when filename looks like a Save-As / revision variant.
    pub is_version_pattern: bool,
}

/// Parse common Save As / revision patterns from a file name (not a path).
pub fn parse_version_filename(file_name: &str) -> ParsedVersionName {
    let path = Path::new(file_name);
    let ext = path
        .extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase());
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| file_name.to_string());

    if let Some(parsed) = try_parse_stem(&stem) {
        return ParsedVersionName {
            base_stem: normalize_stem(&parsed.0),
            version_hint: parsed.1,
            extension: ext,
            is_version_pattern: true,
        };
    }

    ParsedVersionName {
        base_stem: normalize_stem(&stem),
        version_hint: None,
        extension: ext,
        is_version_pattern: false,
    }
}

fn try_parse_stem(stem: &str) -> Option<(String, Option<String>)> {
    // name (12)
    if let Some(open) = stem.rfind(" (") {
        if stem.ends_with(')') {
            let inner = &stem[open + 2..stem.len() - 1];
            if !inner.is_empty() && inner.chars().all(|c| c.is_ascii_digit()) {
                let base = stem[..open].trim();
                if !base.is_empty() {
                    return Some((base.to_string(), Some(inner.to_string())));
                }
            }
        }
    }

    // name - Copy / name – Copia / name - Copy (2)
    let lower = stem.to_lowercase();
    for marker in [" - copy", " – copy", " - copia", " – copia"] {
        if let Some(idx) = lower.rfind(marker) {
            let base = stem[..idx].trim();
            if base.is_empty() {
                continue;
            }
            let rest = &stem[idx + marker.len()..];
            let hint = parse_trailing_paren_num(rest.trim());
            return Some((base.to_string(), hint));
        }
    }

    // Split on last _ or -
    if let Some(idx) = stem.rfind(['_', '-']) {
        let base = stem[..idx].trim();
        let suffix = &stem[idx + 1..];
        if base.is_empty() || suffix.is_empty() {
            return None;
        }
        let suf_l = suffix.to_lowercase();

        // v01 / v1
        if suf_l.starts_with('v') {
            let num = &suf_l[1..];
            if !num.is_empty() && num.chars().all(|c| c.is_ascii_digit()) {
                return Some((base.to_string(), Some(num.to_string())));
            }
        }

        // revA / rev.B / rev02
        if let Some(rest) = suf_l.strip_prefix("rev") {
            let rest = rest.trim_start_matches('.');
            if !rest.is_empty()
                && rest.len() <= 4
                && rest.chars().all(|c| c.is_ascii_alphanumeric())
            {
                // Preserve original hint casing from suffix after "rev"
                let raw = suffix;
                let hint = if let Some(r) = raw
                    .to_lowercase()
                    .strip_prefix("rev")
                    .map(|_| &raw[3..])
                {
                    r.trim_start_matches('.').to_string()
                } else {
                    rest.to_string()
                };
                return Some((base.to_string(), Some(hint)));
            }
        }

        // copy / copia / copy2 / copia_3
        if suf_l == "copy" || suf_l == "copia" {
            return Some((base.to_string(), None));
        }
        for prefix in ["copy", "copia"] {
            if let Some(rest) = suf_l.strip_prefix(prefix) {
                let rest = rest.trim_start_matches(['_', '-']);
                if rest.is_empty() {
                    return Some((base.to_string(), None));
                }
                if rest.chars().all(|c| c.is_ascii_digit()) {
                    return Some((base.to_string(), Some(rest.to_string())));
                }
            }
        }
    }

    None
}

fn parse_trailing_paren_num(s: &str) -> Option<String> {
    let s = s.trim();
    if s.starts_with('(') && s.ends_with(')') {
        let inner = &s[1..s.len() - 1];
        if !inner.is_empty() && inner.chars().all(|c| c.is_ascii_digit()) {
            return Some(inner.to_string());
        }
    }
    None
}

fn normalize_stem(s: &str) -> String {
    // Strip project-code style prefixes; keep SIN/CON CAMIONES (and similar) distinct.
    crate::engineering_files::normalize_group_key(s, &[])
}

/// Normalize with explicit project codes (preferred when known).
pub fn normalize_stem_with_codes(s: &str, project_codes: &[String]) -> String {
    crate::engineering_files::normalize_group_key(s, project_codes)
}

/// Wait until size+mtime are stable (or timeout). Returns false if file vanishes.
pub fn wait_until_stable(path: &Path, settle: Duration, max_wait: Duration) -> bool {
    let start = SystemTime::now();
    let mut last_size: Option<u64> = None;
    let mut last_mtime: Option<SystemTime> = None;
    let mut stable_since: Option<SystemTime> = None;

    while start.elapsed().unwrap_or(max_wait) < max_wait {
        let meta = match fs::metadata(path) {
            Ok(m) if m.is_file() => m,
            _ => return false,
        };
        let size = meta.len();
        let mtime = meta.modified().ok();
        if last_size == Some(size) && last_mtime == mtime {
            if stable_since.is_none() {
                stable_since = Some(SystemTime::now());
            }
            if let Some(since) = stable_since {
                if since.elapsed().unwrap_or_default() >= settle {
                    return true;
                }
            }
        } else {
            last_size = Some(size);
            last_mtime = mtime;
            stable_since = None;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    path.is_file()
}

pub fn fingerprint_meta(path: &Path) -> AppResult<(String, u64, Option<String>)> {
    use std::io::{Read, Seek, SeekFrom};

    let meta = fs::metadata(path)?;
    let size = meta.len();
    let modified = meta.modified().ok().and_then(|t| {
        let dt: DateTime<Utc> = t.into();
        Some(dt.to_rfc3339())
    });
    let mut hasher = Sha256::new();
    hasher.update(size.to_le_bytes());
    if let Some(ref m) = modified {
        hasher.update(m.as_bytes());
    }
    hasher.update(path.to_string_lossy().as_bytes());

    // Never read the whole file into memory — large engineering models (GB)
    // previously caused OOM crashes during open reconcile.
    if let Ok(mut file) = fs::File::open(path) {
        let mut head = vec![0u8; 4096.min(size as usize)];
        if let Ok(n) = file.read(&mut head) {
            hasher.update(&head[..n]);
        }
        if size > 4096 {
            let tail_len = 1024u64.min(size);
            let start = size.saturating_sub(tail_len);
            if file.seek(SeekFrom::Start(start)).is_ok() {
                let mut tail = vec![0u8; tail_len as usize];
                if let Ok(n) = file.read(&mut tail) {
                    hasher.update(&tail[..n]);
                }
            }
        }
        hasher.update(size.to_le_bytes());
    }
    Ok((hex::encode(hasher.finalize()), size, modified))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutoVersionResult {
    pub path: String,
    pub action: String,
    pub document_id: Option<String>,
    pub version_id: Option<String>,
    pub pending_id: Option<String>,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct PendingVersionEvent {
    pub id: String,
    pub project_id: String,
    pub absolute_path: String,
    pub file_name: String,
    pub base_stem: String,
    pub version_hint: Option<String>,
    pub reason: String,
    pub candidate_document_ids_json: Option<String>,
    pub status: String,
    pub resolved_document_id: Option<String>,
    pub created_at: String,
    pub resolved_at: Option<String>,
}

fn snapshots_dir(root: &Path, document_id: &str, version_number: i64) -> PathBuf {
    root.join(".ingmanager")
        .join("snapshots")
        .join(document_id)
        .join(format!("v{version_number:02}"))
}

/// Process a single new/changed file path for auto-versioning.
pub async fn process_path(
    pool: &SqlitePool,
    absolute_path: &Path,
) -> AppResult<Option<AutoVersionResult>> {
    if !absolute_path.is_file() {
        return Ok(None);
    }
    let file_name = absolute_path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    if file_name.is_empty() || file_name.starts_with('.') {
        return Ok(None);
    }
    if absolute_path.components().any(|c| {
        c.as_os_str()
            .to_string_lossy()
            .eq_ignore_ascii_case(".ingmanager")
    }) {
        return Ok(None);
    }

    let abs_str = absolute_path.display().to_string();
    let parsed = parse_version_filename(&file_name);
    let (fp, size, modified) = fingerprint_meta(absolute_path)?;

    let project_id = match find_project_for_path(pool, absolute_path).await? {
        Some(id) => id,
        None => return Ok(None),
    };

    if let Some((track_id, last_fp, doc_id, ver_id)) =
        load_tracking(pool, &project_id, &abs_str).await?
    {
        if last_fp.as_deref() == Some(fp.as_str()) {
            return Ok(Some(AutoVersionResult {
                path: abs_str,
                action: "unchanged".into(),
                document_id: Some(doc_id),
                version_id: ver_id,
                pending_id: None,
                message: "Sin cambios de contenido".into(),
            }));
        }
        sqlx::query(
            "UPDATE file_version_tracking SET
                last_fingerprint = ?, last_size_bytes = ?, last_modified_at = ?,
                updated_at = datetime('now')
             WHERE id = ?",
        )
        .bind(&fp)
        .bind(size as i64)
        .bind(&modified)
        .bind(&track_id)
        .execute(pool)
        .await?;
        if let Some(ref vid) = ver_id {
            sqlx::query(
                "UPDATE document_files SET content_hash = ?, size_bytes = ?, modified_at = ?
                 WHERE document_version_id = ? AND file_name = ?",
            )
            .bind(&fp)
            .bind(size as i64)
            .bind(&modified)
            .bind(vid)
            .bind(&file_name)
            .execute(pool)
            .await
            .ok();
        }
        return Ok(Some(AutoVersionResult {
            path: abs_str,
            action: "overwrite_update".into(),
            document_id: Some(doc_id),
            version_id: ver_id,
            pending_id: None,
            message: "Sobrescritura detectada — sin nueva versión".into(),
        }));
    }

    let candidates =
        find_stem_candidates(pool, &project_id, &parsed.base_stem, parsed.extension.as_deref())
            .await?;

    if candidates.is_empty() {
        if !parsed.is_version_pattern {
            return Ok(None);
        }
        let pending_id = upsert_pending(
            pool,
            &project_id,
            &abs_str,
            &file_name,
            &parsed.base_stem,
            parsed.version_hint.as_deref(),
            "unmatched_pattern",
            &[],
        )
        .await?;
        return Ok(Some(AutoVersionResult {
            path: abs_str,
            action: "pending".into(),
            document_id: None,
            version_id: None,
            pending_id: Some(pending_id),
            message: "Patrón de versión sin documento coincidente".into(),
        }));
    }

    if candidates.len() > 1 {
        let ids: Vec<String> = candidates.iter().map(|(id, _)| id.clone()).collect();
        let pending_id = upsert_pending(
            pool,
            &project_id,
            &abs_str,
            &file_name,
            &parsed.base_stem,
            parsed.version_hint.as_deref(),
            "ambiguous",
            &ids,
        )
        .await?;
        return Ok(Some(AutoVersionResult {
            path: abs_str,
            action: "pending".into(),
            document_id: None,
            version_id: None,
            pending_id: Some(pending_id),
            message: format!("Ambiguo: {} documentos candidatos", ids.len()),
        }));
    }

    let (document_id, _) = &candidates[0];
    let created =
        create_auto_version(pool, &project_id, document_id, absolute_path, &parsed).await?;
    Ok(Some(created))
}

async fn load_tracking(
    pool: &SqlitePool,
    project_id: &str,
    abs: &str,
) -> AppResult<Option<(String, Option<String>, String, Option<String>)>> {
    let row: Option<(String, Option<String>, String, Option<String>)> = sqlx::query_as(
        "SELECT id, last_fingerprint, document_id, document_version_id
         FROM file_version_tracking
         WHERE project_id = ? AND absolute_path = ?",
    )
    .bind(project_id)
    .bind(abs)
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

async fn find_project_for_path(pool: &SqlitePool, path: &Path) -> AppResult<Option<String>> {
    let projects: Vec<(String, String, String)> = sqlx::query_as(
        "SELECT p.id, r.absolute_path, p.root_relative_path
         FROM projects p
         JOIN authorized_roots r ON r.id = p.authorized_root_id
         WHERE p.status = 'active'",
    )
    .fetch_all(pool)
    .await?;

    let mut best: Option<(usize, String)> = None;
    for (id, root, rel) in projects {
        let proj = PathBuf::from(&root).join(&rel);
        if path.starts_with(&proj) {
            let len = proj.as_os_str().len();
            if best.as_ref().map(|(l, _)| len > *l).unwrap_or(true) {
                best = Some((len, id));
            }
        }
    }
    Ok(best.map(|(_, id)| id))
}

async fn find_stem_candidates(
    pool: &SqlitePool,
    project_id: &str,
    base_stem: &str,
    _extension: Option<&str>,
) -> AppResult<Vec<(String, String)>> {
    let mut from_track: Vec<(String, String)> = sqlx::query_as(
        "SELECT DISTINCT document_id, base_stem FROM file_version_tracking
         WHERE project_id = ? AND base_stem = ?",
    )
    .bind(project_id)
    .bind(base_stem)
    .fetch_all(pool)
    .await?;

    let files: Vec<(String, String)> = sqlx::query_as(
        "SELECT d.id, f.file_name
         FROM documents d
         JOIN document_versions v ON v.id = d.current_version_id
         JOIN document_files f ON f.document_version_id = v.id
         WHERE d.project_id = ? AND f.is_available = 1",
    )
    .bind(project_id)
    .fetch_all(pool)
    .await?;

    for (doc_id, fname) in files {
        let p = parse_version_filename(&fname);
        if p.base_stem == base_stem && !from_track.iter().any(|(id, _)| id == &doc_id) {
            from_track.push((doc_id, p.base_stem));
        }
    }

    let titles: Vec<(String, String)> =
        sqlx::query_as("SELECT id, title FROM documents WHERE project_id = ?")
            .bind(project_id)
            .fetch_all(pool)
            .await?;
    for (doc_id, title) in titles {
        if normalize_stem(&title) == base_stem && !from_track.iter().any(|(id, _)| id == &doc_id) {
            from_track.push((doc_id, base_stem.to_string()));
        }
    }

    Ok(from_track)
}

async fn upsert_pending(
    pool: &SqlitePool,
    project_id: &str,
    abs: &str,
    file_name: &str,
    base_stem: &str,
    hint: Option<&str>,
    reason: &str,
    candidates: &[String],
) -> AppResult<String> {
    let json = serde_json::to_string(candidates)?;
    let existing: Option<(String,)> = sqlx::query_as(
        "SELECT id FROM pending_version_events
         WHERE project_id = ? AND absolute_path = ? AND status = 'pending'",
    )
    .bind(project_id)
    .bind(abs)
    .fetch_optional(pool)
    .await?;
    if let Some((id,)) = existing {
        sqlx::query(
            "UPDATE pending_version_events SET
                file_name = ?, base_stem = ?, version_hint = ?, reason = ?,
                candidate_document_ids_json = ?
             WHERE id = ?",
        )
        .bind(file_name)
        .bind(base_stem)
        .bind(hint)
        .bind(reason)
        .bind(&json)
        .bind(&id)
        .execute(pool)
        .await?;
        return Ok(id);
    }
    let id = Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO pending_version_events (
            id, project_id, absolute_path, file_name, base_stem, version_hint,
            reason, candidate_document_ids_json, status, created_at
         ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, 'pending', datetime('now'))",
    )
    .bind(&id)
    .bind(project_id)
    .bind(abs)
    .bind(file_name)
    .bind(base_stem)
    .bind(hint)
    .bind(reason)
    .bind(&json)
    .execute(pool)
    .await?;
    Ok(id)
}

async fn create_auto_version(
    pool: &SqlitePool,
    project_id: &str,
    document_id: &str,
    source: &Path,
    parsed: &ParsedVersionName,
) -> AppResult<AutoVersionResult> {
    let loc = project_paths::resolve_project_location(pool, project_id).await?;
    let root = &loc.root_path;
    let _ = ensure_under_root(root, source)?;

    let (fp, size, modified) = fingerprint_meta(source)?;
    let file_name = source
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "file".into());
    let extension = source
        .extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase());

    let max_n: (i64,) = sqlx::query_as(
        "SELECT COALESCE(MAX(version_number), 0) FROM document_versions WHERE document_id = ?",
    )
    .bind(document_id)
    .fetch_one(pool)
    .await?;
    let next_n = max_n.0 + 1;
    let version_id = Uuid::new_v4().to_string();
    let notes = match &parsed.version_hint {
        Some(h) => format!("Auto-versión (hint: {h})"),
        None => "Auto-versión por patrón Save As".into(),
    };

    let snap_dir = snapshots_dir(root, document_id, next_n);
    fs::create_dir_all(&snap_dir)?;
    let dest = snap_dir.join(&file_name);
    let dest = if dest.exists() {
        crate::fs_util::unique_destination(&snap_dir, &file_name)
    } else {
        dest
    };
    fs::copy(source, &dest)?;
    let snap_rel = pathdiff_rel(root, &snap_dir)?;
    let stored_rel = pathdiff_rel(root, &dest)?;
    let source_rel = pathdiff_rel(root, source).ok();

    let mut tx = pool.begin().await?;
    sqlx::query(
        "INSERT INTO document_versions (
            id, document_id, version_number, label, notes, created_at,
            snapshot_relative_path, registered_fingerprint, source_relative_path, is_snapshot
         ) VALUES (?, ?, ?, ?, ?, datetime('now'), ?, ?, ?, 1)",
    )
    .bind(&version_id)
    .bind(document_id)
    .bind(next_n)
    .bind(version_label(next_n))
    .bind(&notes)
    .bind(&snap_rel)
    .bind(&fp)
    .bind(&source_rel)
    .execute(&mut *tx)
    .await?;

    sqlx::query(
        "UPDATE documents SET current_version_id = ?, updated_at = datetime('now') WHERE id = ?",
    )
    .bind(&version_id)
    .bind(document_id)
    .execute(&mut *tx)
    .await?;

    let file_id = Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO document_files (
            id, document_version_id, relative_path, file_name, extension,
            size_bytes, modified_at, content_hash, is_available, created_at
         ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, 1, datetime('now'))",
    )
    .bind(&file_id)
    .bind(&version_id)
    .bind(&stored_rel)
    .bind(&file_name)
    .bind(&extension)
    .bind(size as i64)
    .bind(&modified)
    .bind(&fp)
    .execute(&mut *tx)
    .await?;

    let track_id = Uuid::new_v4().to_string();
    let abs = source.display().to_string();
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
    .bind(document_id)
    .bind(&version_id)
    .bind(&abs)
    .bind(&parsed.base_stem)
    .bind(&fp)
    .bind(size as i64)
    .bind(&modified)
    .execute(&mut *tx)
    .await?;

    sqlx::query(
        "UPDATE pending_version_events SET status = 'resolved', resolved_document_id = ?,
            resolved_at = datetime('now')
         WHERE project_id = ? AND absolute_path = ? AND status = 'pending'",
    )
    .bind(document_id)
    .bind(project_id)
    .bind(&abs)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;

    Ok(AutoVersionResult {
        path: abs,
        action: "version_created".into(),
        document_id: Some(document_id.to_string()),
        version_id: Some(version_id),
        pending_id: None,
        message: format!("Versión v{next_n:02} creada automáticamente"),
    })
}

fn pathdiff_rel(root: &Path, path: &Path) -> AppResult<String> {
    let rel = path
        .strip_prefix(root)
        .map_err(|_| AppError::msg("Ruta fuera de la raíz autorizada"))?;
    Ok(rel.to_string_lossy().replace('\\', "/"))
}

/// Register an existing working file as the baseline for a document (no new version).
pub async fn register_baseline(
    pool: &SqlitePool,
    project_id: &str,
    document_id: &str,
    version_id: Option<&str>,
    absolute_path: &Path,
) -> AppResult<()> {
    let parsed = parse_version_filename(
        &absolute_path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default(),
    );
    let (fp, size, modified) = fingerprint_meta(absolute_path)?;
    let id = Uuid::new_v4().to_string();
    let abs = absolute_path.display().to_string();
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
    .bind(&id)
    .bind(project_id)
    .bind(document_id)
    .bind(version_id)
    .bind(&abs)
    .bind(&parsed.base_stem)
    .bind(&fp)
    .bind(size as i64)
    .bind(&modified)
    .execute(pool)
    .await?;
    Ok(())
}

/// Reconcile auto-versioning for all files under a project (on open).
///
/// `cancel` is polled between files so a newer open can abort work.
/// Per-file errors are logged and skipped — they must not fail the whole open.
pub async fn reconcile_project(
    pool: &SqlitePool,
    project_id: &str,
) -> AppResult<Vec<AutoVersionResult>> {
    reconcile_project_cancelable(pool, project_id, None, None).await
}

pub async fn reconcile_project_cancelable(
    pool: &SqlitePool,
    project_id: &str,
    cancel: Option<&std::sync::atomic::AtomicBool>,
    expected_generation: Option<(
        &std::sync::atomic::AtomicU64,
        u64,
    )>,
) -> AppResult<Vec<AutoVersionResult>> {
    use std::sync::atomic::Ordering;

    let loc = project_paths::resolve_project_location(pool, project_id).await?;
    let mut results = Vec::new();
    if !loc.absolute_path.is_dir() {
        return Ok(results);
    }
    let mut scanned: u64 = 0;
    for entry in walkdir::WalkDir::new(&loc.absolute_path)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        let cancelled = cancel.is_some_and(|c| c.load(Ordering::SeqCst))
            || expected_generation.is_some_and(|(cell, gen)| cell.load(Ordering::SeqCst) != gen);
        if cancelled {
            crate::diagnostics::log_event(
                "reconcile",
                &format!("cancelled project={project_id} after {scanned} files"),
            );
            break;
        }
        if !entry.file_type().is_file() {
            continue;
        }
        let path = entry.path();
        if path.components().any(|c| {
            c.as_os_str()
                .to_string_lossy()
                .eq_ignore_ascii_case(".ingmanager")
        }) {
            continue;
        }
        scanned += 1;
        let name = entry.file_name().to_string_lossy();
        let parsed = parse_version_filename(&name);
        if !parsed.is_version_pattern {
            let tracked: (i64,) = match sqlx::query_as(
                "SELECT COUNT(*) FROM file_version_tracking WHERE project_id = ? AND base_stem = ?",
            )
            .bind(project_id)
            .bind(&parsed.base_stem)
            .fetch_one(pool)
            .await
            {
                Ok(v) => v,
                Err(e) => {
                    crate::diagnostics::log_event(
                        "reconcile",
                        &format!("track query err {}: {e}", path.display()),
                    );
                    continue;
                }
            };
            if tracked.0 == 0 {
                continue;
            }
        }
        match process_path(pool, path).await {
            Ok(Some(r)) => results.push(r),
            Ok(None) => {}
            Err(e) => {
                crate::diagnostics::log_event(
                    "reconcile",
                    &format!("skip {}: {e}", path.display()),
                );
            }
        }
        // Yield so the async runtime can service other commands / cancel.
        if scanned % 32 == 0 {
            tokio::task::yield_now().await;
        }
    }
    Ok(results)
}

/// Batch-process watcher paths (after index reconcile).
pub async fn process_watcher_batch(
    pool: &SqlitePool,
    paths: &[PathBuf],
) -> AppResult<Vec<AutoVersionResult>> {
    let mut out = Vec::new();
    for path in paths {
        if path.is_file() {
            let _ = wait_until_stable(path, Duration::from_millis(200), Duration::from_millis(800));
        }
        if let Some(r) = process_path(pool, path).await? {
            out.push(r);
        }
        if path.is_dir() {
            if let Ok(rd) = fs::read_dir(path) {
                for e in rd.flatten() {
                    if e.path().is_file() {
                        if let Some(r) = process_path(pool, &e.path()).await? {
                            out.push(r);
                        }
                    }
                }
            }
        }
    }
    Ok(out)
}

pub async fn list_pending(
    pool: &SqlitePool,
    project_id: Option<&str>,
) -> AppResult<Vec<PendingVersionEvent>> {
    if let Some(pid) = project_id {
        sqlx::query_as::<_, PendingVersionEvent>(
            "SELECT id, project_id, absolute_path, file_name, base_stem, version_hint,
                    reason, candidate_document_ids_json, status, resolved_document_id,
                    created_at, resolved_at
             FROM pending_version_events
             WHERE project_id = ? AND status = 'pending'
             ORDER BY created_at DESC",
        )
        .bind(pid)
        .fetch_all(pool)
        .await
        .map_err(AppError::from)
    } else {
        sqlx::query_as::<_, PendingVersionEvent>(
            "SELECT id, project_id, absolute_path, file_name, base_stem, version_hint,
                    reason, candidate_document_ids_json, status, resolved_document_id,
                    created_at, resolved_at
             FROM pending_version_events
             WHERE status = 'pending'
             ORDER BY created_at DESC",
        )
        .fetch_all(pool)
        .await
        .map_err(AppError::from)
    }
}

pub async fn resolve_pending(
    pool: &SqlitePool,
    pending_id: &str,
    document_id: &str,
) -> AppResult<AutoVersionResult> {
    let row: PendingVersionEvent = sqlx::query_as(
        "SELECT id, project_id, absolute_path, file_name, base_stem, version_hint,
                reason, candidate_document_ids_json, status, resolved_document_id,
                created_at, resolved_at
         FROM pending_version_events WHERE id = ?",
    )
    .bind(pending_id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| AppError::msg("Evento pendiente no encontrado"))?;

    if row.status != "pending" {
        return Err(AppError::msg("El evento ya fue resuelto"));
    }
    let path = PathBuf::from(&row.absolute_path);
    if !path.is_file() {
        return Err(AppError::msg("El archivo ya no existe"));
    }
    let parsed = parse_version_filename(&row.file_name);
    create_auto_version(pool, &row.project_id, document_id, &path, &parsed).await
}

pub async fn dismiss_pending(pool: &SqlitePool, pending_id: &str) -> AppResult<()> {
    sqlx::query(
        "UPDATE pending_version_events SET status = 'dismissed', resolved_at = datetime('now')
         WHERE id = ?",
    )
    .bind(pending_id)
    .execute(pool)
    .await?;
    Ok(())
}

#[allow(dead_code)]
pub fn join_project_file(root: &Path, rel: &str) -> AppResult<PathBuf> {
    join_under_root(root, rel)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_v_suffix() {
        let p = parse_version_filename("Modelo_v02.sdb");
        assert!(p.is_version_pattern);
        assert_eq!(p.base_stem, "modelo");
        assert_eq!(p.version_hint.as_deref(), Some("02"));
        assert_eq!(p.extension.as_deref(), Some("sdb"));
    }

    #[test]
    fn parses_paren_and_copy() {
        let p = parse_version_filename("Memoria (2).docx");
        assert!(p.is_version_pattern);
        assert_eq!(p.base_stem, "memoria");
        let c = parse_version_filename("Plano - Copia.pdf");
        assert!(c.is_version_pattern);
        assert_eq!(c.base_stem, "plano");
    }

    #[test]
    fn parses_rev() {
        let p = parse_version_filename("Detalle_RevB.dwg");
        assert!(p.is_version_pattern);
        assert_eq!(p.base_stem, "detalle");
        assert_eq!(p.version_hint.as_deref(), Some("B"));
    }

    #[test]
    fn plain_name_not_pattern() {
        let p = parse_version_filename("Modelo.sdb");
        assert!(!p.is_version_pattern);
        assert_eq!(p.base_stem, "modelo");
    }
}
