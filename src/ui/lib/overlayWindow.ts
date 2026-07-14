// Helpers for the two-window setup: a normal "main" window (full app) and a
// borderless always-on-top "overlay" window (compact live-draft strip). Both
// are declared statically in tauri.conf.json; these helpers just show/hide and
// focus them. All calls no-op outside the Tauri desktop app (e.g. browser dev).

import { Window, getCurrentWindow } from "@tauri-apps/api/window";

const hasTauri = () => "__TAURI_INTERNALS__" in window;

/** The label of the window this code is running in ("main" or "overlay"). */
export function currentWindowLabel(): string {
  if (hasTauri()) return getCurrentWindow().label;
  // Browser-only preview hook for responsive/visual QA of the second Tauri
  // window. It has no effect in the packaged desktop app.
  return new URLSearchParams(window.location.search).get("window") === "overlay" ? "overlay" : "main";
}

/** Show the compact overlay window and bring it to the foreground. */
export async function showOverlayWindow(): Promise<void> {
  if (!hasTauri()) {
    const url = new URL(window.location.href);
    url.searchParams.set("window", "overlay");
    window.open(url, "lt-ai-coach-overlay", "popup,width=980,height=300");
    return;
  }
  try {
    const overlay = await Window.getByLabel("overlay");
    if (!overlay) return;
    await overlay.show();
    await overlay.setFocus();
  } catch {
    // The overlay window may not exist in some build configurations.
  }
}

/** Close only the compact overlay; the full draft board remains untouched. */
export async function closeOverlayWindow(): Promise<void> {
  if (!hasTauri()) {
    window.close();
    return;
  }
  try {
    await getCurrentWindow().hide();
  } catch {
    // The overlay may already be closing.
  }
}

/** Hide the compact overlay window. */
export async function hideOverlayWindow(): Promise<void> {
  if (!hasTauri()) return;
  try {
    const overlay = await Window.getByLabel("overlay");
    if (overlay) await overlay.hide();
  } catch {
    // Ignore — nothing to hide.
  }
}
