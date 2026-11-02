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
