import { describe, expect, it } from "vitest";

import type { Reason } from "../types";
import { resolveReasonValues } from "./ReasonList";

// Stand-in for a non-English dictionary: only the keys listed here resolve,
// and t() echoes the key back for anything else (matching useT's behavior).
const dict: Record<string, string> = {
  "recommendation.survival.likelySurvive": "較可能在紅方下次禁用後存活的角色",
  "role.mid": "中路",
  "champion.exorcist": "驅魔師",
};
const t = (key: string) => dict[key] ?? key;
const championName = (id: string, fallback: string) => dict[`champion.${id}`] ?? fallback;

const reason = (overrides: Partial<Reason>): Reason => ({
  text: "",
  tone: "neutral",
  ...overrides,
});

describe("resolveReasonValues", () => {
  it("translates a phrase placeholder instead of passing the engine's English through", () => {
    const values = resolveReasonValues(
      reason({
        translationKey: "recommendation.reason.blueClaimWithLeftover",
        translationValues: { label: "likely to survive Red's next ban" },
        translationKeys: { label: "recommendation.survival.likelySurvive" },
      }),
      t,
      championName,
    );

    expect(values.label).toBe("較可能在紅方下次禁用後存活的角色");
  });

  it("keeps the engine's English phrase when the key has no translation", () => {
    const values = resolveReasonValues(
      reason({
        translationValues: { label: "at moderate contest risk from Red" },
        translationKeys: { label: "recommendation.survival.moderateRisk" },
      }),
      t,
      championName,
    );

    // Never shows the raw key to the user.
    expect(values.label).toBe("at moderate contest risk from Red");
  });

  it("resolves champion and role placeholders, and leaves plain values alone", () => {
    const values = resolveReasonValues(
      reason({
        translationValues: { claim: "Exorcist", role: "mid", winRate: "55.2", games: "40" },
        translationChampionIds: { claim: "exorcist" },
        translationRoleIds: { role: "mid" },
      }),
      t,
      championName,
    );

    expect(values.claim).toBe("驅魔師");
    expect(values.role).toBe("中路");
    // Numbers are language-neutral and must pass through untouched.
    expect(values.winRate).toBe("55.2");
    expect(values.games).toBe("40");
  });
});
