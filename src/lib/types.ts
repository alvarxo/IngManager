export interface AuthorizedRoot {
  id: string;
  absolute_path: string;
  label: string | null;
  is_primary: number;
  created_at: string;
}

export interface Project {
  id: string;
  code: string | null;
  name: string;
  description: string | null;
  client_id: string | null;
  client_name: string | null;
  status: "active" | "archived" | "cancelled" | "completed" | string;
  authorized_root_id: string;
  root_relative_path: string;
  import_mode: string;
  layout_mode: string;
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
  code?: string | null;
  folder_name?: string | null;
  authorized_root_id?: string | null;
  create_standard_folders?: boolean | null;
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

export interface RemoveProjectResult {
  id: string;
  code: string | null;
  root_relative_path: string;
  message: string;
}

export interface CategorySummary {
  key: string;
  name: string;
  relative_path: string;
  sort_order: number;
  file_count: number;
  model_count: number;
  document_count: number;
}

export interface IndexedFile {
  id: string;
  project_id: string | null;
  authorized_root_id?: string | null;
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
  authorized_roots: AuthorizedRoot[];
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

export type ExternalDropMode = "copy" | "move";

export interface ExternalDropItemPreview {
  source_path: string;
  file_name: string;
  is_directory: boolean;
  size_bytes: number | null;
  conflict: boolean;
  planned_name: string;
  is_engineering: boolean;
}

export interface ExternalDropPreview {
  dest_dir: string;
  items: ExternalDropItemPreview[];
  conflict_count: number;
  engineering_warning: string | null;
}

export interface ExternalDropProgress {
  phase: string;
  current: number;
  total: number;
  current_path: string | null;
  done: boolean;
  message: string | null;
}

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
  current_revision_code: string | null;
  current_revision_status: string | null;
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
  file_role?: string | null;
  /** `calc` | `export` when associated to a Model document. */
  sheet_kind?: string | null;
  /** `copy_on_version` | `version_bound`. */
  version_policy?: string | null;
}

export type ModelSheetKind = "calc" | "export";

export interface AssociateModelSpreadsheetInput {
  document_version_id: string;
  absolute_path: string;
  sheet_kind: ModelSheetKind | string;
  version_policy?: string | null;
}

export interface ModelSpreadsheetStatus {
  document_id: string;
  document_version_id: string;
  is_current: boolean;
  has_live_calc: boolean;
  has_export: boolean;
  export_pending: boolean;
  calc_file_id: string | null;
  export_file_ids: string[];
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
  notes: string | null;
  source_title: string | null;
  source_code: string | null;
  target_title: string | null;
  target_code: string | null;
  source_version_label: string | null;
  target_version_label: string | null;
}

export interface CreateRelationshipInput {
  source_document_id: string;
  target_document_id: string;
  relation_type: string;
  notes?: string | null;
}

export interface SearchHit {
  entity_type: string;
  entity_id: string;
  project_id: string;
  title: string;
  code: string;
  body: string;
  file_name: string;
  rank: number;
}

export interface AuditEntry {
  id: string;
  entity_type: string | null;
  entity_id: string | null;
  action: string;
  detail: string | null;
  created_at: string;
  project_id: string | null;
}

export interface BackupResult {
  backup_dir: string;
  db_snapshot: string;
  manifest_path: string;
  zip_path: string | null;
  project_count: number;
}

export interface CategoryMappingPreview {
  key: string;
  name: string;
  relative_path: string;
  matched: boolean;
}

export interface DetectedFolder {
  absolute_path: string;
  name: string;
  suggested_code: string | null;
  suggested_name: string;
  matched_standard_folders: string[];
  looks_like_project: boolean;
  conflict_with_existing: boolean;
  under_authorized_root: boolean;
  authorized_root_id: string | null;
  relative_to_root: string | null;
  category_mappings: CategoryMappingPreview[];
}

export interface ImportPreview {
  source_path: string;
  mode: string;
  target_relative_path: string;
  target_absolute_path: string;
  authorized_root_id: string | null;
  will_overwrite: boolean;
  will_copy: boolean;
  link_in_place: boolean;
  folders_to_create: string[];
  category_mappings: CategoryMappingPreview[];
  file_count_hint: number;
  requires_authorize_root: boolean;
  suggested_authorize_path: string | null;
}

export type ImportMode = "link" | "copy" | "reorganize_later";

export interface ImportProjectInput {
  source_path: string;
  name: string;
  code?: string | null;
  description?: string | null;
  client_name?: string | null;
  mode?: ImportMode | string | null;
  folder_name?: string | null;
  authorized_root_id?: string | null;
  confirm_overwrite?: boolean | null;
  authorize_parent_path?: string | null;
  create_missing_folders?: boolean | null;
}

export interface ImportProjectResult {
  project_id: string;
  code: string | null;
  target_path: string;
  mode: string;
  copied: boolean;
  linked: boolean;
}

export interface CreateVersionInput {
  document_id: string;
  notes?: string | null;
  /** Link existing file as new version (no copy). */
  source_absolute_path?: string | null;
  from_current?: boolean | null;
}

export interface CreateNextPhysicalVersionInput {
  document_id: string;
  notes?: string | null;
  source_absolute_path?: string | null;
}

export interface NextPhysicalVersionPreview {
  source_absolute_path: string;
  destination_absolute_path: string;
  destination_file_name: string;
  next_version_number: number;
}

export interface RepairVersionFileInput {
  document_version_id: string;
  absolute_path: string;
  replace_file_id?: string | null;
  format_label?: string | null;
}

export interface CreateRevisionInput {
  document_id: string;
  revision_code: string;
  issued_at?: string | null;
  notes?: string | null;
  document_version_id?: string | null;
  file_paths?: string[] | null;
  format_labels?: string[] | null;
  /** Default true: snapshot currently linked working formats. */
  freeze_linked?: boolean | null;
}

export interface AttachFileInput {
  document_version_id: string;
  absolute_path: string;
  format_label?: string | null;
  revision_id?: string | null;
  /** Default false: link working file. True: copy into snapshots. */
  as_snapshot?: boolean | null;
  file_role?: string | null;
}

export type IncorporationMode =
  | "associate_existing"
  | "register_new"
  | "physical_only";

export type PhysicalOp = "none" | "copy" | "move";

export interface IncorporationFileProposal {
  absolute_path: string;
  file_name: string;
  extension: string | null;
  size_bytes: number | null;
  role: string;
  already_linked_document_id: string | null;
  already_linked_code: string | null;
  under_project: boolean;
  relative_path: string | null;
}

export interface ExistingDocMatch {
  document_id: string;
  business_code: string | null;
  title: string;
  file_count: number;
  score: number;
}

export interface IncorporationGroupProposal {
  group_key: string;
  title: string;
  suggested_mode: IncorporationMode;
  mode: IncorporationMode;
  files: IncorporationFileProposal[];
  suggested_document_id: string | null;
  document_id: string | null;
  matches: ExistingDocMatch[];
  notes: string | null;
}

export interface IncorporationPreview {
  project_id: string;
  category_key: string;
  dest_absolute_path: string | null;
  physical_op: PhysicalOp;
  groups: IncorporationGroupProposal[];
  warning: string | null;
}

export interface IncorporationPreviewInput {
  project_id: string;
  category_key: string;
  absolute_paths: string[];
  dest_absolute_path?: string | null;
  physical_op?: PhysicalOp | null;
}

export interface IncorporationGroupDecision {
  group_key: string;
  mode: IncorporationMode;
  document_id?: string | null;
  title?: string | null;
  assign_code?: boolean | null;
}

export interface IncorporationApplyInput {
  project_id: string;
  category_key: string;
  absolute_paths: string[];
  dest_absolute_path?: string | null;
  physical_op?: PhysicalOp | null;
  decisions: IncorporationGroupDecision[];
}

export interface IncorporationGroupResult {
  group_key: string;
  mode: IncorporationMode;
  document_id: string | null;
  business_code: string | null;
  linked_files: number;
  skipped_files: number;
  physical_destinations: string[];
  message: string | null;
}

export interface IncorporationApplyResult {
  groups: IncorporationGroupResult[];
  created_documents: number;
  associated_documents: number;
  physical_only: number;
  warning: string | null;
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

export interface ClassifiedFolder {
  category_key: string;
  category_name: string;
  physical_name: string;
  matched: boolean;
  standard_target: string;
}

export interface OrganizerScanItem {
  project_id: string | null;
  project_name: string | null;
  folder_name: string;
  absolute_path: string;
  classified: ClassifiedFolder[];
  unmatched_folders: string[];
}

export interface OrganizerSession {
  id: string;
  authorized_root_id: string | null;
  scope_absolute_path: string;
  mode: string;
  status: string;
  backup_absolute_path: string | null;
  notes: string | null;
  created_at: string;
  applied_at: string | null;
}

export interface OrganizerPlanItem {
  id: string;
  session_id: string;
  project_id: string | null;
  project_folder_name: string;
  category_key: string | null;
  source_relative_path: string;
  target_relative_path: string | null;
  action: string;
  confidence: number;
  status: string;
  detail: string | null;
  sort_order: number;
  created_at: string;
}

export interface OrganizerPlanResult {
  session: OrganizerSession;
  items: OrganizerPlanItem[];
}

export interface OrganizerApplyResult {
  session_id: string;
  applied: number;
  failed: number;
  skipped: number;
  backup_path: string | null;
  journal_entries: number;
}

export interface InboxItem {
  id: string;
  absolute_path: string;
  file_name: string;
  extension: string | null;
  size_bytes: number | null;
  modified_at: string | null;
  staged_at: string;
  status: string;
  notes: string | null;
}

export interface InboxAssignInput {
  item_ids: string[];
  project_ids: string[];
  category_key: string;
  mode?: string | null;
}

export interface InboxAssignResult {
  assigned: number;
  failures: string[];
  dest_paths: string[];
}

export interface PendingVersionEvent {
  id: string;
  project_id: string;
  absolute_path: string;
  file_name: string;
  base_stem: string;
  version_hint: string | null;
  reason: string;
  candidate_document_ids_json: string | null;
  status: string;
  resolved_document_id: string | null;
  created_at: string;
  resolved_at: string | null;
}

export interface AutoVersionResult {
  path: string;
  action: string;
  document_id: string | null;
  version_id: string | null;
  pending_id: string | null;
  message: string;
}

export interface FolderMappingRow {
  id: string;
  category_key: string;
  physical_relative_path: string;
  mapping_mode: string;
}

export interface EngineeringGroupDto {
  group_key: string;
  title: string;
  principal_file_name: string | null;
  principal_absolute_path: string | null;
  version_file_names: string[];
  auxiliary_count: number;
  auxiliary_names: string[];
  standalone_names: string[];
  already_registered: boolean;
  document_id: string | null;
}

export interface AvailabilityCleanupResult {
  marked_unavailable: number;
  deleted_empty_documents: number;
  document_ids_removed: string[];
}

export interface DocumentOpenFormats {
  document_id: string;
  formats: string[];
  preferred_format: string | null;
  needs_picker: boolean;
}

export type LogicalTreeExpandPolicy = "remember" | "collapsed" | "expanded";

export interface BulkRegisterInput {
  project_id: string;
  category_key: string;
  group_keys?: string[] | null;
  assign_codes?: boolean | null;
}

export interface BulkRegisterResult {
  created: number;
  skipped: number;
  versions_added: number;
  document_ids: string[];
}

export interface SyncModelVersionsInput {
  project_id: string;
  category_key?: string | null;
  document_id?: string | null;
}

export interface UnifySuggestion {
  group_key: string;
  document_ids: string[];
  document_labels: string[];
  file_names: string[];
}

export interface SyncModelVersionsResult {
  documents_touched: number;
  versions_added: number;
  already_linked: number;
  document_ids: string[];
  unify_suggestions: UnifySuggestion[];
}

export interface AssociateModelVersionInput {
  document_id: string;
  absolute_path: string;
  notes?: string | null;
}

export interface UnifyModelsInput {
  project_id: string;
  principal_document_id: string;
  duplicate_document_ids: string[];
  category_key?: string | null;
}

export interface UnifyDocSummary {
  document_id: string;
  business_code: string | null;
  title: string;
  version_count: number;
  file_count: number;
}

export interface UnifyVersionPlan {
  source_document_id: string;
  source_version_id: string;
  file_name: string;
  relative_path: string;
  version_hint: string | null;
  new_version_number: number;
  new_label: string;
  sheet_count: number;
  auxiliary_count: number;
}

export interface UnifyModelsPreview {
  principal: UnifyDocSummary;
  duplicates: UnifyDocSummary[];
  planned_versions: UnifyVersionPlan[];
  relations_to_rewire: number;
  sheets_to_transfer: number;
  duplicates_to_delete: string[];
  group_key: string | null;
  warnings: string[];
}

export interface UnifyModelsResult {
  principal_document_id: string;
  versions_on_principal: number;
  files_transferred: number;
  sheets_transferred: number;
  relations_rewired: number;
  duplicates_deleted: string[];
  db_backup_path: string;
  group_key: string | null;
}

export interface RepairModelHistoryInput {
  project_id: string;
  document_id?: string | null;
}

export interface RepairVersionDecision {
  version_id: string;
  version_number: number;
  label: string | null;
  keep: boolean;
  reason: string;
  principal_file_name: string | null;
  principal_extension: string | null;
  relative_path: string | null;
  new_version_number: number | null;
}

export interface RepairModelHistoryDocPreview {
  document_id: string;
  business_code: string | null;
  title: string;
  versions_before: number;
  versions_after: number;
  decisions: RepairVersionDecision[];
  new_current_version_id: string | null;
}

export interface RepairModelHistoryPreview {
  project_id: string;
  documents: RepairModelHistoryDocPreview[];
  total_versions_before: number;
  total_versions_after: number;
  versions_to_drop: number;
  warnings: string[];
}

export interface RepairModelHistoryResult {
  project_id: string;
  documents_repaired: number;
  versions_kept: number;
  versions_removed: number;
  db_backup_path: string;
  documents: RepairModelHistoryDocPreview[];
}

export interface UpdatePlanItemInput {
  item_id: string;
  action?: string | null;
  category_key?: string | null;
  target_relative_path?: string | null;
  status?: string | null;
  detail?: string | null;
}
