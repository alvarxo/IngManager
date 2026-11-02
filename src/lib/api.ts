import { invoke } from "@tauri-apps/api/core";
import type {
  AppStatus,
  CategorySummary,
  ConflictPolicy,
  CreateProjectInput,
  EntryProperties,
  FileOpResult,
  FsEntry,
  IndexedFile,
  Project,
  TrashItem,
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
};
