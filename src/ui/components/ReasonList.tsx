// Renders a recommendation's reasons with tone-based color and icon so a
// downside ("Weak into Chef") no longer reads like a plus. Shared by the full
// draft panel and the compact overlay to keep the two views consistent.

import { IconCheck, IconAlertTriangle, IconMinus } from "@tabler/icons-react";
import type { Reason, ReasonTone } from "../types";
import { useChampionName, useT } from "../stores/useI18nStore";

const TONE_ICON: Record<ReasonTone, typeof IconCheck> = {
  positive: IconCheck,
  negative: IconAlertTriangle,
  neutral: IconMinus,
};

type Props = {
  reasons: Reason[];
  className?: string;
  limit?: number;
  iconSize?: number;
};

export function ReasonList({ reasons, className, limit = 4, iconSize = 13 }: Props) {
  const t = useT();
  const championName = useChampionName();
  return (
    <ul className={className}>
      {reasons.slice(0, limit).map((reason, index) => {
        const Icon = TONE_ICON[reason.tone] ?? IconMinus;
        const values = { ...reason.translationValues };
        for (const [placeholder, championId] of Object.entries(reason.translationChampionIds ?? {})) {
          values[placeholder] = championName(championId, values[placeholder] ?? championId);
        }
        for (const [placeholder, rawRole] of Object.entries(reason.translationRoleIds ?? {})) {
          const role = rawRole === "bottom" ? "bot" : rawRole;
          const key = `role.${role}`;
          const translated = t(key);
          values[placeholder] = translated === key ? (values[placeholder] ?? rawRole) : translated;
        }
        return (
          <li key={index} className={`reason reason-${reason.tone}`}>
            <Icon size={iconSize} stroke={2.4} />
            <span>{reason.translationKey ? t(reason.translationKey, values) : reason.text}</span>
          </li>
        );
      })}
    </ul>
  );
}
