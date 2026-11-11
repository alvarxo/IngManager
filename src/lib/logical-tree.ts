/** Helpers for Lógicos tree expand keys and open-format UX (nav/open only). */

export type TreeExpandPolicy = "remember" | "collapsed" | "expanded";

export type TreeNodeKind = "doc" | "rev" | "ver" | "file";

export function treeNodeKey(kind: TreeNodeKind, id: string): string {
  return `${kind}:${id}`;
}

export function parseTreeNodeKey(
  key: string,
): { kind: TreeNodeKind; id: string } | null {
  const idx = key.indexOf(":");
  if (idx <= 0) return null;
  const kind = key.slice(0, idx);
  const id = key.slice(idx + 1);
  if (!id) return null;
  if (kind === "doc" || kind === "rev" || kind === "ver" || kind === "file") {
    return { kind, id };
  }
  return null;
}

/** Whether a category uses revision middle nodes (memorias/planos). */
export function categoryUsesRevisions(categoryKey: string): boolean {
  return categoryKey === "memorias" || categoryKey === "planos";
}

/** Column Revisión (_R) — memorias/planos + modelos (parity naming). */
export function categoryShowsRevisionColumn(categoryKey: string): boolean {
  return categoryUsesRevisions(categoryKey) || categoryKey === "modelos";
}

/** Categories that can emit formal `_R…_v01` revisions. */
export function categorySupportsRevisions(categoryKey: string): boolean {
  return categoryShowsRevisionColumn(categoryKey);
}

export function normalizeFormatToken(token: string): string {
  return token.trim().replace(/^\./, "").toLowerCase();
}

/**
 * Initial expanded set from policy + persisted map.
 * - remember: use persisted
 * - collapsed: empty
 * - expanded: caller supplies known doc keys (lazy children still load on demand)
 */
export function initialExpandedKeys(
  policy: TreeExpandPolicy,
  persisted: Record<string, boolean> | null | undefined,
  allExpandedSeed: string[] = [],
): Set<string> {
  if (policy === "collapsed") return new Set();
  if (policy === "expanded") return new Set(allExpandedSeed);
  const out = new Set<string>();
  if (persisted) {
    for (const [k, v] of Object.entries(persisted)) {
      if (v) out.add(k);
    }
  }
  return out;
}

export function expandedSetToRecord(expanded: Set<string>): Record<string, boolean> {
  const out: Record<string, boolean> = {};
  for (const k of expanded) out[k] = true;
  return out;
}

/** Compact label for format chips / picker. */
export function formatChipLabel(ext: string): string {
  const e = normalizeFormatToken(ext);
  return e.toUpperCase();
}
