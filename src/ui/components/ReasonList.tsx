// Renders a recommendation's reasons with tone-based color and icon so a
// downside ("Weak into Chef") no longer reads like a plus. Shared by the full
// draft panel and the compact overlay to keep the two views consistent.

import { IconCheck, IconAlertTriangle, IconMinus } from "@tabler/icons-react";
import type { Reason, ReasonTone } from "../types";

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
  return (
    <ul className={className}>
      {reasons.slice(0, limit).map((reason, index) => {
        const Icon = TONE_ICON[reason.tone] ?? IconMinus;
        return (
          <li key={index} className={`reason reason-${reason.tone}`}>
            <Icon size={iconSize} stroke={2.4} />
            <span>{reason.text}</span>
          </li>
        );
      })}
    </ul>
  );
}
