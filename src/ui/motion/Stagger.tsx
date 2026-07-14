// Reusable list-stagger: wrap a list in <StaggerList> and each child in
// <StaggerItem> and they fade up one after another on mount. Falls back to plain
// divs when reduced motion is on, so callers never branch themselves.

import type { ReactNode } from "react";
import { motion } from "motion/react";
import { listContainer, listItem } from "./config";
import { useReducedMotionPref } from "./useReducedMotionPref";

export function StaggerList({ children, className }: { children: ReactNode; className?: string }) {
  const reduced = useReducedMotionPref();
  if (reduced) return <div className={className}>{children}</div>;
  return <motion.div className={className} variants={listContainer} initial="hidden" animate="show">{children}</motion.div>;
}

export function StaggerItem({ children, className }: { children: ReactNode; className?: string }) {
  const reduced = useReducedMotionPref();
  if (reduced) return <div className={className}>{children}</div>;
  return <motion.div className={className} variants={listItem}>{children}</motion.div>;
}
