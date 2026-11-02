import { invoke } from "@tauri-apps/api/core";
import type {
  AppStatus,
  AttachFileInput,
  CategorySummary,
  ConflictPolicy,
  CreateDocumentInput,
  CreateProjectInput,
  CreateRevisionInput,
  CreateVersionInput,
  DocumentFileRow,
  DocumentRow,
  DocumentVersionRow,
  EntryProperties,
  FileOpResult,
  FsEntry,
  IndexedFile,
  Project,
  RelationshipRow,
  RevisionRow,
  SubcategoryRow,
  TrashItem,
  UpdateDocumentInput,
  UpdateProjectInput,
} from "./types";

export const api = {
  getAppStatus: () => invoke<AppStatus>("get_app_status"),
  setRootPath: (path: string) => invoke<AppStatus>("set_root_path", { path }),
  getRootPath: () => invoke<string | null>("get_root_path"),

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
  listDocumentVersions: (documentId: string) =>
    invoke<DocumentVersionRow[]>("list_document_versions", { documentId }),
  listVersionFiles: (versionId: string) =>
    invoke<DocumentFileRow[]>("list_version_files", { versionId }),
  createDocumentVersion: (input: CreateVersionInput) =>
    invoke<DocumentVersionRow>("create_document_version", { input }),
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
  openDocumentFile: (fileId: string, useTempCopy?: boolean | null) =>
    invoke<string>("open_document_file", {
      fileId,
      useTempCopy: useTempCopy ?? null,
    }),
  listRevisions: (documentId: string) =>
    invoke<RevisionRow[]>("list_revisions", { documentId }),
  createRevision: (input: CreateRevisionInput) =>
    invoke<RevisionRow>("create_revision", { input }),
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
  listDocumentRelationships: (documentId: string) =>
    invoke<RelationshipRow[]>("list_document_relationships", { documentId }),
};
