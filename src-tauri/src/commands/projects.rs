use std::path::PathBuf;

use tauri::State;
use uuid::Uuid;

use crate::codes::{current_year_two_digits, next_project_code};
use crate::db;
use crate::error::{AppError, AppResult};
use crate::models::{
    CategorySummary, CreateProjectInput, Project, UpdateProjectInput, STANDARD_FOLDERS,
};
use crate::path_security::ensure_under_root;
use crate::state::AppState;

async fn require_root(state: &AppState) -> AppResult<PathBuf> {
    let root = db::get_setting(&state.pool, "root_path")
        .await?
        .ok_or_else(|| AppError::msg("Configure la carpeta raíz antes de continuar"))?;
    let path = PathBuf::from(&root);
    if !path.is_dir() {
        return Err(AppError::msg(
            "La carpeta raíz configurada no es accesible",
        ));
    }
    Ok(path)
}

#[tauri::command]
pub async fn preview_next_project_code(state: State<'_, AppState>) -> AppResult<String> {
    let year = current_year_two_digits();
    let rows: Vec<(String,)> = sqlx::query_as("SELECT code FROM projects")
        .fetch_all(&state.pool)
        .await?;
    let codes: Vec<String> = rows.into_iter().map(|r| r.0).collect();
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
             code DESC",
        )
        .fetch_all(&state.pool)
        .await?
    } else {
        sqlx::query_as::<_, Project>(
            "SELECT * FROM projects WHERE status = 'active' ORDER BY code DESC",
        )
        .fetch_all(&state.pool)
        .await?
    };
    Ok(projects)
}

#[tauri::command]
pub async fn get_project(state: State<'_, AppState>, id: String) -> AppResult<Project> {
    sqlx::query_as::<_, Project>("SELECT * FROM projects WHERE id = ?")
        .bind(id)
        .fetch_optional(&state.pool)
        .await?
        .ok_or_else(|| AppError::msg("Proyecto no encontrado"))
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

    let root = require_root(&state).await?;
    let year = current_year_two_digits();
    let rows: Vec<(String,)> = sqlx::query_as("SELECT code FROM projects")
        .fetch_all(&state.pool)
        .await?;
    let codes: Vec<String> = rows.into_iter().map(|r| r.0).collect();
    let code = next_project_code(year, &codes)?;

    let project_dir = ensure_under_root(&root, &root.join(&code))?;
    if project_dir.exists() {
        return Err(AppError::msg(format!(
            "Ya existe la carpeta física {code} bajo la raíz"
        )));
    }

    std::fs::create_dir_all(&project_dir)?;
    for folder in STANDARD_FOLDERS {
        let path = project_dir.join(folder.relative_path);
        std::fs::create_dir_all(&path)?;
        if folder.key == "transmisiones" {
            std::fs::create_dir_all(path.join("ENVIADOS"))?;
            std::fs::create_dir_all(path.join("RECIBIDOS"))?;
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
            id, code, name, description, client_name, status, root_relative_path,
            start_date, end_date, last_opened_at, created_at, updated_at
         ) VALUES (
            ?, ?, ?, ?, ?, 'active', ?,
            ?, ?, datetime('now'), datetime('now'), datetime('now')
         )",
    )
    .bind(&id)
    .bind(&code)
    .bind(&name)
    .bind(&description)
    .bind(&client_name)
    .bind(&code)
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
        "INSERT INTO audit_log (id, entity_type, entity_id, action, detail)
         VALUES (?, 'project', ?, 'create', ?)",
    )
    .bind(Uuid::new_v4().to_string())
    .bind(&id)
    .bind(format!("Created project {code}"))
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;

    get_project(state, id).await
}

#[tauri::command]
pub async fn update_project(
    state: State<'_, AppState>,
    input: UpdateProjectInput,
) -> AppResult<Project> {
    let existing = get_project(state.clone(), input.id.clone()).await?;

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

    get_project(state, input.id).await
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
pub async fn open_project(state: State<'_, AppState>, id: String) -> AppResult<Project> {
    sqlx::query(
        "UPDATE projects SET last_opened_at = datetime('now'), updated_at = datetime('now')
         WHERE id = ?",
    )
    .bind(&id)
    .execute(&state.pool)
    .await?;
    get_project(state, id).await
}

#[tauri::command]
pub async fn list_project_categories(
    state: State<'_, AppState>,
    project_id: String,
) -> AppResult<Vec<CategorySummary>> {
    let categories = sqlx::query_as::<_, CategorySummary>(
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
            ), 0) AS file_count
         FROM categories c
         WHERE c.project_id = ?
         ORDER BY c.sort_order",
    )
    .bind(project_id)
    .fetch_all(&state.pool)
    .await?;
    Ok(categories)
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
        "SELECT * FROM projects WHERE is_favorite = 1 ORDER BY code DESC",
    )
    .fetch_all(&state.pool)
    .await?;
    Ok(projects)
}
