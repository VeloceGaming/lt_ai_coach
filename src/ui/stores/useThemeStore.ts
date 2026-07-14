// Persisted theme settings (mode / surface preset / accent). Applies to the
// document on every change. Shared by both windows via localStorage.

import { create } from "zustand";
import { applyTheme, defaultThemeState, watchSystemTheme, type SurfacePreset, type ThemeMode, type ThemeState, type TypographyStyle } from "../lib/theme";

const STORAGE_KEY = "lt-ai-coach-theme";

export function parseThemeState(stored: string | null): ThemeState {
  try {
    if (!stored) return defaultThemeState;
    const value = JSON.parse(stored) as Partial<ThemeState>;
    return {
      mode: (["light", "dark", "auto"] as ThemeMode[]).includes(value.mode as ThemeMode) ? value.mode as ThemeMode : defaultThemeState.mode,
      surface: (["neutral", "warm", "broadcast"] as SurfacePreset[]).includes(value.surface as SurfacePreset) ? value.surface as SurfacePreset : defaultThemeState.surface,
      accent: typeof value.accent === "string" ? value.accent : defaultThemeState.accent,
      reduceMotion: typeof value.reduceMotion === "boolean" ? value.reduceMotion : defaultThemeState.reduceMotion,
      typography: (["technical", "clean", "geometric"] as TypographyStyle[]).includes(value.typography as TypographyStyle) ? value.typography as TypographyStyle : defaultThemeState.typography,
    };
  } catch {
    return defaultThemeState;
  }
}

function loadTheme(): ThemeState {
  if (typeof window === "undefined") return defaultThemeState;
  try {
    return parseThemeState(window.localStorage.getItem(STORAGE_KEY));
  } catch {
    return defaultThemeState;
  }
}

function persist(state: ThemeState) {
  if (typeof window === "undefined") return;
  try {
    window.localStorage.setItem(STORAGE_KEY, JSON.stringify(state));
  } catch {
    // Storage may be unavailable; the theme still applies for this session.
  }
}

type ThemeStore = ThemeState & {
  setMode: (mode: ThemeMode) => void;
  setSurface: (surface: SurfacePreset) => void;
  setAccent: (accent: string) => void;
  setReduceMotion: (reduceMotion: boolean) => void;
  setTypography: (typography: TypographyStyle) => void;
};

const initial = loadTheme();

export const useThemeStore = create<ThemeStore>((set, get) => {
  const update = (patch: Partial<ThemeState>) => {
    set(patch);
    const { mode, surface, accent, reduceMotion, typography } = get();
    const next = { mode, surface, accent, reduceMotion, typography };
    persist(next);
    applyTheme(next);
  };
  return {
    ...initial,
    setMode: (mode) => update({ mode }),
    setSurface: (surface) => update({ surface }),
    setAccent: (accent) => update({ accent }),
    setReduceMotion: (reduceMotion) => update({ reduceMotion }),
    setTypography: (typography) => update({ typography }),
  };
});

// Apply the persisted theme immediately and keep it in sync with the OS in auto.
export function initTheme() {
  applyTheme(initial);
  watchSystemTheme(() => {
    const { mode, surface, accent, reduceMotion, typography } = useThemeStore.getState();
    return { mode, surface, accent, reduceMotion, typography };
  });
  // Cross-window sync: the overlay is a separate window with its own state, so
  // when the main window changes the theme (writing localStorage), reapply it
  // here. Same-origin windows receive `storage` events for each other's writes.
  if (typeof window !== "undefined") {
    window.addEventListener("storage", (event) => {
      if (event.key !== STORAGE_KEY) return;
      const next = loadTheme();
      useThemeStore.setState(next);
      applyTheme(next);
    });
  }
}
