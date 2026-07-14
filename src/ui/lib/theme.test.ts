import { describe, expect, it } from "vitest";

import { defaultThemeState } from "./theme";
import { parseThemeState } from "../stores/useThemeStore";

describe("parseThemeState", () => {
  it("restores a saved typography style", () => {
    expect(parseThemeState(JSON.stringify({ typography: "geometric" })).typography).toBe("geometric");
  });

  it("defaults old and invalid settings to the technical style", () => {
    expect(parseThemeState(JSON.stringify({ mode: "dark" })).typography).toBe("technical");
    expect(parseThemeState(JSON.stringify({ typography: "unknown" })).typography).toBe(defaultThemeState.typography);
  });
});
