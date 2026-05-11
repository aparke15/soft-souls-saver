# Soft Souls Saver

CLI-first save manager for Souls and Souls-like games.

The current focus is a small Rust workspace that can detect supported Steam games on Linux, resolve save directories from a manifest, and create timestamped zip backups. The code is intentionally structured so future adapters can reuse the same core logic from a desktop UI or background host without moving business logic into those adapters.

## Workspace

- `savecore`: reusable application core and domain logic
- `savectl`: command-line adapter over `savecore`

Important core modules:

- `savecore::api`: application-facing functions for CLI/UI/host adapters
- `savecore::types`: serde-friendly request/response boundary types
- `savecore::steam`: Steam library and app detection
- `savecore::resolve`: save path expansion and candidate selection
- `savecore::backup`: backup planning and zip creation
- `savecore::ops`: low-level zip/hash helpers

## Current Commands

Detect installed supported games:

```sh
cargo run -p savectl -- detect
```

Preview a backup plan:

```sh
cargo run -p savectl -- backup elden-ring --dry-run
```

Create a backup using the default layout:

```sh
cargo run -p savectl -- backup elden-ring
```

Create a backup at a custom zip path or directory:

```sh
cargo run -p savectl -- backup elden-ring --out /tmp/elden-ring-backup.zip
cargo run -p savectl -- backup elden-ring --out /tmp/elden-ring-backup-dir
```

Provide a Steam ID64 explicitly when needed:

```sh
cargo run -p savectl -- backup elden-ring --steam-id64 76561198000000000
```

## Backup Layout

By default, backups are written under:

```text
backups/<game-slug>/<timestamp>/payload.zip
backups/<game-slug>/<timestamp>/metadata.json
```

`metadata.json` records the selected source save path, candidate paths considered, selected patterns, output zip path, SHA-256, timestamp, and dry-run state.

## Development

Format, lint, and test:

```sh
cargo fmt
cargo clippy
cargo test
```

Linux support is first-class for now. Windows paths are represented in the manifest and should stay behind clean abstractions until a Windows implementation pass is made.

## Non-Goals For Now

- No GUI yet
- No daemon/background host yet
- No global hotkeys yet
- No restore command yet
- No Steam Cloud handling yet
- No process injection or game hooks
