import type { FsEntry } from "./types";

export type SortKey = "name" | "size" | "modified" | "ext";
export type SortDir = "asc" | "desc";

export function filterAndSortEntries(
  entries: FsEntry[],
  query: string,
  sortKey: SortKey,
  sortDir: SortDir,
): FsEntry[] {
  const q = query.trim().toLowerCase();
  let list = entries;
  if (q) {
    list = list.filter(
      (e) =>
        e.name.toLowerCase().includes(q) ||
        (e.extension ?? "").toLowerCase().includes(q),
    );
  }

  const dir = sortDir === "asc" ? 1 : -1;
  return [...list].sort((a, b) => {
    if (a.is_directory !== b.is_directory) {
      return a.is_directory ? -1 : 1;
    }
    let cmp = 0;
    switch (sortKey) {
      case "size":
        cmp = (a.size_bytes ?? -1) - (b.size_bytes ?? -1);
        break;
      case "modified":
        cmp = (a.modified_at ?? "").localeCompare(b.modified_at ?? "");
        break;
      case "ext":
        cmp = (a.extension ?? "").localeCompare(b.extension ?? "");
        break;
      default:
        cmp = a.name.localeCompare(b.name, undefined, { sensitivity: "base" });
    }
    return cmp * dir;
  });
}

export function parentSubpath(subpath: string): string {
  const parts = subpath.split("/").filter(Boolean);
  parts.pop();
  return parts.join("/");
}

const SAP_AUX_EXTS = new Set([
  "s2k",
  "$2k",
  "log",
  "out",
  "txt",
  "xml",
  "rtf",
  "mdb",
  "sdbr",
  "ebk",
  "sfk",
  "$sf",
  "bak",
  "dwl",
  "dwl2",
]);

/** Hide SAP/CAD auxiliaries when a principal with the same stem is present. */
export function filterEngineeringAuxiliaries(
  entries: FsEntry[],
  showAuxiliaries: boolean,
): FsEntry[] {
  if (showAuxiliaries) return entries;
  const stemsWithPrincipal = new Set<string>();
  for (const e of entries) {
    if (e.is_directory) continue;
    const ext = (e.extension ?? "").toLowerCase();
    if (ext === "sdb" || ext === "rvt" || ext === "dwg") {
      const stem = e.name.slice(0, e.name.length - (ext.length + 1)).toLowerCase();
      stemsWithPrincipal.add(stem);
    }
  }
  return entries.filter((e) => {
    if (e.is_directory) return true;
    const ext = (e.extension ?? "").toLowerCase();
    if (!SAP_AUX_EXTS.has(ext)) return true;
    const stem = e.name.slice(0, e.name.length - (ext.length + 1)).toLowerCase();
    return !stemsWithPrincipal.has(stem);
  });
}
