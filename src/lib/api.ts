import { invoke } from "@tauri-apps/api/core";
import type {
  AppStatus,
  CategorySummary,
  CreateProjectInput,
  IndexedFile,
  Project,
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
};
