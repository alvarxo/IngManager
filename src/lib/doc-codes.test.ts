import { describe, expect, it } from "vitest";

/** Mirror of Rust doc code formatting for FE smoke coverage */
function nextDocumentCode(
  projectCode: string,
  typeCode: string,
  existing: string[],
): string {
  const prefix = `${projectCode}-${typeCode}-`;
  let max = 0;
  for (const code of existing) {
    if (code.startsWith(prefix)) {
      const n = Number.parseInt(code.slice(prefix.length), 10);
      if (!Number.isNaN(n)) max = Math.max(max, n);
    }
  }
  return `${prefix}${String(max + 1).padStart(3, "0")}`;
}

describe("document business codes", () => {
  it("formats PROYECTO-TIPO-CORRELATIVO", () => {
    expect(nextDocumentCode("2601", "MOD", [])).toBe("2601-MOD-001");
    expect(
      nextDocumentCode("2601", "MOD", ["2601-MOD-001", "2601-PL-001"]),
    ).toBe("2601-MOD-002");
  });
});
