import type { FullScreen } from "../components/NavigationRail";

export type AppCommand = {
  id: `navigate.${FullScreen}`;
  screen: FullScreen;
  labelKey: string;
  shortcut: string;
};

export const navigationCommands: readonly AppCommand[] = [
  { id: "navigate.import", screen: "import", labelKey: "nav.import.label", shortcut: "1" },
  { id: "navigate.players", screen: "players", labelKey: "nav.players.label", shortcut: "2" },
  { id: "navigate.tiers", screen: "tiers", labelKey: "nav.tiers.label", shortcut: "3" },
  { id: "navigate.stats", screen: "stats", labelKey: "nav.stats.label", shortcut: "4" },
  { id: "navigate.draft", screen: "draft", labelKey: "nav.draft.label", shortcut: "5" },
  { id: "navigate.patch", screen: "patch", labelKey: "nav.patch.label", shortcut: "6" },
  { id: "navigate.settings", screen: "settings", labelKey: "nav.settings.label", shortcut: "7" },
] as const;

export function navigationScreenForKey(key: string, modified: boolean): FullScreen | null {
  if (modified) return null;
  return navigationCommands[Number(key) - 1]?.screen ?? null;
}

export function isTextEntryTarget(target: EventTarget | null): boolean {
  if (!(target instanceof HTMLElement)) return false;
  return target.isContentEditable || ["INPUT", "SELECT", "TEXTAREA"].includes(target.tagName);
}
