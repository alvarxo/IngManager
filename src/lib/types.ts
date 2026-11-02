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

export interface DocumentRow {
  id: string;
  project_id: string;
  category_id: string | null;
  business_code: string | null;
  title: string;
  doc_type: string | null;
  current_version_id: string | null;
  status: string;
  subcategory: string | null;
  origin: string | null;
  received_at: string | null;
  notes: string | null;
  metadata_json: string | null;
  created_at: string;
  updated_at: string;
  category_key: string | null;
  current_version_number: number | null;
  current_version_label: string | null;
  file_count: number;
  has_unregistered_change: number;
}

export interface CreateDocumentInput {
  project_id: string;
  category_key: string;
  title: string;
  status?: string | null;
  subcategory?: string | null;
  origin?: string | null;
  received_at?: string | null;
  notes?: string | null;
  assign_code?: boolean | null;
  source_absolute_path?: string | null;
}

export interface UpdateDocumentInput {
  id: string;
  title?: string | null;
  status?: string | null;
  subcategory?: string | null;
  origin?: string | null;
  received_at?: string | null;
  notes?: string | null;
}

export interface DocumentVersionRow {
  id: string;
  document_id: string;
  version_number: number;
  label: string | null;
  notes: string | null;
  created_at: string;
  snapshot_relative_path: string | null;
  registered_fingerprint: string | null;
  source_relative_path: string | null;
  is_snapshot: number;
  is_current: number;
}

export interface DocumentFileRow {
  id: string;
  document_version_id: string;
  relative_path: string;
  file_name: string;
  extension: string | null;
  size_bytes: number | null;
  modified_at: string | null;
  content_hash: string | null;
  is_available: number;
  format_label: string | null;
  revision_id: string | null;
  created_at: string;
}

export interface RevisionRow {
  id: string;
  document_id: string;
  revision_code: string;
  issued_at: string | null;
  notes: string | null;
  created_at: string;
  document_version_id: string | null;
  status: string | null;
}

export interface SubcategoryRow {
  id: string;
  project_id: string;
  category_key: string;
  name: string;
  sort_order: number;
  created_at: string;
}

export interface RelationshipRow {
  id: string;
  source_document_id: string;
  target_document_id: string;
  relation_type: string;
}

export interface CreateVersionInput {
  document_id: string;
  notes?: string | null;
  source_absolute_path?: string | null;
  from_current?: boolean | null;
}

export interface CreateRevisionInput {
  document_id: string;
  revision_code: string;
  issued_at?: string | null;
  notes?: string | null;
  document_version_id?: string | null;
  file_paths?: string[] | null;
  format_labels?: string[] | null;
}

export interface AttachFileInput {
  document_version_id: string;
  absolute_path: string;
  format_label?: string | null;
  revision_id?: string | null;
}

export interface TransmissionRow {
  id: string;
  project_id: string;
  direction: "sent" | "received" | string;
  code: string | null;
  subject: string | null;
  counterpart: string | null;
  sent_at: string | null;
  notes: string | null;
  created_at: string;
  medium: string | null;
  status: string;
  folder_relative_path: string | null;
  zip_relative_path: string | null;
  zip_checksum: string | null;
  linked_transmission_id: string | null;
  original_import_relative_path: string | null;
  extracted_relative_path: string | null;
  antecedentes_relative_path: string | null;
  item_count: number;
}

export interface TransmissionItemRow {
  id: string;
  transmission_id: string;
  document_id: string | null;
  relative_path: string | null;
  file_name: string | null;
  created_at: string;
  document_version_id: string | null;
  checksum: string | null;
  snapshot_relative_path: string | null;
  is_available: number;
}

export interface TransmissionLinkRow {
  id: string;
  sent_transmission_id: string;
  received_transmission_id: string;
  notes: string | null;
  created_at: string;
}

export interface SendItemInput {
  document_id?: string | null;
  document_version_id?: string | null;
  absolute_path: string;
}

export interface CreateSentTransmissionInput {
  project_id: string;
  counterpart: string;
  subject: string;
  medium?: string | null;
  notes?: string | null;
  sent_at?: string | null;
  items: SendItemInput[];
}

export interface PreviewSentItem {
  absolute_path: string;
  file_name: string;
  available: boolean;
  document_id: string | null;
  document_version_id: string | null;
  error: string | null;
}

export interface PreviewSentResult {
  code: string;
  folder_relative_path: string;
  items: PreviewSentItem[];
  all_available: boolean;
}

export interface ImportReceivedInput {
  project_id: string;
  counterpart: string;
  subject?: string | null;
  notes?: string | null;
  received_at?: string | null;
  source_path: string;
  extract_to_antecedentes?: boolean | null;
  link_to_sent_id?: string | null;
}
