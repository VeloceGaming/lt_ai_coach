import { describe, expect, it } from "vitest";
import { navigationCommands, navigationScreenForKey } from "./appCommands";

describe("appCommands", () => {
  it("defines one unique shortcut for every workspace", () => {
    expect(navigationCommands.map((command) => command.screen)).toEqual([
      "import", "players", "tiers", "stats", "draft", "patch", "settings",
    ]);
    expect(new Set(navigationCommands.map((command) => command.shortcut)).size).toBe(navigationCommands.length);
  });

  it.each([
    ["1", "import"], ["2", "players"], ["3", "tiers"], ["4", "stats"],
    ["5", "draft"], ["6", "patch"], ["7", "settings"],
  ])("maps %s to %s", (key, screen) => {
    expect(navigationScreenForKey(key, false)).toBe(screen);
  });

  it("ignores modified and unsupported keys", () => {
    expect(navigationScreenForKey("1", true)).toBeNull();
    expect(navigationScreenForKey("8", false)).toBeNull();
  });
});
