//! Smart virtual category mapping for non-standard project folder layouts.
//! Maps existing folder names → IngManager category keys without renaming.
//! Supports project-code prefixes: `1090 - MODELOS`, `1090_MODELOS`, etc.

use std::collections::HashMap;
use std::path::Path;

use crate::engineering_files::{strip_leading_aann_public, strip_project_code_prefix};
use crate::models::STANDARD_FOLDERS;

/// Alias table: normalized folder name → category key.
fn alias_map() -> HashMap<&'static str, &'static str> {
    let mut m = HashMap::new();
    // Standard numbered folders
    m.insert("01_antecedentes", "antecedentes");
    m.insert("02_modelos", "modelos");
    m.insert("03_planos", "planos");
    m.insert("04_memorias", "memorias");
    m.insert("05_administracion", "admin");
    m.insert("06_transmisiones", "transmisiones");
    m.insert("07_otros", "otros");
    // Spanish / informal
    m.insert("antecedentes", "antecedentes");
    m.insert("modelos", "modelos");
    m.insert("planos", "planos");
    m.insert("memorias", "memorias");
    m.insert("administracion", "admin");
    m.insert("administración", "admin");
    m.insert("admin", "admin");
    m.insert("transmisiones", "transmisiones");
    m.insert("otros", "otros");
    m.insert("otras", "otros");
    // English
    m.insert("models", "modelos");
    m.insert("drawings", "planos");
    m.insert("plans", "planos");
    m.insert("reports", "memorias");
    m.insert("memos", "memorias");
    m.insert("incoming", "antecedentes");
    m.insert("background", "antecedentes");
    m.insert("transmittals", "transmisiones");
    m.insert("other", "otros");
    m.insert("misc", "otros");
    m
}

fn normalize_folder_name(name: &str) -> String {
    name.trim()
        .to_lowercase()
        .replace('á', "a")
        .replace('é', "e")
        .replace('í', "i")
        .replace('ó', "o")
        .replace('ú', "u")
        .replace('ñ', "n")
}

/// Candidate tokens to look up in the alias map (original + stripped prefixes).
fn folder_lookup_tokens(folder_name: &str, project_codes: &[String]) -> Vec<String> {
    let mut tokens = Vec::new();
    let raw = folder_name.trim().to_string();
    if !raw.is_empty() {
        tokens.push(raw.clone());
    }
    let stripped_codes = strip_project_code_prefix(&raw, project_codes);
    if !stripped_codes.is_empty() && !stripped_codes.eq_ignore_ascii_case(&raw) {
        tokens.push(stripped_codes);
    }
    let stripped_aann = strip_leading_aann_public(&raw);
    if !stripped_aann.is_empty()
        && !tokens
            .iter()
            .any(|t| t.eq_ignore_ascii_case(&stripped_aann))
    {
        tokens.push(stripped_aann);
    }
    tokens
}

/// Infer category key from a single folder name (first path segment under project).
pub fn map_folder_name_to_key(folder_name: &str) -> Option<&'static str> {
    map_folder_name_to_key_with_codes(folder_name, &[])
}

/// Same as [`map_folder_name_to_key`] but strips known project codes first.
pub fn map_folder_name_to_key_with_codes(
    folder_name: &str,
    project_codes: &[String],
) -> Option<&'static str> {
    let aliases = alias_map();
    for token in folder_lookup_tokens(folder_name, project_codes) {
        let norm = normalize_folder_name(&token);
        if let Some(key) = aliases.get(norm.as_str()) {
            return Some(*key);
        }
        // Also try without numbered `NN_` prefix already in standard names
        if let Some((_, rest)) = norm.split_once('_') {
            if let Some(key) = aliases.get(rest) {
                return Some(*key);
            }
        }
    }
    None
}

/// Infer category from a path relative to the project root (may include nested segments).
pub fn infer_category_key_flexible(project_relative: &str) -> Option<&'static str> {
    infer_category_key_flexible_with_codes(project_relative, &[])
}

pub fn infer_category_key_flexible_with_codes(
    project_relative: &str,
    project_codes: &[String],
) -> Option<&'static str> {
    let first = project_relative
        .split(['/', '\\'])
        .next()
        .unwrap_or("")
        .trim();
    if first.is_empty() {
        return None;
    }
    map_folder_name_to_key_with_codes(first, project_codes)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CategoryMapping {
    pub key: &'static str,
    pub name: &'static str,
    pub relative_path: String,
    pub sort_order: i32,
    pub matched: bool,
}

/// Build category rows for a project directory using smart mapping.
/// Unmatched standard categories keep their default relative_path (for create-later).
pub fn map_project_categories(project_dir: &Path) -> Vec<CategoryMapping> {
    map_project_categories_with_codes(project_dir, &[])
}

pub fn map_project_categories_with_codes(
    project_dir: &Path,
    project_codes: &[String],
) -> Vec<CategoryMapping> {
    let mut found: HashMap<&'static str, String> = HashMap::new();

    // Prefer code from folder name itself (e.g. project dir `1090`)
    let mut codes = project_codes.to_vec();
    if let Some(name) = project_dir.file_name().map(|n| n.to_string_lossy().to_string()) {
        let digitish: String = name
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric())
            .collect();
        if digitish.len() >= 2 && digitish.len() <= 8 && !codes.iter().any(|c| c == &digitish) {
            codes.push(digitish);
        }
    }

    if let Ok(rd) = std::fs::read_dir(project_dir) {
        for entry in rd.flatten() {
            if !entry.path().is_dir() {
                continue;
            }
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with('.') {
                continue;
            }
            if let Some(key) = map_folder_name_to_key_with_codes(&name, &codes) {
                found.entry(key).or_insert(name);
            }
        }
    }

    STANDARD_FOLDERS
        .iter()
        .map(|folder| {
            if let Some(rel) = found.get(folder.key) {
                CategoryMapping {
                    key: folder.key,
                    name: folder.name,
                    relative_path: rel.clone(),
                    sort_order: folder.sort_order,
                    matched: true,
                }
            } else {
                CategoryMapping {
                    key: folder.key,
                    name: folder.name,
                    relative_path: folder.relative_path.to_string(),
                    sort_order: folder.sort_order,
                    matched: false,
                }
            }
        })
        .collect()
}

/// List of matched folder names for detection UI.
pub fn detect_matched_folder_names(project_dir: &Path) -> Vec<String> {
    map_project_categories(project_dir)
        .into_iter()
        .filter(|m| m.matched)
        .map(|m| m.relative_path)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn maps_standard_and_aliases() {
        assert_eq!(map_folder_name_to_key("02_MODELOS"), Some("modelos"));
        assert_eq!(map_folder_name_to_key("Modelos"), Some("modelos"));
        assert_eq!(map_folder_name_to_key("models"), Some("modelos"));
        assert_eq!(map_folder_name_to_key("Administración"), Some("admin"));
        assert_eq!(map_folder_name_to_key("random"), None);
    }

    #[test]
    fn maps_project_code_prefixed_folders() {
        let codes = vec!["1090".into()];
        assert_eq!(
            map_folder_name_to_key_with_codes("1090 - MODELOS", &codes),
            Some("modelos")
        );
        assert_eq!(
            map_folder_name_to_key_with_codes("1090_PLANOS", &codes),
            Some("planos")
        );
        assert_eq!(
            map_folder_name_to_key_with_codes("1090-MEMORIAS", &codes),
            Some("memorias")
        );
        // Heuristic without explicit codes
        assert_eq!(
            map_folder_name_to_key("1090 - MODELOS"),
            Some("modelos")
        );
    }

    #[test]
    fn maps_project_tree() {
        let dir = tempdir().unwrap();
        fs::create_dir_all(dir.path().join("Modelos")).unwrap();
        fs::create_dir_all(dir.path().join("Planos")).unwrap();
        fs::create_dir_all(dir.path().join("misc_stuff")).unwrap();
        let mapped = map_project_categories(dir.path());
        let modelos = mapped.iter().find(|m| m.key == "modelos").unwrap();
        assert!(modelos.matched);
        assert_eq!(modelos.relative_path, "Modelos");
        let ant = mapped.iter().find(|m| m.key == "antecedentes").unwrap();
        assert!(!ant.matched);
        assert_eq!(ant.relative_path, "01_ANTECEDENTES");
    }

    #[test]
    fn maps_prefixed_tree_under_project_1090() {
        let dir = tempdir().unwrap();
        let proj = dir.path().join("1090");
        fs::create_dir_all(proj.join("1090 - MODELOS")).unwrap();
        fs::create_dir_all(proj.join("1090 - PLANOS")).unwrap();
        let mapped = map_project_categories(&proj);
        let modelos = mapped.iter().find(|m| m.key == "modelos").unwrap();
        assert!(modelos.matched);
        assert_eq!(modelos.relative_path, "1090 - MODELOS");
    }

    #[test]
    fn flexible_infer() {
        assert_eq!(
            infer_category_key_flexible("Modelos/SAP/foo.sdb"),
            Some("modelos")
        );
        assert_eq!(
            infer_category_key_flexible("02_MODELOS/x.rvt"),
            Some("modelos")
        );
        assert_eq!(
            infer_category_key_flexible("1090 - MODELOS/foo.sdb"),
            Some("modelos")
        );
    }
}
