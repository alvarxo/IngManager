use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tauri::State;
use uuid::Uuid;
use walkdir::WalkDir;

use crate::category_mapping::{detect_matched_folder_names, map_project_categories};
use crate::commands::projects::{
    assert_code_unique, insert_mapped_categories, normalize_optional_code,
};
use crate::error::{AppError, AppResult};
use crate::fs_util::copy_recursively;
use crate::path_security::ensure_under_root;
use crate::project_paths::{self, sanitize_folder_name};
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
    pub under_authorized_root: bool,
    pub authorized_root_id: Option<String>,
    pub relative_to_root: Option<String>,
    pub category_mappings: Vec<CategoryMappingPreview>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CategoryMappingPreview {
    pub key: String,
    pub name: String,
    pub relative_path: String,
    pub matched: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportPreview {
    pub source_path: String,
    pub mode: String,
    pub target_relative_path: String,
    pub target_absolute_path: String,
    pub authorized_root_id: Option<String>,
    pub will_overwrite: bool,
    pub will_copy: bool,
    pub link_in_place: bool,
    pub folders_to_create: Vec<String>,
    pub category_mappings: Vec<CategoryMappingPreview>,
    pub file_count_hint: u64,
    pub requires_authorize_root: bool,
    pub suggested_authorize_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportProjectInput {
    pub source_path: String,
    pub name: String,
    pub code: Option<String>,
    pub description: Option<String>,
    pub client_name: Option<String>,
    /// link | copy | reorganize_later
    pub mode: Option<String>,
    /// Folder name under target root when copying (independent of code).
    pub folder_name: Option<String>,
    pub authorized_root_id: Option<String>,
    /// If target exists on copy, require explicit confirmation
    pub confirm_overwrite: Option<bool>,
    /// When linking outside roots: authorize this parent path first
    pub authorize_parent_path: Option<String>,
    /// Create missing standard folders on import (default false for link/reorganize)
    pub create_missing_folders: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportProjectResult {
    pub project_id: String,
    pub code: Option<String>,
    pub target_path: String,
    pub mode: String,
    pub copied: bool,
    pub linked: bool,
}

fn looks_like_aann(name: &str) -> bool {
    name.len() == 4 && name.chars().all(|c| c.is_ascii_digit())
}

fn count_files(dir: &Path) -> u64 {
    let mut file_count = 0u64;
    for entry in WalkDir::new(dir).into_iter().filter_map(|e| e.ok()) {
        if entry.path().is_file() {
            file_count += 1;
            if file_count > 5000 {
                break;
            }
        }
    }
    file_count
}

fn mapping_previews(dir: &Path) -> Vec<CategoryMappingPreview> {
    map_project_categories(dir)
        .into_iter()
        .map(|m| CategoryMappingPreview {
            key: m.key.to_string(),
            name: m.name.to_string(),
            relative_path: m.relative_path,
            matched: m.matched,
        })
        .collect()
}

#[tauri::command]
pub async fn detect_importable_projects(
    state: State<'_, AppState>,
    source_path: String,
) -> AppResult<Vec<DetectedFolder>> {
    let source = PathBuf::from(&source_path);
    if !source.is_dir() {
        return Err(AppError::msg("La ruta fuente no es una carpeta"));
    }

    let existing: Vec<(Option<String>, String, String)> =
        sqlx::query_as("SELECT code, authorized_root_id, root_relative_path FROM projects")
            .fetch_all(&state.pool)
            .await?;
    let existing_codes: std::collections::HashSet<String> = existing
        .iter()
        .filter_map(|(c, _, _)| c.clone())
        .collect();
    let existing_paths: std::collections::HashSet<(String, String)> = existing
        .into_iter()
        .map(|(_, rid, rel)| (rid, rel))
        .collect();

    let mut candidates = Vec::new();

    async fn push_candidate(
        pool: &sqlx::SqlitePool,
        candidates: &mut Vec<DetectedFolder>,
        existing_codes: &std::collections::HashSet<String>,
        existing_paths: &std::collections::HashSet<(String, String)>,
        path: PathBuf,
    ) -> AppResult<()> {
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "proyecto".into());
        let matched = detect_matched_folder_names(&path);
        let aann = looks_like_aann(&name);
        if matched.is_empty() && !aann {
            return Ok(());
        }
        let code = if aann { Some(name.clone()) } else { None };
        let under = project_paths::find_root_for_path(pool, &path).await?;
        let (under_authorized_root, authorized_root_id, relative_to_root) = match &under {
            Some((root, _, rel)) => (true, Some(root.id.clone()), Some(rel.clone())),
            None => (false, None, None),
        };
        let conflict = code
            .as_ref()
            .map(|c| existing_codes.contains(c))
            .unwrap_or(false)
            || under
                .as_ref()
                .map(|(r, _, rel)| existing_paths.contains(&(r.id.clone(), rel.clone())))
                .unwrap_or(false);

        candidates.push(DetectedFolder {
            absolute_path: path.display().to_string(),
            name: name.clone(),
            suggested_code: code,
            suggested_name: name,
            matched_standard_folders: matched,
            looks_like_project: true,
            conflict_with_existing: conflict,
            under_authorized_root,
            authorized_root_id,
            relative_to_root,
            category_mappings: mapping_previews(&path),
        });
        Ok(())
    }

    // Case A: source itself
    let self_matched = detect_matched_folder_names(&source);
    let self_aann = looks_like_aann(
        &source
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default(),
    );
    if !self_matched.is_empty() || self_aann {
        push_candidate(
            &state.pool,
            &mut candidates,
            &existing_codes,
            &existing_paths,
            source.clone(),
        )
        .await?;
    }

    // Case B: children
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
            push_candidate(
                &state.pool,
                &mut candidates,
                &existing_codes,
                &existing_paths,
                path,
            )
            .await?;
        }
    }

    Ok(candidates)
}

#[tauri::command]
pub async fn preview_import_project(
    state: State<'_, AppState>,
    source_path: String,
    code: Option<String>,
    mode: Option<String>,
    folder_name: Option<String>,
    authorized_root_id: Option<String>,
) -> AppResult<ImportPreview> {
    let source = PathBuf::from(&source_path);
    if !source.is_dir() {
        return Err(AppError::msg("Fuente inválida"));
    }
    let mode = mode
        .unwrap_or_else(|| "link".into())
        .trim()
        .to_ascii_lowercase();
    if !["link", "copy", "reorganize_later"].contains(&mode.as_str()) {
        return Err(AppError::msg(
            "Modo inválido: use link, copy o reorganize_later",
        ));
    }

    let _code = normalize_optional_code(code.as_deref())?;
    let mappings = mapping_previews(&source);
    let file_count = count_files(&source);

    if mode == "link" || mode == "reorganize_later" {
        let under = project_paths::find_root_for_path(&state.pool, &source).await?;
        if let Some((root, _, rel)) = under {
            if rel.is_empty() {
                return Err(AppError::msg(
                    "No se puede vincular la raíz autorizada completa como proyecto",
                ));
            }
            return Ok(ImportPreview {
                source_path: source.display().to_string(),
                mode,
                target_relative_path: rel.clone(),
                target_absolute_path: source.display().to_string(),
                authorized_root_id: Some(root.id),
                will_overwrite: false,
                will_copy: false,
                link_in_place: true,
                folders_to_create: vec![],
                category_mappings: mappings,
                file_count_hint: file_count,
                requires_authorize_root: false,
                suggested_authorize_path: None,
            });
        }
        let parent = source
            .parent()
            .map(|p| p.display().to_string())
            .unwrap_or_default();
        return Ok(ImportPreview {
            source_path: source.display().to_string(),
            mode,
            target_relative_path: source
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default(),
            target_absolute_path: source.display().to_string(),
            authorized_root_id: None,
            will_overwrite: false,
            will_copy: false,
            link_in_place: true,
            folders_to_create: vec![],
            category_mappings: mappings,
            file_count_hint: file_count,
            requires_authorize_root: true,
            suggested_authorize_path: Some(parent),
        });
    }

    // copy mode
    let root = if let Some(rid) = authorized_root_id.as_deref().filter(|s| !s.is_empty()) {
        sqlx::query_as::<_, project_paths::AuthorizedRoot>(
            "SELECT id, absolute_path, label, is_primary, created_at FROM authorized_roots WHERE id = ?",
        )
        .bind(rid)
        .fetch_optional(&state.pool)
        .await?
        .ok_or_else(|| AppError::msg("Raíz autorizada no encontrada"))?
    } else {
        project_paths::primary_root(&state.pool).await?
    };
    let root_path = PathBuf::from(&root.absolute_path);
    let folder = if let Some(f) = folder_name.as_deref().filter(|s| !s.is_empty()) {
        sanitize_folder_name(f)?
    } else if let Some(c) = _code.as_deref() {
        c.to_string()
    } else {
        sanitize_folder_name(
            &source
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| "proyecto".into()),
        )?
    };
    let target = ensure_under_root(&root_path, &root_path.join(&folder))?;
    let will_overwrite = target.exists();
    let folders: Vec<String> = mappings
        .iter()
        .filter(|m| !m.matched)
        .map(|m| m.relative_path.clone())
        .collect();

    Ok(ImportPreview {
        source_path: source.display().to_string(),
        mode: "copy".into(),
        target_relative_path: folder,
        target_absolute_path: target.display().to_string(),
        authorized_root_id: Some(root.id),
        will_overwrite,
        will_copy: true,
        link_in_place: false,
        folders_to_create: folders,
        category_mappings: mappings,
        file_count_hint: file_count,
        requires_authorize_root: false,
        suggested_authorize_path: None,
    })
}

#[tauri::command]
pub async fn import_existing_project(
    state: State<'_, AppState>,
    input: ImportProjectInput,
) -> AppResult<ImportProjectResult> {
    let source = PathBuf::from(&input.source_path);
    if !source.is_dir() {
        return Err(AppError::msg("Fuente inválida"));
    }
    let name = input.name.trim().to_string();
    if name.is_empty() {
        return Err(AppError::msg("El nombre es obligatorio"));
    }
    let code = normalize_optional_code(input.code.as_deref())?;
    if let Some(ref c) = code {
        assert_code_unique(&state.pool, c, None).await?;
    }

    let mode = input
        .mode
        .clone()
        .unwrap_or_else(|| "link".into())
        .trim()
        .to_ascii_lowercase();
    if !["link", "copy", "reorganize_later"].contains(&mode.as_str()) {
        return Err(AppError::msg("Modo de importación inválido"));
    }

    // Optionally authorize parent so link works
    if let Some(parent) = input.authorize_parent_path.as_deref().filter(|s| !s.is_empty()) {
        project_paths::add_authorized_root(
            &state.pool,
            Path::new(parent),
            Some("Importada"),
            false,
        )
        .await?;
    }

    let (root_id, rel_path, target_abs, copied) = if mode == "copy" {
        let preview = preview_import_project(
            state.clone(),
            input.source_path.clone(),
            code.clone(),
            Some("copy".into()),
            input.folder_name.clone(),
            input.authorized_root_id.clone(),
        )
        .await?;
        let root_id = preview
            .authorized_root_id
            .ok_or_else(|| AppError::msg("Sin raíz destino"))?;
        let root: project_paths::AuthorizedRoot = sqlx::query_as(
            "SELECT id, absolute_path, label, is_primary, created_at FROM authorized_roots WHERE id = ?",
        )
        .bind(&root_id)
        .fetch_one(&state.pool)
        .await?;
        let root_path = PathBuf::from(&root.absolute_path);
        let target = ensure_under_root(
            &root_path,
            &root_path.join(&preview.target_relative_path),
        )?;
        if target.exists() && !input.confirm_overwrite.unwrap_or(false) {
            return Err(AppError::msg(format!(
                "La carpeta destino {} ya existe. Confirme sobrescritura/mezcla explícitamente.",
                preview.target_relative_path
            )));
        }
        // Never silently overwrite: merge without replacing unless confirmed
        if target.exists() {
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
                // else skip — no silent overwrite
            }
        } else {
            copy_recursively(&source, &target)?;
        }
        (
            root_id,
            preview.target_relative_path,
            target,
            true,
        )
    } else {
        // link / reorganize_later
        let under = project_paths::find_root_for_path(&state.pool, &source)
            .await?
            .ok_or_else(|| {
                AppError::msg(
                    "La carpeta no está bajo ninguna raíz autorizada. Autorice el directorio padre o use modo copia.",
                )
            })?;
        let (root, _root_path, rel) = under;
        if rel.is_empty() {
            return Err(AppError::msg(
                "No se puede vincular la raíz autorizada completa como proyecto",
            ));
        }
        let existing: Option<(String,)> = sqlx::query_as(
            "SELECT id FROM projects WHERE authorized_root_id = ? AND root_relative_path = ?",
        )
        .bind(&root.id)
        .bind(&rel)
        .fetch_optional(&state.pool)
        .await?;
        if existing.is_some() {
            return Err(AppError::msg(
                "Ya hay un proyecto registrado en esa carpeta física",
            ));
        }
        (root.id, rel, source.clone(), false)
    };

    let layout_mode = if mode == "reorganize_later" {
        "pending_reorganize"
    } else {
        let mapped = map_project_categories(&target_abs);
        let nonstandard = mapped.iter().any(|m| {
            m.matched
                && crate::models::STANDARD_FOLDERS
                    .iter()
                    .find(|f| f.key == m.key)
                    .map(|f| f.relative_path != m.relative_path)
                    .unwrap_or(false)
        });
        if nonstandard {
            "mapped"
        } else {
            "standard"
        }
    };
    let import_mode = if mode == "copy" {
        "copied"
    } else {
        "linked"
    };

    let create_missing = input
        .create_missing_folders
        .unwrap_or(mode == "copy");

    let id = Uuid::new_v4().to_string();
    let mut tx = state.pool.begin().await?;
    sqlx::query(
        "INSERT INTO projects (
            id, code, name, description, client_name, status,
            authorized_root_id, root_relative_path, import_mode, layout_mode,
            created_at, updated_at
         ) VALUES (?, ?, ?, ?, ?, 'active', ?, ?, ?, ?, datetime('now'), datetime('now'))",
    )
    .bind(&id)
    .bind(&code)
    .bind(&name)
    .bind(&input.description)
    .bind(&input.client_name)
    .bind(&root_id)
    .bind(&rel_path)
    .bind(import_mode)
    .bind(layout_mode)
    .execute(&mut *tx)
    .await?;

    insert_mapped_categories(&mut tx, &id, &target_abs, create_missing).await?;
    tx.commit().await?;

    crate::audit::log_event(
        &state.pool,
        Some(&id),
        "project",
        Some(&id),
        "import_existing",
        Some(&format!(
            "mode={mode} from {} → {}",
            source.display(),
            target_abs.display()
        )),
    )
    .await
    .ok();

    search_index::upsert_fts(
        &state.pool,
        "project",
        &id,
        Some(&id),
        &name,
        code.as_deref(),
        input.description.as_deref(),
        None,
    )
    .await
    .ok();

    Ok(ImportProjectResult {
        project_id: id,
        code,
        target_path: target_abs.display().to_string(),
        mode,
        copied,
        linked: !copied,
    })
}

#[tauri::command]
pub async fn propose_import_code(
    state: State<'_, AppState>,
    preferred: Option<String>,
) -> AppResult<Option<String>> {
    use crate::codes::{current_year_two_digits, next_project_code};
    if let Some(p) = preferred {
        let p = p.trim().to_string();
        if !p.is_empty() {
            let exists: Option<(String,)> =
                sqlx::query_as("SELECT id FROM projects WHERE code = ?")
                    .bind(&p)
                    .fetch_optional(&state.pool)
                    .await?;
            if exists.is_none() {
                return Ok(Some(p));
            }
        }
    }
    let rows: Vec<(Option<String>,)> = sqlx::query_as("SELECT code FROM projects")
        .fetch_all(&state.pool)
        .await?;
    let codes: Vec<String> = rows.into_iter().filter_map(|r| r.0).collect();
    Ok(Some(next_project_code(current_year_two_digits(), &codes)?))
}
