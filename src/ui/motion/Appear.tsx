// A panel/content fade-in (and fade-out when wrapped in <AnimatePresence>). Used
// for detail panels and other content that appears on demand. No-op under reduced
// motion. Re-mount it with a `key` to replay on content change.

import type { ReactNode } from "react";
import { motion } from "motion/react";
import { appearVariants } from "./config";
import { useReducedMotionPref } from "./useReducedMotionPref";

export function Appear({ children, className }: { children: ReactNode; className?: string }) {
  const reduced = useReducedMotionPref();
  if (reduced) return <div className={className}>{children}</div>;
  return <motion.div className={className} variants={appearVariants} initial="hidden" animate="show" exit="exit">{children}</motion.div>;
}
