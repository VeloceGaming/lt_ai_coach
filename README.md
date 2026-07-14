# LT AI Coach

LT AI Coach is a Windows draft companion for Teamfight Manager 2. It imports
game data through the LT AI Coach Exporter mod and can follow a live draft
through the LT AI Coach Bridge mod. Recommendations are calculated locally
from match history, role coverage, player proficiency, champion interactions,
patch context, and the current draft state.

The application supports Normal, Fearless, and Fearless (Hard) series,
including live draft-rule detection and one to five bans per side.

This reposit mainly serves the purpose of source-transparancy and source-sharing. If you are a casual user, download the executable from Steam workshop is the easist way for you, if you trust me, of course.

## Companion mods

- [LT AI Coach Bridge](https://github.com/VeloceGaming/lt_ai_coach_bridge)
  reports live team context, draft rules, bans, and picks.
- [LT AI Coach Exporter](https://github.com/VeloceGaming/lt_ai_coach_exporter)
  exports the game database used by the coach.

Both mods are required for the complete live workflow. The Coach can still be
used for manual draft exploration after data has been imported.

## Features

- Live and manual draft recommendations.
- Normal, Fearless, and Fearless (Hard) series rules.
- Configurable one-to-five-ban manual drafts.
- Role-aware champion statistics and role switching.
- Tier list, player proficiency, synergy, matchup, and patch evidence.
- Automatic live draft-mode and ban-count detection.
- Traditional Chinese translation support and editable community translations.
- Opt-in performance diagnostics spanning the Coach, Bridge, and Exporter.

## Privacy

Recommendation scoring runs locally.Debug performance logging is disabled by
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
- `data/catalog/` — champion and portrait catalogs compiled into the app.
- `translations/` — community-maintained interface translations.
- `scripts/` — catalog and performance-report utilities.
- `docs/` — architecture, requirements, asset provenance, and diagnostics.

## Translations

English text is built into the application. Additional languages use
`translations/<language-tag>/base.json`. The app can export a translation
template from Settings and automatically adds new English fallback keys when a
selected translation is missing entries.

See `translations/README-FIRST.txt` for the translation workflow.

If you made a translation and want me to pack it along with the executable, send me the `<language-tag>/base.json of your language. Please ensure the quality of the translation, I don't want people get confused.

## Assets

Teamfight Manager 2 names and game-derived artwork remain the property of their
respective rights holders @TeamSamoyed. See `docs/asset-sources.md` for provenance and
reproduction notes. Fonts retain their respective licenses.

## License

The original LT AI Coach source code is licensed under the GNU General Public
License version 3 only (`GPL-3.0-only`). See [LICENSE](LICENSE).

Game-derived artwork and other third-party assets are not relicensed under the
GPL. They remain subject to their respective rights and licenses, as described
in [docs/asset-sources.md](docs/asset-sources.md).
