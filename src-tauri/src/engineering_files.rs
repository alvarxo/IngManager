//! Engineering file roles and SAP2000-style grouping.
//!
//! Principal models (e.g. `.sdb`) are the identity users manage.
//! Auxiliaries share the same stem and are hidden by default.
//! Distinct stems like `… SIN CAMIONES` vs `… CON CAMIONES` are never merged.

use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

/// Role of a physical engineering-related file within a group.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FileRole {
    Principal,
    Auxiliary,
    Standalone,
}

impl FileRole {
    pub fn as_str(self) -> &'static str {
        match self {
            FileRole::Principal => "principal",
            FileRole::Auxiliary => "auxiliary",
            FileRole::Standalone => "standalone",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "principal" => Some(FileRole::Principal),
            "auxiliary" => Some(FileRole::Auxiliary),
            "standalone" => Some(FileRole::Standalone),
            _ => None,
        }
    }
}

/// Extensions that can be the principal of a SAP2000 / CSI model group.
const SAP_PRINCIPAL_EXTS: &[&str] = &["sdb"];

/// Companion / scratch / export extensions typically paired with a `.sdb`.
const SAP_AUX_EXTS: &[&str] = &[
    "s2k", "$2k", "log", "out", "txt", "xml", "rtf", "mdb", "sdbr", "ebk", "sfk", "$sf", "avi",
    "gif", "bmp", "jpg", "jpeg", "png", "xls", "xlsx", "csv",
];

/// Other CAD/BIM principals (standalone unless companions share stem).
const OTHER_PRINCIPAL_EXTS: &[&str] = &["rvt", "rfa", "dwg", "dxf", "idea", "ideastatica"];

const OTHER_AUX_EXTS: &[&str] = &["bak", "dwl", "dwl2", "rws", "dat", "tmp", "sv$", "ac$"];

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ClassifiedFile {
    pub file_name: String,
    pub extension: Option<String>,
    pub stem: String,
    /// Identity key after project-prefix strip (keeps SIN/CON distinctions).
    pub group_key: String,
    pub role: FileRole,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineeringGroup {
    pub group_key: String,
    pub title: String,
    pub principal: Option<ClassifiedFile>,
    pub auxiliaries: Vec<ClassifiedFile>,
    pub standalone: Vec<ClassifiedFile>,
}

fn extension_of(name: &str) -> Option<String> {
    Path::new(name)
        .extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase())
}

fn raw_stem(name: &str) -> String {
    Path::new(name)
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| name.to_string())
}

/// Strip optional project-code prefixes from a stem/folder token.
/// Examples: `1090 - MODELO`, `1090_MODELO`, `1090-MODELO`, `P1090 MODELO`.
pub fn strip_project_code_prefix(name: &str, project_codes: &[String]) -> String {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    let mut rest = trimmed.to_string();
    for code in project_codes {
        let code = code.trim();
        if code.is_empty() {
            continue;
        }
        // Try several separators after the code
        for sep in [" - ", " – ", " — ", "-", "_", " ", "."] {
            let prefix = format!("{code}{sep}");
            if rest.len() > prefix.len()
                && rest[..prefix.len()].eq_ignore_ascii_case(&prefix)
            {
                rest = rest[prefix.len()..].trim().to_string();
                break;
            }
        }
        // Bare code as whole token
        if rest.eq_ignore_ascii_case(code) {
            return String::new();
        }
    }
    rest
}

/// Normalize a group identity: lowercase, collapse whitespace, keep SIN/CON wording.
pub fn normalize_group_key(stem: &str, project_codes: &[String]) -> String {
    let stripped = strip_project_code_prefix(stem, project_codes);
    // Also strip a leading AANN-looking code even if not in the list
    let stripped = strip_leading_aann_public(&stripped);
    stripped
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

/// Public wrapper used by category mapping for folder tokens like `1090 - MODELOS`.
pub fn strip_leading_aann_public(name: &str) -> String {
    strip_leading_aann_like(name)
}

fn strip_leading_aann_like(name: &str) -> String {
    let trimmed = name.trim();
    let bytes = trimmed.as_bytes();
    // 2–6 digit / alnum project-ish token then separator
    let mut i = 0usize;
    while i < bytes.len() && (bytes[i].is_ascii_alphanumeric()) {
        i += 1;
    }
    if i >= 2 && i <= 8 && i < bytes.len() {
        let sep = bytes[i];
        if matches!(sep, b'-' | b'_' | b'.' | b' ') {
            let mut j = i;
            while j < bytes.len()
                && (bytes[j] == b'-'
                    || bytes[j] == b'_'
                    || bytes[j] == b'.'
                    || bytes[j] == b' '
                    || bytes[j] == 0xE2) // start of en-dash utf8 — handled loosely
            {
                // skip ASCII separators; for unicode dashes fall through to trim below
                if bytes[j].is_ascii() {
                    j += 1;
                } else {
                    break;
                }
            }
            // Unicode en/em dash " – " / " — "
            let rest = trimmed[i..].trim_start_matches(['-', '_', '.', ' ', '–', '—']);
            if !rest.is_empty() && looks_like_category_or_model(rest) {
                return rest.to_string();
            }
            let rest2 = trimmed[j..].trim();
            if !rest2.is_empty() {
                return rest2.to_string();
            }
        }
    }
    trimmed.to_string()
}

fn looks_like_category_or_model(s: &str) -> bool {
    let n = s.to_lowercase();
    n.contains("modelo")
        || n.contains("model")
        || n.contains("plano")
        || n.contains("memoria")
        || n.contains("antecedente")
        || n.contains("admin")
        || n.contains("camion")
        || n.contains("estructura")
        || n.contains("sap")
        || n.len() > 2
}

fn is_sap_principal(ext: &str) -> bool {
    SAP_PRINCIPAL_EXTS.iter().any(|e| *e == ext)
}

fn is_sap_aux(ext: &str) -> bool {
    SAP_AUX_EXTS.iter().any(|e| *e == ext)
}

fn is_other_principal(ext: &str) -> bool {
    OTHER_PRINCIPAL_EXTS.iter().any(|e| *e == ext)
}

fn is_other_aux(ext: &str) -> bool {
    OTHER_AUX_EXTS.iter().any(|e| *e == ext)
}

/// Classify a single file name into role + group key.
pub fn classify_file_name(file_name: &str, project_codes: &[String]) -> ClassifiedFile {
    let extension = extension_of(file_name);
    let stem = raw_stem(file_name);
    let group_key = normalize_group_key(&stem, project_codes);
    let ext = extension.as_deref().unwrap_or("");

    let role = if is_sap_principal(ext) || is_other_principal(ext) {
        FileRole::Principal
    } else if is_sap_aux(ext) || is_other_aux(ext) {
        FileRole::Auxiliary
    } else if extension.is_some() {
        // Unknown extension: treat as standalone (still groupable by stem)
        FileRole::Standalone
    } else {
        FileRole::Standalone
    };

    ClassifiedFile {
        file_name: file_name.to_string(),
        extension,
        stem,
        group_key,
        role,
    }
}

/// Group a list of file names. Principals win; auxiliaries attach to same group_key.
/// `SIN CAMIONES` and `CON CAMIONES` remain separate groups.
pub fn group_files(file_names: &[String], project_codes: &[String]) -> Vec<EngineeringGroup> {
    let classified: Vec<ClassifiedFile> = file_names
        .iter()
        .map(|n| classify_file_name(n, project_codes))
        .collect();

    let mut map: BTreeMap<String, Vec<ClassifiedFile>> = BTreeMap::new();
    for c in classified {
        map.entry(c.group_key.clone()).or_default().push(c);
    }

    let mut groups = Vec::new();
    for (group_key, mut files) in map {
        files.sort_by(|a, b| a.file_name.to_lowercase().cmp(&b.file_name.to_lowercase()));
        let mut principal = None;
        let mut auxiliaries = Vec::new();
        let mut standalone = Vec::new();

        // Prefer SAP/other principal; if multiple principals (unusual), keep first by name
        for f in files {
            match f.role {
                FileRole::Principal => {
                    if principal.is_none() {
                        principal = Some(f);
                    } else {
                        // Extra principal with same key — keep as standalone sibling
                        standalone.push(f);
                    }
                }
                FileRole::Auxiliary => auxiliaries.push(f),
                FileRole::Standalone => {
                    // If no principal yet, a standalone can become the display principal later
                    standalone.push(f);
                }
            }
        }

        // If only auxiliaries/standalone exist, promote first standalone or leave principal None
        if principal.is_none() {
            if let Some(idx) = standalone
                .iter()
                .position(|s| {
                    s.extension
                        .as_deref()
                        .map(|e| is_other_principal(e) || e == "pdf" || e == "docx")
                        .unwrap_or(false)
                })
                .or_else(|| standalone.first().map(|_| 0))
            {
                let promoted = standalone.remove(idx);
                principal = Some(ClassifiedFile {
                    role: FileRole::Principal,
                    ..promoted
                });
            }
        }

        // Auxiliaries without a principal stay listed under this group
        let title = principal
            .as_ref()
            .map(|p| p.stem.clone())
            .or_else(|| auxiliaries.first().map(|a| a.stem.clone()))
            .or_else(|| standalone.first().map(|s| s.stem.clone()))
            .unwrap_or_else(|| group_key.clone());

        groups.push(EngineeringGroup {
            group_key,
            title,
            principal,
            auxiliaries,
            standalone,
        });
    }

    groups.sort_by(|a, b| a.title.to_lowercase().cmp(&b.title.to_lowercase()));
    groups
}

/// Count principal models (groups that have a principal, or any group as fallback).
pub fn count_principals(file_names: &[String], project_codes: &[String]) -> usize {
    group_files(file_names, project_codes)
        .into_iter()
        .filter(|g| g.principal.is_some())
        .count()
}

/// Whether two stems must remain distinct models (SIN vs CON CAMIONES, etc.).
pub fn stems_are_distinct_models(a: &str, b: &str, project_codes: &[String]) -> bool {
    normalize_group_key(a, project_codes) != normalize_group_key(b, project_codes)
}

/// Filter file names for UI: hide auxiliaries when `show_auxiliaries` is false.
pub fn filter_visible_names(
    file_names: &[String],
    project_codes: &[String],
    show_auxiliaries: bool,
) -> Vec<String> {
    if show_auxiliaries {
        return file_names.to_vec();
    }
    let groups = group_files(file_names, project_codes);
    let mut out = Vec::new();
    for g in groups {
        if let Some(p) = g.principal {
            out.push(p.file_name);
        }
        for s in g.standalone {
            out.push(s.file_name);
        }
        // Auxiliaries omitted
        let _ = g.auxiliaries;
    }
    out.sort_by(|a, b| a.to_lowercase().cmp(&b.to_lowercase()));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn codes() -> Vec<String> {
        vec!["1090".into()]
    }

    #[test]
    fn strips_project_prefix_from_folder_tokens() {
        assert_eq!(
            strip_project_code_prefix("1090 - MODELOS", &codes()),
            "MODELOS"
        );
        assert_eq!(
            strip_project_code_prefix("1090_MODELOS", &codes()),
            "MODELOS"
        );
        assert_eq!(
            strip_project_code_prefix("1090-MODELOS", &codes()),
            "MODELOS"
        );
    }

    #[test]
    fn sin_con_camiones_are_distinct() {
        let a = normalize_group_key("1090 - MODELO SIN CAMIONES", &codes());
        let b = normalize_group_key("1090 - MODELO CON CAMIONES", &codes());
        assert_ne!(a, b);
        assert!(stems_are_distinct_models(
            "1090 - MODELO SIN CAMIONES",
            "1090 - MODELO CON CAMIONES",
            &codes()
        ));
    }

    #[test]
    fn groups_sdb_with_auxiliaries() {
        let names = vec![
            "Estructura.sdb".into(),
            "Estructura.$2k".into(),
            "Estructura.log".into(),
            "Other.pdf".into(),
        ];
        let groups = group_files(&names, &[]);
        let sap = groups.iter().find(|g| g.group_key == "estructura").unwrap();
        assert!(sap.principal.as_ref().unwrap().file_name.ends_with(".sdb"));
        assert_eq!(sap.auxiliaries.len(), 2);
        assert!(groups.iter().any(|g| g.group_key == "other"));
    }

    #[test]
    fn hide_auxiliaries_filter() {
        let names = vec![
            "A.sdb".into(),
            "A.log".into(),
            "B.sdb".into(),
            "B.s2k".into(),
        ];
        let visible = filter_visible_names(&names, &[], false);
        assert_eq!(visible, vec!["A.sdb".to_string(), "B.sdb".to_string()]);
        assert_eq!(count_principals(&names, &[]), 2);
    }
}
