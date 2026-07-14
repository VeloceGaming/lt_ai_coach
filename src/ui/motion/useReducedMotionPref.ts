// Effective "reduce motion" = the user's in-app toggle OR the OS setting. Motion
// components call this and skip animating when it's true.

import { useReducedMotion } from "motion/react";
import { useThemeStore } from "../stores/useThemeStore";

export function useReducedMotionPref(): boolean {
  const os = useReducedMotion();
  const user = useThemeStore((state) => state.reduceMotion);
  return Boolean(user || os);
}
