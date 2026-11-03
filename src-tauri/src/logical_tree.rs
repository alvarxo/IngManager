//! Logical-document tree navigation helpers (open prefs + expand policy).
//! Navigation/open only — does not create versions or revisions.

use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use sqlx::SqlitePool;

use crate::error::{AppError, AppResult};

pub const EXPAND_POLICY_KEY: &str = "logical_tree_expand_policy";
pub const EXPAND_STATE_PREFIX: &str = "logical_tree_expand:";
pub const PREFERRED_OPEN_EXT_FIELD: &str = "preferred_open_ext";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExpandPolicy {
    Remember,
    Collapsed,
    Expanded,
}

impl ExpandPolicy {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Remember => "remember",
            Self::Collapsed => "collapsed",
            Self::Expanded => "expanded",
        }
    }

    pub fn parse(raw: &str) -> Self {
        match raw.trim().to_ascii_lowercase().as_str() {
            "collapsed" | "all_collapsed" | "all-collapsed" => Self::Collapsed,
            "expanded" | "all_expanded" | "all-expanded" => Self::Expanded,
            _ => Self::Remember,
        }
    }
}

pub fn expand_state_key(project_id: &str) -> String {
    format!("{EXPAND_STATE_PREFIX}{project_id}")
}

/// Read preferred open extension from documents.metadata_json.
pub fn preferred_open_ext_from_metadata(metadata_json: Option<&str>) -> Option<String> {
    let raw = metadata_json?.trim();
    if raw.is_empty() {
        return None;
    }
    let v: Value = serde_json::from_str(raw).ok()?;
    v.get(PREFERRED_OPEN_EXT_FIELD)
        .and_then(|x| x.as_str())
        .map(|s| s.trim().trim_start_matches('.').to_ascii_lowercase())
        .filter(|s| !s.is_empty())
}

/// Merge preferred_open_ext into metadata_json (preserve other keys).
pub fn merge_preferred_open_ext(
    metadata_json: Option<&str>,
    ext: Option<&str>,
) -> AppResult<String> {
    let mut map: Map<String, Value> = match metadata_json.map(str::trim).filter(|s| !s.is_empty()) {
        Some(raw) => match serde_json::from_str::<Value>(raw)? {
            Value::Object(m) => m,
            _ => Map::new(),
        },
        None => Map::new(),
    };
    match ext.map(str::trim).filter(|s| !s.is_empty()) {
        Some(e) => {
            map.insert(
                PREFERRED_OPEN_EXT_FIELD.to_string(),
                Value::String(e.trim_start_matches('.').to_ascii_lowercase()),
            );
        }
        None => {
            map.remove(PREFERRED_OPEN_EXT_FIELD);
        }
    }
    Ok(Value::Object(map).to_string())
}

/// Normalize a format token (extension or label) for matching.
pub fn normalize_format_token(token: &str) -> String {
    token
        .trim()
        .trim_start_matches('.')
        .to_ascii_lowercase()
}

/// True when a file row matches the requested open format (ext or format_label).
pub fn file_matches_format(
    extension: Option<&str>,
    format_label: Option<&str>,
    file_name: &str,
    want: &str,
) -> bool {
    let want = normalize_format_token(want);
    if want.is_empty() {
        return false;
    }
    if let Some(ext) = extension {
        if normalize_format_token(ext) == want {
            return true;
        }
    }
    if let Some(label) = format_label {
        if normalize_format_token(label) == want {
            return true;
        }
    }
    // Fallback: extension from file name
    std::path::Path::new(file_name)
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| normalize_format_token(e) == want)
        .unwrap_or(false)
}

/// Distinct openable format tokens for a set of available files (prefer extension).
pub fn distinct_open_formats<'a, I>(files: I) -> Vec<String>
where
    I: IntoIterator<Item = (Option<&'a str>, Option<&'a str>, &'a str)>,
{
    let mut out: Vec<String> = Vec::new();
    for (ext, label, name) in files {
        let token = ext
            .map(normalize_format_token)
            .filter(|s| !s.is_empty())
            .or_else(|| {
                label
                    .map(normalize_format_token)
                    .filter(|s| !s.is_empty())
            })
            .or_else(|| {
                std::path::Path::new(name)
                    .extension()
                    .and_then(|e| e.to_str())
                    .map(normalize_format_token)
                    .filter(|s| !s.is_empty())
            });
        if let Some(t) = token {
            if !out.iter().any(|x| x == &t) {
                out.push(t);
            }
        }
    }
    out.sort();
    out
}

pub async fn get_expand_policy(pool: &SqlitePool) -> AppResult<ExpandPolicy> {
    let raw = crate::db::get_setting(pool, EXPAND_POLICY_KEY).await?;
    Ok(raw
        .as_deref()
        .map(ExpandPolicy::parse)
        .unwrap_or(ExpandPolicy::Remember))
}

pub async fn set_expand_policy(pool: &SqlitePool, policy: ExpandPolicy) -> AppResult<()> {
    crate::db::set_setting(pool, EXPAND_POLICY_KEY, policy.as_str()).await
}

pub async fn get_expand_state_json(
    pool: &SqlitePool,
    project_id: &str,
) -> AppResult<Value> {
    let key = expand_state_key(project_id);
    let raw = crate::db::get_setting(pool, &key).await?;
    match raw {
        Some(s) if !s.trim().is_empty() => {
            Ok(serde_json::from_str(&s).unwrap_or_else(|_| json!({})))
        }
        _ => Ok(json!({})),
    }
}

pub async fn set_expand_state_json(
    pool: &SqlitePool,
    project_id: &str,
    state: &Value,
) -> AppResult<()> {
    let key = expand_state_key(project_id);
    let s = serde_json::to_string(state).map_err(AppError::from)?;
    crate::db::set_setting(pool, &key, &s).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preferred_ext_roundtrip() {
        let merged = merge_preferred_open_ext(None, Some("DOCX")).unwrap();
        assert_eq!(
            preferred_open_ext_from_metadata(Some(&merged)).as_deref(),
            Some("docx")
        );
        let cleared = merge_preferred_open_ext(Some(&merged), None).unwrap();
        assert_eq!(preferred_open_ext_from_metadata(Some(&cleared)), None);
    }

    #[test]
    fn distinct_formats_sorted_unique() {
        let files = [
            (Some("docx"), Some("DOCX"), "a.docx"),
            (Some("pdf"), None, "a.pdf"),
            (Some("DOCX"), None, "b.docx"),
        ];
        assert_eq!(
            distinct_open_formats(files),
            vec!["docx".to_string(), "pdf".to_string()]
        );
    }

    #[test]
    fn expand_policy_parse() {
        assert_eq!(ExpandPolicy::parse("remember"), ExpandPolicy::Remember);
        assert_eq!(ExpandPolicy::parse("all_collapsed"), ExpandPolicy::Collapsed);
        assert_eq!(ExpandPolicy::parse("expanded"), ExpandPolicy::Expanded);
    }
}
