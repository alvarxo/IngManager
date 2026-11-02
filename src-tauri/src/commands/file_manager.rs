use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, State};
use uuid::Uuid;

use crate::error::{AppError, AppResult};
use crate::fs_util::{
    copy_recursively, engineering_warning_message, ensure_not_into_self, is_engineering_model,
    is_protected_system_name, move_entry, trash_root, unique_destination, validate_entry_name,
};
use crate::index_sync::{self, infer_category_key};
use crate::path_security::{ensure_under_root, join_under_root};
use crate::state::AppState;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FsEntry {
    pub name: String,
    pub absolute_path: String,
    pub relative_path: String,
    pub is_directory: bool,
    pub extension: Option<String>,
    pub size_bytes: Option<i64>,
    pub modified_at: Option<String>,
    pub is_available: bool,
    pub is_engineering: bool,
    pub category_key: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileOpResult {
    pub paths: Vec<String>,
    pub warning: Option<String>,
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntryProperties {
    pub name: String,
    pub absolute_path: String,
    pub relative_path: String,
    pub is_directory: bool,
    pub extension: Option<String>,
    pub size_bytes: Option<i64>,
    pub modified_at: Option<String>,
    pub created_at: Option<String>,
    pub is_engineering: bool,
    pub readonly: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct TrashItem {
    pub id: String,
    pub project_id: Option<String>,
    pub original_path: String,
    pub trash_path: String,
    pub deleted_at: String,
    pub purge_after: Option<String>,
    pub name: Option<String>,
    pub is_directory: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ConflictPolicy {
    Rename,
    Overwrite,
    Abort,
}

async fn require_root(state: &AppState) -> AppResult<PathBuf> {
    crate::project_paths::require_primary_root_path(&state.pool).await
}

async fn resolve_abs_under_roots(state: &AppState, absolute_path: &str) -> AppResult<(PathBuf, PathBuf)> {
    let roots = crate::project_paths::load_root_paths(&state.pool).await?;
    let (root, path) = crate::project_paths::ensure_under_any_root(&roots, Path::new(absolute_path))?;
    Ok((root.to_path_buf(), path))
}

fn to_rfc3339(time: SystemTime) -> Option<String> {
    let dt: DateTime<Utc> = time.into();
    Some(dt.to_rfc3339())
}

fn rel_from_root(root: &Path, abs: &Path) -> AppResult<String> {
    Ok(abs
        .strip_prefix(root)
        .map_err(|_| AppError::msg("Ruta fuera de la raíz"))?
        .to_string_lossy()
        .replace('\\', "/"))
}

fn entry_from_path(root: &Path, path: &Path) -> AppResult<FsEntry> {
    let meta = fs::metadata(path)?;
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    let rel = rel_from_root(root, path)?;
    let is_directory = meta.is_dir();
    let extension = if is_directory {
        None
    } else {
        path.extension()
            .map(|e| e.to_string_lossy().to_ascii_lowercase())
    };
    let category_key = rel
        .split_once('/')
        .map(|(_, rest)| rest)
        .and_then(infer_category_key)
        .map(|s| s.to_string());
    Ok(FsEntry {
        name,
        absolute_path: path.display().to_string(),
        relative_path: rel,
        is_directory,
        extension,
        size_bytes: if is_directory {
            None
        } else {
            Some(meta.len() as i64)
        },
        modified_at: meta.modified().ok().and_then(to_rfc3339),
        is_available: true,
        is_engineering: is_engineering_model(path),
        category_key,
    })
}

fn resolve_dest_path(
    dest_dir: &Path,
    file_name: &str,
    policy: &ConflictPolicy,
) -> AppResult<PathBuf> {
    let candidate = dest_dir.join(file_name);
    if !candidate.exists() {
        return Ok(candidate);
    }
    match policy {
        ConflictPolicy::Abort => Err(AppError::msg(format!(
            "Conflicto de nombre: ya existe '{file_name}'"
        ))),
        ConflictPolicy::Overwrite => Ok(candidate),
        ConflictPolicy::Rename => Ok(unique_destination(dest_dir, file_name)),
    }
}

#[tauri::command]
pub async fn list_directory(
    state: State<'_, AppState>,
    absolute_path: String,
) -> AppResult<Vec<FsEntry>> {
    let (root, dir) = resolve_abs_under_roots(&state, &absolute_path).await?;
    let dir = dir;
    if !dir.is_dir() {
        return Err(AppError::msg("La ruta no es una carpeta"));
    }

    let mut entries = Vec::new();
    for entry in fs::read_dir(&dir)? {
        let entry = entry?;
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();
        if is_protected_system_name(&name) || index_sync::should_skip_name(&name) {
            continue;
        }
        entries.push(entry_from_path(&root, &path)?);
    }

    entries.sort_by(|a, b| match (a.is_directory, b.is_directory) {
        (true, false) => std::cmp::Ordering::Less,
        (false, true) => std::cmp::Ordering::Greater,
        _ => a.name.to_ascii_lowercase().cmp(&b.name.to_ascii_lowercase()),
    });

    Ok(entries)
}

#[tauri::command]
pub async fn resolve_project_category_path(
    state: State<'_, AppState>,
    project_id: String,
    category_key: String,
    relative_subpath: Option<String>,
) -> AppResult<FsEntry> {
    let loc = crate::project_paths::resolve_project_location(&state.pool, &project_id).await?;
    let root = loc.root_path.clone();

    let cat: (String,) = sqlx::query_as(
        "SELECT relative_path FROM categories WHERE project_id = ? AND key = ?",
    )
    .bind(&project_id)
    .bind(&category_key)
    .fetch_optional(&state.pool)
    .await?
    .ok_or_else(|| AppError::msg("Categoría no encontrada"))?;

    let mut rel = format!("{}/{}", loc.root_relative_path, cat.0);
    if let Some(sub) = relative_subpath {
        let sub = sub.trim_matches('/').replace('\\', "/");
        if !sub.is_empty() {
            if Path::new(&sub)
                .components()
                .any(|c| matches!(c, std::path::Component::ParentDir))
            {
                return Err(AppError::msg("Subruta inválida"));
            }
            rel = format!("{rel}/{sub}");
        }
    }

    let path = join_under_root(&root, &rel)?;
    if !path.exists() {
        fs::create_dir_all(&path)?;
        index_sync::upsert_path(&state.pool, &root, &path).await?;
    }
    entry_from_path(&root, &path)
}

#[tauri::command]
pub async fn create_folder(
    state: State<'_, AppState>,
    parent_path: String,
    name: String,
) -> AppResult<FileOpResult> {
    validate_entry_name(&name)?;
    let (root, parent) = resolve_abs_under_roots(&state, &parent_path).await?;
    if !parent.is_dir() {
        return Err(AppError::msg("La carpeta padre no existe"));
    }
    let dest = parent.join(name.trim());
    let dest = ensure_under_root(&root, &dest)?;
    if dest.exists() {
        return Err(AppError::msg("Ya existe un elemento con ese nombre"));
    }
    fs::create_dir_all(&dest)?;
    index_sync::upsert_path(&state.pool, &root, &dest).await?;
    let rel = rel_from_root(&root, &dest)?;
    index_sync::log_file_operation(
        &state.pool,
        None,
        "create_folder",
        None,
        Some(&rel),
        "completed",
        None,
    )
    .await?;

    Ok(FileOpResult {
        paths: vec![dest.display().to_string()],
        warning: None,
        message: Some("Carpeta creada".into()),
    })
}

#[tauri::command]
pub async fn rename_entry(
    state: State<'_, AppState>,
    absolute_path: String,
    new_name: String,
    acknowledge_engineering: Option<bool>,
) -> AppResult<FileOpResult> {
    validate_entry_name(&new_name)?;
    let (root, source) = resolve_abs_under_roots(&state, &absolute_path).await?;
    if !source.exists() {
        return Err(AppError::msg("El elemento no existe"));
    }
    let parent = source
        .parent()
        .ok_or_else(|| AppError::msg("Sin carpeta padre"))?;
    let dest = ensure_under_root(&root, &parent.join(new_name.trim()))?;
    if dest.exists() {
        return Err(AppError::msg("Ya existe un elemento con ese nombre"));
    }

    let warning = engineering_warning_message(std::slice::from_ref(&source));
    if warning.is_some() && !acknowledge_engineering.unwrap_or(false) {
        return Ok(FileOpResult {
            paths: vec![],
            warning,
            message: Some("Se requiere confirmación para modelos de ingeniería".into()),
        });
    }

    let old_rel = rel_from_root(&root, &source)?;
    move_entry(&source, &dest)?;
    index_sync::remove_or_mark_unavailable(&state.pool, &root, Path::new(&old_rel)).await?;
    // Hard-remove old index rows for renamed path prefix then refresh
    sqlx::query(
        "DELETE FROM indexed_files WHERE relative_path = ? OR relative_path LIKE ?",
    )
    .bind(&old_rel)
    .bind(format!("{old_rel}/%"))
    .execute(&state.pool)
    .await?;
    if dest.is_dir() {
        index_sync::refresh_subtree(&state.pool, &root, &dest).await?;
    } else {
        index_sync::upsert_path(&state.pool, &root, &dest).await?;
    }

    let new_rel = rel_from_root(&root, &dest)?;
    index_sync::log_file_operation(
        &state.pool,
        None,
        "rename",
        Some(&old_rel),
        Some(&new_rel),
        "completed",
        warning.as_deref(),
    )
    .await?;

    Ok(FileOpResult {
        paths: vec![dest.display().to_string()],
        warning,
        message: Some("Renombrado".into()),
    })
}

#[tauri::command]
pub async fn copy_entries(
    state: State<'_, AppState>,
    sources: Vec<String>,
    dest_dir: String,
    conflict_policy: Option<ConflictPolicy>,
) -> AppResult<FileOpResult> {
    let policy = conflict_policy.unwrap_or(ConflictPolicy::Rename);
    let (root, dest) = resolve_abs_under_roots(&state, &dest_dir).await?;
    if !dest.is_dir() {
        return Err(AppError::msg("El destino no es una carpeta"));
    }

    let mut out = Vec::new();
    let mut src_paths = Vec::new();
    for s in &sources {
        let src = ensure_under_root(&root, Path::new(s))?;
        ensure_not_into_self(&src, &dest)?;
        src_paths.push(src);
    }
    let warning = engineering_warning_message(&src_paths);

    for src in &src_paths {
        let name = src
            .file_name()
            .ok_or_else(|| AppError::msg("Nombre inválido"))?
            .to_string_lossy()
            .to_string();
        let target = resolve_dest_path(&dest, &name, &policy)?;
        if matches!(policy, ConflictPolicy::Overwrite) && target.exists() {
            if target.is_dir() {
                fs::remove_dir_all(&target)?;
            } else {
                fs::remove_file(&target)?;
            }
        }
        let target = ensure_under_root(&root, &target)?;
        copy_recursively(src, &target)?;
        if target.is_dir() {
            index_sync::refresh_subtree(&state.pool, &root, &target).await?;
        } else {
            index_sync::upsert_path(&state.pool, &root, &target).await?;
        }
        out.push(target.display().to_string());
        let _ = index_sync::log_file_operation(
            &state.pool,
            None,
            "copy",
            Some(&rel_from_root(&root, src)?),
            Some(&rel_from_root(&root, &target)?),
            "completed",
            None,
        )
        .await;
    }

    Ok(FileOpResult {
        paths: out,
        warning,
        message: Some("Copia completada".into()),
    })
}

#[tauri::command]
pub async fn move_entries(
    state: State<'_, AppState>,
    sources: Vec<String>,
    dest_dir: String,
    conflict_policy: Option<ConflictPolicy>,
    acknowledge_engineering: Option<bool>,
) -> AppResult<FileOpResult> {
    let policy = conflict_policy.unwrap_or(ConflictPolicy::Rename);
    let (root, dest) = resolve_abs_under_roots(&state, &dest_dir).await?;
    if !dest.is_dir() {
        return Err(AppError::msg("El destino no es una carpeta"));
    }

    let mut src_paths = Vec::new();
    for s in &sources {
        let src = ensure_under_root(&root, Path::new(s))?;
        ensure_not_into_self(&src, &dest)?;
        // Don't move into same parent with same name noop handled by conflict
        if src.parent() == Some(dest.as_path()) {
            continue;
        }
        src_paths.push(src);
    }

    let warning = engineering_warning_message(&src_paths);
    if warning.is_some() && !acknowledge_engineering.unwrap_or(false) {
        return Ok(FileOpResult {
            paths: vec![],
            warning,
            message: Some("Se requiere confirmación para modelos de ingeniería".into()),
        });
    }

    let mut out = Vec::new();
    for src in &src_paths {
        let name = src
            .file_name()
            .ok_or_else(|| AppError::msg("Nombre inválido"))?
            .to_string_lossy()
            .to_string();
        let target = resolve_dest_path(&dest, &name, &policy)?;
        if matches!(policy, ConflictPolicy::Overwrite) && target.exists() {
            if target.is_dir() {
                fs::remove_dir_all(&target)?;
            } else {
                fs::remove_file(&target)?;
            }
        }
        let target = ensure_under_root(&root, &target)?;
        let old_rel = rel_from_root(&root, src)?;
        move_entry(src, &target)?;
        sqlx::query(
            "DELETE FROM indexed_files WHERE relative_path = ? OR relative_path LIKE ?",
        )
        .bind(&old_rel)
        .bind(format!("{old_rel}/%"))
        .execute(&state.pool)
        .await?;
        if target.is_dir() {
            index_sync::refresh_subtree(&state.pool, &root, &target).await?;
        } else {
            index_sync::upsert_path(&state.pool, &root, &target).await?;
        }
        out.push(target.display().to_string());
        let _ = index_sync::log_file_operation(
            &state.pool,
            None,
            "move",
            Some(&old_rel),
            Some(&rel_from_root(&root, &target)?),
            "completed",
            warning.as_deref(),
        )
        .await;
    }

    Ok(FileOpResult {
        paths: out,
        warning,
        message: Some("Movimiento completado".into()),
    })
}

#[tauri::command]
pub async fn import_files(
    state: State<'_, AppState>,
    source_paths: Vec<String>,
    dest_dir: String,
    conflict_policy: Option<ConflictPolicy>,
) -> AppResult<FileOpResult> {
    let policy = conflict_policy.unwrap_or(ConflictPolicy::Rename);
    let (root, dest) = resolve_abs_under_roots(&state, &dest_dir).await?;
    if !dest.is_dir() {
        return Err(AppError::msg("El destino no es una carpeta"));
    }

    let mut out = Vec::new();
    let mut eng = Vec::new();
    for s in source_paths {
        let src = PathBuf::from(&s);
        if !src.exists() {
            return Err(AppError::msg(format!("No existe: {s}")));
        }
        // Source may be outside root (import). Destination must stay inside.
        if is_engineering_model(&src) {
            eng.push(src.clone());
        }
        let name = src
            .file_name()
            .ok_or_else(|| AppError::msg("Nombre inválido"))?
            .to_string_lossy()
            .to_string();
        let target = resolve_dest_path(&dest, &name, &policy)?;
        if matches!(policy, ConflictPolicy::Overwrite) && target.exists() {
            if target.is_dir() {
                fs::remove_dir_all(&target)?;
            } else {
                fs::remove_file(&target)?;
            }
        }
        let target = ensure_under_root(&root, &target)?;
        copy_recursively(&src, &target)?;
        if target.is_dir() {
            index_sync::refresh_subtree(&state.pool, &root, &target).await?;
        } else {
            index_sync::upsert_path(&state.pool, &root, &target).await?;
        }
        out.push(target.display().to_string());
        let _ = index_sync::log_file_operation(
            &state.pool,
            None,
            "import",
            Some(&s),
            Some(&rel_from_root(&root, &target)?),
            "completed",
            None,
        )
        .await;
    }

    Ok(FileOpResult {
        paths: out,
        warning: engineering_warning_message(&eng),
        message: Some("Importación completada (copia)".into()),
    })
}

#[tauri::command]
pub async fn delete_to_trash(
    state: State<'_, AppState>,
    paths: Vec<String>,
    acknowledge_engineering: Option<bool>,
) -> AppResult<FileOpResult> {
    let mut root = require_root(&state).await?;
    let mut src_paths = Vec::new();
    for p in &paths {
        let (r, src) = resolve_abs_under_roots(&state, p).await?;
        root = r;
        if !src.exists() {
            return Err(AppError::msg(format!("No existe: {p}")));
        }
        // Never trash the project root itself or category roots? Allow category contents.
        // Protect .ingmanager
        if src == root {
            return Err(AppError::msg("No se puede eliminar la carpeta raíz"));
        }
        src_paths.push(src);
    }

    let warning = engineering_warning_message(&src_paths);
    if warning.is_some() && !acknowledge_engineering.unwrap_or(false) {
        return Ok(FileOpResult {
            paths: vec![],
            warning,
            message: Some("Se requiere confirmación para modelos de ingeniería".into()),
        });
    }

    let trash_base = trash_root(&root);
    fs::create_dir_all(&trash_base)?;

    let mut out = Vec::new();
    for src in &src_paths {
        let id = Uuid::new_v4().to_string();
        let slot = trash_base.join(&id);
        fs::create_dir_all(&slot)?;
        let name = src
            .file_name()
            .ok_or_else(|| AppError::msg("Nombre inválido"))?
            .to_os_string();
        let trash_path = slot.join(&name);
        let original_rel = rel_from_root(&root, src)?;
        let is_dir = src.is_dir();
        move_entry(src, &trash_path)?;

        // Infer project_id from first path segment
        let project_code = original_rel.split('/').next().unwrap_or("");
        let project_id: Option<(String,)> =
            sqlx::query_as("SELECT id FROM projects WHERE root_relative_path = ?")
                .bind(project_code)
                .fetch_optional(&state.pool)
                .await?;

        sqlx::query(
            "INSERT INTO trash_items (id, project_id, original_path, trash_path, deleted_at)
             VALUES (?, ?, ?, ?, datetime('now'))",
        )
        .bind(&id)
        .bind(project_id.map(|p| p.0))
        .bind(&original_rel)
        .bind(trash_path.display().to_string())
        .execute(&state.pool)
        .await?;

        // Store metadata via audit for name/dir flags (schema-stable)
        let _ = index_sync::log_file_operation(
            &state.pool,
            None,
            "trash",
            Some(&original_rel),
            Some(&trash_path.display().to_string()),
            "completed",
            Some(&format!(
                "name={};is_directory={}",
                name.to_string_lossy(),
                is_dir
            )),
        )
        .await;

        sqlx::query(
            "DELETE FROM indexed_files WHERE relative_path = ? OR relative_path LIKE ?",
        )
        .bind(&original_rel)
        .bind(format!("{original_rel}/%"))
        .execute(&state.pool)
        .await?;

        out.push(id);
    }

    Ok(FileOpResult {
        paths: out,
        warning,
        message: Some("Enviado a la papelera".into()),
    })
}

#[tauri::command]
pub async fn list_trash(state: State<'_, AppState>) -> AppResult<Vec<TrashItem>> {
    let items = sqlx::query_as::<_, TrashItem>(
        "SELECT id, project_id, original_path, trash_path, deleted_at, purge_after,
                NULL as name, NULL as is_directory
         FROM trash_items
         ORDER BY deleted_at DESC",
    )
    .fetch_all(&state.pool)
    .await?;

    let mut enriched = Vec::new();
    for mut item in items {
        let tp = PathBuf::from(&item.trash_path);
        item.name = tp
            .file_name()
            .map(|n| n.to_string_lossy().to_string());
        item.is_directory = Some(if tp.is_dir() { 1 } else { 0 });
        enriched.push(item);
    }
    Ok(enriched)
}

#[tauri::command]
pub async fn restore_trash_item(
    state: State<'_, AppState>,
    trash_id: String,
    conflict_policy: Option<ConflictPolicy>,
) -> AppResult<FileOpResult> {
    let policy = conflict_policy.unwrap_or(ConflictPolicy::Rename);
    let root = require_root(&state).await?;
    let item: (String, String) =
        sqlx::query_as("SELECT original_path, trash_path FROM trash_items WHERE id = ?")
            .bind(&trash_id)
            .fetch_optional(&state.pool)
            .await?
            .ok_or_else(|| AppError::msg("Elemento de papelera no encontrado"))?;

    let (original_rel, trash_path_str) = item;
    let trash_path = PathBuf::from(&trash_path_str);
    if !trash_path.exists() {
        return Err(AppError::msg(
            "El contenido en papelera ya no está disponible",
        ));
    }

    let original = join_under_root(&root, &original_rel)?;
    let parent = original
        .parent()
        .ok_or_else(|| AppError::msg("Ruta original inválida"))?;
    fs::create_dir_all(parent)?;

    let name = trash_path
        .file_name()
        .ok_or_else(|| AppError::msg("Nombre inválido"))?
        .to_string_lossy()
        .to_string();
    let target = if !original.exists() {
        original.clone()
    } else {
        resolve_dest_path(parent, &name, &policy)?
    };
    let target = ensure_under_root(&root, &target)?;
    move_entry(&trash_path, &target)?;

    // Cleanup empty trash slot
    if let Some(slot) = trash_path.parent() {
        let _ = fs::remove_dir(slot);
    }

    sqlx::query("DELETE FROM trash_items WHERE id = ?")
        .bind(&trash_id)
        .execute(&state.pool)
        .await?;

    if target.is_dir() {
        index_sync::refresh_subtree(&state.pool, &root, &target).await?;
    } else {
        index_sync::upsert_path(&state.pool, &root, &target).await?;
    }

    let _ = index_sync::log_file_operation(
        &state.pool,
        None,
        "restore",
        Some(&trash_path_str),
        Some(&rel_from_root(&root, &target)?),
        "completed",
        None,
    )
    .await;

    Ok(FileOpResult {
        paths: vec![target.display().to_string()],
        warning: None,
        message: Some("Restaurado".into()),
    })
}

#[tauri::command]
pub async fn purge_trash_item(state: State<'_, AppState>, trash_id: String) -> AppResult<()> {
    let root = require_root(&state).await?;
    let trash_path: (String,) =
        sqlx::query_as("SELECT trash_path FROM trash_items WHERE id = ?")
            .bind(&trash_id)
            .fetch_optional(&state.pool)
            .await?
            .ok_or_else(|| AppError::msg("Elemento de papelera no encontrado"))?;

    let path = PathBuf::from(&trash_path.0);
    // Must remain under trash root
    let troot = trash_root(&root);
    let _ = ensure_under_root(&root, &path)?;
    if !path.starts_with(&troot) && ensure_under_root(&troot, &path).is_err() {
        // path_is_under via ensure on trash root parent
        let canon_t = ensure_under_root(&root, &troot)?;
        let canon_p = ensure_under_root(&root, &path)?;
        if !crate::path_security::path_is_under(&canon_t, &canon_p) {
            return Err(AppError::msg("Ruta de papelera inválida"));
        }
    }

    if path.exists() {
        if path.is_dir() {
            fs::remove_dir_all(&path)?;
        } else {
            fs::remove_file(&path)?;
        }
    }
    if let Some(slot) = path.parent() {
        if slot.starts_with(&troot) {
            let _ = fs::remove_dir_all(slot);
        }
    }

    sqlx::query("DELETE FROM trash_items WHERE id = ?")
        .bind(&trash_id)
        .execute(&state.pool)
        .await?;

    let _ = index_sync::log_file_operation(
        &state.pool,
        None,
        "purge_trash",
        Some(&trash_path.0),
        None,
        "completed",
        None,
    )
    .await;
    Ok(())
}

#[tauri::command]
pub async fn get_entry_properties(
    state: State<'_, AppState>,
    absolute_path: String,
) -> AppResult<EntryProperties> {
    let (root, path) = resolve_abs_under_roots(&state, &absolute_path).await?;
    if !path.exists() {
        return Err(AppError::msg("El elemento no existe"));
    }
    let meta = fs::metadata(&path)?;
    let is_directory = meta.is_dir();
    Ok(EntryProperties {
        name: path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default(),
        absolute_path: path.display().to_string(),
        relative_path: rel_from_root(&root, &path)?,
        is_directory,
        extension: if is_directory {
            None
        } else {
            path.extension()
                .map(|e| e.to_string_lossy().to_ascii_lowercase())
        },
        size_bytes: if is_directory {
            None
        } else {
            Some(meta.len() as i64)
        },
        modified_at: meta.modified().ok().and_then(to_rfc3339),
        created_at: meta.created().ok().and_then(to_rfc3339),
        is_engineering: is_engineering_model(&path),
        readonly: meta.permissions().readonly(),
    })
}

#[tauri::command]
pub async fn open_path(state: State<'_, AppState>, absolute_path: String) -> AppResult<()> {
    let (_root, path) = resolve_abs_under_roots(&state, &absolute_path).await?;
    if !path.exists() {
        return Err(AppError::msg("La ruta no existe"));
    }
    open::that(&path).map_err(|e| AppError::msg(format!("No se pudo abrir: {e}")))?;
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ExternalDropMode {
    Copy,
    Move,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExternalDropItemPreview {
    pub source_path: String,
    pub file_name: String,
    pub is_directory: bool,
    pub size_bytes: Option<i64>,
    pub conflict: bool,
    pub planned_name: String,
    pub is_engineering: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExternalDropPreview {
    pub dest_dir: String,
    pub items: Vec<ExternalDropItemPreview>,
    pub conflict_count: usize,
    pub engineering_warning: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExternalDropProgress {
    pub phase: String,
    pub current: usize,
    pub total: usize,
    pub current_path: Option<String>,
    pub done: bool,
    pub message: Option<String>,
}

/// Preview OS/external paths before copy/move into a managed folder (File Manager drop).
#[tauri::command]
pub async fn preview_external_drop(
    state: State<'_, AppState>,
    source_paths: Vec<String>,
    dest_dir: String,
    conflict_policy: Option<ConflictPolicy>,
) -> AppResult<ExternalDropPreview> {
    let policy = conflict_policy.unwrap_or(ConflictPolicy::Rename);
    let (_root, dest) = resolve_abs_under_roots(&state, &dest_dir).await?;
    if !dest.is_dir() {
        return Err(AppError::msg("El destino no es una carpeta"));
    }

    let mut items = Vec::new();
    let mut eng = Vec::new();
    for s in source_paths {
        let src = PathBuf::from(&s);
        if !src.exists() {
            return Err(AppError::msg(format!("No existe: {s}")));
        }
        let name = src
            .file_name()
            .ok_or_else(|| AppError::msg("Nombre inválido"))?
            .to_string_lossy()
            .to_string();
        let meta = fs::metadata(&src)?;
        let is_directory = meta.is_dir();
        let conflict = dest.join(&name).exists();
        let planned = resolve_dest_path(&dest, &name, &policy)?;
        let planned_name = planned
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| name.clone());
        if is_engineering_model(&src) {
            eng.push(src.clone());
        }
        items.push(ExternalDropItemPreview {
            source_path: src.display().to_string(),
            file_name: name,
            is_directory,
            size_bytes: if is_directory {
                None
            } else {
                Some(meta.len() as i64)
            },
            conflict,
            planned_name,
            is_engineering: is_engineering_model(&src),
        });
    }

    let conflict_count = items.iter().filter(|i| i.conflict).count();
    Ok(ExternalDropPreview {
        dest_dir: dest.display().to_string(),
        items,
        conflict_count,
        engineering_warning: engineering_warning_message(&eng),
    })
}

/// Apply OS/external drop: copy (default) or move into dest; emits `external-drop-progress`.
#[tauri::command]
pub async fn apply_external_drop(
    app: AppHandle,
    state: State<'_, AppState>,
    source_paths: Vec<String>,
    dest_dir: String,
    mode: Option<ExternalDropMode>,
    conflict_policy: Option<ConflictPolicy>,
) -> AppResult<FileOpResult> {
    let mode = mode.unwrap_or(ExternalDropMode::Copy);
    let policy = conflict_policy.unwrap_or(ConflictPolicy::Rename);
    let (root, dest) = resolve_abs_under_roots(&state, &dest_dir).await?;
    if !dest.is_dir() {
        return Err(AppError::msg("El destino no es una carpeta"));
    }

    let total = source_paths.len();
    let mut out = Vec::new();
    let mut eng = Vec::new();

    let _ = app.emit(
        "external-drop-progress",
        ExternalDropProgress {
            phase: "start".into(),
            current: 0,
            total,
            current_path: None,
            done: false,
            message: Some(match mode {
                ExternalDropMode::Copy => "Importando (copia)…".into(),
                ExternalDropMode::Move => "Importando (mover)…".into(),
            }),
        },
    );

    for (idx, s) in source_paths.iter().enumerate() {
        let src = PathBuf::from(s);
        if !src.exists() {
            let _ = app.emit(
                "external-drop-progress",
                ExternalDropProgress {
                    phase: "error".into(),
                    current: idx,
                    total,
                    current_path: Some(s.clone()),
                    done: true,
                    message: Some(format!("No existe: {s}")),
                },
            );
            return Err(AppError::msg(format!("No existe: {s}")));
        }
        if is_engineering_model(&src) {
            eng.push(src.clone());
        }

        let _ = app.emit(
            "external-drop-progress",
            ExternalDropProgress {
                phase: "item".into(),
                current: idx + 1,
                total,
                current_path: Some(src.display().to_string()),
                done: false,
                message: None,
            },
        );

        let name = src
            .file_name()
            .ok_or_else(|| AppError::msg("Nombre inválido"))?
            .to_string_lossy()
            .to_string();
        let target = resolve_dest_path(&dest, &name, &policy)?;
        if matches!(policy, ConflictPolicy::Overwrite) && target.exists() {
            if target.is_dir() {
                fs::remove_dir_all(&target)?;
            } else {
                fs::remove_file(&target)?;
            }
        }
        let target = ensure_under_root(&root, &target)?;

        // Prefer in-root move when source is already under an authorized root.
        let managed_src = crate::project_paths::load_root_paths(&state.pool)
            .await
            .ok()
            .and_then(|roots| {
                crate::project_paths::ensure_under_any_root(&roots, &src)
                    .ok()
                    .map(|(r, p)| (r.to_path_buf(), p.to_path_buf()))
            });

        match mode {
            ExternalDropMode::Copy => {
                copy_recursively(&src, &target)?;
            }
            ExternalDropMode::Move => {
                if let Some((src_root, managed_path)) = managed_src {
                    ensure_not_into_self(&managed_path, &dest)?;
                    let old_rel = rel_from_root(&src_root, &managed_path).ok();
                    move_entry(&managed_path, &target)?;
                    if let Some(old_rel) = old_rel {
                        let _ = sqlx::query(
                            "DELETE FROM indexed_files WHERE relative_path = ? OR relative_path LIKE ?",
                        )
                        .bind(&old_rel)
                        .bind(format!("{old_rel}/%"))
                        .execute(&state.pool)
                        .await;
                    }
                } else {
                    copy_recursively(&src, &target)?;
                    if src.is_dir() {
                        fs::remove_dir_all(&src).map_err(|e| {
                            AppError::msg(format!(
                                "Copiado a destino, pero no se pudo borrar origen: {e}"
                            ))
                        })?;
                    } else {
                        fs::remove_file(&src).map_err(|e| {
                            AppError::msg(format!(
                                "Copiado a destino, pero no se pudo borrar origen: {e}"
                            ))
                        })?;
                    }
                }
            }
        }

        if target.is_dir() {
            index_sync::refresh_subtree(&state.pool, &root, &target).await?;
        } else {
            index_sync::upsert_path(&state.pool, &root, &target).await?;
        }
        out.push(target.display().to_string());
        let op = match mode {
            ExternalDropMode::Copy => "external_drop_copy",
            ExternalDropMode::Move => "external_drop_move",
        };
        let _ = index_sync::log_file_operation(
            &state.pool,
            None,
            op,
            Some(s),
            Some(&rel_from_root(&root, &target)?),
            "completed",
            None,
        )
        .await;
    }

    let message = match mode {
        ExternalDropMode::Copy => "Importación por drop completada (copia)",
        ExternalDropMode::Move => "Importación por drop completada (mover)",
    };
    let warning = engineering_warning_message(&eng);
    let _ = app.emit(
        "external-drop-progress",
        ExternalDropProgress {
            phase: "done".into(),
            current: total,
            total,
            current_path: None,
            done: true,
            message: Some(message.into()),
        },
    );

    Ok(FileOpResult {
        paths: out,
        warning,
        message: Some(message.into()),
    })
}
