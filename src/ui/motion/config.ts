// Shared motion settings so every animation uses the same subtle timing. Mirrors
// the --motion-* / easing tokens in tokens.css (Motion needs JS numbers/arrays).

export const DURATION = { fast: 0.14, base: 0.24, slow: 0.34 } as const;
export const EASE_OUT = [0, 0, 0.2, 1] as const;
export const EASE_STANDARD = [0.4, 0, 0.2, 1] as const;

// Subtle list-stagger: children fade up a few px, one shortly after another.
export const listContainer = {
  hidden: {},
  show: { transition: { staggerChildren: 0.04, delayChildren: 0.02 } },
};

export const listItem = {
  hidden: { opacity: 0, y: 6 },
  show: { opacity: 1, y: 0, transition: { duration: DURATION.base, ease: EASE_OUT } },
};

// Panel/content appear (used by <Appear>).
export const appearVariants = {
  hidden: { opacity: 0, y: 8 },
  show: { opacity: 1, y: 0, transition: { duration: DURATION.base, ease: EASE_OUT } },
  exit: { opacity: 0, y: 4, transition: { duration: DURATION.fast, ease: EASE_STANDARD } },
};
