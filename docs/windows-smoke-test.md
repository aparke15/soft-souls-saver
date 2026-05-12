# Windows Manual Smoke Test

This checklist is for a real Windows Steam machine. It does not require an installer.

## Setup

1. Unzip `soft-souls-saver-windows-x86_64.zip` into a writable folder, for example `C:\Users\<you>\Downloads\soft-souls-saver`.
2. Open PowerShell in the unzipped folder.
3. Make sure Steam is installed and at least one supported game is installed:
   - Elden Ring
   - Lies of P
   - Dark Souls III
4. Close the game before testing backup or restore writes.

## CLI Smoke Test

Run detection:

```powershell
.\savectl.exe detect
```

Expected: installed supported games are listed. If nothing is listed, verify Steam is installed normally and the game is installed in Steam.

Show config:

```powershell
.\savectl.exe show-config
```

Expected: the backup root is shown. By default it is `backups` relative to the current folder.

Check process/activity warnings:

```powershell
.\savectl.exe check-running ds3
```

Expected: reports whether the selected game appears to be running. Try once with the game closed and, if practical, once with it open.

Preview a backup:

```powershell
.\savectl.exe backup ds3 --dry-run
```

Expected: source save path and output zip path are shown. Use another supported slug if DS3 is not installed: `elden-ring` or `lies-of-p`.

Create a backup:

```powershell
.\savectl.exe backup ds3
```

Expected: `backups\<game-slug>\<timestamp>\payload.zip` and `metadata.json` are created.

List and inspect snapshots:

```powershell
.\savectl.exe list-snapshots ds3
.\savectl.exe show-snapshot ds3/<timestamp>
```

Expected: the new snapshot appears and can be inspected.

Preview restore only:

```powershell
.\savectl.exe plan-restore ds3 ds3/<timestamp>
.\savectl.exe restore ds3 ds3/<timestamp> --dry-run
```

Expected: destination, file count, warnings, and safety-backup information are shown. Blocking warnings should prevent actual restore.

Perform restore only if you intentionally want to test writes:

```powershell
.\savectl.exe restore ds3 ds3/<timestamp>
```

Expected: restore completes and creates a safety backup first if an existing save directory is replaced.

## UI Smoke Test

Run:

```powershell
.\saveui.exe
```

Expected:

- Detected supported games appear in the left pane.
- Selecting a game shows snapshots for that game.
- Backup creates a new snapshot.
- Plan restore shows destination, file count, warnings, and blocking conditions.
- Restore is disabled until the restore confirmation checkbox is checked and the plan has no blocking warnings.

## Useful Overrides

For testing against a copied Steam folder, set a Steam root override before launching commands:

```powershell
$env:soft_souls_steam_root = "D:\Steam"
.\savectl.exe detect
```

Unset it when done:

```powershell
Remove-Item Env:soft_souls_steam_root
```

## Notes

- The release bundle embeds the game manifest at compile time; no separate asset file is required.
- This is a zip bundle only. There is no installer, code signing, auto-update, tray app, daemon, or background watcher.
- Windows support still needs real-machine validation across common Steam library layouts and game installs.
