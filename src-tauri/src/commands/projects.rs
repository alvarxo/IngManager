use std::path::PathBuf;

use tauri::State;
use uuid::Uuid;

use crate::category_mapping::map_project_categories;
use crate::codes::{current_year_two_digits, next_project_code};
use crate::error::{AppError, AppResult};
use crate::fs_util::move_entry;
use crate::index_sync;
use crate::models::{
    CategorySummary, CreateProjectInput, Project, RemoveProjectResult, UpdateProjectInput,
    STANDARD_FOLDERS,
};
use crate::path_security::{ensure_under_root, join_under_root};
use crate::project_paths::{self, business_code_prefix, sanitize_folder_name, AuthorizedRoot};
use crate::state::AppState;

/// Rewrite `OLD-…` → `NEW-…` for a business/transmission code. Returns None if no match.
pub(crate) fn reprefix_business_code(
    code: &str,
    old_prefix: &str,
    new_prefix: &str,
) -> Option<String> {
    if old_prefix.is_empty() || old_prefix == new_prefix {
        return None;
    }
    let needle = format!("{old_prefix}-");
    code.strip_prefix(&needle)
        .map(|rest| format!("{new_prefix}-{rest}"))
}

/// Rewrite authorized-root-relative path when the project folder segment changes.
pub(crate) fn rewrite_root_rel_path(
    path: &str,
    old_folder: &str,
    new_folder: &str,
) -> Option<String> {
    if old_folder.is_empty() || old_folder == new_folder {
        return None;
    }
    if path == old_folder {
        return Some(new_folder.to_string());
    }
    let needle = format!("{old_folder}/");
    path.strip_prefix(&needle)
        .map(|rest| format!("{new_folder}/{rest}"))
}

async fn fetch_project(state: &AppState, id: &str) -> AppResult<Project> {
    sqlx::query_as::<_, Project>("SELECT * FROM projects WHERE id = ?")
        .bind(id)
        .fetch_optional(&state.pool)
        .await?
        .ok_or_else(|| AppError::msg("Proyecto no encontrado"))
}

pub(crate) fn normalize_optional_code(raw: Option<&str>) -> AppResult<Option<String>> {
    match raw.map(str::trim).filter(|s| !s.is_empty()) {
        None => Ok(None),
        Some(c) => {
            if c.contains(['/', '\\']) {
                return Err(AppError::msg("El código no puede contener separadores de ruta"));
            }
            Ok(Some(c.to_string()))
        }
    }
}

pub(crate) async fn assert_code_unique(pool: &sqlx::SqlitePool, code: &str, exclude_id: Option<&str>) -> AppResult<()> {
    let row: Option<(String,)> = if let Some(id) = exclude_id {
        sqlx::query_as("SELECT id FROM projects WHERE code = ? AND id != ?")
            .bind(code)
            .bind(id)
            .fetch_optional(pool)
            .await?
    } else {
        sqlx::query_as("SELECT id FROM projects WHERE code = ?")
            .bind(code)
            .fetch_optional(pool)
            .await?
    };
    if row.is_some() {
        return Err(AppError::msg(format!(
            "Ya existe un proyecto con código {code}"
        )));
    }
    Ok(())
}

async fn resolve_root_for_create(
    state: &AppState,
    root_id: Option<&str>,
) -> AppResult<AuthorizedRoot> {
    if let Some(id) = root_id.map(str::trim).filter(|s| !s.is_empty()) {
        sqlx::query_as::<_, AuthorizedRoot>(
            "SELECT id, absolute_path, label, is_primary, created_at FROM authorized_roots WHERE id = ?",
        )
        .bind(id)
        .fetch_optional(&state.pool)
        .await?
        .ok_or_else(|| AppError::msg("Raíz autorizada no encontrada"))
    } else {
        project_paths::primary_root(&state.pool).await
    }
}

#[tauri::command]
pub async fn preview_next_project_code(state: State<'_, AppState>) -> AppResult<String> {
    let year = current_year_two_digits();
    let rows: Vec<(Option<String>,)> = sqlx::query_as("SELECT code FROM projects")
        .fetch_all(&state.pool)
        .await?;
    let codes: Vec<String> = rows.into_iter().filter_map(|r| r.0).collect();
    next_project_code(year, &codes)
}

#[tauri::command]
pub async fn list_projects(
    state: State<'_, AppState>,
    include_archived: Option<bool>,
) -> AppResult<Vec<Project>> {
    let include = include_archived.unwrap_or(false);
    let projects = if include {
        sqlx::query_as::<_, Project>(
            "SELECT * FROM projects ORDER BY
             CASE status WHEN 'active' THEN 0 WHEN 'completed' THEN 1 ELSE 2 END,
             COALESCE(code, root_relative_path) DESC",
        )
        .fetch_all(&state.pool)
        .await?
    } else {
        sqlx::query_as::<_, Project>(
            "SELECT * FROM projects WHERE status = 'active'
             ORDER BY COALESCE(code, root_relative_path) DESC",
        )
        .fetch_all(&state.pool)
        .await?
    };
    Ok(projects)
}

#[tauri::command]
pub async fn get_project(state: State<'_, AppState>, id: String) -> AppResult<Project> {
    crate::diagnostics::log_event("project", &format!("get begin id={id}"));
    let p = fetch_project(&state, &id).await?;
    crate::diagnostics::log_event("project", &format!("get done id={id}"));
    Ok(p)
}

#[tauri::command]
pub async fn create_project(
    state: State<'_, AppState>,
    input: CreateProjectInput,
) -> AppResult<Project> {
    let name = input.name.trim().to_string();
    if name.is_empty() {
        return Err(AppError::msg("El nombre del proyecto es obligatorio"));
    }

    let code = normalize_optional_code(input.code.as_deref())?;
    if let Some(ref c) = code {
        assert_code_unique(&state.pool, c, None).await?;
    }

    let root = resolve_root_for_create(&state, input.authorized_root_id.as_deref()).await?;
    let root_path = PathBuf::from(&root.absolute_path);
    if !root_path.is_dir() {
        return Err(AppError::msg("La raíz autorizada no es accesible"));
    }

    let folder_name = if let Some(f) = input.folder_name.as_deref() {
        sanitize_folder_name(f)?
    } else if let Some(ref c) = code {
        c.clone()
    } else {
        sanitize_folder_name(&name.replace(' ', "_"))?
    };

    // Path collision independent of code
    let existing_path: Option<(String,)> = sqlx::query_as(
        "SELECT id FROM projects WHERE authorized_root_id = ? AND root_relative_path = ?",
    )
    .bind(&root.id)
    .bind(&folder_name)
    .fetch_optional(&state.pool)
    .await?;
    if existing_path.is_some() {
        return Err(AppError::msg(format!(
            "Ya hay un proyecto registrado en la carpeta {folder_name}"
        )));
    }

    let project_dir = ensure_under_root(&root_path, &root_path.join(&folder_name))?;
    if project_dir.exists() {
        return Err(AppError::msg(format!(
            "Ya existe la carpeta física {folder_name} bajo la raíz. Use «Añadir existente» para vincularla."
        )));
    }

    std::fs::create_dir_all(&project_dir)?;
    let create_folders = input.create_standard_folders.unwrap_or(true);
    if create_folders {
        for folder in STANDARD_FOLDERS {
            let path = project_dir.join(folder.relative_path);
            std::fs::create_dir_all(&path)?;
            if folder.key == "transmisiones" {
                std::fs::create_dir_all(path.join("ENVIADOS"))?;
                std::fs::create_dir_all(path.join("RECIBIDOS"))?;
            }
        }
    }

    let id = Uuid::new_v4().to_string();
    let client_name = input
        .client_name
        .as_ref()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    let description = input
        .description
        .as_ref()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());

    let mut tx = state.pool.begin().await?;

    sqlx::query(
        "INSERT INTO projects (
            id, code, name, description, client_name, status,
            authorized_root_id, root_relative_path, import_mode, layout_mode,
            start_date, end_date, last_opened_at, created_at, updated_at
         ) VALUES (
            ?, ?, ?, ?, ?, 'active',
            ?, ?, 'created', 'standard',
            ?, ?, datetime('now'), datetime('now'), datetime('now')
         )",
    )
    .bind(&id)
    .bind(&code)
    .bind(&name)
    .bind(&description)
    .bind(&client_name)
    .bind(&root.id)
    .bind(&folder_name)
    .bind(&input.start_date)
    .bind(&input.end_date)
    .execute(&mut *tx)
    .await?;

    for folder in STANDARD_FOLDERS {
        let cat_id = Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT INTO categories (id, project_id, key, name, relative_path, sort_order)
             VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(&cat_id)
        .bind(&id)
        .bind(folder.key)
        .bind(folder.name)
        .bind(folder.relative_path)
        .bind(folder.sort_order)
        .execute(&mut *tx)
        .await?;
    }

    sqlx::query(
        "INSERT INTO audit_log (id, entity_type, entity_id, action, detail, project_id)
         VALUES (?, 'project', ?, 'create', ?, ?)",
    )
    .bind(Uuid::new_v4().to_string())
    .bind(&id)
    .bind(format!(
        "Created project {} at {}",
        code.as_deref().unwrap_or("(sin código)"),
        folder_name
    ))
    .bind(&id)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;

    crate::search_index::upsert_fts(
        &state.pool,
        "project",
        &id,
        Some(&id),
        &name,
        code.as_deref(),
        description.as_deref(),
        None,
    )
    .await
    .ok();

    fetch_project(&state, &id).await
}

#[tauri::command]
pub async fn update_project(
    state: State<'_, AppState>,
    input: UpdateProjectInput,
) -> AppResult<Project> {
    update_project_inner(&state, input).await
}

pub async fn update_project_inner(
    state: &AppState,
    input: UpdateProjectInput,
) -> AppResult<Project> {
    let existing = fetch_project(state, &input.id).await?;

    let name = input
        .name
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or(existing.name.clone());
    let description = input
        .description
        .or(existing.description.clone())
        .map(|s| s.trim().to_string());
    let client_name = input
        .client_name
        .or(existing.client_name.clone())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    // `Some` replaces the logical code (empty → clear). Omitted/`None` keeps existing.
    let code = if let Some(raw) = input.code.as_deref() {
        let normalized = normalize_optional_code(Some(raw))?;
        if let Some(ref c) = normalized {
            if existing.code.as_deref() != Some(c.as_str()) {
                // Global uniqueness (DB UNIQUE on projects.code), not per-root.
                assert_code_unique(&state.pool, c, Some(&input.id)).await?;
            }
        }
        normalized
    } else {
        existing.code.clone()
    };
    let status = input.status.unwrap_or(existing.status.clone());
    let allowed = ["active", "archived", "cancelled", "completed"];
    if !allowed.contains(&status.as_str()) {
        return Err(AppError::msg("Estado de proyecto inválido"));
    }
    let start_date = input.start_date.or(existing.start_date.clone());
    let end_date = input.end_date.or(existing.end_date.clone());
    let is_favorite = input
        .is_favorite
        .map(|v| if v { 1 } else { 0 })
        .unwrap_or(existing.is_favorite);
    let physical_revision_folders = input
        .physical_revision_folders
        .map(|v| if v { 1 } else { 0 })
        .unwrap_or(existing.physical_revision_folders);

    let old_folder = existing.root_relative_path.clone();
    let mut new_folder = old_folder.clone();
    let rename_requested = input.rename_root_folder.unwrap_or(false);
    let mut renamed_abs: Option<(PathBuf, PathBuf, PathBuf)> = None; // (root, old, new)

    if rename_requested {
        let Some(ref new_code) = code else {
            return Err(AppError::msg(
                "Para renombrar la carpeta raíz hace falta un código de proyecto",
            ));
        };
        let folder = sanitize_folder_name(new_code)?;
        if folder != old_folder {
            let root = sqlx::query_as::<_, AuthorizedRoot>(
                "SELECT id, absolute_path, label, is_primary, created_at
                 FROM authorized_roots WHERE id = ?",
            )
            .bind(&existing.authorized_root_id)
            .fetch_optional(&state.pool)
            .await?
            .ok_or_else(|| AppError::msg("Raíz autorizada no encontrada"))?;

            let root_path = PathBuf::from(&root.absolute_path);
            if !root_path.is_dir() {
                return Err(AppError::msg("La raíz autorizada no es accesible"));
            }

            let collision: Option<(String,)> = sqlx::query_as(
                "SELECT id FROM projects
                 WHERE authorized_root_id = ? AND root_relative_path = ? AND id != ?",
            )
            .bind(&existing.authorized_root_id)
            .bind(&folder)
            .bind(&input.id)
            .fetch_optional(&state.pool)
            .await?;
            if collision.is_some() {
                return Err(AppError::msg(format!(
                    "Ya hay un proyecto registrado en la carpeta {folder}"
                )));
            }

            let old_abs = join_under_root(&root_path, &old_folder)?;
            let new_abs = ensure_under_root(&root_path, &root_path.join(&folder))?;
            if !old_abs.exists() {
                return Err(AppError::msg(format!(
                    "No se encuentra la carpeta física «{old_folder}» bajo la raíz"
                )));
            }
            if new_abs.exists() {
                return Err(AppError::msg(format!(
                    "Ya existe la carpeta física «{folder}» bajo la raíz"
                )));
            }

            move_entry(&old_abs, &new_abs)?;
            renamed_abs = Some((root_path, old_abs, new_abs));
            new_folder = folder;
        }
    }

    // Collect OLD prefixes to rewrite → new project code.
    // Includes the prior logical prefix, and (when renaming) the old folder name so a
    // later rename still fixes docs left stale after a code-only edit.
    let sync_from_prefixes: Vec<String> = {
        let mut out = Vec::new();
        if let Some(ref new_prefix) = code {
            let logical =
                business_code_prefix(existing.code.as_deref(), &existing.root_relative_path);
            if logical != *new_prefix {
                out.push(logical);
            }
            if new_folder != old_folder
                && old_folder != *new_prefix
                && !out.iter().any(|p| p == &old_folder)
            {
                out.push(old_folder.clone());
            }
        }
        out
    };
    let did_sync_codes = !sync_from_prefixes.is_empty();

    let db_result = async {
        let mut tx = state.pool.begin().await?;

        sqlx::query(
            "UPDATE projects SET
                name = ?, description = ?, client_name = ?, code = ?, status = ?,
                start_date = ?, end_date = ?, is_favorite = ?,
                physical_revision_folders = ?, root_relative_path = ?,
                updated_at = datetime('now')
             WHERE id = ?",
        )
        .bind(&name)
        .bind(&description)
        .bind(&client_name)
        .bind(&code)
        .bind(&status)
        .bind(&start_date)
        .bind(&end_date)
        .bind(is_favorite)
        .bind(physical_revision_folders)
        .bind(&new_folder)
        .bind(&input.id)
        .execute(&mut *tx)
        .await?;

        if let Some(ref new_prefix) = code {
            for old_prefix in &sync_from_prefixes {
                sync_document_and_transmission_codes(
                    &mut tx,
                    &input.id,
                    old_prefix,
                    new_prefix,
                )
                .await?;
            }
        }

        if new_folder != old_folder {
            rewrite_project_path_prefixes(&mut tx, &input.id, &old_folder, &new_folder).await?;
        }

        sqlx::query(
            "INSERT INTO audit_log (id, entity_type, entity_id, action, detail, project_id)
             VALUES (?, 'project', ?, 'update', ?, ?)",
        )
        .bind(Uuid::new_v4().to_string())
        .bind(&input.id)
        .bind(format!(
            "Updated project code={:?} rename_folder={} path={}->{}",
            code,
            rename_requested,
            old_folder,
            new_folder
        ))
        .bind(&input.id)
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;
        Ok::<(), AppError>(())
    }
    .await;

    if let Err(e) = db_result {
        if let Some((ref root_path, ref old_abs, ref new_abs)) = renamed_abs {
            // Best-effort undo of the FS rename if DB failed.
            let _ = move_entry(new_abs, old_abs);
            let _ = root_path; // silence
        }
        return Err(e);
    }

    if let Some((ref root_path, _, ref new_abs)) = renamed_abs {
        // Drop stale index rows for the old folder, then reindex the new tree.
        sqlx::query(
            "DELETE FROM indexed_files
             WHERE authorized_root_id = ?
               AND (relative_path = ? OR relative_path LIKE ?)",
        )
        .bind(&existing.authorized_root_id)
        .bind(&old_folder)
        .bind(format!("{old_folder}/%"))
        .execute(&state.pool)
        .await
        .ok();
        let _ = index_sync::refresh_subtree(&state.pool, root_path, new_abs).await;
    }

    crate::search_index::upsert_fts(
        &state.pool,
        "project",
        &input.id,
        Some(&input.id),
        &name,
        code.as_deref(),
        description.as_deref(),
        None,
    )
    .await
    .ok();

    if did_sync_codes {
        refresh_project_code_fts(&state.pool, &input.id).await.ok();
    }

    fetch_project(state, &input.id).await
}

async fn sync_document_and_transmission_codes(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    project_id: &str,
    old_prefix: &str,
    new_prefix: &str,
) -> AppResult<()> {
    if old_prefix.is_empty() || old_prefix == new_prefix {
        return Ok(());
    }

    // documents.business_code: OLD-… → NEW-…
    sqlx::query(
        "UPDATE documents
         SET business_code = ? || substr(business_code, length(?) + 1),
             updated_at = datetime('now')
         WHERE project_id = ?
           AND business_code IS NOT NULL
           AND business_code LIKE ? || '-%'",
    )
    .bind(new_prefix)
    .bind(old_prefix)
    .bind(project_id)
    .bind(old_prefix)
    .execute(&mut **tx)
    .await?;

    // transmissions.code (ENV/REC) use the same project prefix
    sqlx::query(
        "UPDATE transmissions
         SET code = ? || substr(code, length(?) + 1)
         WHERE project_id = ?
           AND code IS NOT NULL
           AND code LIKE ? || '-%'",
    )
    .bind(new_prefix)
    .bind(old_prefix)
    .bind(project_id)
    .bind(old_prefix)
    .execute(&mut **tx)
    .await?;

    Ok(())
}

async fn rewrite_project_path_prefixes(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    project_id: &str,
    old_folder: &str,
    new_folder: &str,
) -> AppResult<()> {
    // document_files (via versions → documents)
    sqlx::query(
        "UPDATE document_files
         SET relative_path = ? || substr(relative_path, length(?) + 1)
         WHERE (relative_path = ? OR relative_path LIKE ? || '/%')
           AND document_version_id IN (
             SELECT dv.id FROM document_versions dv
             JOIN documents d ON d.id = dv.document_id
             WHERE d.project_id = ?
           )",
    )
    .bind(new_folder)
    .bind(old_folder)
    .bind(old_folder)
    .bind(old_folder)
    .bind(project_id)
    .execute(&mut **tx)
    .await?;

    sqlx::query(
        "UPDATE document_versions
         SET source_relative_path = CASE
               WHEN source_relative_path = ? OR source_relative_path LIKE ? || '/%'
                 THEN ? || substr(source_relative_path, length(?) + 1)
               ELSE source_relative_path
             END,
             snapshot_relative_path = CASE
               WHEN snapshot_relative_path = ? OR snapshot_relative_path LIKE ? || '/%'
                 THEN ? || substr(snapshot_relative_path, length(?) + 1)
               ELSE snapshot_relative_path
             END
         WHERE document_id IN (SELECT id FROM documents WHERE project_id = ?)",
    )
    .bind(old_folder)
    .bind(old_folder)
    .bind(new_folder)
    .bind(old_folder)
    .bind(old_folder)
    .bind(old_folder)
    .bind(new_folder)
    .bind(old_folder)
    .bind(project_id)
    .execute(&mut **tx)
    .await?;

    // Transmission path columns (authorized-root-relative)
    for col in [
        "folder_relative_path",
        "zip_relative_path",
        "original_import_relative_path",
        "extracted_relative_path",
        "antecedentes_relative_path",
    ] {
        let sql = format!(
            "UPDATE transmissions
             SET {col} = CASE
               WHEN {col} = ? OR {col} LIKE ? || '/%'
                 THEN ? || substr({col}, length(?) + 1)
               ELSE {col}
             END
             WHERE project_id = ?"
        );
        sqlx::query(&sql)
            .bind(old_folder)
            .bind(old_folder)
            .bind(new_folder)
            .bind(old_folder)
            .bind(project_id)
            .execute(&mut **tx)
            .await?;
    }

    sqlx::query(
        "UPDATE transmission_items
         SET relative_path = CASE
               WHEN relative_path = ? OR relative_path LIKE ? || '/%'
                 THEN ? || substr(relative_path, length(?) + 1)
               ELSE relative_path
             END,
             snapshot_relative_path = CASE
               WHEN snapshot_relative_path = ? OR snapshot_relative_path LIKE ? || '/%'
                 THEN ? || substr(snapshot_relative_path, length(?) + 1)
               ELSE snapshot_relative_path
             END
         WHERE transmission_id IN (SELECT id FROM transmissions WHERE project_id = ?)",
    )
    .bind(old_folder)
    .bind(old_folder)
    .bind(new_folder)
    .bind(old_folder)
    .bind(old_folder)
    .bind(old_folder)
    .bind(new_folder)
    .bind(old_folder)
    .bind(project_id)
    .execute(&mut **tx)
    .await?;

    // Soft references — best effort (original_path may be abs or root-relative)
    sqlx::query(
        "UPDATE trash_items
         SET original_path = CASE
               WHEN original_path = ? OR original_path LIKE ? || '/%'
                 THEN ? || substr(original_path, length(?) + 1)
               ELSE original_path
             END
         WHERE project_id = ?",
    )
    .bind(old_folder)
    .bind(old_folder)
    .bind(new_folder)
    .bind(old_folder)
    .bind(project_id)
    .execute(&mut **tx)
    .await?;

    Ok(())
}

async fn refresh_project_code_fts(pool: &sqlx::SqlitePool, project_id: &str) -> AppResult<()> {
    let docs: Vec<(String, Option<String>, String, Option<String>, Option<String>)> =
        sqlx::query_as(
            "SELECT id, business_code, title, notes, origin FROM documents WHERE project_id = ?",
        )
        .bind(project_id)
        .fetch_all(pool)
        .await?;
    for (id, code, title, notes, origin) in docs {
        let body = [notes, origin]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>()
            .join(" ");
        crate::search_index::upsert_fts(
            pool,
            "document",
            &id,
            Some(project_id),
            &title,
            code.as_deref(),
            if body.is_empty() { None } else { Some(&body) },
            None,
        )
        .await?;
    }

    let txs: Vec<(String, Option<String>, Option<String>, Option<String>)> = sqlx::query_as(
        "SELECT id, code, subject, counterpart FROM transmissions
         WHERE project_id = ? AND status = 'complete'",
    )
    .bind(project_id)
    .fetch_all(pool)
    .await?;
    for (id, code, subject, counterpart) in txs {
        crate::search_index::upsert_fts(
            pool,
            "transmission",
            &id,
            Some(project_id),
            subject.as_deref().unwrap_or("transmisión"),
            code.as_deref(),
            counterpart.as_deref(),
            None,
        )
        .await?;
    }
    Ok(())
}

#[tauri::command]
pub async fn archive_project(state: State<'_, AppState>, id: String) -> AppResult<Project> {
    update_project_inner(
        &state,
        UpdateProjectInput {
            id,
            name: None,
            description: None,
            client_name: None,
            code: None,
            status: Some("archived".into()),
            start_date: None,
            end_date: None,
            is_favorite: None,
            physical_revision_folders: None,
            rename_root_folder: None,
        },
    )
    .await
}

#[tauri::command]
pub async fn open_project(state: State<'_, AppState>, id: String) -> AppResult<Project> {
    crate::diagnostics::log_event("open", &format!("begin id={id}"));
    let project = match fetch_project(&state, &id).await {
        Ok(p) => p,
        Err(e) => {
            crate::diagnostics::log_event("open", &format!("missing id={id}: {e}"));
            return Err(e);
        }
    };

    // Timestamp update must not block a successful open.
    if let Err(e) = sqlx::query(
        "UPDATE projects SET last_opened_at = datetime('now'), updated_at = datetime('now')
         WHERE id = ?",
    )
    .bind(&id)
    .execute(&state.pool)
    .await
    {
        crate::diagnostics::log_event("open", &format!("timestamp update failed id={id}: {e}"));
    }

    // Repair empty category rows (linked projects) without touching disk.
    if let Err(e) = ensure_standard_categories_if_empty(&state.pool, &id).await {
        crate::diagnostics::log_event(
            "open",
            &format!("ensure categories failed id={id}: {e}"),
        );
    }

    // Default OFF after 1090 hang: WalkDir on Windows junctions can stall forever
    // before any progress log. Opt-in via Settings → reconcile_on_open=1.
    let reconcile_on_open = crate::db::get_setting(&state.pool, "reconcile_on_open")
        .await
        .ok()
        .flatten()
        .map(|v| {
            let t = v.trim().to_ascii_lowercase();
            matches!(t.as_str(), "1" | "true" | "yes" | "on")
        })
        .unwrap_or(false);

    if !reconcile_on_open {
        crate::diagnostics::log_event(
            "open",
            &format!(
                "returned id={id} (reconcile skipped; enable Settings reconcile_on_open); next FE: get_project + list_project_categories + list_documents (SQLite-only)"
            ),
        );
        return Ok(project);
    }

    // Heavy reconcile must never block open; hard timeout + cancelable generation.
    let gen = state.begin_open_reconcile();
    let pool = state.pool.clone();
    let cancel = state.open_reconcile_cancel.clone();
    let gen_cell = state.open_reconcile_generation.clone();
    let project_id = id.clone();
    let folder_label = project.root_relative_path.clone();
    let timeout_secs = crate::auto_version::RECONCILE_TIMEOUT_SECS;
    tokio::spawn(async move {
        crate::diagnostics::log_event(
            "reconcile",
            &format!(
                "start project={project_id} folder={folder_label} gen={gen} timeout_s={timeout_secs}"
            ),
        );
        let fut = crate::auto_version::reconcile_project_cancelable(
            &pool,
            &project_id,
            Some(cancel.as_ref()),
            Some((gen_cell.as_ref(), gen)),
        );
        match tokio::time::timeout(std::time::Duration::from_secs(timeout_secs), fut).await {
            Ok(Ok(results)) => {
                crate::diagnostics::log_event(
                    "reconcile",
                    &format!(
                        "done project={project_id} gen={gen} results={}",
                        results.len()
                    ),
                );
            }
            Ok(Err(e)) => {
                crate::diagnostics::log_event(
                    "reconcile",
                    &format!("error project={project_id} gen={gen}: {e}"),
                );
            }
            Err(_) => {
                cancel.store(true, std::sync::atomic::Ordering::SeqCst);
                crate::diagnostics::log_event(
                    "reconcile",
                    &format!(
                        "timeout project={project_id} gen={gen} after {timeout_secs}s — cancelled"
                    ),
                );
            }
        }
    });

    crate::diagnostics::log_event("open", &format!("returned id={id} (reconcile background)"));
    Ok(project)
}

/// If a project has zero category rows, seed STANDARD_FOLDERS (DB only — no disk walk).
pub(crate) async fn ensure_standard_categories_if_empty(
    pool: &sqlx::SqlitePool,
    project_id: &str,
) -> AppResult<()> {
    let count: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM categories WHERE project_id = ?")
            .bind(project_id)
            .fetch_one(pool)
            .await?;
    if count.0 > 0 {
        return Ok(());
    }
    for folder in STANDARD_FOLDERS {
        let cat_id = Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT OR IGNORE INTO categories (id, project_id, key, name, relative_path, sort_order)
             VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(&cat_id)
        .bind(project_id)
        .bind(folder.key)
        .bind(folder.name)
        .bind(folder.relative_path)
        .bind(folder.sort_order)
        .execute(pool)
        .await?;
    }
    crate::diagnostics::log_event(
        "open",
        &format!("seeded standard categories for project={project_id}"),
    );
    Ok(())
}

/// Expected confirmation token for remove: business code, else folder name.
pub(crate) fn expected_remove_confirm(project: &Project) -> String {
    project
        .code
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or(project.root_relative_path.trim())
        .to_string()
}

/// Delete SQLite registration only — never opens, reconciles, indexes, or touches disk files.
pub async fn remove_project_registration_inner(
    pool: &sqlx::SqlitePool,
    id: &str,
    confirm_code: &str,
) -> AppResult<RemoveProjectResult> {
    let project = sqlx::query_as::<_, Project>("SELECT * FROM projects WHERE id = ?")
        .bind(id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| AppError::msg("Proyecto no encontrado"))?;

    let expected = expected_remove_confirm(&project);
    if confirm_code.trim() != expected {
        return Err(AppError::msg(format!(
            "CONFIRM_CODE:Escriba «{expected}» para confirmar"
        )));
    }

    // Resolve path only to report it — do not open/index/reconcile.
    let root_relative_path = project.root_relative_path.clone();

    // FTS has no FK cascade.
    sqlx::query("DELETE FROM search_fts WHERE project_id = ? OR entity_id = ?")
        .bind(id)
        .bind(id)
        .execute(pool)
        .await
        .ok();

    sqlx::query("DELETE FROM projects WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await?;

    Ok(RemoveProjectResult {
        id: id.to_string(),
        code: project.code,
        root_relative_path,
        message: "Proyecto quitado de IngManager; los archivos se conservan.".to_string(),
    })
}

/// Remove project from the app by UUID. SQLite only; physical files untouched.
/// Does not call open_project, reconcile, or index.
#[tauri::command]
pub async fn remove_project_registration(
    state: State<'_, AppState>,
    id: String,
    confirm_code: String,
) -> AppResult<RemoveProjectResult> {
    remove_project_registration_inner(&state.pool, &id, &confirm_code).await
}

#[tauri::command]
pub async fn list_project_categories(
    state: State<'_, AppState>,
    project_id: String,
) -> AppResult<Vec<CategorySummary>> {
    crate::diagnostics::log_event(
        "categories",
        &format!("begin project={project_id}"),
    );
    let started = std::time::Instant::now();

    // Pure SQLite — never touch the filesystem on open/list (1090 hang class).
    let mut categories = sqlx::query_as::<_, CategorySummary>(
        "SELECT
            c.key,
            c.name,
            c.relative_path,
            c.sort_order,
            COALESCE((
                SELECT COUNT(*) FROM indexed_files f
                WHERE f.project_id = c.project_id
                  AND f.category_key = c.key
                  AND f.is_directory = 0
                  AND f.is_available = 1
            ), 0) AS file_count,
            0 AS model_count,
            COALESCE((
                SELECT COUNT(*) FROM documents d
                WHERE d.project_id = c.project_id AND d.category_id = c.id
            ), 0) AS document_count
         FROM categories c
         WHERE c.project_id = ?
         ORDER BY c.sort_order",
    )
    .bind(&project_id)
    .fetch_all(&state.pool)
    .await?;

    // Approximate model principals with an extension filter in SQL.
    // Previously loaded EVERY modelos file_name into RAM and ran count_principals
    // in-process — OOM/hang risk on huge indexed trees (project 1090).
    if let Some(modelos) = categories.iter_mut().find(|c| c.key == "modelos") {
        let count: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM indexed_files
             WHERE project_id = ? AND category_key = 'modelos'
               AND is_directory = 0 AND is_available = 1
               AND lower(COALESCE(extension, '')) IN (
                    'sdb', 'rvt', 'rfa', 'rtd', 'edb', 'idea', 'xlsx', 'xls'
               )",
        )
        .bind(&project_id)
        .fetch_one(&state.pool)
        .await
        .unwrap_or((0,));
        // Prefer principal-ish count; UI falls back to file_count when 0.
        modelos.model_count = if count.0 > 0 {
            count.0
        } else {
            modelos.file_count
        };
    }

    crate::diagnostics::log_event(
        "categories",
        &format!(
            "done project={project_id} n={} ms={}",
            categories.len(),
            started.elapsed().as_millis()
        ),
    );
    Ok(categories)
}

#[allow(dead_code)]
async fn project_codes_for(pool: &sqlx::SqlitePool, project_id: &str) -> AppResult<Vec<String>> {
    let row: Option<(Option<String>, String)> =
        sqlx::query_as("SELECT code, root_relative_path FROM projects WHERE id = ?")
            .bind(project_id)
            .fetch_optional(pool)
            .await?;
    let mut codes = Vec::new();
    if let Some((code, rel)) = row {
        if let Some(c) = code {
            if !c.trim().is_empty() {
                codes.push(c.trim().to_string());
            }
        }
        if let Some(base) = std::path::Path::new(&rel)
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

#[tauri::command]
pub async fn list_recent_projects(
    state: State<'_, AppState>,
    limit: Option<i64>,
) -> AppResult<Vec<Project>> {
    let lim = limit.unwrap_or(8).clamp(1, 50);
    let projects = sqlx::query_as::<_, Project>(
        "SELECT * FROM projects
         WHERE last_opened_at IS NOT NULL
         ORDER BY last_opened_at DESC
         LIMIT ?",
    )
    .bind(lim)
    .fetch_all(&state.pool)
    .await?;
    Ok(projects)
}

#[tauri::command]
pub async fn list_favorite_projects(state: State<'_, AppState>) -> AppResult<Vec<Project>> {
    let projects = sqlx::query_as::<_, Project>(
        "SELECT * FROM projects WHERE is_favorite = 1
         ORDER BY COALESCE(code, root_relative_path) DESC",
    )
    .fetch_all(&state.pool)
    .await?;
    Ok(projects)
}

#[tauri::command]
pub async fn get_project_absolute_path(
    state: State<'_, AppState>,
    project_id: String,
) -> AppResult<String> {
    let loc = project_paths::resolve_project_location(&state.pool, &project_id).await?;
    Ok(loc.absolute_path.display().to_string())
}

/// Insert category rows from smart mapping of an existing directory.
pub(crate) async fn insert_mapped_categories(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    project_id: &str,
    project_dir: &std::path::Path,
    create_missing_standard: bool,
) -> AppResult<Vec<String>> {
    let mapped = map_project_categories(project_dir);
    let mut matched = Vec::new();
    for m in &mapped {
        if m.matched {
            matched.push(m.relative_path.clone());
        } else if create_missing_standard {
            let path = project_dir.join(&m.relative_path);
            std::fs::create_dir_all(&path)?;
            if m.key == "transmisiones" {
                std::fs::create_dir_all(path.join("ENVIADOS"))?;
                std::fs::create_dir_all(path.join("RECIBIDOS"))?;
            }
        }
        sqlx::query(
            "INSERT INTO categories (id, project_id, key, name, relative_path, sort_order)
             VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(Uuid::new_v4().to_string())
        .bind(project_id)
        .bind(m.key)
        .bind(m.name)
        .bind(&m.relative_path)
        .bind(m.sort_order)
        .execute(&mut **tx)
        .await?;
    }
    Ok(matched)
}

