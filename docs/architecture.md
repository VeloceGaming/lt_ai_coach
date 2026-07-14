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
            `--> local candidate display
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
    `-- draft state and legality
```

The frontend never receives raw replay files, direct database access, or
arbitrary process execution. Commands return compact serializable view models.

The UI and recommendation engine depend on the normalized schema, not directly
on the probe's debug text files. This allows the save provider to be replaced
without rewriting the rest of the application.
