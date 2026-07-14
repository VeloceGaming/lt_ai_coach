# Architecture

```text
save_*.data
    |
    v
SaveDataProvider
    |-- ExistingProbeProvider
    `-- NativeParserProvider (future)
    |
    v
Normalized SQLite/JSON data
    |
    +--> Statistics engine
    |       role performance
    |       synergy and counters
    |       player proficiency
    |       confidence
    |
    +--> Draft state and rules
    |       Normal/Fearless/Fearless Hard
    |       manual actions and history
    |
    `--> Lineup optimizer
            |
            +--> local candidate display
            `--> compact LLM evidence request
```

## Runtime Split

```text
React + TypeScript webview
    |
    | high-level Tauri commands
    v
Rust application backend
    |-- save provider and process execution
    |-- normalized SQLite database
    |-- statistics and lineup optimizer
    |-- draft state and legality
    `-- LLM provider clients and response cache
```

The frontend never receives raw replay files, database access, API keys, or
arbitrary process execution. Commands return compact serializable view models.

The UI and recommendation engine depend on the normalized schema, not directly
on the probe's debug text files. This allows the save provider to be replaced
without rewriting the rest of the application.

LLM requests are explicit and cached by a hash of:

- Save/statistics version.
- Draft mode, set, side, picks, bans, and Fearless exclusions.
- Active roster and scoring configuration.
- Model and prompt version.
