// Runtime UI language. English text lives in code (lib/i18n/strings.ts) as the
// permanent fallback; packaged translations are read-only defaults, while
// user overrides and generated mod.json files live in local app data.
// See SettingsScreen's Language row for the picker / export / open-folder UI.

import { create } from "zustand";
import { invoke } from "@tauri-apps/api/core";
import { en } from "../lib/i18n/strings";

const STORAGE_KEY = "lt-ai-coach-language";
const isTauri = () => "__TAURI_INTERNALS__" in window;

const WINDOWS_SYSTEM_TYPOGRAPHY_LOCALES = new Set(["ja", "ko", "th", "zh-hans", "zh-cn", "zh-hant", "zh-tw"]);

export function usesSystemTypography(languageId: string): boolean {
  return WINDOWS_SYSTEM_TYPOGRAPHY_LOCALES.has(languageId.toLowerCase());
}

export type LanguageMeta = { id: string; name: string; direction: string };

type I18nStore = {
  languageId: string;
  dict: Record<string, string>;
  languages: LanguageMeta[];
  languageWarnings: string[];
  ready: boolean;
  setLanguage: (id: string) => void;
  refreshLanguages: () => void;
};

function loadPersistedLanguage(): string {
  if (typeof window === "undefined") return "en";
  try {
    const id = window.localStorage.getItem(STORAGE_KEY) ?? "en";
    if (id.toLowerCase() === "zh-tw") return "zh-hant";
    if (id.toLowerCase() === "zh-cn") return "zh-hans";
    return id;
  } catch {
    return "en";
  }
}

function persistLanguage(id: string) {
  if (typeof window === "undefined") return;
  try {
    window.localStorage.setItem(STORAGE_KEY, id);
  } catch {
    // Storage may be unavailable; the choice still applies for this session.
  }
}

// English needs no file on disk; every other id is read from the translations
// folder. Any read failure (missing/corrupt file) just falls back to English
// rather than blocking the UI.
function fetchDict(id: string): Promise<Record<string, string>> {
  if (!isTauri()) return Promise.resolve({});
  return invoke<Record<string, string>>("load_translation", { id, fallbackEntries: Object.entries(en) }).catch(() => ({}));
}

export function interpolateTranslation(template: string, values: Record<string, string | number> = {}): string {
  return template.replace(/\{([A-Za-z0-9_]+)\}/g, (match, name: string) =>
    Object.hasOwn(values, name) ? String(values[name]) : match,
  );
}

export function translateNow(key: string, values: Record<string, string | number> = {}): string {
  const dict = useI18nStore.getState().dict;
  const template = dict[key] ?? en[key] ?? key;
  return interpolateTranslation(template, values);
}

export const useI18nStore = create<I18nStore>((set) => ({
  languageId: loadPersistedLanguage(),
  dict: {},
  languages: [],
  languageWarnings: [],
  ready: false,

  setLanguage: (id) => {
    persistLanguage(id);
    if (typeof document !== "undefined") document.documentElement.lang = id;
    set({ languageId: id, ready: false });
    fetchDict(id).then((dict) => set({ dict, ready: true }));
  },

  refreshLanguages: () => {
    if (!isTauri()) return;
    invoke<{ languages: LanguageMeta[]; warnings: string[] }>("list_translations")
      .then(({ languages, warnings }) => set({ languages, languageWarnings: warnings }))
      .catch((error) => set({ languageWarnings: [String(error)] }));
  },
}));

// Loads the persisted language's text and the list of language files found on
// disk. Call once at startup, mirroring initTheme().
export function initI18n() {
  useI18nStore.getState().refreshLanguages();
  const { languageId } = useI18nStore.getState();
  if (typeof document !== "undefined") document.documentElement.lang = languageId;
  fetchDict(languageId).then((dict) => useI18nStore.setState({ dict, ready: true }));

  // Cross-window sync: the overlay is a separate window created once at
  // launch and only shown/hidden afterward, so it never re-reads localStorage
  // on its own. Same-origin windows receive `storage` events for each other's
  // writes — reload the dict here when the main window changes the language.
  if (typeof window !== "undefined") {
    window.addEventListener("storage", (event) => {
      if (event.key !== STORAGE_KEY) return;
      const id = event.newValue ?? "en";
      if (typeof document !== "undefined") document.documentElement.lang = id;
      useI18nStore.setState({ languageId: id, ready: false });
      fetchDict(id).then((dict) => useI18nStore.setState({ dict, ready: true }));
    });
  }
}

// Champion display names are NOT routed through t()/en — there's no bundled
// "English" champion name to fall back to (the default is whatever the
// backend already resolved: a user override, the catalog name, or a
// humanized id). A translation file only overrides the name when it defines
// `champion.<id>`; otherwise the existing default passes through untouched,
// so English/no-translation behavior never changes.
export function useChampionName() {
  const dict = useI18nStore((s) => s.dict);
  return (id: string, fallback: string) => dict[`champion.${id}`] ?? fallback;
}

// t(key): the translated string for `key`, falling back to the built-in
// English text, and finally to the key itself (so a typo shows up as an
// obviously-wrong string instead of crashing).
export function useT() {
  const dict = useI18nStore((s) => s.dict);
  return (key: string, values: Record<string, string | number> = {}) =>
    interpolateTranslation(dict[key] ?? en[key] ?? key, values);
}
