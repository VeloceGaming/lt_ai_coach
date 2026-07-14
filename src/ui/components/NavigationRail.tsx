import { IconChartBar, IconDatabaseImport, IconLayoutGrid, IconNotes, IconSettings, IconStairsUp, IconUsers } from "@tabler/icons-react";
import { navigationCommands } from "../commands/appCommands";
import { useT } from "../stores/useI18nStore";

export type FullScreen = "import" | "players" | "tiers" | "stats" | "draft" | "patch" | "settings";

const icons: Record<FullScreen, typeof IconUsers> = {
  import: IconDatabaseImport,
  players: IconUsers,
  tiers: IconStairsUp,
  stats: IconChartBar,
  draft: IconLayoutGrid,
  patch: IconNotes,
  settings: IconSettings,
};

export function NavigationRail({ screen, expanded, onExpandedChange, onScreenChange }: { screen: FullScreen; expanded: boolean; onExpandedChange: (expanded: boolean) => void; onScreenChange: (screen: FullScreen) => void }) {
  const t = useT();
  return <nav className={`navigation-rail${expanded ? " expanded" : ""}`} aria-label={t("nav.aria")} onMouseEnter={() => onExpandedChange(true)} onMouseLeave={() => onExpandedChange(false)}>
    <div className="rail-items">
      {navigationCommands.map(({ screen: target, labelKey, shortcut }, index) => {
        const Icon = icons[target];
        const label = t(labelKey);
        return <button key={target} type="button" className={`rail-item${screen === target ? " active" : ""}${index === navigationCommands.length - 1 ? " rail-settings" : ""}`} aria-current={screen === target ? "page" : undefined} aria-label={label} title={expanded ? undefined : label} onClick={() => onScreenChange(target)}><Icon size={19} stroke={2.15} /><span>{label}</span></button>;
      })}
    </div>
  </nav>;
}
