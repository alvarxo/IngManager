//! Engineering file roles and SAP2000-style grouping.
//!
//! Principal models (e.g. `.sdb`) are the identity users manage.
//! Auxiliaries share the same stem and are hidden by default.
//! Distinct stems like `… SIN CAMIONES` vs `… CON CAMIONES` are never merged.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

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
/// Includes `.sbk` (SAP2000 backup) — never a model version principal.
const SAP_AUX_EXTS: &[&str] = &[
    "sbk", "s2k", "$2k", "log", "out", "txt", "xml", "rtf", "mdb", "sdbr", "ebk", "sfk", "$sf",
    "avi", "gif", "bmp", "jpg", "jpeg", "png", "xls", "xlsx", "csv",
];

/// True when `ext` is a SAP2000/CSI model principal (`.sdb` only).
pub fn is_sap_principal_extension(ext: &str) -> bool {
    let e = ext.trim_start_matches('.').to_ascii_lowercase();
    SAP_PRINCIPAL_EXTS.iter().any(|x| *x == e.as_str())
}

/// True when `ext` is a SAP companion/aux (incl. `.sbk`) — never creates model versions.
pub fn is_sap_auxiliary_extension(ext: &str) -> bool {
    let e = ext.trim_start_matches('.').to_ascii_lowercase();
    SAP_AUX_EXTS.iter().any(|x| *x == e.as_str())
}

/// Extensions that may create auto-versions. SAP aux / `.sbk` always false.
/// Model versions require `.sdb`; other principals (docx/dwg/rvt/…) remain eligible.
pub fn can_auto_version_extension(ext: Option<&str>) -> bool {
    let Some(raw) = ext.map(|e| e.trim_start_matches('.').to_ascii_lowercase()) else {
        return false;
    };
    if is_sap_auxiliary_extension(&raw) || OTHER_AUX_EXTS.iter().any(|a| *a == raw.as_str()) {
        return false;
    }
    if is_sap_principal_extension(&raw) {
        return true;
    }
    matches!(
        raw.as_str(),
        "docx"
            | "doc"
            | "dwg"
            | "dxf"
            | "rvt"
            | "rfa"
            | "pdf"
            | "idea"
            | "ideastatica"
            | "rtd"
            | "edb"
            | "e2k"
    ) || OTHER_PRINCIPAL_EXTS.iter().any(|a| *a == raw.as_str())
}

/// Other CAD/BIM principals (standalone unless companions share stem).
const OTHER_PRINCIPAL_EXTS: &[&str] = &[
    "rvt",
    "rfa",
    "dwg",
    "dxf",
    "idea",
    "ideastatica",
    "rtd", // Robot Structural Analysis
    "edb", // ETABS
    "e2k",
];

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
    /// All principal files that share this identity (e.g. `Modelo.sdb`, `Modelo_v02.sdb`).
    /// Ordered oldest → newest by version hint when present.
    pub version_principals: Vec<ClassifiedFile>,
    pub auxiliaries: Vec<ClassifiedFile>,
    pub standalone: Vec<ClassifiedFile>,
}

fn extension_of(name: &str) -> Option<String> {
    Path::new(name)
        .extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase())
}

pub fn raw_stem(name: &str) -> String {
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

/// Strip Save-As suffixes that are **internal versions**, not identity.
/// Examples: `_v01`, `-v2`, ` (2)`, ` - Copy`, `_copia3`.
/// Does **not** strip `_RA` / `_R0` / `_RevA` (those are formal revisions — see
/// `doc_revisions`). Does **not** touch SIN/CON CAMIONES wording.
pub fn strip_version_suffix(stem: &str) -> (String, Option<String>) {
    let stem = stem.trim();
    if stem.is_empty() {
        return (String::new(), None);
    }

    // name (12)
    if let Some(open) = stem.rfind(" (") {
        if stem.ends_with(')') {
            let inner = &stem[open + 2..stem.len() - 1];
            if !inner.is_empty() && inner.chars().all(|c| c.is_ascii_digit()) {
                let base = stem[..open].trim();
                if !base.is_empty() {
                    return (base.to_string(), Some(inner.to_string()));
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
            let hint = trailing_paren_num(rest.trim());
            return (base.to_string(), hint);
        }
    }

    if let Some(idx) = stem.rfind(['_', '-']) {
        let base = stem[..idx].trim();
        let suffix = &stem[idx + 1..];
        if !base.is_empty() && !suffix.is_empty() {
            let suf_l = suffix.to_lowercase();

            // v01 / v1 only — never `_RA` / `_RevB` (revisions).
            if suf_l.starts_with('v') {
                let num = &suf_l[1..];
                if !num.is_empty() && num.chars().all(|c| c.is_ascii_digit()) {
                    return (base.to_string(), Some(num.to_string()));
                }
            }

            if suf_l == "copy" || suf_l == "copia" {
                return (base.to_string(), None);
            }
            for prefix in ["copy", "copia"] {
                if let Some(rest) = suf_l.strip_prefix(prefix) {
                    let rest = rest.trim_start_matches(['_', '-']);
                    if rest.is_empty() {
                        return (base.to_string(), None);
                    }
                    if rest.chars().all(|c| c.is_ascii_digit()) {
                        return (base.to_string(), Some(rest.to_string()));
                    }
                }
            }
        }
    }

    (stem.to_string(), None)
}

fn trailing_paren_num(s: &str) -> Option<String> {
    let s = s.trim();
    if s.starts_with('(') && s.ends_with(')') {
        let inner = &s[1..s.len() - 1];
        if !inner.is_empty() && inner.chars().all(|c| c.is_ascii_digit()) {
            return Some(inner.to_string());
        }
    }
    None
}

/// Parse version hint from a file name (stem or full name).
pub fn version_hint_from_name(file_name: &str) -> Option<String> {
    let stem = raw_stem(file_name);
    strip_version_suffix(&stem).1
}

/// Normalize a group identity: lowercase, collapse whitespace, keep SIN/CON wording.
/// Version suffixes (`_v01`, …) are **not** part of identity. Revision tokens
/// (`_RA`, `_RevA`) are left in place (use `doc_revisions` to strip those).
pub fn normalize_group_key(stem: &str, project_codes: &[String]) -> String {
    let stripped = strip_project_code_prefix(stem, project_codes);
    // Also strip a leading AANN-looking code even if not in the list
    let stripped = strip_leading_aann_public(&stripped);
    let (without_ver, _) = strip_version_suffix(&stripped);
    without_ver
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
    // Project-code-ish token: must *start with a digit* (1090, P1090→ handled by letter+digits
    // only when mostly numeric). Never strip words like MODELO_v01 → v01.
    let mut i = 0usize;
    while i < bytes.len() && (bytes[i].is_ascii_alphanumeric()) {
        i += 1;
    }
    if i >= 2 && i <= 8 && i < bytes.len() {
        let token = &trimmed[..i];
        if !looks_like_project_code_token(token) {
            return trimmed.to_string();
        }
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

fn looks_like_project_code_token(token: &str) -> bool {
    if token.is_empty() {
        return false;
    }
    let digits = token.chars().filter(|c| c.is_ascii_digit()).count();
    let letters = token.chars().filter(|c| c.is_ascii_alphabetic()).count();
    // Pure digits (1090) or letter+digits project-ish (P1090, AANN) — not words (MODELO)
    digits >= 2 && digits >= letters
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

fn version_sort_key(file_name: &str) -> (u8, u64, String) {
    match version_hint_from_name(file_name) {
        Some(h) => {
            if let Ok(n) = h.parse::<u64>() {
                (1, n, h.to_lowercase())
            } else {
                // letter revisions: A < B; treat as mid-tier after numeric
                let ord = h.chars().next().map(|c| c.to_ascii_lowercase() as u64).unwrap_or(0);
                (2, ord, h.to_lowercase())
            }
        }
        // Unversioned baseline sorts first
        None => (0, 0, file_name.to_lowercase()),
    }
}

/// Group a list of file names. Principals win; auxiliaries attach to same group_key.
/// Versioned stems (`_v01` / `_v02`) share identity; `SIN`/`CON CAMIONES` stay separate.
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
        let mut version_principals = Vec::new();
        let mut auxiliaries = Vec::new();
        let mut standalone = Vec::new();

        for f in files {
            match f.role {
                FileRole::Principal => version_principals.push(f),
                FileRole::Auxiliary => auxiliaries.push(f),
                FileRole::Standalone => standalone.push(f),
            }
        }

        // If only auxiliaries/standalone exist, promote first standalone
        if version_principals.is_empty() {
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
                version_principals.push(ClassifiedFile {
                    role: FileRole::Principal,
                    ..promoted
                });
            }
        }

        version_principals.sort_by(|a, b| {
            version_sort_key(&a.file_name).cmp(&version_sort_key(&b.file_name))
        });

        // Display / "current" principal = newest version variant when present
        let principal = version_principals.last().cloned();

        let title = principal
            .as_ref()
            .map(|p| {
                // Prefer identity stem without version suffix for logical title
                let (base, _) = strip_version_suffix(&p.stem);
                if base.trim().is_empty() {
                    p.stem.clone()
                } else {
                    base
                }
            })
            .or_else(|| auxiliaries.first().map(|a| a.stem.clone()))
            .or_else(|| standalone.first().map(|s| s.stem.clone()))
            .unwrap_or_else(|| group_key.clone());

        groups.push(EngineeringGroup {
            group_key,
            title,
            principal,
            version_principals,
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
        if !g.version_principals.is_empty() {
            for p in g.version_principals {
                out.push(p.file_name);
            }
        } else if let Some(p) = g.principal {
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

/// Preferred open order for logical-document principal files (Lógicos dbl-click).
/// DOCX → DWG → SDB → RVT, then other principals; auxiliaries last.
pub fn principal_open_rank(extension: Option<&str>) -> u8 {
    match extension.map(|e| e.trim_start_matches('.').to_ascii_lowercase()).as_deref() {
        Some("docx") | Some("doc") => 0,
        Some("dwg") | Some("dxf") => 1,
        Some("sdb") => 2,
        Some("rvt") | Some("rfa") => 3,
        Some(ext) if OTHER_PRINCIPAL_EXTS.contains(&ext) => 4,
        Some(ext) if SAP_PRINCIPAL_EXTS.contains(&ext) => 4,
        Some(ext)
            if SAP_AUX_EXTS.iter().any(|a| a.eq_ignore_ascii_case(ext))
                || OTHER_AUX_EXTS.iter().any(|a| a.eq_ignore_ascii_case(ext)) =>
        {
            90
        }
        _ => 50,
    }
}

/// Pick the best file index for opening among (file_name, extension) pairs.
/// Lower `principal_open_rank` wins; ties keep the first occurrence.
pub fn pick_principal_open_index<'a, I>(files: I) -> Option<usize>
where
    I: IntoIterator<Item = (&'a str, Option<&'a str>)>,
{
    let mut best: Option<(usize, u8)> = None;
    for (idx, (_name, ext)) in files.into_iter().enumerate() {
        let rank = principal_open_rank(ext);
        match best {
            None => best = Some((idx, rank)),
            Some((_, best_rank)) if rank < best_rank => best = Some((idx, rank)),
            _ => {}
        }
    }
    best.map(|(i, _)| i)
}

/// Build the next working filename for a physical copy: `{base}_v{n:02}{ext}`.
/// Preserves revision suffix when present: `Memo_RA_v01.docx` → `Memo_RA_v02.docx`.
/// Modelos (no `_R`): `1126 - MODELO_v01.sdb` → `1126 - MODELO_v02.sdb`.
pub fn next_working_filename(current_file_name: &str, next_version_number: i64) -> String {
    let path = Path::new(current_file_name);
    let stem = raw_stem(current_file_name);
    let n = if next_version_number < 1 {
        1
    } else {
        next_version_number
    };
    let ext = path
        .extension()
        .map(|e| format!(".{}", e.to_string_lossy()))
        .unwrap_or_default();

    // Prefer compound parse so `_RA` / `_R0` stay when bumping `_vNN`.
    let compound = crate::doc_revisions::parse_compound_stem(&stem);
    if compound.revision.is_some() || compound.version_number.is_some() {
        let rev = compound
            .revision
            .as_ref()
            .map(|r| r.revision_code.as_str());
        let identity = if compound.identity_stem.is_empty() {
            stem.as_str()
        } else {
            compound.identity_stem.as_str()
        };
        let new_stem =
            crate::doc_revisions::build_compound_stem(identity, rev, Some(n));
        return format!("{new_stem}{ext}");
    }

    // Legacy Save-As patterns: `(2)`, `- Copy`, `_revB` as version-ish strip.
    let (base, _) = strip_version_suffix(&stem);
    let base = if base.is_empty() { stem } else { base };
    format!("{base}_v{n:02}{ext}")
}

/// Resolve a free destination path beside `source`, starting at `next_version_number`.
/// Bumps the `_vNN` suffix until the path does not exist (cap 999).
pub fn allocate_next_working_path(
    source: &Path,
    next_version_number: i64,
) -> Result<PathBuf, String> {
    let parent = source
        .parent()
        .ok_or_else(|| "Archivo fuente sin directorio padre".to_string())?;
    let file_name = source
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or_else(|| "Nombre de archivo inválido".to_string())?;
    let mut n = next_version_number.max(1);
    for _ in 0..1000 {
        let candidate_name = next_working_filename(file_name, n);
        let dest = parent.join(&candidate_name);
        if !dest.exists() {
            return Ok(dest);
        }
        n += 1;
    }
    Err("No hay nombre libre para la siguiente versión física".into())
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

    #[test]
    fn version_suffixes_are_not_identity() {
        let a = normalize_group_key("Puente_v01", &[]);
        let b = normalize_group_key("Puente_v02", &[]);
        let c = normalize_group_key("Puente", &[]);
        assert_eq!(a, b);
        assert_eq!(a, c);
        let names = vec![
            "1126 - MODELO_v01.sdb".into(),
            "1126 - MODELO_v02.sdb".into(),
        ];
        let groups = group_files(&names, &["1126".into()]);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].version_principals.len(), 2);
        assert_eq!(count_principals(&names, &["1126".into()]), 1);
    }

    #[test]
    fn sin_con_stay_distinct_even_with_versions() {
        let codes = codes();
        let names = vec![
            "1090 - MODELO SIN CAMIONES_v01.sdb".into(),
            "1090 - MODELO SIN CAMIONES_v02.sdb".into(),
            "1090 - MODELO CON CAMIONES_v01.sdb".into(),
        ];
        let groups = group_files(&names, &codes);
        assert_eq!(groups.len(), 2);
        assert_eq!(count_principals(&names, &codes), 2);
    }

    #[test]
    fn next_working_filename_bumps_vnn() {
        assert_eq!(
            next_working_filename("Modelo_v01.sdb", 2),
            "Modelo_v02.sdb"
        );
        assert_eq!(
            next_working_filename("1126 - MODELO_v01.sdb", 2),
            "1126 - MODELO_v02.sdb"
        );
        assert_eq!(next_working_filename("Base.sdb", 1), "Base_v01.sdb");
        assert_eq!(
            next_working_filename("Memo_RA_v01.docx", 2),
            "Memo_RA_v02.docx"
        );
        assert_eq!(
            next_working_filename("1126-MC-002_R0.docx", 1),
            "1126-MC-002_R0_v01.docx"
        );
    }

    #[test]
    fn principal_open_prefers_docx_dwg_sdb_rvt() {
        let files = [
            ("a.log", Some("log")),
            ("model.sdb", Some("sdb")),
            ("note.docx", Some("docx")),
            ("plan.dwg", Some("dwg")),
        ];
        let idx = pick_principal_open_index(files.iter().map(|(n, e)| (*n, *e))).unwrap();
        assert_eq!(files[idx].0, "note.docx");

        let files2 = [
            ("model.sdb", Some("sdb")),
            ("plan.dwg", Some("dwg")),
            ("bim.rvt", Some("rvt")),
        ];
        let idx2 = pick_principal_open_index(files2.iter().map(|(n, e)| (*n, *e))).unwrap();
        assert_eq!(files2[idx2].0, "plan.dwg");
    }
}
