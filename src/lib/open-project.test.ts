import { describe, expect, it, vi, beforeEach } from "vitest";

vi.mock("./api", () => ({
  api: {
    openProject: vi.fn(),
  },
}));

import { api } from "./api";
import { openProjectSafe } from "./open-project";

describe("openProjectSafe", () => {
  beforeEach(() => {
    vi.mocked(api.openProject).mockReset();
  });

  it("returns project on success", async () => {
    vi.mocked(api.openProject).mockResolvedValue({
      id: "p1",
      code: "1090",
      name: "Test",
    } as never);
    const r = await openProjectSafe("p1");
    expect(r.ok).toBe(true);
    if (r.ok) expect(r.project.code).toBe("1090");
  });

  it("surfaces errors without throwing", async () => {
    vi.mocked(api.openProject).mockRejectedValue(new Error("boom"));
    const r = await openProjectSafe("p1");
    expect(r.ok).toBe(false);
    if (!r.ok) expect(r.error).toContain("boom");
  });
});
