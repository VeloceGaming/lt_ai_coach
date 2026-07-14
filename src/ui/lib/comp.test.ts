import { describe, expect, it } from "vitest";
import { analyzeComp, championTags } from "./comp";

describe("analyzeComp", () => {
  const tags = {
    fighter: ["AD", "Tank", "CC"],
    exorcist: ["AD", "Tank", "CC"],
    test_mod_jhin: ["AD", "Range"],
    test_mod_fizz: ["AP", "Magic"],
  };

  it("reads game-native rawTags for base and modded champions", () => {
    expect(championTags("fighter", tags)).toEqual(expect.arrayContaining(["AD", "Tank", "CC"]));
    expect(championTags("test_mod_fizz", tags)).toEqual(expect.arrayContaining(["AP", "Magic"]));
    expect(championTags("not_a_real_champion", tags)).toEqual([]);
  });

  it("counts damage types and flags an all-physical comp", () => {
    const result = analyzeComp(["fighter", "exorcist", "test_mod_jhin"], tags);
    expect(result.physical).toBe(3);
    expect(result.magic).toBe(0);
    expect(result.counts["Tank"]).toBe(2);
    expect(result.gaps).toContain("comp.gap.allPhysical");
    expect(result.gaps).not.toContain("comp.gap.noFrontline");
    expect(result.gaps).not.toContain("comp.gap.noCC");
  });

  it("clears the all-physical flag once a magic champion joins", () => {
    const result = analyzeComp(["fighter", "exorcist", "test_mod_jhin", "test_mod_fizz"], tags);
    expect(result.magic).toBe(1);
    expect(result.gaps).not.toContain("comp.gap.allPhysical");
  });

  it("does not flag gaps with fewer than three picks", () => {
    expect(analyzeComp([]).gaps).toEqual([]);
    expect(analyzeComp(["fighter", "test_mod_jhin"]).gaps).toEqual([]);
  });
});
