export interface Project {
  id: string;
  code: string;
  name: string;
  description: string | null;
  client_id: string | null;
  client_name: string | null;
  status: "active" | "archived" | "cancelled" | "completed" | string;
  root_relative_path: string;
  start_date: string | null;
  end_date: string | null;
  is_favorite: number;
  last_opened_at: string | null;
  created_at: string;
  updated_at: string;
}

export interface CreateProjectInput {
  name: string;
  description?: string | null;
  client_name?: string | null;
  start_date?: string | null;
  end_date?: string | null;
}

export interface UpdateProjectInput {
  id: string;
  name?: string | null;
  description?: string | null;
  client_name?: string | null;
  status?: string | null;
  start_date?: string | null;
  end_date?: string | null;
  is_favorite?: boolean | null;
}

export interface CategorySummary {
  key: string;
  name: string;
  relative_path: string;
  sort_order: number;
  file_count: number;
}

export interface IndexedFile {
  id: string;
  project_id: string | null;
  relative_path: string;
  absolute_path: string;
  file_name: string;
  extension: string | null;
  size_bytes: number | null;
  modified_at: string | null;
  is_directory: number;
  is_available: number;
  category_key: string | null;
  indexed_at: string;
}

export interface AppStatus {
  root_path: string | null;
  root_valid: boolean;
  project_count: number;
  db_path: string;
}

export interface IndexProgress {
  phase: string;
  scanned: number;
  total_hint: number | null;
  current_path: string | null;
  done: boolean;
  message: string | null;
}

export interface FsEntry {
  name: string;
  absolute_path: string;
  relative_path: string;
  is_directory: boolean;
  extension: string | null;
  size_bytes: number | null;
  modified_at: string | null;
  is_available: boolean;
  is_engineering: boolean;
  category_key: string | null;
}

export interface FileOpResult {
  paths: string[];
  warning: string | null;
  message: string | null;
}

export type ConflictPolicy = "rename" | "overwrite" | "abort";

export interface EntryProperties {
  name: string;
  absolute_path: string;
  relative_path: string;
  is_directory: boolean;
  extension: string | null;
  size_bytes: number | null;
  modified_at: string | null;
  created_at: string | null;
  is_engineering: boolean;
  readonly: boolean;
}

export interface TrashItem {
  id: string;
  project_id: string | null;
  original_path: string;
  trash_path: string;
  deleted_at: string;
  purge_after: string | null;
  name: string | null;
  is_directory: number | null;
}
