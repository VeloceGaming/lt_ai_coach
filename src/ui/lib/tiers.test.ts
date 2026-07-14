import { describe, expect, it } from "vitest";
import { automaticTier, patchDirection, readablePatchField } from "./tiers";

describe("tier list helpers", () => {
  it("maps performance scores to the six visible tiers", () => {
    expect([0.56, 0.53, 0.50, 0.47, 0.44, 0.40].map(automaticTier)).toEqual(["S", "A", "B", "C", "D", "F"]);
  });

  it("labels patch movement", () => {
    expect(patchDirection(0.01)).toBe("buff");
    expect(patchDirection(-0.01)).toBe("nerf");
    expect(patchDirection(0)).toBe("unchanged");
  });

  it("turns backend field paths into readable labels", () => {
    expect(readablePatchField("attack_speed.ratio")).toBe("Attack Speed · Ratio");
  });
});
