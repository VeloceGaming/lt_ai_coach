import { describe, expect, it } from "vitest";
import { formatPatchLabel } from "./patchFormat";

const translations: Record<string, string> = {
  "field.skill1": "技能1",
  "field.skill2": "技能2",
  "field.heal": "基礎恢復量",
};

const t = (key: string) => translations[key] ?? key;

describe("patch label formatting", () => {
  it("normalizes the game's primary skill container to Skill1", () => {
    expect(formatPatchLabel("skill", null, "heal", t)).toBe("技能1 · 基礎恢復量");
    expect(formatPatchLabel("skill1", null, "heal", t)).toBe("技能1 · 基礎恢復量");
  });

  it("normalizes Monk's heal-skill container to Skill1", () => {
    expect(formatPatchLabel("heal_skill", null, "heal", t)).toBe("技能1 · 基礎恢復量");
  });

  it("keeps Skill2 distinct", () => {
    expect(formatPatchLabel("skill2", null, "heal", t)).toBe("技能2 · 基礎恢復量");
  });
});
