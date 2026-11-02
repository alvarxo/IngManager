import { describe, expect, it } from "vitest";
import { isoToUiDate, uiDateToIso } from "./dates";

describe("dates", () => {
  it("converts ISO to DD-MM-YYYY", () => {
    expect(isoToUiDate("2026-11-02")).toBe("02-11-2026");
    expect(isoToUiDate("2026-11-02T10:00:00Z")).toBe("02-11-2026");
  });

  it("parses UI date to ISO", () => {
    expect(uiDateToIso("02-11-2026")).toBe("2026-11-02");
    expect(uiDateToIso("bad")).toBeNull();
  });
});
