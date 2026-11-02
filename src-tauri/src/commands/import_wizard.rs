use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tauri::State;
use uuid::Uuid;
use walkdir::WalkDir;

use crate::db;
use crate::error::{AppError, AppResult};
use crate::fs_util::copy_recursively;
use crate::models::STANDARD_FOLDERS;
use crate::path_security::ensure_under_root;
use crate::search_index;
use crate::state::AppState;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DetectedFolder {
    pub absolute_path: String,
    pub name: String,
    pub suggested_code: Option<String>,
    pub suggested_name: String,
    pub matched_standard_folders: Vec<String>,
    pub looks_like_project: bool,
    pub conflict_with_existing: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportPreview {
    pub source_path: String,
    pub target_relative_path: String,
    pub target_absolute_path: String,
    pub will_overwrite: bool,
    pub folders_to_create: Vec<String>,
    pub file_count_hint: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportProjectInput {
    pub source_path: String,
    pub code: String,
    pub name: String,
    pub description: Option<String>,
    pub client_name: Option<String>,
    /// If target exists, require explicit confirmation
    pub confirm_overwrite: Option<bool>,
    /// Copy files (true) vs register only metadata over existing path (false) — Phase 5 always copies into root
    pub copy_files: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportProjectResult {
    pub project_id: String,
    pub code: String,
    pub target_path: String,
    pub copied: bool,
}

async fn require_root(state: &AppState) -> AppResult<PathBuf> {
    let root = db::get_setting(&state.pool, "root_path")
        .await?
        .ok_or_else(|| AppError::msg("Configure la carpeta raíz antes de continuar"))?;
    let path = PathBuf::from(&root);
    if !path.is_dir() {
        return Err(AppError::msg("La carpeta raíz no es accesible"));
    }
    Ok(path)
}

fn looks_like_aann(name: &str) -> bool {
    name.len() == 4 && name.chars().all(|c| c.is_ascii_digit())
}

fn detect_standard_folders(dir: &Path) -> Vec<String> {
    let mut matched = Vec::new();
    for folder in STANDARD_FOLDERS {
        if dir.join(folder.relative_path).is_dir() {
            matched.push(folder.relative_path.to_string());
        }
    }
    // Also accept ENVIADOS/RECIBIDOS under transmissions
    let tx = dir.join("06_TRANSMISIONES");
    if tx.join("ENVIADOS").is_dir() {
        matched.push("06_TRANSMISIONES/ENVIADOS".into());
    }
    if tx.join("RECIBIDOS").is_dir() {
        matched.push("06_TRANSMISIONES/RECIBIDOS".into());
    }
    matched
}

#[tauri::command]
pub async fn detect_importable_projects(
    state: State<'_, AppState>,
    source_path: String,
) -> AppResult<Vec<DetectedFolder>> {
    let root = require_root(&state).await?;
    let source = PathBuf::from(&source_path);
    if !source.is_dir() {
        return Err(AppError::msg("La ruta fuente no es una carpeta"));
    }

    let existing: Vec<(String,)> = sqlx::query_as("SELECT code FROM projects")
        .fetch_all(&state.pool)
        .await?;
    let existing_codes: std::collections::HashSet<String> =
        existing.into_iter().map(|r| r.0).collect();

    let mut candidates = Vec::new();

    // Case A: source itself looks like a project
    let self_matched = detect_standard_folders(&source);
    if !self_matched.is_empty() || looks_like_aann(&source.file_name().unwrap_or_default().to_string_lossy()) {
        let name = source
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "proyecto".into());
        let code = if looks_like_aann(&name) {
            Some(name.clone())
        } else {
            None
        };
        let conflict = code
            .as_ref()
            .map(|c| existing_codes.contains(c) || root.join(c).exists())
            .unwrap_or(false);
        candidates.push(DetectedFolder {
            absolute_path: source.display().to_string(),
            name: name.clone(),
            suggested_code: code,
            suggested_name: name,
            matched_standard_folders: self_matched,
            looks_like_project: true,
            conflict_with_existing: conflict,
        });
    }

    // Case B: children that look like projects
    if let Ok(rd) = fs::read_dir(&source) {
        for entry in rd.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with('.') {
                continue;
            }
            let matched = detect_standard_folders(&path);
            let aann = looks_like_aann(&name);
            if matched.is_empty() && !aann {
                continue;
            }
            let code = if aann { Some(name.clone()) } else { None };
            let conflict = code
                .as_ref()
                .map(|c| existing_codes.contains(c) || root.join(c).exists())
                .unwrap_or_else(|| root.join(&name).exists());
            candidates.push(DetectedFolder {
                absolute_path: path.display().to_string(),
                name: name.clone(),
                suggested_code: code,
                suggested_name: name,
                matched_standard_folders: matched,
                looks_like_project: true,
                conflict_with_existing: conflict,
            });
        }
    }

    Ok(candidates)
}

#[tauri::command]
pub async fn preview_import_project(
    state: State<'_, AppState>,
    source_path: String,
    code: String,
) -> AppResult<ImportPreview> {
    let root = require_root(&state).await?;
    let source = PathBuf::from(&source_path);
    if !source.is_dir() {
        return Err(AppError::msg("Fuente inválida"));
    }
    let code = code.trim().to_string();
    if code.is_empty() {
        return Err(AppError::msg("Código obligatorio"));
    }
    let target = ensure_under_root(&root, &root.join(&code))?;
    let will_overwrite = target.exists();

    let mut folders = STANDARD_FOLDERS
        .iter()
        .map(|f| f.relative_path.to_string())
        .collect::<Vec<_>>();
    folders.push("06_TRANSMISIONES/ENVIADOS".into());
    folders.push("06_TRANSMISIONES/RECIBIDOS".into());

    let mut file_count = 0u64;
    for entry in WalkDir::new(&source).into_iter().filter_map(|e| e.ok()) {
        if entry.path().is_file() {
            file_count += 1;
            if file_count > 5000 {
                break;
            }
        }
    }

    Ok(ImportPreview {
        source_path: source.display().to_string(),
        target_relative_path: code,
        target_absolute_path: target.display().to_string(),
        will_overwrite,
        folders_to_create: folders,
        file_count_hint: file_count,
    })
}

#[tauri::command]
pub async fn import_existing_project(
    state: State<'_, AppState>,
    input: ImportProjectInput,
) -> AppResult<ImportProjectResult> {
    let root = require_root(&state).await?;
    let source = PathBuf::from(&input.source_path);
    if !source.is_dir() {
        return Err(AppError::msg("Fuente inválida"));
    }
    let code = input.code.trim().to_string();
    let name = input.name.trim().to_string();
    if code.is_empty() || name.is_empty() {
        return Err(AppError::msg("Código y nombre son obligatorios"));
    }
    if !looks_like_aann(&code) {
        return Err(AppError::msg(
            "El código debe ser AANN (4 dígitos). Use un código libre o confirme ampliación futura.",
        ));
    }

    let existing: Option<(String,)> = sqlx::query_as("SELECT id FROM projects WHERE code = ?")
        .bind(&code)
        .fetch_optional(&state.pool)
        .await?;
    if existing.is_some() {
        return Err(AppError::msg(format!(
            "Ya existe un proyecto con código {code} en la base de datos"
        )));
    }

    let target = ensure_under_root(&root, &root.join(&code))?;
    if target.exists() && !input.confirm_overwrite.unwrap_or(false) {
        return Err(AppError::msg(format!(
            "La carpeta destino {code} ya existe. Confirme sobrescritura/mezcla explícitamente."
        )));
    }

    let copy = input.copy_files.unwrap_or(true);
    if copy {
        // Copy without mass-renaming: preserve source tree under target code folder
        if target.exists() {
            // Merge copy: copy contents into existing without deleting user files unless same name
            for entry in fs::read_dir(&source)? {
                let entry = entry?;
                let dest = target.join(entry.file_name());
                if entry.path().is_dir() {
                    copy_recursively(&entry.path(), &dest)?;
                } else if !dest.exists() {
                    fs::copy(entry.path(), &dest)?;
                } else if input.confirm_overwrite.unwrap_or(false) {
                    fs::copy(entry.path(), &dest)?;
                }
                // else skip existing file — no silent overwrite
            }
        } else {
            copy_recursively(&source, &target)?;
        }
        // Ensure standard folders exist (create missing only)
        for folder in STANDARD_FOLDERS {
            let p = target.join(folder.relative_path);
            fs::create_dir_all(&p)?;
            if folder.key == "transmisiones" {
                fs::create_dir_all(p.join("ENVIADOS"))?;
                fs::create_dir_all(p.join("RECIBIDOS"))?;
            }
        }
    } else {
        return Err(AppError::msg(
            "Importación sin copia no está habilitada en Fase 5 (solo importación por copia)",
        ));
    }

    let id = Uuid::new_v4().to_string();
    let mut tx = state.pool.begin().await?;
    sqlx::query(
        "INSERT INTO projects (
            id, code, name, description, client_name, status, root_relative_path,
            created_at, updated_at
         ) VALUES (?, ?, ?, ?, ?, 'active', ?, datetime('now'), datetime('now'))",
    )
    .bind(&id)
    .bind(&code)
    .bind(&name)
    .bind(&input.description)
    .bind(&input.client_name)
    .bind(&code)
    .execute(&mut *tx)
    .await?;

    for folder in STANDARD_FOLDERS {
        sqlx::query(
            "INSERT INTO categories (id, project_id, key, name, relative_path, sort_order)
             VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(Uuid::new_v4().to_string())
        .bind(&id)
        .bind(folder.key)
        .bind(folder.name)
        .bind(folder.relative_path)
        .bind(folder.sort_order)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;

    crate::audit::log_event(
        &state.pool,
        Some(&id),
        "project",
        Some(&id),
        "import_existing",
        Some(&format!("Imported from {}", source.display())),
    )
    .await
    .ok();

    search_index::upsert_fts(
        &state.pool,
        "project",
        &id,
        Some(&id),
        &name,
        Some(&code),
        input.description.as_deref(),
        None,
    )
    .await
    .ok();

    Ok(ImportProjectResult {
        project_id: id,
        code,
        target_path: target.display().to_string(),
        copied: true,
    })
}

#[tauri::command]
pub async fn propose_import_code(
    state: State<'_, AppState>,
    preferred: Option<String>,
) -> AppResult<String> {
    use crate::codes::{current_year_two_digits, next_project_code};
    if let Some(p) = preferred {
        let p = p.trim().to_string();
        if looks_like_aann(&p) {
            let exists: Option<(String,)> =
                sqlx::query_as("SELECT id FROM projects WHERE code = ?")
                    .bind(&p)
                    .fetch_optional(&state.pool)
                    .await?;
            if exists.is_none() {
                return Ok(p);
            }
        }
    }
    let rows: Vec<(String,)> = sqlx::query_as("SELECT code FROM projects")
        .fetch_all(&state.pool)
        .await?;
    let codes: Vec<String> = rows.into_iter().map(|r| r.0).collect();
    next_project_code(current_year_two_digits(), &codes)
}
