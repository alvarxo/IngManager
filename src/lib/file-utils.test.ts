import { describe, expect, it } from "vitest";
import {
  filterAndSortEntries,
  filterEngineeringAuxiliaries,
  parentSubpath,
} from "./file-utils";
import type { FsEntry } from "./types";

function entry(partial: Partial<FsEntry> & Pick<FsEntry, "name">): FsEntry {
  return {
    absolute_path: `/root/${partial.name}`,
    relative_path: partial.name,
    is_directory: false,
    extension: null,
    size_bytes: 10,
    modified_at: "2026-01-01T00:00:00Z",
    is_available: true,
    is_engineering: false,
    category_key: "modelos",
    ...partial,
  };
}

describe("file-utils", () => {
  it("computes parent subpath", () => {
    expect(parentSubpath("a/b/c")).toBe("a/b");
    expect(parentSubpath("a")).toBe("");
  });

  it("filters and keeps directories first", () => {
    const list = [
      entry({ name: "z.txt", extension: "txt" }),
      entry({ name: "docs", is_directory: true, size_bytes: null }),
      entry({ name: "a.rvt", extension: "rvt", is_engineering: true }),
    ];
    const result = filterAndSortEntries(list, "a", "name", "asc");
    expect(result.map((e) => e.name)).toEqual(["a.rvt"]);
    const sorted = filterAndSortEntries(list, "", "name", "asc");
    expect(sorted[0]?.name).toBe("docs");
  });

  it("hides SAP auxiliaries when principal present", () => {
    const list = [
      entry({ name: "A.sdb", extension: "sdb", is_engineering: true }),
      entry({ name: "A.log", extension: "log" }),
      entry({ name: "nota.pdf", extension: "pdf" }),
    ];
    const hidden = filterEngineeringAuxiliaries(list, false);
    expect(hidden.map((e) => e.name).sort()).toEqual(["A.sdb", "nota.pdf"]);
    expect(filterEngineeringAuxiliaries(list, true)).toHaveLength(3);
  });
});
