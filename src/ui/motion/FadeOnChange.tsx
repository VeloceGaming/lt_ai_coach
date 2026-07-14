// Cross-fades its contents whenever `changeKey` changes (e.g. a new set of
// recommendations), leaving selection/hover within unaffected. No-op under
// reduced motion. The key on the inner element makes React replay the fade.

import type { ReactNode } from "react";
import { motion } from "motion/react";
import { DURATION, EASE_OUT } from "./config";
import { useReducedMotionPref } from "./useReducedMotionPref";

export function FadeOnChange({ changeKey, className, duration = DURATION.base, children }: { changeKey: string; className?: string; duration?: number; children: ReactNode }) {
  const reduced = useReducedMotionPref();
  if (reduced) return <div className={className}>{children}</div>;
  return <motion.div key={changeKey} className={className} initial={{ opacity: 0 }} animate={{ opacity: 1 }} transition={{ duration, ease: EASE_OUT }}>{children}</motion.div>;
}
