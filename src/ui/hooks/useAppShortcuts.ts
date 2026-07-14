import { useEffect } from "react";
import type { FullScreen } from "../components/NavigationRail";
import { isTextEntryTarget, navigationScreenForKey } from "../commands/appCommands";

export function useAppShortcuts(onNavigate: (screen: FullScreen) => void) {
  useEffect(() => {
    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.defaultPrevented || isTextEntryTarget(event.target)) return;
      const modified = event.altKey || event.ctrlKey || event.metaKey || event.shiftKey;
      if (event.key === "/" && !modified) {
        const search = document.querySelector<HTMLInputElement>('.full-screen-content input[type="search"]');
        if (search) {
          event.preventDefault();
          search.focus();
          search.select();
        }
        return;
      }
      const screen = navigationScreenForKey(event.key, modified);
      if (!screen) return;
      event.preventDefault();
      onNavigate(screen);
    };
    document.addEventListener("keydown", handleKeyDown);
    return () => document.removeEventListener("keydown", handleKeyDown);
  }, [onNavigate]);
}
