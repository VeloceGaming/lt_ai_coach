import { describe, expect, it } from "vitest";

import { interpolateTranslation, usesSystemTypography } from "./useI18nStore";

describe("interpolateTranslation", () => {
  it("substitutes named values wherever the language places them", () => {
    expect(interpolateTranslation("在強度梯隊中查看「{champion}」", { champion: "武僧" })).toBe("在強度梯隊中查看「武僧」");
  });

  it("leaves an unprovided placeholder visible", () => {
    expect(interpolateTranslation("Open {champion}")).toBe("Open {champion}");
  });
});

describe("usesSystemTypography", () => {
  it.each(["ja", "ko", "th", "zh-hans", "zh-Hans", "zh-CN", "zh-hant", "zh-Hant", "zh-TW"])("locks typography for %s", (languageId) => {
    expect(usesSystemTypography(languageId)).toBe(true);
  });

  it.each(["en", "ru", "vi"])("keeps typography selectable for %s", (languageId) => {
    expect(usesSystemTypography(languageId)).toBe(false);
  });
});
