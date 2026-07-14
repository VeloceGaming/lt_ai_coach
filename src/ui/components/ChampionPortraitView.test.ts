import { describe, expect, it } from "vitest";

import type { ChampionPortrait } from "../types";
import { portraitScale } from "./ChampionPortraitView";

const portrait = (width: number, height: number): ChampionPortrait => ({
  path: "/champion.png",
  sheetWidth: 400,
  sheetHeight: 100,
  x: 0,
  y: 0,
  width,
  height,
  faceOffsetX: 0,
  faceOffsetY: 0,
  centerOffsetX: 0,
  centerOffsetY: 0,
});

describe("portraitScale", () => {
  it("preserves relative champion sizes in recommendation cards", () => {
    const soldierScale = portraitScale(portrait(51, 41), 104, 150, "champion");
    const whipMasterScale = portraitScale(portrait(27, 47), 104, 150, "champion");

    expect(whipMasterScale).toBeCloseTo(soldierScale);
    expect(27 * whipMasterScale).toBeLessThan(51 * soldierScale);
  });

  it("caps tall champion frames to the available height", () => {
    expect(portraitScale(portrait(30, 200), 104, 150, "champion")).toBe(0.75);
  });
});
