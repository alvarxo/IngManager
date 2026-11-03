use std::path::PathBuf;

use tauri::{AppHandle, Emitter, State};
use uuid::Uuid;

use crate::category_mapping::map_project_categories;
use crate::codes::{current_year_two_digits, next_project_code};
use crate::diagnostics;
use crate::error::{AppError, AppResult};
use crate::models::{
    CategorySummary, CreateProjectInput, DeleteProjectResult, Project, UpdateProjectInput,
    STANDARD_FOLDERS,
};
use crate::path_security::ensure_under_root;
use crate::project_paths::{self, sanitize_folder_name, AuthorizedRoot};
use crate::state::AppState;

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
    fetch_project(&state, &id).await
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

    let mut folder_name = if let Some(f) = input.folder_name.as_deref() {
        sanitize_folder_name(f)?
    } else if let Some(ref c) = code {
        c.clone()
    } else {
        sanitize_folder_name(&name.replace(' ', "_"))?
    };

    let conflict = input
        .folder_conflict_policy
        .as_deref()
        .unwrap_or("abort")
        .trim()
        .to_ascii_lowercase();
    if !matches!(conflict.as_str(), "abort" | "use_existing" | "rename") {
        return Err(AppError::msg(
            "Política de conflicto de carpeta inválida (abort|use_existing|rename)",
        ));
    }

    // Path collision independent of code (another registered project)
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

    let mut project_dir = ensure_under_root(&root_path, &root_path.join(&folder_name))?;
    let mut used_existing = false;
    if project_dir.exists() {
        match conflict.as_str() {
            "abort" => {
                return Err(AppError::msg(format!(
                    "FOLDER_EXISTS:{folder_name}:Ya existe la carpeta física bajo la raíz. Elija usar la existente, renombrar, o «Añadir existente»."
                )));
            }
            "use_existing" => {
                if !project_dir.is_dir() {
                    return Err(AppError::msg(format!(
                        "La ruta {folder_name} existe pero no es una carpeta"
                    )));
                }
                used_existing = true;
            }
            "rename" => {
                let base = folder_name.clone();
                let mut n = 2u32;
                loop {
                    let candidate = format!("{base}_{n}");
                    let cand_dir = ensure_under_root(&root_path, &root_path.join(&candidate))?;
                    let registered: Option<(String,)> = sqlx::query_as(
                        "SELECT id FROM projects WHERE authorized_root_id = ? AND root_relative_path = ?",
                    )
                    .bind(&root.id)
                    .bind(&candidate)
                    .fetch_optional(&state.pool)
                    .await?;
                    if registered.is_none() && !cand_dir.exists() {
                        folder_name = candidate;
                        project_dir = cand_dir;
                        break;
                    }
                    n += 1;
                    if n > 9999 {
                        return Err(AppError::msg(
                            "No se pudo generar un nombre de carpeta libre",
                        ));
                    }
                }
            }
            _ => unreachable!(),
        }
    }

    let import_mode = if used_existing { "linked" } else { "created" };
    if !used_existing {
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
            ?, ?, ?, ?,
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
    .bind(import_mode)
    .bind(if used_existing { "mapped" } else { "standard" })
    .bind(&input.start_date)
    .bind(&input.end_date)
    .execute(&mut *tx)
    .await?;

    if used_existing {
        insert_mapped_categories(&mut tx, &id, &project_dir, false).await?;
    } else {
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
    let existing = fetch_project(&state, &input.id).await?;

    let name = input
        .name
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or(existing.name);
    let description = input
        .description
        .or(existing.description)
        .map(|s| s.trim().to_string());
    let client_name = input
        .client_name
        .or(existing.client_name)
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    let status = input.status.unwrap_or(existing.status);
    let allowed = ["active", "archived", "cancelled", "completed"];
    if !allowed.contains(&status.as_str()) {
        return Err(AppError::msg("Estado de proyecto inválido"));
    }
    let start_date = input.start_date.or(existing.start_date);
    let end_date = input.end_date.or(existing.end_date);
    let is_favorite = input
        .is_favorite
        .map(|v| if v { 1 } else { 0 })
        .unwrap_or(existing.is_favorite);

    sqlx::query(
        "UPDATE projects SET
            name = ?, description = ?, client_name = ?, status = ?,
            start_date = ?, end_date = ?, is_favorite = ?, updated_at = datetime('now')
         WHERE id = ?",
    )
    .bind(&name)
    .bind(&description)
    .bind(&client_name)
    .bind(&status)
    .bind(&start_date)
    .bind(&end_date)
    .bind(is_favorite)
    .bind(&input.id)
    .execute(&state.pool)
    .await?;

    crate::search_index::upsert_fts(
        &state.pool,
        "project",
        &input.id,
        Some(&input.id),
        &name,
        existing.code.as_deref(),
        description.as_deref(),
        None,
    )
    .await
    .ok();

    fetch_project(&state, &input.id).await
}

#[tauri::command]
pub async fn archive_project(state: State<'_, AppState>, id: String) -> AppResult<Project> {
    update_project(
        state,
        UpdateProjectInput {
            id,
            name: None,
            description: None,
            client_name: None,
            status: Some("archived".into()),
            start_date: None,
            end_date: None,
            is_favorite: None,
        },
    )
    .await
}

#[tauri::command]
pub async fn open_project(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
) -> AppResult<Project> {
    diagnostics::log_event("open", &format!("begin id={id}"));
    let project = match fetch_project(&state, &id).await {
        Ok(p) => p,
        Err(e) => {
            diagnostics::log_event("open", &format!("missing id={id}: {e}"));
            return Err(e);
        }
    };

    if let Err(e) = sqlx::query(
        "UPDATE projects SET last_opened_at = datetime('now'), updated_at = datetime('now')
         WHERE id = ?",
    )
    .bind(&id)
    .execute(&state.pool)
    .await
    {
        diagnostics::log_event("open", &format!("timestamp update failed id={id}: {e}"));
        // Still return the project — opening must stay recoverable.
    }

    // Heavy reconcile is cancelable and never blocks UI open.
    let gen = state.begin_open_reconcile();
    let pool = state.pool.clone();
    let cancel = state.open_reconcile_cancel.clone();
    let gen_cell = state.open_reconcile_generation.clone();
    let project_id = id.clone();
    tokio::spawn(async move {
        diagnostics::log_event(
            "reconcile",
            &format!("start project={project_id} gen={gen}"),
        );
        match crate::auto_version::reconcile_project_cancelable(
            &pool,
            &project_id,
            Some(cancel.as_ref()),
            Some((gen_cell.as_ref(), gen)),
        )
        .await
        {
            Ok(results) => {
                diagnostics::log_event(
                    "reconcile",
                    &format!(
                        "done project={project_id} gen={gen} results={}",
                        results.len()
                    ),
                );
                let _ = app.emit(
                    "open-reconcile",
                    serde_json::json!({
                        "projectId": project_id,
                        "generation": gen,
                        "ok": true,
                        "resultCount": results.len(),
                    }),
                );
            }
            Err(e) => {
                diagnostics::log_event(
                    "reconcile",
                    &format!("error project={project_id} gen={gen}: {e}"),
                );
                let _ = app.emit(
                    "open-reconcile",
                    serde_json::json!({
                        "projectId": project_id,
                        "generation": gen,
                        "ok": false,
                        "message": e.to_string(),
                    }),
                );
            }
        }
    });

    diagnostics::log_event(
        "open",
        &format!(
            "ready id={id} code={}",
            project.code.as_deref().unwrap_or("-")
        ),
    );
    Ok(project)
}

#[tauri::command]
pub async fn cancel_open_reconcile(state: State<'_, AppState>) -> AppResult<()> {
    state.cancel_open_reconcile();
    diagnostics::log_event("reconcile", "cancel requested");
    Ok(())
}

/// Remove project from SQLite only — keep files on disk.
#[tauri::command]
pub async fn remove_project_from_app(
    state: State<'_, AppState>,
    id: String,
) -> AppResult<DeleteProjectResult> {
    delete_project_inner(&state, &id, false, None).await
}

/// Delete project registration and send physical folder to the OS recycle bin.
/// Linked projects are register-only (files untouched).
/// `confirm_code` must match the project business code when one is set.
#[tauri::command]
pub async fn delete_project_with_files(
    state: State<'_, AppState>,
    id: String,
    confirm_code: Option<String>,
) -> AppResult<DeleteProjectResult> {
    delete_project_inner(&state, &id, true, confirm_code.as_deref()).await
}

async fn delete_project_inner(
    state: &AppState,
    id: &str,
    delete_files: bool,
    confirm_code: Option<&str>,
) -> AppResult<DeleteProjectResult> {
    let project = fetch_project(state, id).await?;
    if delete_files {
        let expected = project
            .code
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .unwrap_or(project.root_relative_path.trim());
        let provided = confirm_code.map(str::trim).unwrap_or("");
        if provided != expected {
            return Err(AppError::msg(format!(
                "CONFIRM_CODE:Escriba «{expected}» para confirmar la eliminación"
            )));
        }
    }

    // Cancel any reconcile targeting this project.
    state.cancel_open_reconcile();

    let loc = project_paths::resolve_project_location(&state.pool, id).await?;
    let is_linked = project.import_mode == "linked";
    let (files_action, message) = if delete_files && !is_linked {
        if loc.absolute_path.is_dir() {
            // Safety: refuse deleting the authorized root itself.
            if loc.absolute_path == loc.root_path {
                return Err(AppError::msg(
                    "No se puede enviar la raíz autorizada a la papelera",
                ));
            }
            trash::delete(&loc.absolute_path).map_err(|e| {
                AppError::msg(format!(
                    "No se pudo enviar la carpeta a la papelera del sistema: {e}"
                ))
            })?;
            diagnostics::log_event(
                "delete",
                &format!(
                    "recycled id={id} path={}",
                    loc.absolute_path.display()
                ),
            );
            (
                "recycled".to_string(),
                "Proyecto eliminado y carpeta enviada a la papelera del sistema.".to_string(),
            )
        } else {
            (
                "missing".to_string(),
                "Proyecto eliminado del registro; la carpeta física ya no existía.".to_string(),
            )
        }
    } else if delete_files && is_linked {
        (
            "register_only".to_string(),
            "Proyecto vinculado: solo se eliminó el registro (archivos intactos).".to_string(),
        )
    } else {
        (
            "kept".to_string(),
            "Proyecto quitado de IngManager; los archivos se conservan.".to_string(),
        )
    };

    // FTS has no FK cascade.
    sqlx::query("DELETE FROM search_fts WHERE project_id = ? OR entity_id = ?")
        .bind(id)
        .bind(id)
        .execute(&state.pool)
        .await
        .ok();

    sqlx::query("DELETE FROM projects WHERE id = ?")
        .bind(id)
        .execute(&state.pool)
        .await?;

    diagnostics::log_event(
        "delete",
        &format!("db_removed id={id} files_action={files_action}"),
    );

    Ok(DeleteProjectResult {
        id: id.to_string(),
        code: project.code,
        files_action,
        message,
    })
}

#[tauri::command]
pub async fn list_project_categories(
    state: State<'_, AppState>,
    project_id: String,
) -> AppResult<Vec<CategorySummary>> {
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

    // For modelos, count principal engineering groups (not every physical aux file).
    if let Some(modelos) = categories.iter_mut().find(|c| c.key == "modelos") {
        let names: Vec<(String,)> = sqlx::query_as(
            "SELECT file_name FROM indexed_files
             WHERE project_id = ? AND category_key = 'modelos'
               AND is_directory = 0 AND is_available = 1",
        )
        .bind(&project_id)
        .fetch_all(&state.pool)
        .await?;
        let codes = project_codes_for(&state.pool, &project_id).await?;
        let file_names: Vec<String> = names.into_iter().map(|r| r.0).collect();
        modelos.model_count =
            crate::engineering_files::count_principals(&file_names, &codes) as i64;
    }

    Ok(categories)
}

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

