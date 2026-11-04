import { invoke } from "@tauri-apps/api/core";
import type {
  AppStatus,
  AttachFileInput,
  CategorySummary,
  ConflictPolicy,
  ExternalDropMode,
  ExternalDropPreview,
  CreateDocumentInput,
  CreateProjectInput,
  CreateRevisionInput,
  CreateSentTransmissionInput,
  CreateNextPhysicalVersionInput,
  CreateVersionInput,
  DocumentFileRow,
  NextPhysicalVersionPreview,
  RepairVersionFileInput,
  DocumentRow,
  DocumentVersionRow,
  EntryProperties,
  FileOpResult,
  FsEntry,
  ImportReceivedInput,
  IndexedFile,
  PreviewSentResult,
  Project,
  RemoveProjectResult,
  AuditEntry,
  AuthorizedRoot,
  BackupResult,
  CreateRelationshipInput,
  DetectedFolder,
  ImportPreview,
  ImportProjectInput,
  ImportProjectResult,
  RelationshipRow,
  RevisionRow,
  SearchHit,
  SubcategoryRow,
  TransmissionItemRow,
  TransmissionLinkRow,
  TransmissionRow,
  TrashItem,
  UpdateDocumentInput,
  UpdateProjectInput,
  OrganizerScanItem,
  OrganizerPlanResult,
  OrganizerApplyResult,
  OrganizerSession,
  InboxItem,
  InboxAssignInput,
  InboxAssignResult,
  PendingVersionEvent,
  AutoVersionResult,
  FolderMappingRow,
  EngineeringGroupDto,
  BulkRegisterInput,
  BulkRegisterResult,
  SyncModelVersionsInput,
  SyncModelVersionsResult,
  AssociateModelVersionInput,
  AvailabilityCleanupResult,
  DocumentOpenFormats,
  LogicalTreeExpandPolicy,
  UpdatePlanItemInput,
  OrganizerPlanItem,
  IncorporationPreviewInput,
  IncorporationPreview,
  IncorporationApplyInput,
  IncorporationApplyResult,
} from "./types";

export const api = {
  getAppStatus: () => invoke<AppStatus>("get_app_status"),
  setRootPath: (path: string) => invoke<AppStatus>("set_root_path", { path }),
  getRootPath: () => invoke<string | null>("get_root_path"),
  listAuthorizedRoots: () =>
    invoke<AuthorizedRoot[]>("list_authorized_roots"),
  addAuthorizedRoot: (
    path: string,
    label?: string | null,
    makePrimary?: boolean | null,
  ) =>
    invoke<AuthorizedRoot>("add_authorized_root", {
      path,
      label: label ?? null,
      makePrimary: makePrimary ?? null,
    }),
  removeAuthorizedRoot: (rootId: string) =>
    invoke<void>("remove_authorized_root", { rootId }),

  previewNextProjectCode: () => invoke<string>("preview_next_project_code"),
  listProjects: (includeArchived = false) =>
    invoke<Project[]>("list_projects", { includeArchived }),
  getProject: (id: string) => invoke<Project>("get_project", { id }),
  createProject: (input: CreateProjectInput) =>
    invoke<Project>("create_project", { input }),
  updateProject: (input: UpdateProjectInput) =>
    invoke<Project>("update_project", { input }),
  archiveProject: (id: string) => invoke<Project>("archive_project", { id }),
  openProject: (id: string) => invoke<Project>("open_project", { id }),
  removeProjectRegistration: (id: string, confirmCode: string) =>
    invoke<RemoveProjectResult>("remove_project_registration", {
      id,
      confirmCode,
    }),
  getProjectAbsolutePath: (projectId: string) =>
    invoke<string>("get_project_absolute_path", { projectId }),
  listProjectCategories: (projectId: string) =>
    invoke<CategorySummary[]>("list_project_categories", { projectId }),
  listRecentProjects: (limit = 8) =>
    invoke<Project[]>("list_recent_projects", { limit }),
  listFavoriteProjects: () => invoke<Project[]>("list_favorite_projects"),

  listCategoryFiles: (projectId: string, categoryKey: string) =>
    invoke<IndexedFile[]>("list_category_files", { projectId, categoryKey }),
  openFile: (absolutePath: string) =>
    invoke<void>("open_file", { absolutePath }),
  revealInExplorer: (absolutePath: string) =>
    invoke<void>("reveal_in_explorer", { absolutePath }),
  openProjectFolder: (projectId: string) =>
    invoke<void>("open_project_folder", { projectId }),

  startInitialIndex: () => invoke<void>("start_initial_index"),
  reindexRoot: () => invoke<void>("reindex_root"),
  startFileWatcher: () => invoke<void>("start_file_watcher"),

  listDirectory: (absolutePath: string) =>
    invoke<FsEntry[]>("list_directory", { absolutePath }),
  resolveProjectCategoryPath: (
    projectId: string,
    categoryKey: string,
    relativeSubpath?: string | null,
  ) =>
    invoke<FsEntry>("resolve_project_category_path", {
      projectId,
      categoryKey,
      relativeSubpath: relativeSubpath ?? null,
    }),
  createFolder: (parentPath: string, name: string) =>
    invoke<FileOpResult>("create_folder", { parentPath, name }),
  renameEntry: (
    absolutePath: string,
    newName: string,
    acknowledgeEngineering = false,
  ) =>
    invoke<FileOpResult>("rename_entry", {
      absolutePath,
      newName,
      acknowledgeEngineering,
    }),
  copyEntries: (
    sources: string[],
    destDir: string,
    conflictPolicy: ConflictPolicy = "rename",
  ) =>
    invoke<FileOpResult>("copy_entries", {
      sources,
      destDir,
      conflictPolicy,
    }),
  moveEntries: (
    sources: string[],
    destDir: string,
    conflictPolicy: ConflictPolicy = "rename",
    acknowledgeEngineering = false,
  ) =>
    invoke<FileOpResult>("move_entries", {
      sources,
      destDir,
      conflictPolicy,
      acknowledgeEngineering,
    }),
  importFiles: (
    sourcePaths: string[],
    destDir: string,
    conflictPolicy: ConflictPolicy = "rename",
  ) =>
    invoke<FileOpResult>("import_files", {
      sourcePaths,
      destDir,
      conflictPolicy,
    }),
  previewExternalDrop: (
    sourcePaths: string[],
    destDir: string,
    conflictPolicy: ConflictPolicy = "rename",
  ) =>
    invoke<ExternalDropPreview>("preview_external_drop", {
      sourcePaths,
      destDir,
      conflictPolicy,
    }),
  applyExternalDrop: (
    sourcePaths: string[],
    destDir: string,
    mode: ExternalDropMode = "copy",
    conflictPolicy: ConflictPolicy = "rename",
  ) =>
    invoke<FileOpResult>("apply_external_drop", {
      sourcePaths,
      destDir,
      mode,
      conflictPolicy,
    }),
  deleteToTrash: (paths: string[], acknowledgeEngineering = false) =>
    invoke<FileOpResult>("delete_to_trash", {
      paths,
      acknowledgeEngineering,
    }),
  listTrash: () => invoke<TrashItem[]>("list_trash"),
  restoreTrashItem: (
    trashId: string,
    conflictPolicy: ConflictPolicy = "rename",
  ) =>
    invoke<FileOpResult>("restore_trash_item", { trashId, conflictPolicy }),
  purgeTrashItem: (trashId: string) =>
    invoke<void>("purge_trash_item", { trashId }),
  getEntryProperties: (absolutePath: string) =>
    invoke<EntryProperties>("get_entry_properties", { absolutePath }),
  openPath: (absolutePath: string) =>
    invoke<void>("open_path", { absolutePath }),

  listDocuments: (
    projectId: string,
    categoryKey?: string | null,
    currentOnly = true,
  ) =>
    invoke<DocumentRow[]>("list_documents", {
      projectId,
      categoryKey: categoryKey ?? null,
      currentOnly,
    }),
  getDocument: (id: string) => invoke<DocumentRow>("get_document", { id }),
  previewNextDocumentCode: (projectId: string, categoryKey: string) =>
    invoke<string | null>("preview_next_document_code", {
      projectId,
      categoryKey,
    }),
  createDocument: (input: CreateDocumentInput) =>
    invoke<DocumentRow>("create_document", { input }),
  updateDocument: (input: UpdateDocumentInput) =>
    invoke<DocumentRow>("update_document", { input }),
  attachFileToVersion: (input: AttachFileInput) =>
    invoke<DocumentFileRow>("attach_file_to_version", { input }),
  previewDocumentIncorporation: (input: IncorporationPreviewInput) =>
    invoke<IncorporationPreview>("preview_document_incorporation", { input }),
  applyDocumentIncorporation: (input: IncorporationApplyInput) =>
    invoke<IncorporationApplyResult>("apply_document_incorporation", { input }),
  listDocumentVersions: (documentId: string) =>
    invoke<DocumentVersionRow[]>("list_document_versions", { documentId }),
  listVersionFiles: (versionId: string) =>
    invoke<DocumentFileRow[]>("list_version_files", { versionId }),
  createDocumentVersion: (input: CreateVersionInput) =>
    invoke<DocumentVersionRow>("create_document_version", { input }),
  previewNextPhysicalVersion: (
    documentId: string,
    sourceAbsolutePath?: string | null,
  ) =>
    invoke<NextPhysicalVersionPreview>("preview_next_physical_version", {
      documentId,
      sourceAbsolutePath: sourceAbsolutePath ?? null,
    }),
  createNextPhysicalVersion: (input: CreateNextPhysicalVersionInput) =>
    invoke<DocumentVersionRow>("create_next_physical_version", { input }),
  repairVersionFile: (input: RepairVersionFileInput) =>
    invoke<DocumentFileRow>("repair_version_file", { input }),
  restoreDocumentVersion: (
    documentId: string,
    versionId: string,
    notes?: string | null,
  ) =>
    invoke<DocumentVersionRow>("restore_document_version", {
      documentId,
      versionId,
      notes: notes ?? null,
    }),
  deleteLogicalDocument: (documentId: string) =>
    invoke<void>("delete_logical_document", { documentId }),
  cleanupDocumentAvailability: (
    projectId: string,
    deleteEmptyLogicals = true,
  ) =>
    invoke<AvailabilityCleanupResult>("cleanup_document_availability", {
      projectId,
      deleteEmptyLogicals,
    }),
  openDocumentFile: (fileId: string, useTempCopy?: boolean | null) =>
    invoke<string>("open_document_file", {
      fileId,
      useTempCopy: useTempCopy ?? null,
    }),
  openDocumentPrincipal: (
    documentId: string,
    useTempCopy?: boolean | null,
  ) =>
    invoke<string>("open_document_principal", {
      documentId,
      useTempCopy: useTempCopy ?? null,
    }),
  listDocumentOpenFormats: (documentId: string) =>
    invoke<DocumentOpenFormats>("list_document_open_formats", { documentId }),
  getDocumentOpenPreference: (documentId: string) =>
    invoke<string | null>("get_document_open_preference", { documentId }),
  setDocumentOpenPreference: (documentId: string, format?: string | null) =>
    invoke<string | null>("set_document_open_preference", {
      documentId,
      format: format ?? null,
    }),
  openDocumentWithFormat: (
    documentId: string,
    format: string,
    remember = false,
    useTempCopy?: boolean | null,
  ) =>
    invoke<string>("open_document_with_format", {
      documentId,
      format,
      remember,
      useTempCopy: useTempCopy ?? null,
    }),
  listRevisionFiles: (revisionId: string) =>
    invoke<DocumentFileRow[]>("list_revision_files", { revisionId }),
  getAppSetting: (key: string) =>
    invoke<string | null>("get_app_setting", { key }),
  setAppSetting: (key: string, value: string) =>
    invoke<void>("set_app_setting", { key, value }),
  getLogicalTreeExpandPolicy: () =>
    invoke<LogicalTreeExpandPolicy>("get_logical_tree_expand_policy"),
  setLogicalTreeExpandPolicy: (policy: LogicalTreeExpandPolicy | string) =>
    invoke<LogicalTreeExpandPolicy>("set_logical_tree_expand_policy", {
      policy,
    }),
  getLogicalTreeExpandState: (projectId: string) =>
    invoke<Record<string, boolean>>("get_logical_tree_expand_state", {
      projectId,
    }),
  setLogicalTreeExpandState: (
    projectId: string,
    stateJson: Record<string, boolean>,
  ) =>
    invoke<void>("set_logical_tree_expand_state", {
      projectId,
      stateJson,
    }),
  listRevisions: (documentId: string) =>
    invoke<RevisionRow[]>("list_revisions", { documentId }),
  createRevision: (input: CreateRevisionInput) =>
    invoke<RevisionRow>("create_revision", { input }),
  repairDocumentRevisions: (documentId: string) =>
    invoke<{
      document_id: string;
      revisions_created: number;
      files_reassigned: number;
      empty_versions_removed: number;
      current_revision_code: string | null;
    }>("repair_document_revisions", {
      input: { document_id: documentId },
    }),
  listSubcategories: (projectId: string, categoryKey: string) =>
    invoke<SubcategoryRow[]>("list_subcategories", { projectId, categoryKey }),
  createSubcategory: (
    projectId: string,
    categoryKey: string,
    name: string,
  ) =>
    invoke<SubcategoryRow>("create_subcategory", {
      projectId,
      categoryKey,
      name,
    }),
  updateSubcategory: (id: string, name: string) =>
    invoke<SubcategoryRow>("update_subcategory", { id, name }),
  deleteSubcategory: (id: string) =>
    invoke<void>("delete_subcategory", { id }),
  listDocumentRelationships: (documentId: string) =>
    invoke<RelationshipRow[]>("list_document_relationships", { documentId }),
  createDocumentRelationship: (input: CreateRelationshipInput) =>
    invoke<RelationshipRow>("create_document_relationship", { input }),
  deleteDocumentRelationship: (relationshipId: string) =>
    invoke<void>("delete_document_relationship", { relationshipId }),
  setDocumentFavorite: (documentId: string, favorite: boolean) =>
    invoke<void>("set_document_favorite", { documentId, favorite }),
  listFavoriteDocuments: (projectId?: string | null) =>
    invoke<DocumentRow[]>("list_favorite_documents", {
      projectId: projectId ?? null,
    }),
  isDocumentFavorite: (documentId: string) =>
    invoke<boolean>("is_document_favorite", { documentId }),

  globalSearch: (query: string, limit?: number | null) =>
    invoke<SearchHit[]>("global_search", {
      query,
      limit: limit ?? null,
    }),
  rebuildSearchIndex: () => invoke<number>("rebuild_search_index"),

  listProjectAudit: (projectId: string, limit?: number | null) =>
    invoke<AuditEntry[]>("list_project_audit", {
      projectId,
      limit: limit ?? null,
    }),

  createPortableBackup: (
    destinationDir: string,
    includeFiles?: boolean | null,
    projectIds?: string[] | null,
  ) =>
    invoke<BackupResult>("create_portable_backup", {
      destinationDir,
      includeFiles: includeFiles ?? null,
      projectIds: projectIds ?? null,
    }),
  listBackupCandidates: (path: string) =>
    invoke<string[]>("list_backup_candidates", { path }),

  detectImportableProjects: (sourcePath: string) =>
    invoke<DetectedFolder[]>("detect_importable_projects", { sourcePath }),
  previewImportProject: (
    sourcePath: string,
    code?: string | null,
    mode?: string | null,
    folderName?: string | null,
    authorizedRootId?: string | null,
  ) =>
    invoke<ImportPreview>("preview_import_project", {
      sourcePath,
      code: code ?? null,
      mode: mode ?? null,
      folderName: folderName ?? null,
      authorizedRootId: authorizedRootId ?? null,
    }),
  importExistingProject: (input: ImportProjectInput) =>
    invoke<ImportProjectResult>("import_existing_project", { input }),
  proposeImportCode: (preferred?: string | null) =>
    invoke<string | null>("propose_import_code", {
      preferred: preferred ?? null,
    }),

  listTransmissions: (projectId: string, direction?: string | null) =>
    invoke<TransmissionRow[]>("list_transmissions", {
      projectId,
      direction: direction ?? null,
    }),
  getTransmission: (id: string) =>
    invoke<TransmissionRow>("get_transmission", { id }),
  listTransmissionItems: (transmissionId: string) =>
    invoke<TransmissionItemRow[]>("list_transmission_items", {
      transmissionId,
    }),
  previewNextTransmissionCode: (projectId: string, kind: string) =>
    invoke<string>("preview_next_transmission_code", { projectId, kind }),
  previewSentTransmission: (input: CreateSentTransmissionInput) =>
    invoke<PreviewSentResult>("preview_sent_transmission", { input }),
  createSentTransmission: (input: CreateSentTransmissionInput) =>
    invoke<TransmissionRow>("create_sent_transmission", { input }),
  importReceivedTransmission: (input: ImportReceivedInput) =>
    invoke<TransmissionRow>("import_received_transmission", { input }),
  linkTransmissions: (
    sentId: string,
    receivedId: string,
    notes?: string | null,
  ) =>
    invoke<TransmissionLinkRow>("link_transmissions", {
      sentId,
      receivedId,
      notes: notes ?? null,
    }),
  listTransmissionLinks: (transmissionId: string) =>
    invoke<TransmissionLinkRow[]>("list_transmission_links", {
      transmissionId,
    }),
  openTransmissionFolder: (transmissionId: string) =>
    invoke<void>("open_transmission_folder", { transmissionId }),

  organizerScan: (scopeAbsolutePath: string) =>
    invoke<OrganizerScanItem[]>("organizer_scan", { scopeAbsolutePath }),
  organizerBuildPlan: (
    scopeAbsolutePath: string,
    mode: string,
    projectIds?: string[] | null,
  ) =>
    invoke<OrganizerPlanResult>("organizer_build_plan", {
      scopeAbsolutePath,
      mode,
      projectIds: projectIds ?? null,
    }),
  organizerGetPlan: (sessionId: string) =>
    invoke<OrganizerPlanResult>("organizer_get_plan", { sessionId }),
  organizerApplyPlan: (sessionId: string) =>
    invoke<OrganizerApplyResult>("organizer_apply_plan", { sessionId }),
  organizerUpdatePlanItem: (input: UpdatePlanItemInput) =>
    invoke<OrganizerPlanItem>("organizer_update_plan_item", { input }),
  organizerListSessions: () =>
    invoke<OrganizerSession[]>("organizer_list_sessions"),
  listProjectFolderMappings: (projectId: string) =>
    invoke<FolderMappingRow[]>("list_project_folder_mappings", { projectId }),

  listEngineeringGroups: (
    projectId: string,
    categoryKey: string,
    includeAuxiliaries = true,
  ) =>
    invoke<EngineeringGroupDto[]>("list_engineering_groups", {
      projectId,
      categoryKey,
      includeAuxiliaries,
    }),
  listCategoryFilesFiltered: (
    projectId: string,
    categoryKey: string,
    showAuxiliaries = false,
  ) =>
    invoke<IndexedFile[]>("list_category_files_filtered", {
      projectId,
      categoryKey,
      showAuxiliaries,
    }),
  bulkRegisterEngineeringDocs: (input: BulkRegisterInput) =>
    invoke<BulkRegisterResult>("bulk_register_engineering_docs", { input }),
  syncModelVersions: (input: SyncModelVersionsInput) =>
    invoke<SyncModelVersionsResult>("sync_model_versions", { input }),
  associateModelVersion: (input: AssociateModelVersionInput) =>
    invoke<DocumentVersionRow>("associate_model_version", { input }),

  inboxEnsureDir: () => invoke<string>("inbox_ensure_dir"),
  inboxList: () => invoke<InboxItem[]>("inbox_list"),
  inboxStageFiles: (paths: string[], copyInto = true) =>
    invoke<InboxItem[]>("inbox_stage_files", { paths, copyInto }),
  inboxRemoveItems: (ids: string[], deleteFiles = true) =>
    invoke<number>("inbox_remove_items", { ids, deleteFiles }),
  inboxAssign: (input: InboxAssignInput) =>
    invoke<InboxAssignResult>("inbox_assign", { input }),

  listPendingVersions: (projectId?: string | null) =>
    invoke<PendingVersionEvent[]>("list_pending_versions", {
      projectId: projectId ?? null,
    }),
  resolvePendingVersion: (pendingId: string, documentId: string) =>
    invoke<AutoVersionResult>("resolve_pending_version", {
      pendingId,
      documentId,
    }),
  dismissPendingVersion: (pendingId: string) =>
    invoke<void>("dismiss_pending_version", { pendingId }),
  reconcileProjectVersions: (projectId: string) =>
    invoke<AutoVersionResult[]>("reconcile_project_versions", { projectId }),
  registerFileVersionBaseline: (
    projectId: string,
    documentId: string,
    absolutePath: string,
    versionId?: string | null,
  ) =>
    invoke<void>("register_file_version_baseline", {
      projectId,
      documentId,
      versionId: versionId ?? null,
      absolutePath,
    }),
};
