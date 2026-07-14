# Performance logging

Performance logging is disabled by default. Enable **Settings → Performance
Diagnostics → Debug mode** before entering Stadium Entrance. While enabled, LT
AI Coach writes structured performance events to:

`%LOCALAPPDATA%/com.lttools.lt-ai-coach/logs/performance.jsonl`

Turning Debug mode off stops new events. The log rotates to
`performance.jsonl.1` at 10 MiB. A background writer batches
coach-side file writes and flushes once per second.

Each JSON line contains:

- `timestampUnixMs` and `sessionId` for correlation;
- `component`: `exporter`, `bridge`, or `coach`;
- `action`: the measured stage or lifecycle event;
- `durationUs`: elapsed microseconds when the event has a duration;
- `details`: action count, bridge revision, status, or similar context.

For each live ban or pick, expect `bridge/draft_action`,
`coach/draft_action_received`, and `coach/live_recommendation` events. Imports
include exporter serialization/write stages, export wait time, SQLite import,
recommendation-cache preparation, and total import time.

When Debug mode is enabled, the exporter writes one small
`performance_export.tsv` after completing its measured stages. The manifest
remains the final export file, so the coach never ingests a partial trace. The
bridge sends performance samples over its existing loopback UDP channel and
performs no performance-log disk I/O in the game. Both mods check the shared
debug flag only at export or Stadium Entrance, not every frame.

Summarize the complete log with:

```powershell
.\scripts\summarize-performance.ps1
```

Use `-Tail 500` to analyze only the most recent events. The report shows count,
average, p50, p95, maximum, and the 15 slowest individual events.
