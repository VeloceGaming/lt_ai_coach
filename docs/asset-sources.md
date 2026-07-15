# Asset Sources

This project contains game-derived base champion portraits. It does not bundle
Workshop champion artwork or third-party champion metadata catalogs.

## Champion Metadata

The LT AI Coach Exporter reads each enabled champion from the user's loaded
game and writes `champion_metadata.json`. Its champion IDs, categories, and raw
tags come from the game database, including locally installed Workshop
champions. The exporter derives LT AI Coach's role prior from those fields with
a small, documented heuristic in its source. Actual match-role evidence
continuously replaces that eight-game prior in the Coach.

No champion metadata from Meta Dashboard is shipped with this repository.

## Champion Portraits

`data/catalog/portraits.json` is independently generated from the locally
installed game's `bundle.game_data` by `scripts/build-portrait-catalog.py`. It
reads each champion `#sheet.png`, the first `idle` frame in `#anim.fanim`, and
the game's `style/champion_view` face/center offsets, then normalizes the frame
to its visible alpha bounds. Install Pillow and run `npm run catalog:portraits`
to reproduce it. This catalog does not use the Meta Dashboard-derived portrait
metadata above.

Workshop champion portraits are not bundled. The portrait repair command asks
the Exporter to read them from the user's own game installation and stores the
result in the runtime `generated/mod-portraits/` cache.

## Implementation Lessons

- The animation metadata is authoritative. Resolve the portrait from the
  champion's exact `#sheet.png` and the first `idle` frame in `#anim.fanim`;
  do not guess a crop or special-case champions such as Kog'Maw.
- Apply the same frame-extraction and visible-alpha normalization to bundled
  base champions and repaired Workshop champions. Sprite sheets often contain
  transparent padding, so displaying the raw frame rectangle can make an
  otherwise correct sprite look too small or vertically off-center.
- Center the normalized visible image in the UI and retain only meaningful
  game-provided `champion_view` offsets. Avoid per-champion visual patches and
  global CSS nudges that merely compensate for transparent source padding.
- Workshop repair must not scan or accept arbitrary filesystem paths. The
  Exporter only reads exact resource filenames and directories registered for
  the loaded mod by the game SDK, then validates path containment, file size,
  PNG signatures, and FANIM JSON before returning data to the Coach.
- Champion display names are localization data, not stable IDs. Repair writes
  generated `<language>/mod.json` entries from the loaded game's champion
  translations, including `en/mod.json`, so English does not fall back to a
  raw internal mod name such as `test mod Azir`.
- Keep the generated base catalog deterministic and verify that every emitted
  portrait is tightly alpha-bounded. This catches pipeline regressions across
  the whole roster instead of relying on a few visually inspected champions.

Game-derived artwork remains subject to the game's applicable rights.
