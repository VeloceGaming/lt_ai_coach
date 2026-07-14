export type MarkingDirection = "north" | "east" | "south" | "west";

export function resolveMarkingDirection(deltaX: number, deltaY: number, deadZone = 34): MarkingDirection | null {
  if (Math.hypot(deltaX, deltaY) < deadZone) return null;
  if (Math.abs(deltaX) > Math.abs(deltaY)) return deltaX > 0 ? "east" : "west";
  return deltaY > 0 ? "south" : "north";
}
