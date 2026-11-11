//! Issued / draft revision codes for memorias, planos, and modelos.
//!
//! Filename patterns (`_RA`, `_RB`, `_R0`, `_RevA`, …) are **revisions**, not
//! internal working versions (`_v01`). Sequence: A → B → 0 → 1 → 2.
//!
//! Combined physical naming: `{identity}_R{code}_v{NN}{ext}`
//! e.g. `1126-MC-002_RA_v01.docx`, `1126-SAP-001_RB_v01.sdb`.
//! Modelos may keep legacy `_vNN` only until a revision is emitted.

use std::path::Path;

/// Parsed revision info from a file stem or name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedRevision {
    /// Stem without the revision suffix (document identity).
    pub identity_stem: String,
    /// Canonical code: `A`, `B`, `0`, `1`, … (no leading `R`).
    pub revision_code: String,
    /// True for letter codes (draft); false for numeric (emitted).
    pub is_draft: bool,
}

/// Compound stem: identity + optional revision + optional `_vNN` version.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedCompoundStem {
    pub identity_stem: String,
    pub revision: Option<ParsedRevision>,
    /// Numeric version from `_vNN` / `-vN` only (not `_RA` / `_RevA`).
    pub version_number: Option<u32>,
}

/// Strip a revision suffix from a stem. Returns `(identity, Some(parsed))` or `(stem, None)`.
///
/// Digit-only tokens after `-` (e.g. business correlativo `1126-SAP-001`) are **not**
/// revisions — require `_0` / `_R0` / `Rev0`. Hyphen remains valid for `Rev*` / letter
/// forms (`Plano-Rev.B`).
pub fn strip_revision_suffix(stem: &str) -> (String, Option<ParsedRevision>) {
    let stem = stem.trim();
    if stem.is_empty() {
        return (String::new(), None);
    }

    if let Some(idx) = stem.rfind(['_', '-']) {
        let base = stem[..idx].trim();
        let suffix = &stem[idx + 1..];
        if base.is_empty() || suffix.is_empty() {
            return (stem.to_string(), None);
        }
        let sep = stem.as_bytes()[idx] as char;
        if let Some(parsed) = parse_revision_token(suffix) {
            let digit_only = parsed.0.chars().all(|c| c.is_ascii_digit());
            let suf_l = suffix.to_ascii_lowercase();
            let explicit_rev_word = suf_l.starts_with("rev");
            // `1126-SAP-001` / `1126-MC-010` → correlativo, not revision code.
            if sep == '-' && digit_only && !explicit_rev_word {
                return (stem.to_string(), None);
            }
            return (
                base.to_string(),
                Some(ParsedRevision {
                    identity_stem: base.to_string(),
                    revision_code: parsed.0,
                    is_draft: parsed.1,
                }),
            );
        }
    }

    (stem.to_string(), None)
}

/// Strip only `_vNN` / `-vN` (digits). Does **not** treat `_RA` / `_RevA` as versions.
pub fn strip_vnn_suffix(stem: &str) -> (String, Option<u32>) {
    let stem = stem.trim();
    if stem.is_empty() {
        return (String::new(), None);
    }
    if let Some(idx) = stem.rfind(['_', '-']) {
        let base = stem[..idx].trim();
        let suffix = &stem[idx + 1..];
        if !base.is_empty() && !suffix.is_empty() {
            let suf_l = suffix.to_ascii_lowercase();
            if let Some(num) = suf_l.strip_prefix('v') {
                if !num.is_empty() && num.chars().all(|c| c.is_ascii_digit()) {
                    if let Ok(n) = num.parse::<u32>() {
                        return (base.to_string(), Some(n));
                    }
                }
            }
        }
    }
    (stem.to_string(), None)
}

/// Parse identity + revision + `_vNN` in stable order (version outermost, then revision).
/// `Doc_RA_v01` → identity `Doc`, rev `A`, version `1`.
pub fn parse_compound_stem(stem: &str) -> ParsedCompoundStem {
    let stem = stem.trim();
    let (without_v, version_number) = strip_vnn_suffix(stem);
    let (identity, revision) = strip_revision_suffix(&without_v);
    let identity = if identity.is_empty() {
        without_v.clone()
    } else {
        identity
    };
    let revision = revision.map(|mut r| {
        r.identity_stem = identity.clone();
        r
    });
    ParsedCompoundStem {
        identity_stem: identity,
        revision,
        version_number,
    }
}

/// Filename token for a revision code: `A` → `RA`, `0` → `R0`.
pub fn format_revision_token(code: &str) -> String {
    let c = code.trim();
    if c.is_empty() {
        return String::new();
    }
    if c.chars().all(|ch| ch.is_ascii_digit()) {
        format!("R{c}")
    } else {
        format!("R{}", c.to_ascii_uppercase())
    }
}

/// Build `{identity}_R{code}_v{NN}` (omit missing parts). Revision before version.
pub fn build_compound_stem(
    identity: &str,
    revision_code: Option<&str>,
    version_number: Option<i64>,
) -> String {
    let mut s = identity.trim().to_string();
    if let Some(code) = revision_code.map(str::trim).filter(|c| !c.is_empty()) {
        let token = format_revision_token(code);
        if !token.is_empty() {
            s = format!("{s}_{token}");
        }
    }
    if let Some(n) = version_number {
        let n = if n < 1 { 1 } else { n };
        s = format!("{s}_v{n:02}");
    }
    s
}

/// Rebuild a file name with a new revision + version, preserving extension.
/// Strips prior `_R*` / `_vNN` from the stem first.
pub fn apply_revision_version_filename(
    current_file_name: &str,
    revision_code: &str,
    version_number: i64,
) -> String {
    let path = Path::new(current_file_name);
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| current_file_name.to_string());
    let parsed = parse_compound_stem(&stem);
    let identity = if parsed.identity_stem.is_empty() {
        stem
    } else {
        parsed.identity_stem
    };
    let new_stem = build_compound_stem(&identity, Some(revision_code), Some(version_number));
    let ext = path
        .extension()
        .map(|e| format!(".{}", e.to_string_lossy()))
        .unwrap_or_default();
    format!("{new_stem}{ext}")
}

/// Parse a lone revision token: `RA`, `RB`, `R0`, `RevA`, `Rev.B`, `rev02`, `A`, `0`.
fn parse_revision_token(token: &str) -> Option<(String, bool)> {
    let t = token.trim();
    if t.is_empty() {
        return None;
    }
    let lower = t.to_ascii_lowercase();

    // revA / rev.B / rev02 / rev_0
    if let Some(rest) = lower.strip_prefix("rev") {
        let rest = rest.trim_start_matches(['.', '_', '-']);
        return classify_code_body(rest, true);
    }

    // RA / RB / R0 / R12 — single R + one letter OR 1–3 digits (not "roof"/"report")
    if lower.starts_with('r') && lower.len() >= 2 {
        let rest = &lower[1..];
        if rest.len() == 1 && rest.chars().all(|c| c.is_ascii_alphabetic()) {
            return classify_code_body(rest, false);
        }
        if (1..=3).contains(&rest.len()) && rest.chars().all(|c| c.is_ascii_digit()) {
            return classify_code_body(rest, false);
        }
    }

    // Bare A / B / 0 / 12 after separator
    classify_code_body(&lower, false)
}

fn classify_code_body(body: &str, allow_multi_letter: bool) -> Option<(String, bool)> {
    if body.is_empty() || body.len() > 3 {
        return None;
    }
    if body.chars().all(|c| c.is_ascii_alphabetic()) {
        if !allow_multi_letter && body.len() != 1 {
            return None;
        }
        let code = body.to_ascii_uppercase();
        return Some((code, true));
    }
    if body.chars().all(|c| c.is_ascii_digit()) {
        let code = if body.chars().all(|c| c == '0') {
            "0".to_string()
        } else {
            body.trim_start_matches('0').to_string()
        };
        return Some((code, false));
    }
    None
}

/// Parse revision from a file name (with extension).
///
/// Handles compound stems from naming #20: `Doc_RB_v01.docx` → revision `B`
/// (version stripped first). Bare `_RA` / legacy `_RevA` still work.
pub fn parse_revision_filename(file_name: &str) -> Option<ParsedRevision> {
    let stem = Path::new(file_name)
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| file_name.to_string());
    parse_compound_stem(&stem).revision
}

/// Document identity key for grouping (lowercase, revision + `_vNN` stripped).
pub fn revision_identity_key(file_name: &str) -> String {
    let stem = Path::new(file_name)
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| file_name.to_string());
    parse_compound_stem(&stem)
        .identity_stem
        .to_ascii_lowercase()
}

/// Status string for a revision code: `draft` (letters) or `issued` (digits).
pub fn status_for_code(code: &str) -> &'static str {
    let c = code.trim();
    if c.is_empty() {
        return "draft";
    }
    if c.chars().all(|ch| ch.is_ascii_digit()) {
        "issued"
    } else {
        "draft"
    }
}

/// Sort key for revision sequence A → B → … → 0 → 1 → 2.
/// Letters (drafts) come before numeric (emitted).
pub fn revision_sort_key(code: &str) -> (u8, u64, String) {
    let c = code.trim();
    if c.is_empty() {
        return (9, 0, String::new());
    }
    if c.chars().all(|ch| ch.is_ascii_digit()) {
        let n: u64 = c.parse().unwrap_or(0);
        return (1, n, c.to_string());
    }
    if c.chars().all(|ch| ch.is_ascii_alphabetic()) {
        let upper = c.to_ascii_uppercase();
        // A=1, B=2, … multi-letter lexicographic via first chars packed
        let mut ord: u64 = 0;
        for ch in upper.chars().take(3) {
            ord = ord * 27 + (ch as u64 - b'A' as u64 + 1);
        }
        return (0, ord, upper);
    }
    (2, 0, c.to_ascii_lowercase())
}

/// Compare two revision codes by engineering sequence.
pub fn cmp_revision_codes(a: &str, b: &str) -> std::cmp::Ordering {
    revision_sort_key(a).cmp(&revision_sort_key(b))
}

/// Human display: `A`, `B`, `0` → often shown as-is; optional `R` prefix for UI.
pub fn display_revision_code(code: &str) -> String {
    let c = code.trim();
    if c.is_empty() {
        return "—".into();
    }
    c.to_string()
}

/// Whether this category can emit / parse formal revisions (`_R…`).
/// Includes modelos (parity with memorias/planos naming).
pub fn category_supports_revisions(category_key: &str) -> bool {
    matches!(category_key, "memorias" | "planos" | "modelos")
}

/// Whether the logical tree middle level is revision (memorias / planos).
/// Modelos keep version branches in the tree; revision is column + emit flow.
pub fn category_uses_revisions(category_key: &str) -> bool {
    matches!(category_key, "memorias" | "planos")
}

#[cfg(test)]
mod unit_tests {
    use super::*;

    #[test]
    fn parses_ra_rb_r0_reva() {
        let cases = [
            ("1126-MC-002_RB", "1126-MC-002", "B", true),
            ("1126-MC-002_R0", "1126-MC-002", "0", false),
            ("1126-MC-002_RA", "1126-MC-002", "A", true),
            ("Memoria_RevA", "Memoria", "A", true),
            ("Plano-Rev.B", "Plano", "B", true),
            ("Doc_rev02", "Doc", "2", false),
        ];
        for (stem, id, code, draft) in cases {
            let (base, parsed) = strip_revision_suffix(stem);
            assert_eq!(base, id, "identity for {stem}");
            let p = parsed.expect(stem);
            assert_eq!(p.revision_code, code, "code for {stem}");
            assert_eq!(p.is_draft, draft, "draft for {stem}");
        }
        // Business correlativo must not be eaten as revision.
        let (base, parsed) = strip_revision_suffix("1126-SAP-001");
        assert!(parsed.is_none(), " -001 is correlativo");
        assert_eq!(base, "1126-SAP-001");
        let p = parse_compound_stem("1126-SAP-001_RB_v01");
        assert_eq!(p.identity_stem, "1126-SAP-001");
        assert_eq!(
            apply_revision_version_filename("1126-SAP-001.sdb", "B", 1),
            "1126-SAP-001_RB_v01.sdb"
        );
    }

    #[test]
    fn does_not_strip_v01_as_revision() {
        let (base, parsed) = strip_revision_suffix("MODELO_v01");
        assert!(parsed.is_none());
        assert_eq!(base, "MODELO_v01");
    }

    #[test]
    fn sequence_a_b_0_1_2() {
        let mut codes = vec!["1", "B", "0", "A", "2"];
        codes.sort_by(|a, b| cmp_revision_codes(a, b));
        assert_eq!(codes, vec!["A", "B", "0", "1", "2"]);
    }

    #[test]
    fn identity_groups_rb_and_r0() {
        assert_eq!(
            revision_identity_key("1126-MC-002_RB.docx"),
            revision_identity_key("1126-MC-002_R0.pdf")
        );
    }

    #[test]
    fn compound_ra_v01_and_build() {
        let p = parse_compound_stem("1126-MC-002_RA_v01");
        assert_eq!(p.identity_stem, "1126-MC-002");
        assert_eq!(p.revision.as_ref().map(|r| r.revision_code.as_str()), Some("A"));
        assert_eq!(p.version_number, Some(1));
        assert_eq!(
            build_compound_stem("1126-MC-002", Some("A"), Some(1)),
            "1126-MC-002_RA_v01"
        );
        assert_eq!(
            build_compound_stem("1126-MC-002", Some("0"), Some(2)),
            "1126-MC-002_R0_v02"
        );
        assert_eq!(
            apply_revision_version_filename("Memo_RA_v03.docx", "B", 1),
            "Memo_RB_v01.docx"
        );
        assert_eq!(
            apply_revision_version_filename("Memo.docx", "A", 1),
            "Memo_RA_v01.docx"
        );
    }

    #[test]
    fn strip_vnn_does_not_eat_ra() {
        let (base, ver) = strip_vnn_suffix("Doc_RA");
        assert!(ver.is_none());
        assert_eq!(base, "Doc_RA");
        let (base2, ver2) = strip_vnn_suffix("Doc_RA_v02");
        assert_eq!(base2, "Doc_RA");
        assert_eq!(ver2, Some(2));
    }

    #[test]
    fn filename_rb_v01_detects_revision_b() {
        let p = parse_revision_filename("1133_3-000-MC-0001_RB_v01.docx").expect("_RB_v01");
        assert_eq!(p.revision_code, "B");
        assert!(p.is_draft);
        assert_eq!(p.identity_stem, "1133_3-000-MC-0001");

        let compound = parse_compound_stem("1133_3-000-MC-0001_RB_v01");
        assert_eq!(
            compound.revision.as_ref().map(|r| r.revision_code.as_str()),
            Some("B")
        );
        assert_eq!(compound.version_number, Some(1));

        assert_eq!(
            parse_revision_filename("1133_3-000-MC-0001_RA_v02.pdf")
                .map(|r| r.revision_code),
            Some("A".into())
        );
        assert_eq!(
            parse_revision_filename("Doc_R0_v01.docx").map(|r| r.revision_code),
            Some("0".into())
        );
        assert_eq!(
            parse_revision_filename("Doc_R1.docx").map(|r| r.revision_code),
            Some("1".into())
        );
        // No `_R` → null (version-only / bare stem).
        assert!(parse_revision_filename("Doc_v01.docx").is_none());
        assert!(parse_revision_filename("Doc.docx").is_none());
        // `_vNN` alone is not a revision; legacy `_Rev*` still parses as revision.
        assert!(parse_revision_filename("MODELO_v01.sdb").is_none());
        assert_eq!(
            parse_revision_filename("1126-SAP-001_RB_v01.sdb").map(|r| r.revision_code),
            Some("B".into())
        );
        assert_eq!(
            parse_revision_filename("Memoria_RevA.docx").map(|r| r.revision_code),
            Some("A".into())
        );
        assert!(category_supports_revisions("modelos"));
        assert!(category_supports_revisions("memorias"));
        assert!(!category_uses_revisions("modelos"));
    }

    #[test]
    fn identity_key_strips_compound_rb_v01() {
        assert_eq!(
            revision_identity_key("1126-MC-002_RB_v01.docx"),
            revision_identity_key("1126-MC-002_RA_v02.pdf")
        );
        assert_eq!(
            revision_identity_key("1126-MC-002_RB_v01.docx"),
            "1126-mc-002"
        );
    }
}
