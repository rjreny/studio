import { describe, expect, it } from "vitest";
import { nextWindowLayout } from "./windowLayout";

describe("nextWindowLayout", () => {
  it("keeps the restored size when the window is maximized", () => {
    const previous = { x: 80, y: 40, width: 1280, height: 800, maximized: false };
    const saved = nextWindowLayout(previous, {
      x: -8,
      y: -8,
      width: 1920,
      height: 1080,
      maximized: true,
    });
    expect(saved).toEqual({ x: 80, y: 40, width: 1280, height: 800, maximized: true });
  });

  it("stores the latest restored bounds", () => {
    const saved = nextWindowLayout(undefined, {
      x: 120,
      y: 60,
      width: 1400,
      height: 900,
      maximized: false,
    });
    expect(saved).toEqual({ x: 120, y: 60, width: 1400, height: 900, maximized: false });
  });
});
