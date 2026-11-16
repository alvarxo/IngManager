import { api } from "./api";
import type { Project } from "./types";

/** Open a project without letting backend errors crash the UI shell. */
export async function openProjectSafe(
  projectId: string,
): Promise<{ ok: true; project: Project } | { ok: false; error: string }> {
  try {
    const project = await api.openProject(projectId);
    return { ok: true, project };
  } catch (e) {
    const error = e instanceof Error ? e.message : String(e);
    return { ok: false, error };
  }
}
