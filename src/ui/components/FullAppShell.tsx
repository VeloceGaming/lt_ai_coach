import type { ReactNode } from "react";
import { useState } from "react";
import { IconMinus, IconX } from "@tabler/icons-react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { NavigationRail, type FullScreen } from "./NavigationRail";
import { useT } from "../stores/useI18nStore";

export function FullAppShell({ screen, screenTitle, onScreenChange, header, children }: { screen: FullScreen; screenTitle: string; onScreenChange: (screen: FullScreen) => void; header?: ReactNode; children: ReactNode }) {
  const t = useT();
  const [expanded, setExpanded] = useState(false);
  const appWindow = getCurrentWindow();
  return <div className={`full-app-shell${expanded ? " rail-open" : ""}`} data-screen={screen} onContextMenu={(event) => event.preventDefault()}>
    <header className="app-titlebar" data-tauri-drag-region>
      <div className="app-titlebar-identity" data-tauri-drag-region><strong data-tauri-drag-region>LT AI Coach</strong><span data-tauri-drag-region>{screenTitle}</span></div>
      <div className="app-window-controls">
        <button type="button" aria-label={t("titlebar.minimizeAria")} title={t("titlebar.minimizeTitle")} onClick={() => void appWindow.minimize()}><IconMinus size={15} /></button>
        <button type="button" className="app-window-close" aria-label={t("titlebar.closeAria")} title={t("titlebar.closeTitle")} onClick={() => void appWindow.close()}><IconX size={15} /></button>
      </div>
    </header>
    <NavigationRail screen={screen} expanded={expanded} onExpandedChange={setExpanded} onScreenChange={onScreenChange} />
    <main className="full-workspace" aria-label={`${screenTitle} ${t("app.workspaceSuffix")}`}>
      {header ?? null}
      <div className="full-screen-content" key={screen}>{children}</div>
    </main>
  </div>;
}
