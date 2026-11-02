//! Mass project organizer: scan → classify → plan → apply (virtual or physical).
//! Physical mode: backup under `.ingmanager/organizer_backups/<session>/` + journal.

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use uuid::Uuid;
use walkdir::WalkDir;

use crate::category_mapping::{
    map_folder_name_to_key_with_codes, map_project_categories_with_codes,
};
use crate::error::{AppError, AppResult};
use crate::models::STANDARD_FOLDERS;
use crate::path_security::{ensure_under_root, path_is_under};
use crate::project_paths::{self, AuthorizedRoot};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrganizerScanItem {
    pub project_id: Option<String>,
    pub project_name: Option<String>,
    pub folder_name: String,
    pub absolute_path: String,
    pub classified: Vec<ClassifiedFolder>,
    pub unmatched_folders: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClassifiedFolder {
    pub category_key: String,
    pub category_name: String,
    pub physical_name: String,
    pub matched: bool,
    pub standard_target: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct OrganizerSession {
    pub id: String,
    pub authorized_root_id: Option<String>,
    pub scope_absolute_path: String,
    pub mode: String,
    pub status: String,
    pub backup_absolute_path: Option<String>,
    pub notes: Option<String>,
    pub created_at: String,
    pub applied_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct OrganizerPlanItem {
    pub id: String,
    pub session_id: String,
    pub project_id: Option<String>,
    pub project_folder_name: String,
    pub category_key: Option<String>,
    pub source_relative_path: String,
    pub target_relative_path: Option<String>,
    pub action: String,
    pub confidence: f64,
    pub status: String,
    pub detail: Option<String>,
    pub sort_order: i64,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrganizerPlanResult {
    pub session: OrganizerSession,
    pub items: Vec<OrganizerPlanItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrganizerApplyResult {
    pub session_id: String,
    pub applied: usize,
    pub failed: usize,
    pub skipped: usize,
    pub backup_path: Option<String>,
    pub journal_entries: usize,
}

/// Scan a scope (authorized root or folder under it) for project-like directories.
pub async fn scan_scope(
    pool: &SqlitePool,
    scope_absolute_path: &str,
) -> AppResult<Vec<OrganizerScanItem>> {
    let scope = PathBuf::from(scope_absolute_path);
    if !scope.is_dir() {
        return Err(AppError::msg("La ruta de alcance no es un directorio"));
    }
    ensure_authorized(pool, &scope).await?;

    // Prefer registered projects under this scope; also detect unknown folders.
    let projects: Vec<(String, String, String, String)> = sqlx::query_as(
        "SELECT p.id, p.name, p.root_relative_path, r.absolute_path
         FROM projects p
         JOIN authorized_roots r ON r.id = p.authorized_root_id
         WHERE p.status IN ('active', 'completed')",
    )
    .fetch_all(pool)
    .await?;

    let mut items = Vec::new();
    let mut seen_paths = std::collections::HashSet::new();

    for (id, name, rel, root) in &projects {
        let abs = PathBuf::from(root).join(rel);
        if abs != scope && !abs.starts_with(&scope) {
            continue;
        }
        if !abs.is_dir() {
            continue;
        }
        seen_paths.insert(abs.display().to_string());
        let codes = codes_for_project(pool, id).await.unwrap_or_default();
        items.push(classify_project_dir(
            Some(id.clone()),
            Some(name.clone()),
            &abs,
            &codes,
        ));
    }

    // Child folders of scope that look like projects (have subfolders) but aren't registered
    if let Ok(rd) = fs::read_dir(&scope) {
        for entry in rd.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with('.') || name.eq_ignore_ascii_case("_INGMANAGER_INBOX") {
                continue;
            }
            let key = path.display().to_string();
            if seen_paths.contains(&key) {
                continue;
            }
            // Heuristic: directory with at least one subfolder or known alias
            let folder_codes = heuristic_codes_from_path(&path);
            let mapped = map_project_categories_with_codes(&path, &folder_codes);
            let any_match = mapped.iter().any(|m| m.matched);
            let has_sub = fs::read_dir(&path)
                .map(|rd| rd.filter_map(|e| e.ok()).any(|e| e.path().is_dir()))
                .unwrap_or(false);
            if any_match || has_sub {
                items.push(classify_project_dir(None, None, &path, &folder_codes));
            }
        }
    }

    items.sort_by(|a, b| a.folder_name.to_lowercase().cmp(&b.folder_name.to_lowercase()));
    Ok(items)
}

fn heuristic_codes_from_path(abs: &Path) -> Vec<String> {
    let mut codes = Vec::new();
    if let Some(name) = abs.file_name().map(|n| n.to_string_lossy().to_string()) {
        let digitish: String = name
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric())
            .collect();
        if digitish.len() >= 2 && digitish.len() <= 8 {
            codes.push(digitish);
        }
    }
    codes
}

async fn codes_for_project(pool: &SqlitePool, project_id: &str) -> AppResult<Vec<String>> {
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
                .take_while(|c| c.is_ascii_alphanumeric())
                .collect();
            if digitish.len() >= 2 && !codes.iter().any(|c| c == &digitish) {
                codes.push(digitish);
            }
        }
    }
    Ok(codes)
}

fn classify_project_dir(
    project_id: Option<String>,
    project_name: Option<String>,
    abs: &Path,
    project_codes: &[String],
) -> OrganizerScanItem {
    let folder_name = abs
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| abs.display().to_string());
    let mapped = map_project_categories_with_codes(abs, project_codes);
    let classified: Vec<ClassifiedFolder> = mapped
        .iter()
        .map(|m| {
            let std = STANDARD_FOLDERS
                .iter()
                .find(|s| s.key == m.key)
                .map(|s| s.relative_path.to_string())
                .unwrap_or_else(|| m.relative_path.clone());
            ClassifiedFolder {
                category_key: m.key.to_string(),
                category_name: m.name.to_string(),
                physical_name: if m.matched {
                    m.relative_path.clone()
                } else {
                    String::new()
                },
                matched: m.matched,
                standard_target: std,
            }
        })
        .collect();

    let mut unmatched = Vec::new();
    if let Ok(rd) = fs::read_dir(abs) {
        for entry in rd.flatten() {
            if !entry.path().is_dir() {
                continue;
            }
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with('.') {
                continue;
            }
            if map_folder_name_to_key_with_codes(&name, project_codes).is_none() {
                unmatched.push(name);
            }
        }
    }

    OrganizerScanItem {
        project_id,
        project_name,
        folder_name,
        absolute_path: abs.display().to_string(),
        classified,
        unmatched_folders: unmatched,
    }
}

async fn ensure_authorized(pool: &SqlitePool, path: &Path) -> AppResult<AuthorizedRoot> {
    let roots = project_paths::list_authorized_roots(pool).await?;
    for r in roots {
        let root = PathBuf::from(&r.absolute_path);
        if path_is_under(&root, path) || path == root {
            return Ok(r);
        }
    }
    Err(AppError::msg(
        "La ruta no está bajo ninguna raíz autorizada",
    ))
}

/// Build a plan session (virtual mappings and/or physical renames).
pub async fn build_plan(
    pool: &SqlitePool,
    scope_absolute_path: &str,
    mode: &str,
    project_ids: Option<Vec<String>>,
) -> AppResult<OrganizerPlanResult> {
    let mode = if mode == "physical" {
        "physical"
    } else {
        "virtual"
    };
    let scan = scan_scope(pool, scope_absolute_path).await?;
    let root = ensure_authorized(pool, Path::new(scope_absolute_path)).await?;

    let session_id = Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO organizer_sessions (
            id, authorized_root_id, scope_absolute_path, mode, status, created_at
         ) VALUES (?, ?, ?, ?, 'planned', datetime('now'))",
    )
    .bind(&session_id)
    .bind(&root.id)
    .bind(scope_absolute_path)
    .bind(mode)
    .execute(pool)
    .await?;

    let mut sort = 0i64;
    let mut items = Vec::new();

    for scan_item in scan {
        if let Some(ref filter) = project_ids {
            if let Some(ref pid) = scan_item.project_id {
                if !filter.contains(pid) {
                    continue;
                }
            } else {
                continue;
            }
        }

        for c in &scan_item.classified {
            if !c.matched {
                // Offer create_standard only in physical mode
                if mode == "physical" {
                    let item = insert_plan_item(
                        pool,
                        &session_id,
                        scan_item.project_id.as_deref(),
                        &scan_item.folder_name,
                        Some(&c.category_key),
                        "",
                        Some(&c.standard_target),
                        "create_standard",
                        0.5,
                        sort,
                        Some("Carpeta estándar ausente"),
                    )
                    .await?;
                    items.push(item);
                    sort += 1;
                }
                continue;
            }

            let already_standard = c.physical_name.eq_ignore_ascii_case(&c.standard_target);
            let action = if mode == "virtual" || already_standard {
                "map_virtual"
            } else {
                "rename_folder"
            };
            let target = if action == "rename_folder" {
                Some(c.standard_target.as_str())
            } else {
                Some(c.physical_name.as_str())
            };
            let item = insert_plan_item(
                pool,
                &session_id,
                scan_item.project_id.as_deref(),
                &scan_item.folder_name,
                Some(&c.category_key),
                &c.physical_name,
                target,
                action,
                if already_standard { 1.0 } else { 0.85 },
                sort,
                None,
            )
            .await?;
            items.push(item);
            sort += 1;
        }
    }

    let session = fetch_session(pool, &session_id).await?;
    Ok(OrganizerPlanResult { session, items })
}

async fn insert_plan_item(
    pool: &SqlitePool,
    session_id: &str,
    project_id: Option<&str>,
    folder_name: &str,
    category_key: Option<&str>,
    source: &str,
    target: Option<&str>,
    action: &str,
    confidence: f64,
    sort_order: i64,
    detail: Option<&str>,
) -> AppResult<OrganizerPlanItem> {
    let id = Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO organizer_plan_items (
            id, session_id, project_id, project_folder_name, category_key,
            source_relative_path, target_relative_path, action, confidence,
            status, detail, sort_order, created_at
         ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, 'pending', ?, ?, datetime('now'))",
    )
    .bind(&id)
    .bind(session_id)
    .bind(project_id)
    .bind(folder_name)
    .bind(category_key)
    .bind(source)
    .bind(target)
    .bind(action)
    .bind(confidence)
    .bind(detail)
    .bind(sort_order)
    .execute(pool)
    .await?;

    sqlx::query_as::<_, OrganizerPlanItem>(
        "SELECT id, session_id, project_id, project_folder_name, category_key,
                source_relative_path, target_relative_path, action, confidence,
                status, detail, sort_order, created_at
         FROM organizer_plan_items WHERE id = ?",
    )
    .bind(id)
    .fetch_one(pool)
    .await
    .map_err(AppError::from)
}

async fn fetch_session(pool: &SqlitePool, id: &str) -> AppResult<OrganizerSession> {
    sqlx::query_as::<_, OrganizerSession>(
        "SELECT id, authorized_root_id, scope_absolute_path, mode, status,
                backup_absolute_path, notes, created_at, applied_at
         FROM organizer_sessions WHERE id = ?",
    )
    .bind(id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| AppError::msg("Sesión de organizador no encontrada"))
}

pub async fn get_plan(pool: &SqlitePool, session_id: &str) -> AppResult<OrganizerPlanResult> {
    let session = fetch_session(pool, session_id).await?;
    let items = sqlx::query_as::<_, OrganizerPlanItem>(
        "SELECT id, session_id, project_id, project_folder_name, category_key,
                source_relative_path, target_relative_path, action, confidence,
                status, detail, sort_order, created_at
         FROM organizer_plan_items
         WHERE session_id = ?
         ORDER BY sort_order",
    )
    .bind(session_id)
    .fetch_all(pool)
    .await?;
    Ok(OrganizerPlanResult { session, items })
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdatePlanItemInput {
    pub item_id: String,
    pub action: Option<String>,
    pub category_key: Option<String>,
    pub target_relative_path: Option<String>,
    pub status: Option<String>,
    pub detail: Option<String>,
}

/// Edit a draft/planned item before apply (skip, remap category, change action).
pub async fn update_plan_item(
    pool: &SqlitePool,
    input: UpdatePlanItemInput,
) -> AppResult<OrganizerPlanItem> {
    let existing = sqlx::query_as::<_, OrganizerPlanItem>(
        "SELECT id, session_id, project_id, project_folder_name, category_key,
                source_relative_path, target_relative_path, action, confidence,
                status, detail, sort_order, created_at
         FROM organizer_plan_items WHERE id = ?",
    )
    .bind(&input.item_id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| AppError::msg("Ítem de plan no encontrado"))?;

    let session = fetch_session(pool, &existing.session_id).await?;
    if session.status == "applied" {
        return Err(AppError::msg("La sesión ya fue aplicada; no se puede editar"));
    }

    let action = input.action.unwrap_or(existing.action);
    if !["map_virtual", "rename_folder", "move_contents", "create_standard", "skip"]
        .contains(&action.as_str())
    {
        return Err(AppError::msg("Acción de plan inválida"));
    }
    let status = input.status.unwrap_or_else(|| {
        if action == "skip" {
            "skipped".into()
        } else {
            existing.status
        }
    });
    if !["pending", "skipped"].contains(&status.as_str()) {
        return Err(AppError::msg("Estado de ítem inválido para edición"));
    }
    let category_key = input.category_key.or(existing.category_key);
    let target = input
        .target_relative_path
        .or(existing.target_relative_path);
    let detail = input.detail.or(existing.detail);

    sqlx::query(
        "UPDATE organizer_plan_items SET
            action = ?, category_key = ?, target_relative_path = ?,
            status = ?, detail = ?
         WHERE id = ?",
    )
    .bind(&action)
    .bind(&category_key)
    .bind(&target)
    .bind(&status)
    .bind(&detail)
    .bind(&input.item_id)
    .execute(pool)
    .await?;

    sqlx::query_as::<_, OrganizerPlanItem>(
        "SELECT id, session_id, project_id, project_folder_name, category_key,
                source_relative_path, target_relative_path, action, confidence,
                status, detail, sort_order, created_at
         FROM organizer_plan_items WHERE id = ?",
    )
    .bind(&input.item_id)
    .fetch_one(pool)
    .await
    .map_err(AppError::from)
}

/// Apply plan: persist virtual mappings and/or physically reorganize with backup+journal.
pub async fn apply_plan(pool: &SqlitePool, session_id: &str) -> AppResult<OrganizerApplyResult> {
    let session = fetch_session(pool, session_id).await?;
    if session.status == "applied" {
        return Err(AppError::msg("La sesión ya fue aplicada"));
    }
    let plan = get_plan(pool, session_id).await?;
    let scope = PathBuf::from(&session.scope_absolute_path);
    let root = ensure_authorized(pool, &scope).await?;
    let root_path = PathBuf::from(&root.absolute_path);

    let mut backup_path: Option<PathBuf> = None;
    if session.mode == "physical" {
        let backup = root_path
            .join(".ingmanager")
            .join("organizer_backups")
            .join(&session_id);
        fs::create_dir_all(&backup)?;
        // Snapshot affected project folders before mutation
        let mut backed = std::collections::HashSet::new();
        for item in &plan.items {
            if item.action == "rename_folder" || item.action == "move_contents" {
                let proj_dir = resolve_project_folder(&scope, &root_path, item)?;
                if backed.insert(proj_dir.display().to_string()) {
                    let dest = backup.join(&item.project_folder_name);
                    copy_dir_recursive(&proj_dir, &dest)?;
                }
            }
        }
        sqlx::query(
            "UPDATE organizer_sessions SET backup_absolute_path = ? WHERE id = ?",
        )
        .bind(backup.display().to_string())
        .bind(session_id)
        .execute(pool)
        .await?;
        backup_path = Some(backup);
    }

    let mut applied = 0usize;
    let mut failed = 0usize;
    let mut skipped = 0usize;
    let mut journal_entries = 0usize;

    for item in &plan.items {
        if item.status == "skipped" || item.action == "skip" {
            skipped += 1;
            sqlx::query("UPDATE organizer_plan_items SET status = 'skipped' WHERE id = ?")
                .bind(&item.id)
                .execute(pool)
                .await?;
            continue;
        }
        match apply_one(pool, &session, item, &scope, &root_path).await {
            Ok(true) => {
                applied += 1;
                journal_entries += 1;
                sqlx::query("UPDATE organizer_plan_items SET status = 'applied' WHERE id = ?")
                    .bind(&item.id)
                    .execute(pool)
                    .await?;
            }
            Ok(false) => {
                skipped += 1;
                sqlx::query("UPDATE organizer_plan_items SET status = 'skipped' WHERE id = ?")
                    .bind(&item.id)
                    .execute(pool)
                    .await?;
            }
            Err(e) => {
                failed += 1;
                journal_entries += 1;
                let _ = write_journal(
                    pool,
                    session_id,
                    Some(&item.id),
                    &item.action,
                    Some(&item.source_relative_path),
                    item.target_relative_path.as_deref(),
                    false,
                    &e.to_string(),
                )
                .await;
                sqlx::query(
                    "UPDATE organizer_plan_items SET status = 'failed', detail = ? WHERE id = ?",
                )
                .bind(e.to_string())
                .bind(&item.id)
                .execute(pool)
                .await?;
            }
        }
    }

    let status = if failed > 0 && applied == 0 {
        "failed"
    } else {
        "applied"
    };
    sqlx::query(
        "UPDATE organizer_sessions SET status = ?, applied_at = datetime('now') WHERE id = ?",
    )
    .bind(status)
    .bind(session_id)
    .execute(pool)
    .await?;

    Ok(OrganizerApplyResult {
        session_id: session_id.to_string(),
        applied,
        failed,
        skipped,
        backup_path: backup_path.map(|p| p.display().to_string()),
        journal_entries,
    })
}

fn resolve_project_folder(
    scope: &Path,
    _root: &Path,
    item: &OrganizerPlanItem,
) -> AppResult<PathBuf> {
    // Prefer path under scope by folder name
    let candidate = scope.join(&item.project_folder_name);
    if candidate.is_dir() {
        return Ok(candidate);
    }
    Err(AppError::msg(format!(
        "No se encontró carpeta de proyecto {}",
        item.project_folder_name
    )))
}

async fn apply_one(
    pool: &SqlitePool,
    session: &OrganizerSession,
    item: &OrganizerPlanItem,
    scope: &Path,
    root_path: &Path,
) -> AppResult<bool> {
    match item.action.as_str() {
        "map_virtual" => {
            let Some(project_id) = item.project_id.as_deref() else {
                return Ok(false);
            };
            let Some(cat) = item.category_key.as_deref() else {
                return Ok(false);
            };
            let physical = if item.source_relative_path.is_empty() {
                item.target_relative_path.clone().unwrap_or_default()
            } else {
                item.source_relative_path.clone()
            };
            if physical.is_empty() {
                return Ok(false);
            }
            persist_mapping(pool, project_id, cat, &physical, "virtual").await?;
            // Update categories.relative_path for UI
            sqlx::query(
                "UPDATE categories SET relative_path = ? WHERE project_id = ? AND key = ?",
            )
            .bind(&physical)
            .bind(project_id)
            .bind(cat)
            .execute(pool)
            .await?;
            sqlx::query(
                "UPDATE projects SET layout_mode = 'mapped', updated_at = datetime('now') WHERE id = ?",
            )
            .bind(project_id)
            .execute(pool)
            .await?;
            write_journal(
                pool,
                &session.id,
                Some(&item.id),
                "map_virtual",
                Some(&physical),
                Some(cat),
                true,
                "Mapeo virtual persistido",
            )
            .await?;
            Ok(true)
        }
        "rename_folder" => {
            if session.mode != "physical" {
                return Ok(false);
            }
            let proj = resolve_project_folder(scope, root_path, item)?;
            let src_name = &item.source_relative_path;
            let Some(target_name) = item.target_relative_path.as_ref() else {
                return Ok(false);
            };
            let src = proj.join(src_name);
            let dest = proj.join(target_name);
            if !src.is_dir() {
                return Err(AppError::msg(format!("Origen inexistente: {src_name}")));
            }
            if dest.exists() {
                return Err(AppError::msg(format!(
                    "Destino ya existe: {target_name}"
                )));
            }
            // Ensure we don't create watcher cycles: dest under .ingmanager is forbidden for user folders
            fs::rename(&src, &dest)?;
            if let Some(project_id) = item.project_id.as_deref() {
                if let Some(cat) = item.category_key.as_deref() {
                    persist_mapping(pool, project_id, cat, target_name, "physical").await?;
                    sqlx::query(
                        "UPDATE categories SET relative_path = ? WHERE project_id = ? AND key = ?",
                    )
                    .bind(target_name)
                    .bind(project_id)
                    .bind(cat)
                    .execute(pool)
                    .await?;
                    sqlx::query(
                        "UPDATE projects SET layout_mode = 'standard', updated_at = datetime('now') WHERE id = ?",
                    )
                    .bind(project_id)
                    .execute(pool)
                    .await?;
                }
            }
            write_journal(
                pool,
                &session.id,
                Some(&item.id),
                "rename_folder",
                Some(&src.display().to_string()),
                Some(&dest.display().to_string()),
                true,
                "Carpeta renombrada",
            )
            .await?;
            Ok(true)
        }
        "create_standard" => {
            if session.mode != "physical" {
                return Ok(false);
            }
            let proj = resolve_project_folder(scope, root_path, item)?;
            let Some(target_name) = item.target_relative_path.as_ref() else {
                return Ok(false);
            };
            let dest = proj.join(target_name);
            if !dest.exists() {
                fs::create_dir_all(&dest)?;
            }
            if let (Some(project_id), Some(cat)) =
                (item.project_id.as_deref(), item.category_key.as_deref())
            {
                persist_mapping(pool, project_id, cat, target_name, "physical").await?;
                sqlx::query(
                    "UPDATE categories SET relative_path = ? WHERE project_id = ? AND key = ?",
                )
                .bind(target_name)
                .bind(project_id)
                .bind(cat)
                .execute(pool)
                .await?;
            }
            write_journal(
                pool,
                &session.id,
                Some(&item.id),
                "create_standard",
                None,
                Some(&dest.display().to_string()),
                true,
                "Carpeta estándar creada",
            )
            .await?;
            Ok(true)
        }
        "skip" => Ok(false),
        other => Err(AppError::msg(format!("Acción desconocida: {other}"))),
    }
}

async fn persist_mapping(
    pool: &SqlitePool,
    project_id: &str,
    category_key: &str,
    physical: &str,
    mode: &str,
) -> AppResult<()> {
    let id = Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO project_folder_mappings (
            id, project_id, category_key, physical_relative_path, mapping_mode,
            created_at, updated_at
         ) VALUES (?, ?, ?, ?, ?, datetime('now'), datetime('now'))
         ON CONFLICT(project_id, category_key) DO UPDATE SET
            physical_relative_path = excluded.physical_relative_path,
            mapping_mode = excluded.mapping_mode,
            updated_at = datetime('now')",
    )
    .bind(&id)
    .bind(project_id)
    .bind(category_key)
    .bind(physical)
    .bind(mode)
    .execute(pool)
    .await?;
    Ok(())
}

async fn write_journal(
    pool: &SqlitePool,
    session_id: &str,
    plan_item_id: Option<&str>,
    op: &str,
    source: Option<&str>,
    dest: Option<&str>,
    ok: bool,
    message: &str,
) -> AppResult<()> {
    let id = Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO organizer_journal (
            id, session_id, plan_item_id, op, source_path, dest_path, ok, message, created_at
         ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, datetime('now'))",
    )
    .bind(&id)
    .bind(session_id)
    .bind(plan_item_id)
    .bind(op)
    .bind(source)
    .bind(dest)
    .bind(if ok { 1 } else { 0 })
    .bind(message)
    .execute(pool)
    .await?;
    Ok(())
}

fn copy_dir_recursive(src: &Path, dest: &Path) -> AppResult<()> {
    fs::create_dir_all(dest)?;
    for entry in WalkDir::new(src).into_iter().filter_map(|e| e.ok()) {
        let rel = entry.path().strip_prefix(src).unwrap_or(entry.path());
        // Skip nested organizer backups / snapshots to avoid huge copies & cycles
        if rel.components().any(|c| {
            c.as_os_str()
                .to_string_lossy()
                .eq_ignore_ascii_case(".ingmanager")
        }) {
            continue;
        }
        let target = dest.join(rel);
        if entry.file_type().is_dir() {
            fs::create_dir_all(&target)?;
        } else if entry.file_type().is_file() {
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::copy(entry.path(), &target)?;
        }
    }
    Ok(())
}

pub async fn list_mappings(
    pool: &SqlitePool,
    project_id: &str,
) -> AppResult<Vec<(String, String, String, String)>> {
    // id, category_key, physical_relative_path, mapping_mode
    sqlx::query_as(
        "SELECT id, category_key, physical_relative_path, mapping_mode
         FROM project_folder_mappings WHERE project_id = ?
         ORDER BY category_key",
    )
    .bind(project_id)
    .fetch_all(pool)
    .await
    .map_err(AppError::from)
}

/// Suppress watcher feedback loops: paths under organizer backups are ignored by watcher already via `.ingmanager`.
pub fn is_organizer_internal(path: &Path) -> bool {
    path.components().any(|c| {
        let s = c.as_os_str().to_string_lossy();
        s.eq_ignore_ascii_case(".ingmanager")
    })
}

#[allow(dead_code)]
pub fn ensure_under(root: &Path, path: &Path) -> AppResult<PathBuf> {
    ensure_under_root(root, path)
}
