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
the game's `style/champion_view` face/center offsets. Run
`npm run catalog:portraits` to reproduce it. This catalog does not use the Meta
Dashboard-derived portrait metadata above.

Workshop champion portraits are not bundled. The portrait repair command asks
the Exporter to read them from the user's own game installation and stores the
result in the runtime `generated/mod-portraits/` cache.

Game-derived artwork remains subject to the game's applicable rights.
