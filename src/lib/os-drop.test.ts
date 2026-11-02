import { describe, expect, it } from "vitest";
import { physicalToCssPoint, pointInElement } from "./os-drop";

describe("os-drop helpers", () => {
  it("converts physical pixels with devicePixelRatio", () => {
    expect(physicalToCssPoint({ x: 200, y: 100 }, 2)).toEqual({
      x: 100,
      y: 50,
    });
    expect(physicalToCssPoint({ x: 90, y: 45 }, 1)).toEqual({ x: 90, y: 45 });
  });

  it("hit-tests drop zones", () => {
    const box = { left: 10, top: 20, right: 110, bottom: 120 };
    expect(pointInElement({ x: 50, y: 50 }, box)).toBe(true);
    expect(pointInElement({ x: 10, y: 20 }, box)).toBe(true);
    expect(pointInElement({ x: 110, y: 120 }, box)).toBe(true);
    expect(pointInElement({ x: 9, y: 50 }, box)).toBe(false);
    expect(pointInElement({ x: 50, y: 121 }, box)).toBe(false);
  });
});
