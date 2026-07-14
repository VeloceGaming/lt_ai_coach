# Architecture

LT AI Coach consists of a Windows desktop application and two Teamfight
Manager 2 mods. All communication and recommendation processing remain on the
user's computer.

## Runtime data flow

```text
Teamfight Manager 2
    |
    +-- LT AI Coach Exporter
    |       reads the loaded game database and match history
    |       writes a requested snapshot to local application data
    |                           |
    |                           v
    |                    Coach import pipeline
    |                           |
    |                           v
    |                    Local SQLite database
    |                           |
    |                           v
    |              Statistics and recommendation engine
    |                           |
    +-- LT AI Coach Bridge      |
            sends live draft   |
            state over UDP ----+
                                |
                                v
                       React/Tauri interface
```

## Coach application

The React and TypeScript frontend in `src/ui/` displays imported statistics,
manual draft tools, live draft state, and recommendations. It calls a limited
set of Tauri commands and does not read the game or SQLite database directly.

The Rust backend in `src-tauri/src/` owns the application data and contains:

- the Exporter request and import workflow;
- the normalized SQLite database;
- role statistics, champion interactions, and patch evidence;
- draft legality, Fearless history, and lineup optimization;
- live Bridge state received over local UDP; and
- translation, portrait-repair, and opt-in performance-log support.

## Exporter snapshots

When an import begins, the Coach writes a request under
`%LOCALAPPDATA%/com.lttools.lt-ai-coach/exporter/`. The Exporter responds from
inside the running game with a fresh manifest and snapshot files containing
enabled champions, game-native champion metadata, teams, players, match
records, athlete proficiency, and patch history.

The Coach validates and imports the snapshot into SQLite. Recommendations then
use the imported data; the raw snapshot files are not sent to an online
service.

The portrait-repair workflow uses the same request directory. Missing Workshop
portraits are read from the user's loaded game by the Exporter and cached by
the Coach instead of being bundled in this repository.

## Live Bridge

The Bridge reports team identity, champion tags, Stadium Entrance draft rules,
ban-slot count, bans, and picks over local UDP. The Coach treats this as live
session state: it can override the visible manual draft settings while a series
is active without changing the user's saved preferences.

If the Bridge is unavailable, imported statistics and manual draft exploration
continue to work. A fresh Exporter snapshot is still required whenever the
underlying game data must be updated.

## Local storage and diagnostics

The SQLite database, Exporter exchange files, repaired portrait cache, user
translations, and performance logs live under the Coach's local application
data directory. Official translations shipped beside the executable are
read-only defaults. At runtime the Coach layers user `base.json` overrides and
generated `mod.json` champion names over those defaults, so Steam Workshop
updates can refresh official files without replacing user work. Debug
performance logging is disabled by default and is recorded across the Coach,
Bridge, and Exporter only while Debug Mode is enabled.
