// Renders a single champion portrait cropped from its sprite sheet.

import type { ChampionPortrait } from "../types";

type PortraitScale = "contain" | "champion";

// Champion sprites are tightly cropped, so fitting every crop independently
// makes narrow champions look much larger than wide ones. Recommendation cards
// use a shared 51 px source-width scale (Soldier's crop) to preserve the game's
// relative sprite proportions while still capping unusually tall frames.
export function portraitScale(portrait: ChampionPortrait, width: number, height: number, scaleMode: PortraitScale) {
  return scaleMode === "champion"
    ? Math.min(width / 51, height / portrait.height)
    : Math.min(width / portrait.width, height / portrait.height);
}

export function ChampionPortraitView({ portrait, width = 40, height = 40, scaleMode = "champion", fixedCenter = false }: { portrait: ChampionPortrait | null; width?: number; height?: number; scaleMode?: PortraitScale; fixedCenter?: boolean }) {
  const frameStyle = { width: `${width}px`, height: `${height}px` };
  const className = `champion-portrait${fixedCenter ? " fixed-center" : ""}`;
  if (!portrait) return <span className={`${className} missing`} style={frameStyle} aria-hidden="true" />;
  const scale = portraitScale(portrait, width, height, scaleMode);
  const cropWidth = `${portrait.width * scale}px`;
  const cropHeight = `${portrait.height * scale}px`;
  return <span className={className} style={frameStyle} aria-hidden="true">
    <span className="champion-portrait-frame image-crop" style={{ position: fixedCenter ? undefined : "relative", width: cropWidth, minWidth: cropWidth, maxWidth: cropWidth, height: cropHeight, minHeight: cropHeight, maxHeight: cropHeight }}>
      <img src={portrait.path} alt="" draggable={false} style={{ left: `${-portrait.x * scale}px`, top: `${-portrait.y * scale}px`, width: `${portrait.sheetWidth * scale}px`, height: `${portrait.sheetHeight * scale}px` }} />
    </span>
  </span>;
}
