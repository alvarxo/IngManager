import { describe, expect, it } from "vitest";
import type { UpdateProjectInput } from "./types";

/** Mirror of Rust normalize_optional_code for FE smoke coverage. */
function normalizeOptionalCode(raw: string | null | undefined): string | null {
  const trimmed = raw?.trim() ?? "";
  if (!trimmed) return null;
  if (trimmed.includes("/") || trimmed.includes("\\")) {
    throw new Error("El código no puede contener separadores de ruta");
  }
  return trimmed;
}

/** Mirror of Rust reprefix_business_code. */
function reprefixBusinessCode(
  code: string,
  oldPrefix: string,
  newPrefix: string,
): string | null {
  if (!oldPrefix || oldPrefix === newPrefix) return null;
  const needle = `${oldPrefix}-`;
  if (!code.startsWith(needle)) return null;
  return `${newPrefix}-${code.slice(needle.length)}`;
}

describe("update project code (metadata)", () => {
  it("UpdateProjectInput accepts code without forcing rename", () => {
    const input: UpdateProjectInput = {
      id: "p1",
      code: "9999",
      name: "Renamed",
      rename_root_folder: false,
    };
    expect(input.rename_root_folder).toBe(false);
    expect(input.code).toBe("9999");
  });

  it("empty / whitespace code clears logical code", () => {
    expect(normalizeOptionalCode("")).toBeNull();
    expect(normalizeOptionalCode("   ")).toBeNull();
  });

  it("rejects path separators in code", () => {
    expect(() => normalizeOptionalCode("a/b")).toThrow(/separadores/);
    expect(() => normalizeOptionalCode("a\\b")).toThrow(/separadores/);
  });

  it("trims and keeps free-form business codes", () => {
    expect(normalizeOptionalCode("  1090  ")).toBe("1090");
    expect(normalizeOptionalCode("PRJ-ALPHA")).toBe("PRJ-ALPHA");
  });

  it("metadata save payload can opt into folder rename", () => {
    const editCode = "  1133_2 ";
    const payload: UpdateProjectInput = {
      id: "p1",
      code: editCode.trim(),
      name: "Proyecto",
      client_name: null,
      description: null,
      rename_root_folder: true,
    };
    expect(payload.code).toBe("1133_2");
    expect(payload.rename_root_folder).toBe(true);
  });

  it("reprefixes document business codes when project code changes", () => {
    expect(reprefixBusinessCode("1133_3-MC-001", "1133_3", "1133_2")).toBe(
      "1133_2-MC-001",
    );
    expect(reprefixBusinessCode("1133_3-SAP-002", "1133_3", "1133_2")).toBe(
      "1133_2-SAP-002",
    );
    expect(reprefixBusinessCode("OTHER-MC-001", "1133_3", "1133_2")).toBeNull();
    expect(reprefixBusinessCode("1133_3-MC-001", "1133_3", "1133_3")).toBeNull();
  });
});
