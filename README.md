# LT AI Coach

LT AI Coach is a Windows draft companion for Teamfight Manager 2. It imports
game data through the LT AI Coach Exporter mod and can follow a live draft
through the LT AI Coach Bridge mod. Recommendations are calculated locally
from match history, role coverage, player proficiency, champion interactions,
patch context, and the current draft state.

The application supports Normal, Fearless, and Fearless (Hard) series,
including live draft-rule detection and one to five bans per side.

This repository primarily serves to provide source transparency and source
sharing. Casual users should download the executable from the Steam Workshop—it
is the easiest option, provided you trust me, of course.

## Companion mods

- [LT AI Coach Bridge](https://github.com/VeloceGaming/lt_ai_coach_bridge)
  reports live team context, draft rules, bans, and picks.
- [LT AI Coach Exporter](https://github.com/VeloceGaming/lt_ai_coach_exporter)
  exports the game database used by the coach.

Both mods are required for the complete live workflow. The Coach can still be
used for manual draft exploration after data has been imported.

## How it works

The Coach requests a fresh snapshot from the Exporter through its local
application-data folder. The Exporter reads the currently loaded game and
writes champion metadata, teams, players, match history, athlete proficiency,
and patch history. The Coach imports that snapshot into its local SQLite
database and calculates recommendations without an online service.

During a live draft, the Bridge sends the current teams, draft rules, bans, and
picks to the Coach over local UDP. See [docs/architecture.md](docs/architecture.md)
for the complete runtime flow and component boundaries.

## Features

- Live and manual draft recommendations.
- Normal, Fearless, and Fearless (Hard) series rules.
- Manual drafts with one to five bans per side.
- Role-aware champion statistics and role switching.
- Tier list, player proficiency, synergy, matchup, and patch evidence.
- Automatic live draft-mode and ban-count detection.
- Traditional Chinese translation support and editable community translations.
- Opt-in performance diagnostics spanning the Coach, Bridge, and Exporter.

## Privacy

Recommendation scoring runs locally. Debug performance logging is disabled by
default and records only when Debug Mode is enabled in Settings.

## Requirements

- Windows 10 or Windows 11.
- Teamfight Manager 2.
- Microsoft Edge WebView2 Runtime.
- Node.js with npm and the Rust MSVC toolchain when building from source.

## Development

Install dependencies and launch the Tauri development application:

```powershell
npm install
npm run tauri:dev
```

Run automated checks:

```powershell
npm test -- --run
cd src-tauri
cargo test
```

Build an optimized executable:

```powershell
npm run tauri:build
```

## Project layout

- `src/` — React and TypeScript interface, stores, and frontend tests.
- `src-tauri/` — Rust backend, importer, recommendation engine, live bridge,
  and Windows application configuration.
- `assets/` — interface fonts, glyphs, icons, and champion artwork.
- `data/catalog/` — reproducible base-game portrait metadata compiled into the app.
- `translations/` — community-maintained interface translations.
- `scripts/` — catalog and performance-report utilities.
- `docs/` — architecture, requirements, asset provenance, and diagnostics.

## Translations

English text is built into the application. Additional languages use
`translations/<language-tag>/base.json`. The app can export a translation
template from Settings and automatically adds new English fallback keys when a
selected translation is missing entries.

See `translations/README-FIRST.txt` for the translation workflow.

If you have created a translation and want it packaged with the executable,
send me your `<language-tag>/base.json` file. Please ensure that the translation
is accurate and clear so users are not confused.

## Assets

Teamfight Manager 2 names and game-derived artwork remain the property of Team
Samoyed and their respective rights holders. See `docs/asset-sources.md` for
provenance and reproduction notes. Fonts retain their respective licenses.

## License

The original LT AI Coach source code is licensed under the GNU General Public
License version 3 only (`GPL-3.0-only`). See [LICENSE](LICENSE).

Game-derived artwork and other third-party assets are not relicensed under the
GPL. They remain subject to their respective rights and licenses, as described
in [docs/asset-sources.md](docs/asset-sources.md).
