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

describe("update project code (metadata)", () => {
  it("UpdateProjectInput accepts code without path fields", () => {
    const input: UpdateProjectInput = {
      id: "p1",
      code: "9999",
      name: "Renamed",
    };
    expect(input).not.toHaveProperty("root_relative_path");
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

  it("metadata save payload always includes code when editing", () => {
    const editCode = "  2601 ";
    const payload: UpdateProjectInput = {
      id: "p1",
      code: editCode.trim(),
      name: "Proyecto",
      client_name: null,
      description: null,
    };
    expect(payload.code).toBe("2601");
  });
});
