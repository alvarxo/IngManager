import { describe, expect, it } from "vitest";
import {
  categoryShowsRevisionColumn,
  categorySupportsRevisions,
  categoryUsesRevisions,
  expandedSetToRecord,
  formatChipLabel,
  initialExpandedKeys,
  normalizeFormatToken,
  parseTreeNodeKey,
  treeNodeKey,
} from "./logical-tree";

describe("logical-tree helpers", () => {
  it("builds and parses node keys", () => {
    expect(treeNodeKey("doc", "abc")).toBe("doc:abc");
    expect(parseTreeNodeKey("rev:r1")).toEqual({ kind: "rev", id: "r1" });
    expect(parseTreeNodeKey("bad")).toBeNull();
  });

  it("detects revision categories", () => {
    expect(categoryUsesRevisions("memorias")).toBe(true);
    expect(categoryUsesRevisions("planos")).toBe(true);
    expect(categoryUsesRevisions("modelos")).toBe(false);
    expect(categoryShowsRevisionColumn("modelos")).toBe(true);
    expect(categorySupportsRevisions("modelos")).toBe(true);
  });

  it("applies expand policy", () => {
    const persisted = { "doc:a": true, "rev:b": true };
    expect([...initialExpandedKeys("collapsed", persisted)]).toEqual([]);
    expect([...initialExpandedKeys("remember", persisted)].sort()).toEqual([
      "doc:a",
      "rev:b",
    ]);
    expect([...initialExpandedKeys("expanded", persisted, ["doc:x"])]).toEqual([
      "doc:x",
    ]);
  });

  it("serializes expanded set", () => {
    expect(expandedSetToRecord(new Set(["doc:1"]))).toEqual({ "doc:1": true });
  });

  it("normalizes format tokens", () => {
    expect(normalizeFormatToken(".DOCX")).toBe("docx");
    expect(formatChipLabel("pdf")).toBe("PDF");
  });
});
