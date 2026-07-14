import { describe, expect, it } from "vitest";
import { analyzeComp, championTags } from "./comp";

describe("analyzeComp", () => {
  it("reads game-native rawTags for base and modded champions", () => {
    expect(championTags("fighter")).toEqual(expect.arrayContaining(["AD", "Tank", "CC"]));
    expect(championTags("test_mod_fizz")).toEqual(expect.arrayContaining(["AP", "Magic"]));
    expect(championTags("not_a_real_champion")).toEqual([]);
  });

  it("counts damage types and flags an all-physical comp", () => {
    const result = analyzeComp(["fighter", "exorcist", "test_mod_jhin"]);
    expect(result.physical).toBe(3);
    expect(result.magic).toBe(0);
    expect(result.counts["Tank"]).toBe(2);
    expect(result.gaps).toContain("comp.gap.allPhysical");
    expect(result.gaps).not.toContain("comp.gap.noFrontline");
    expect(result.gaps).not.toContain("comp.gap.noCC");
  });

  it("clears the all-physical flag once a magic champion joins", () => {
    const result = analyzeComp(["fighter", "exorcist", "test_mod_jhin", "test_mod_fizz"]);
    expect(result.magic).toBe(1);
    expect(result.gaps).not.toContain("comp.gap.allPhysical");
  });

  it("does not flag gaps with fewer than three picks", () => {
    expect(analyzeComp([]).gaps).toEqual([]);
    expect(analyzeComp(["fighter", "test_mod_jhin"]).gaps).toEqual([]);
  });
});
