# Soft Souls Saver

CLI-first save manager for Souls and Souls-like games, with a small native desktop shell.

The current focus is a small Rust workspace that can detect supported Steam games on Linux and Windows, resolve save directories from a manifest, create timestamped zip backups, inspect snapshots, and restore through an explicit planning/safety flow. The code is intentionally structured so adapters reuse the same core logic without moving business logic out of the core crate.

## Workspace

- `savecore`: reusable application core and domain logic
- `savectl`: command-line adapter over `savecore`
- `saveui`: minimal `egui/eframe` desktop shell over `savecore`

Important core modules:

- `savecore::api`: application-facing functions for CLI/UI/host adapters
- `savecore::types`: serde-friendly request/response boundary types
- `savecore::steam`: Steam library and app detection
- `savecore::resolve`: save path expansion and candidate selection
- `savecore::backup`: backup planning and zip creation
- `savecore::restore`: restore planning and execution safety
- `savecore::snapshot`: snapshot listing and inspection
- `savecore::config`: effective settings resolution
- `savecore::process`: Linux and Windows process/activity warnings
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

List and inspect snapshots:

```sh
cargo run -p savectl -- list-snapshots
cargo run -p savectl -- show-snapshot ds3/<timestamp>
```

Preview and perform a restore:

```sh
cargo run -p savectl -- plan-restore ds3 ds3/<timestamp>
cargo run -p savectl -- restore ds3 ds3/<timestamp> --dry-run
cargo run -p savectl -- restore ds3 ds3/<timestamp>
```

Inspect effective config and running-game activity:

```sh
cargo run -p savectl -- show-config
cargo run -p savectl -- check-running ds3
```

Emit typed JSON for automation:

```sh
cargo run -p savectl -- --json detect
cargo run -p savectl -- backup ds3 --dry-run --json
```

Run the desktop shell:

```sh
cargo run -p saveui
```

## Backup Layout

By default, backups are written under:

```text
backups/<game-slug>/<timestamp>/payload.zip
backups/<game-slug>/<timestamp>/metadata.json
```

`metadata.json` records the selected source save path, candidate paths considered, selected patterns, output zip path, SHA-256, timestamp, dry-run state, and advisory activity warnings.

Restore execution always goes through restore planning first. Blocking restore warnings prevent writes; non-blocking warnings and Linux process/activity warnings are surfaced to callers and adapters.

## Platform Support

Linux support detects common Steam roots, parses `libraryfolders.vdf`, resolves Proton save paths through `{steamLibrary}`, and checks running processes through `/proc`.

Windows support in `savecore` detects Steam roots from the native registry keys for Valve Steam, falls back to `ProgramFiles(x86)\Steam` and `ProgramFiles\Steam`, reuses the same Steam library/app manifest parsing, expands Windows manifest variables such as `%APPDATA%`, `%LOCALAPPDATA%`, `%USERPROFILE%`, and `%Documents%`, and checks running processes through `sysinfo`.

The currently supported Windows save locations are manifest-backed for Elden Ring, Lies of P, and Dark Souls III. Windows support has fixture and cross-target type-check coverage, but still needs real-machine manual validation against actual Steam installs and save directories.

## Desktop UI

`saveui` is intentionally small. It lists detected supported games, shows snapshots for the selected game, creates backups, previews restore plans, and only enables restore after an explicit confirmation step. Potentially blocking core calls run on short-lived worker threads and report back to the UI over a channel.

The UI calls `savecore::api` directly. It does not shell out to `savectl` and does not duplicate path resolution, snapshot discovery, backup, or restore safety logic.

## Development

Format, lint, and test:

```sh
cargo fmt
cargo clippy
cargo test
```

Linux support remains the most exercised path. Windows core support exists, but packaging, installer behavior, and release signing are intentionally out of scope.

## Windows Manual Testing

Build Windows release binaries from Linux with the GNU Windows target:

```sh
rustup target add x86_64-pc-windows-gnu
cargo build --release --target x86_64-pc-windows-gnu -p savectl -p saveui
```

Generated zip bundles are written under `dist/` when packaging locally. The bundle contains `savectl.exe`, `saveui.exe`, this README, and the Windows smoke-test checklist. Runtime game metadata is embedded into the binaries, so no separate asset directory is required.

See `docs/windows-smoke-test.md` for the manual checklist to run on a real Windows Steam machine.

## Non-Goals For Now

- No daemon/background host yet
- No global hotkeys yet
- No tray app yet
- No file watching yet
- No automatic backups on exit yet
- No cloud sync yet
- No Windows installer or code signing yet
- No Steam Cloud handling yet
- No process injection or game hooks
