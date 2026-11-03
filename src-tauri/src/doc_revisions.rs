//! Issued / draft revision codes for memorias and planos.
//!
//! Filename patterns (`_RA`, `_RB`, `_R0`, `_RevA`, …) are **revisions**, not
//! internal working versions (`_v01`). Sequence: A → B → 0 → 1 → 2.

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

/// Strip a revision suffix from a stem. Returns `(identity, Some(parsed))` or `(stem, None)`.
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
        if let Some(parsed) = parse_revision_token(suffix) {
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
pub fn parse_revision_filename(file_name: &str) -> Option<ParsedRevision> {
    let stem = Path::new(file_name)
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| file_name.to_string());
    strip_revision_suffix(&stem).1
}

/// Document identity key for grouping (lowercase, revision stripped).
pub fn revision_identity_key(file_name: &str) -> String {
    let stem = Path::new(file_name)
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| file_name.to_string());
    let (identity, _) = strip_revision_suffix(&stem);
    identity.to_ascii_lowercase()
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

/// Whether this category uses issued revisions (memorias / planos).
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
}
