use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Project {
    pub id: String,
    /// Optional business code (AANN or custom). Independent of physical path.
    pub code: Option<String>,
    pub name: String,
    pub description: Option<String>,
    pub client_id: Option<String>,
    pub client_name: Option<String>,
    pub status: String,
    pub authorized_root_id: String,
    /// Path relative to the authorized root — independent of `code`.
    pub root_relative_path: String,
    pub import_mode: String,
    pub layout_mode: String,
    pub start_date: Option<String>,
    pub end_date: Option<String>,
    pub is_favorite: i64,
    pub last_opened_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateProjectInput {
    pub name: String,
    pub description: Option<String>,
    pub client_name: Option<String>,
    pub start_date: Option<String>,
    pub end_date: Option<String>,
    /// Optional business code. Empty/null → no code.
    pub code: Option<String>,
    /// Physical folder name under the chosen root (defaults to code or sanitized name).
    pub folder_name: Option<String>,
    /// Authorized root id (defaults to primary).
    pub authorized_root_id: Option<String>,
    /// Create standard physical folders (default true).
    pub create_standard_folders: Option<bool>,
    /// When the physical folder already exists:
    /// `abort` (default) | `use_existing` | `rename`.
    pub folder_conflict_policy: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeleteProjectResult {
    pub id: String,
    pub code: Option<String>,
    pub files_action: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateProjectInput {
    pub id: String,
    pub name: Option<String>,
    pub description: Option<String>,
    pub client_name: Option<String>,
    pub status: Option<String>,
    pub start_date: Option<String>,
    pub end_date: Option<String>,
    pub is_favorite: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct CategorySummary {
    pub key: String,
    pub name: String,
    pub relative_path: String,
    pub sort_order: i64,
    /// Physical indexed files in the category.
    pub file_count: i64,
    /// Principal engineering models (meaningful for `modelos`).
    pub model_count: i64,
    /// Logical documents registered in the category.
    pub document_count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct IndexedFile {
    pub id: String,
    pub project_id: Option<String>,
    pub authorized_root_id: Option<String>,
    pub relative_path: String,
    pub absolute_path: String,
    pub file_name: String,
    pub extension: Option<String>,
    pub size_bytes: Option<i64>,
    pub modified_at: Option<String>,
    pub is_directory: i64,
    pub is_available: i64,
    pub category_key: Option<String>,
    pub indexed_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppStatus {
    pub root_path: Option<String>,
    pub root_valid: bool,
    pub project_count: i64,
    pub db_path: String,
    pub authorized_roots: Vec<crate::project_paths::AuthorizedRoot>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexProgress {
    pub phase: String,
    pub scanned: u64,
    pub total_hint: Option<u64>,
    pub current_path: Option<String>,
    pub done: bool,
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StandardFolder {
    pub key: &'static str,
    pub name: &'static str,
    pub relative_path: &'static str,
    pub sort_order: i32,
}

pub const STANDARD_FOLDERS: &[StandardFolder] = &[
    StandardFolder {
        key: "antecedentes",
        name: "Antecedentes",
        relative_path: "01_ANTECEDENTES",
        sort_order: 1,
    },
    StandardFolder {
        key: "modelos",
        name: "Modelos",
        relative_path: "02_MODELOS",
        sort_order: 2,
    },
    StandardFolder {
        key: "planos",
        name: "Planos",
        relative_path: "03_PLANOS",
        sort_order: 3,
    },
    StandardFolder {
        key: "memorias",
        name: "Memorias",
        relative_path: "04_MEMORIAS",
        sort_order: 4,
    },
    StandardFolder {
        key: "admin",
        name: "Administración",
        relative_path: "05_ADMINISTRACION",
        sort_order: 5,
    },
    StandardFolder {
        key: "transmisiones",
        name: "Transmisiones",
        relative_path: "06_TRANSMISIONES",
        sort_order: 6,
    },
    StandardFolder {
        key: "otros",
        name: "Otros",
        relative_path: "07_OTROS",
        sort_order: 7,
    },
];
