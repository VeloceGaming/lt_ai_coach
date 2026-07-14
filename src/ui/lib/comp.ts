// Composition analysis from the game-native champion rawTags (AD/AP/Tank/CC/
// Shield/Magic/Heal/Range/Dot/Melee). Display-only: it summarizes YOUR drafted
// side and flags gaps; it does NOT feed the recommendation engine. Tags come from
// imported exporter metadata, with fresher live Bridge values overriding it.
// Champions without tag metadata simply do not contribute.

export type CompAnalysis = {
  pickCount: number;
  taggedCount: number;
  counts: Record<string, number>;
  physical: number;
  magic: number;
  // Translation keys, not English text — call t() on each to display.
  gaps: string[];
};

export type LiveTags = Record<string, string[]>;

/** Look up a champion's imported/live game-native tags. */
export function championTags(championId: string, liveTags?: LiveTags): string[] {
  return liveTags?.[championId] ?? [];
}

export function analyzeComp(championIds: string[], liveTags?: LiveTags): CompAnalysis {
  const counts: Record<string, number> = {};
  let taggedCount = 0;
  let physical = 0;
  let magic = 0;
  for (const id of championIds) {
    const tags = championTags(id, liveTags);
    if (tags.length === 0) continue;
    taggedCount += 1;
    for (const tag of tags) counts[tag] = (counts[tag] ?? 0) + 1;
    if (tags.includes("AD")) physical += 1;
    if (tags.includes("AP") || tags.includes("Magic")) magic += 1;
  }

  const gaps: string[] = [];
  // Only judge once the comp is taking shape and we actually have tag data.
  if (championIds.length >= 3 && taggedCount > 0) {
    if (!counts["Tank"]) gaps.push("comp.gap.noFrontline");
    if (!counts["CC"]) gaps.push("comp.gap.noCC");
    if (physical > 0 && magic === 0) gaps.push("comp.gap.allPhysical");
    else if (magic > 0 && physical === 0) gaps.push("comp.gap.allMagic");
  }

  return { pickCount: championIds.length, taggedCount, counts, physical, magic, gaps };
}
