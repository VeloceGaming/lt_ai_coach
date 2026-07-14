// Theming engine: resolves the user's {mode, surface, accent} choice into CSS
// variables on :root. Mode is applied via the `data-theme` attribute (tokens.css
// defines the dark palette under [data-theme="dark"]); the surface preset and a
// custom accent are applied as inline variable overrides. The Settings UI (T3)
// just calls the store setters — all the CSS work happens here.

export type ThemeMode = "light" | "dark" | "auto";
export type SurfacePreset = "neutral" | "warm" | "broadcast";
export type TypographyStyle = "technical" | "clean" | "geometric";

export type ThemeState = {
  mode: ThemeMode;
  surface: SurfacePreset;
  // Empty string = use the built-in accent; otherwise a hex like "#7c5cff".
  accent: string;
  // User toggle; effective reduce-motion is this OR the OS setting.
  reduceMotion: boolean;
  // Curated app-wide type system; "technical" preserves the original UI.
  typography: TypographyStyle;
};

export const defaultThemeState: ThemeState = { mode: "auto", surface: "neutral", accent: "", reduceMotion: false, typography: "technical" };
export const DEFAULT_ACCENT = "#378add";

// Surface tokens a preset may override. Listed so we can fully clear stale inline
// values before applying a new preset.
const SURFACE_TOKENS = [
  "--color-background-canvas", "--color-background-primary", "--color-background-secondary", "--color-background-tertiary",
  "--color-text-primary", "--color-text-secondary", "--color-text-tertiary",
  "--color-border-tertiary", "--color-border-secondary", "--color-border-primary",
] as const;

const ACCENT_TOKENS = ["--color-accent", "--color-accent-text", "--color-accent-surface", "--color-accent-border"] as const;

type Vars = Record<string, string>;

// Neutral = the built-in tokens.css values (no override). Warm and Broadcast each
// define a light + dark surface ramp; text auto-pairs for contrast.
const SURFACE_PRESETS: Record<Exclude<SurfacePreset, "neutral">, { light: Vars; dark: Vars }> = {
  warm: {
    light: {
      "--color-background-canvas": "#ece6da", "--color-background-primary": "#fffdf7", "--color-background-secondary": "#f5efe3", "--color-background-tertiary": "#ebe3d4",
      "--color-text-primary": "#211f1a", "--color-text-secondary": "#645d52", "--color-text-tertiary": "#8c8475",
      "--color-border-tertiary": "rgba(60, 40, 10, 0.12)", "--color-border-secondary": "rgba(60, 40, 10, 0.22)", "--color-border-primary": "rgba(60, 40, 10, 0.32)",
    },
    dark: {
      "--color-background-canvas": "#1b1813", "--color-background-primary": "#2b2823", "--color-background-secondary": "#33302a", "--color-background-tertiary": "#3b3731",
      "--color-text-primary": "#f5f1e9", "--color-text-secondary": "#c7c0b4", "--color-text-tertiary": "#9a9384",
      "--color-border-tertiary": "rgba(255, 245, 225, 0.12)", "--color-border-secondary": "rgba(255, 245, 225, 0.20)", "--color-border-primary": "rgba(255, 245, 225, 0.28)",
    },
  },
  broadcast: {
    light: {
      "--color-background-canvas": "#e4e7ec", "--color-background-primary": "#fbfcfe", "--color-background-secondary": "#eef1f5", "--color-background-tertiary": "#e3e8ee",
      "--color-text-primary": "#141a22", "--color-text-secondary": "#515c68", "--color-text-tertiary": "#7e8a98",
      "--color-border-tertiary": "rgba(10, 25, 45, 0.12)", "--color-border-secondary": "rgba(10, 25, 45, 0.22)", "--color-border-primary": "rgba(10, 25, 45, 0.32)",
    },
    dark: {
      "--color-background-canvas": "#0e1117", "--color-background-primary": "#161b22", "--color-background-secondary": "#1c222b", "--color-background-tertiary": "#232a34",
      "--color-text-primary": "#eef2f7", "--color-text-secondary": "#aeb8c4", "--color-text-tertiary": "#7c8794",
      "--color-border-tertiary": "rgba(255, 255, 255, 0.12)", "--color-border-secondary": "rgba(255, 255, 255, 0.20)", "--color-border-primary": "rgba(255, 255, 255, 0.30)",
    },
  },
};

// A few representative colors per preset for the Settings preview swatches
// (background / card / text / border), for both light and dark.
type Swatch = { bg: string; card: string; text: string; border: string };
export const SURFACE_PREVIEW: Record<SurfacePreset, { light: Swatch; dark: Swatch }> = {
  neutral: {
    light: { bg: "#e9e8e2", card: "#ffffff", text: "#1a1a18", border: "rgba(0, 0, 0, 0.14)" },
    dark: { bg: "#1a1a1c", card: "#2b2b2e", text: "#f4f4f5", border: "rgba(255, 255, 255, 0.16)" },
  },
  warm: {
    light: { bg: "#ece6da", card: "#fffdf7", text: "#211f1a", border: "rgba(60, 40, 10, 0.16)" },
    dark: { bg: "#1b1813", card: "#2b2823", text: "#f5f1e9", border: "rgba(255, 245, 225, 0.16)" },
  },
  broadcast: {
    light: { bg: "#e4e7ec", card: "#fbfcfe", text: "#141a22", border: "rgba(10, 25, 45, 0.16)" },
    dark: { bg: "#0e1117", card: "#161b22", text: "#eef2f7", border: "rgba(255, 255, 255, 0.16)" },
  },
};

function prefersDark(): boolean {
  return Boolean(typeof window !== "undefined" && window.matchMedia && window.matchMedia("(prefers-color-scheme: dark)").matches);
}

function prefersReducedMotion(): boolean {
  return Boolean(typeof window !== "undefined" && window.matchMedia && window.matchMedia("(prefers-reduced-motion: reduce)").matches);
}

/** The effective light/dark a theme resolves to right now (auto follows the OS). */
export function resolveMode(mode: ThemeMode): "light" | "dark" {
  return mode === "auto" ? (prefersDark() ? "dark" : "light") : mode;
}

// A custom accent becomes the border/solid color directly; the surface is a
// translucent tint (works on any background), and the text/icon color is nudged
// toward black (light) or white (dark) so it stays readable on highlights.
function accentVars(hex: string, effective: "light" | "dark"): Vars {
  return {
    "--color-accent": hex,
    "--color-accent-border": hex,
    "--color-accent-surface": `color-mix(in srgb, ${hex} 16%, transparent)`,
    "--color-accent-text": effective === "dark" ? `color-mix(in srgb, ${hex}, white 30%)` : `color-mix(in srgb, ${hex}, black 25%)`,
  };
}

/** Apply a theme to the document (no-op outside the browser/webview). */
export function applyTheme(state: ThemeState): void {
  if (typeof document === "undefined") return;
  const root = document.documentElement;
  const effective = resolveMode(state.mode);
  root.setAttribute("data-theme", effective);
  root.setAttribute("data-typography", state.typography);
  root.classList.toggle("reduce-motion", state.reduceMotion || prefersReducedMotion());

  // Clear anything we previously set inline so switching presets/accent is clean.
  for (const token of [...SURFACE_TOKENS, ...ACCENT_TOKENS]) root.style.removeProperty(token);

  if (state.surface !== "neutral") {
    const preset = SURFACE_PRESETS[state.surface][effective];
    for (const [token, value] of Object.entries(preset)) root.style.setProperty(token, value);
  }
  if (state.accent && state.accent !== DEFAULT_ACCENT) {
    const vars = accentVars(state.accent, effective);
    for (const [token, value] of Object.entries(vars)) root.style.setProperty(token, value);
  }
}

// Re-apply when the OS theme flips while in "auto", so presets/accent re-resolve
// for the new light/dark. `getState` lets callers pass the live theme.
export function watchSystemTheme(getState: () => ThemeState): void {
  if (typeof window === "undefined" || !window.matchMedia) return;
  window.matchMedia("(prefers-color-scheme: dark)").addEventListener("change", () => {
    if (getState().mode === "auto") applyTheme(getState());
  });
  // Re-apply when the OS reduce-motion setting flips (affects the document class).
  window.matchMedia("(prefers-reduced-motion: reduce)").addEventListener("change", () => applyTheme(getState()));
}
