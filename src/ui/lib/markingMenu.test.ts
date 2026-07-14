import { describe, expect, it } from "vitest";
import { resolveMarkingDirection } from "./markingMenu";

describe("resolveMarkingDirection", () => {
  it("keeps short releases in the cancellation dead zone", () => {
    expect(resolveMarkingDirection(0, 0)).toBeNull();
    expect(resolveMarkingDirection(20, 20)).toBeNull();
  });

  it.each([
    [0, -60, "north"], [60, 0, "east"], [0, 60, "south"], [-60, 0, "west"],
  ] as const)("maps %s,%s to %s", (x, y, direction) => {
    expect(resolveMarkingDirection(x, y)).toBe(direction);
  });
});
