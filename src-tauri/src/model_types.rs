//! Model software subtypes and business-code prefixes.
//!
//! Persist `model_type` on Modelos documents and allocate independent
//! per-project per-prefix correlativos (SAP / RVT / IDE / ROB / ETB / MOD).

use serde::{Deserialize, Serialize};

/// Software subtype for engineering model documents.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelType {
    Sap2000,
    Revit,
    IdeaStatica,
    Robot,
    Etabs,
    Other,
}

impl ModelType {
    pub fn as_str(self) -> &'static str {
        match self {
            ModelType::Sap2000 => "sap2000",
            ModelType::Revit => "revit",
            ModelType::IdeaStatica => "idea_statica",
            ModelType::Robot => "robot",
            ModelType::Etabs => "etabs",
            ModelType::Other => "other",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "sap2000" | "sap" => Some(ModelType::Sap2000),
            "revit" | "rvt" => Some(ModelType::Revit),
            "idea_statica" | "idea" | "ide" | "ideastatica" => Some(ModelType::IdeaStatica),
            "robot" | "rob" => Some(ModelType::Robot),
            "etabs" | "etb" => Some(ModelType::Etabs),
            "other" | "mod" => Some(ModelType::Other),
            _ => None,
        }
    }

    /// Business-code type segment: `PROYECTO-{PREFIX}-NNN`.
    pub fn code_prefix(self) -> &'static str {
        match self {
            ModelType::Sap2000 => "SAP",
            ModelType::Revit => "RVT",
            ModelType::IdeaStatica => "IDE",
            ModelType::Robot => "ROB",
            ModelType::Etabs => "ETB",
            ModelType::Other => "MOD",
        }
    }

}

/// Detect subtype from a principal file extension (no leading dot).
pub fn detect_model_type_from_extension(ext: Option<&str>) -> ModelType {
    let Some(raw) = ext.map(|e| e.trim_start_matches('.').to_ascii_lowercase()) else {
        return ModelType::Other;
    };
    match raw.as_str() {
        "sdb" => ModelType::Sap2000,
        "rvt" | "rfa" => ModelType::Revit,
        "idea" | "ideastatica" => ModelType::IdeaStatica,
        "rtd" => ModelType::Robot,
        "edb" | "e2k" => ModelType::Etabs,
        _ => ModelType::Other,
    }
}

/// Detect from a file name (uses extension).
pub fn detect_model_type_from_file_name(file_name: &str) -> ModelType {
    let ext = std::path::Path::new(file_name)
        .extension()
        .map(|e| e.to_string_lossy().to_string());
    detect_model_type_from_extension(ext.as_deref())
}

/// Type-code used when allocating a Modelos business code.
/// Falls back to `MOD` when `model_type` is missing/unknown.
pub fn model_code_prefix(model_type: Option<&str>) -> &'static str {
    model_type
        .and_then(ModelType::parse)
        .unwrap_or(ModelType::Other)
        .code_prefix()
}

#[cfg(test)]
mod unit_tests {
    use super::*;

    #[test]
    fn prefixes_match_brief() {
        assert_eq!(ModelType::Sap2000.code_prefix(), "SAP");
        assert_eq!(ModelType::Revit.code_prefix(), "RVT");
        assert_eq!(ModelType::IdeaStatica.code_prefix(), "IDE");
        assert_eq!(ModelType::Robot.code_prefix(), "ROB");
        assert_eq!(ModelType::Etabs.code_prefix(), "ETB");
        assert_eq!(ModelType::Other.code_prefix(), "MOD");
    }

    #[test]
    fn detect_by_extension() {
        assert_eq!(
            detect_model_type_from_extension(Some("sdb")),
            ModelType::Sap2000
        );
        assert_eq!(
            detect_model_type_from_extension(Some("RVT")),
            ModelType::Revit
        );
        assert_eq!(
            detect_model_type_from_extension(Some("idea")),
            ModelType::IdeaStatica
        );
        assert_eq!(
            detect_model_type_from_extension(Some("rtd")),
            ModelType::Robot
        );
        assert_eq!(
            detect_model_type_from_extension(Some("edb")),
            ModelType::Etabs
        );
        assert_eq!(
            detect_model_type_from_extension(Some("dwg")),
            ModelType::Other
        );
        assert_eq!(detect_model_type_from_extension(None), ModelType::Other);
    }
}
