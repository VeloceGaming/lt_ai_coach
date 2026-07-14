// Auto-discovers champion sprites that aren't in the bundled catalog. Modded (and
// base) champions keep their art as `<id>.png` in the served sprite folders, so for
// any champion the backend hands us with no portrait we probe those folders by id.
// The sheets are horizontal strips of square frames (frame side = sheet height), so
// we crop the first frame. Results are cached, and missing files resolve to null.

import type { ChampionPortrait } from "../types";
import championCatalog from "../../../data/catalog/champions.json";

// Served from Vite's publicDir ("assets"), so these map to /<folder>/<id>.png.
const SPRITE_FOLDERS = ["mod-champions", "champions"];

const cache = new Map<string, Promise<ChampionPortrait | null>>();
const catalogPortraits = new Map(
  (championCatalog.champions as Array<{
    id: string;
    asset?: {
      sheet: string;
      sheetWidth: number;
      sheetHeight: number;
      frame: { x: number; y: number; w: number; h: number };
    };
  }>)
    .filter((champion) => champion.asset)
    .map((champion) => {
      const asset = champion.asset!;
      return [champion.id, {
        path: `/${asset.sheet.replace(/^assets[\\/]/, "").replaceAll("\\", "/")}`,
        sheetWidth: asset.sheetWidth,
        sheetHeight: asset.sheetHeight,
        x: asset.frame.x,
        y: asset.frame.y,
        width: asset.frame.w,
        height: asset.frame.h,
      } satisfies ChampionPortrait] as const;
    }),
);

function loadImageSize(src: string): Promise<{ width: number; height: number } | null> {
  return new Promise((resolve) => {
    const image = new Image();
    image.onload = () => resolve({ width: image.naturalWidth, height: image.naturalHeight });
    image.onerror = () => resolve(null);
    image.src = src;
  });
}

async function resolveOne(championId: string): Promise<ChampionPortrait | null> {
  const catalogPortrait = catalogPortraits.get(championId);
  if (catalogPortrait) return catalogPortrait;
  for (const folder of SPRITE_FOLDERS) {
    const path = `/${folder}/${championId}.png`;
    const size = await loadImageSize(path);
    if (size && size.width > 0 && size.height > 0) {
      // Strip of square frames; crop the first one (frame side = sheet height).
      const side = Math.min(size.height, size.width);
      return { path, sheetWidth: size.width, sheetHeight: size.height, x: 0, y: 0, width: side, height: side };
    }
  }
  return null;
}

/** Resolve (and cache) a sprite for one champion id, or null if no image exists. */
export function resolvePortrait(championId: string): Promise<ChampionPortrait | null> {
  let pending = cache.get(championId);
  if (!pending) {
    pending = resolveOne(championId);
    cache.set(championId, pending);
  }
  return pending;
}

/** Resolve sprites for the given ids, returning only the ones that were found. */
export async function resolveMissingPortraits(championIds: string[]): Promise<Map<string, ChampionPortrait>> {
  const found = new Map<string, ChampionPortrait>();
  await Promise.all(
    [...new Set(championIds)].map(async (id) => {
      const portrait = await resolvePortrait(id);
      if (portrait) found.set(id, portrait);
    }),
  );
  return found;
}
