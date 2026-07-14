# UI Modernization Notes

## Current stage

Stage 2 — Player Hub proving ground (in progress).

## Protected behavior

- Game detection, bridge polling, import, recommendation, scoring, draft legality, data schemas, and Zustand action semantics remain unchanged.
- Stage 1 adds navigation commands, shortcut guards, focus presentation, and automatic-state semantics only.

## Stage 1 changes

- Added one shared definition for the seven existing workspace navigation commands.
- Added `Ctrl+1` through `Ctrl+7` navigation with guards for text-entry controls.
- Added consistent keyboard-focus styling.
- Added semantic workspace and connection-state attributes without changing bridge behavior.
- No dependencies added.

## Verification

- `npm test -- --run`: passed, 48 tests across 6 files.
- `npm run build`: passed (`tsc` and Vite production build).
- Manual Tauri interaction checklist: passed and confirmed by the user.

## Stage checkpoint

- Stage 1 is accepted.
- Current version: Stage 1 complete; Stage 2 has not started.
- Last tested: navigation rail, `Ctrl+1`–`Ctrl+7`, text-entry shortcut guards, visible keyboard focus, stable bridge updates, and live Draft updates.

## Current files

- Added: `src/ui/commands/appCommands.ts`
- Added: `src/ui/commands/appCommands.test.ts`
- Added: `src/ui/hooks/useAppShortcuts.ts`
- Added: `docs/ui-modernization-notes.md`
- Updated: `src/ui/App.tsx`
- Updated: `src/ui/components/NavigationRail.tsx`
- Updated: `src/ui/components/FullAppShell.tsx`
- Updated: `src/ui/styles.css`

## Stage 2 plan in implementation

- Player Hub UI state moved to a UI-only Zustand store so it survives workspace navigation.
- Player directory gains arrow/Home/End keyboard selection and Shift+F10 context access.
- Player rows gain an initial marking menu using existing inspect and filter operations only.
- Layout is tightened into bounded, independently scrolling panes using the supplied reference for density and selection treatment.
- Protected game and analysis behavior remains unchanged.

## Stage 2 verification so far

- `npm test -- --run`: passed, 50 tests across 7 files.
- `npm run build`: passed.
- `npm run tauri:build`: passed; release executable rebuilt.
- Tauri Player Hub normal state: visually inspected at 1600 × 1032.
- Right-click marking menu and Escape cancellation: passed.
- Athlete selection persisted across a Tier List round trip: passed.
- Stage 2 remains awaiting user visual acceptance.

## Stage 2 visual reset

- The first coded Player Hub pass was rejected at the visual checkpoint: it remained too close to the existing dashboard treatment and its Inspect radial command was redundant.
- Production Stage 2 implementation is paused.
- A standalone interactive prototype now lives at `design/prototypes/player-hub/` and is the active discussion artifact.
- The prototype follows the supplied elastic-layout brief and does not modify production code.

## Interactive prototype verification

- Current artifact: `design/prototypes/player-hub/`.
- Browser viewport: 1440 × 1024.
- Verified: Hunting, Reading, Digging, Comparing, Scanning, light Reading, roster/pool filters, stat morph, pin comparison, scan ranking, and radial dead-zone cancellation.
- `node --check design/prototypes/player-hub/prototype.js`: passed.
- `npm test -- --run`: passed, 50 tests across 7 files.
- Production Stage 2 remains paused until this prototype is accepted or revised.

## Practical current-app fix pass

- Removed visible navigation shortcut labels while preserving `Ctrl+1`–`Ctrl+7` behavior.
- Reworked Player Hub marking input to right-press, directional drag, and release; dead-zone and unused-direction releases cancel.
- Removed the redundant Inspect command.
- Scaled the full desktop UI by 6% inside the existing window dimensions.
- Unified Player Hub portrait/card surfaces and tightened the Stats champion atlas spacing.
- Verification pending.

### Verification

- `npm test -- --run`: passed, 55 tests across 8 files.
- `npm run build`: passed.
- `npm run tauri:build`: passed; release executable rebuilt and relaunched.
- Manual Player Hub, Patch Notes, Stats parity, and marking-menu checks remain with the user because active user input was detected in the Tauri window.

### Verification

- `npm test -- --run`: passed, 55 tests across 8 files.
- `npm run build`: passed.
- `npm run tauri:build`: passed; release executable rebuilt.
- Fixed 1600 × 1000 WebView: 6% UI scale visually fits without changing the native window size.
- Expanded navigation rail: verified visible shortcut labels are removed.
- Automated window input was stopped after user interaction was detected; Player Hub marking gestures, portrait surface, and Stats atlas density await the user's manual checkpoint.

## Corrective readability and right-click pass

- Removed the global CSS zoom because it was too subtle and shifted fixed-position pointer UI.
- Player Hub and Patch Notes now use directly enlarged text, rows, portraits, cards, filters, and headings.
- Native WebView context menus are suppressed inside the full app shell.
- Right-click targeting no longer changes the selected athlete.
- Stats champion tiles now match Tier List: 64 × 78 portraits, 64 px tile width, 12 px grid gap, and matching name treatment.
- Verification pending.
