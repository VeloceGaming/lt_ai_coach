# Asset Sources

This prototype contains local, game-derived and Workshop-derived files for
development and personal use.

## Save Probe

Source installation:

`Steam\steamapps\workshop\content\3009300\3738242091`

Bundled file:

`tools/tfm2_save_probe.exe`

SHA-256:

`6110D093772B733D2B041A266C963CDEEA6C2EED2C26571430DE9454DD417315`

The probe is treated as an external provider. Redistribution rights have not
been established. Replace it with an independently implemented parser before
public distribution unless permission is obtained.

## Champion Assets and Catalog

Base and mod champion sprite sheets were copied from the locally installed
Meta Dashboard output. The Eagle development asset came from this workspace's
example mod.

`data/catalog/champions.json` is a reduced export of champion metadata already
produced by the locally installed dashboard. It intentionally excludes match
statistics, team data, player data, and save-specific performance results.

`data/catalog/portraits.json` is independently generated from the locally
installed game's `bundle.game_data` by `scripts/build-portrait-catalog.py`. It
reads each champion `#sheet.png`, the first `idle` frame in `#anim.fanim`, and
the game's `style/champion_view` face/center offsets. Run
`npm run catalog:portraits` to reproduce it. This catalog does not use the Meta
Dashboard-derived portrait metadata above.

`data/catalog/mod-portraits.json` and `assets/generated/mod-portraits/` are
generated from locally installed Workshop champion `.aseprite` resources using
TFM2 Forge's game-compatible conversion semantics. Run
`npm run catalog:mod-portraits` to reproduce them. The generator defaults to the
four locally configured Workshop champion mods and accepts repeatable `--mod`
arguments for other mod directories.

Game-derived artwork and data remain subject to the game's applicable rights.
Do not assume the prototype asset bundle can be redistributed publicly.
