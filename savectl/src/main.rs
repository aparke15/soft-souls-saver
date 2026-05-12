use anyhow::Result;
use camino::Utf8PathBuf;
use clap::{Parser, Subcommand};
use savecore::{
    api,
    types::{
        BackupOptions, CheckGameActivityOptions, DetectOptions, EffectiveSettingsOptions,
        ListSnapshotsOptions, RestoreOptions, RestorePlanOptions, ShowSnapshotOptions,
    },
};
use serde::Serialize;
use std::{io, process::ExitCode};

#[derive(Parser)]
#[command(name = "savectl", version, about = "souls-like save manager")]
struct Cli {
    #[arg(long, global = true, help = "Write command results as JSON")]
    json: bool,
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    Detect,
    Backup {
        game_slug: String,
        #[arg(long)]
        out: Option<Utf8PathBuf>,
        #[arg(long)]
        steam_id64: Option<String>,
        #[arg(long)]
        dry_run: bool,
    },
    PlanRestore {
        game_slug: String,
        snapshot: Utf8PathBuf,
        #[arg(long)]
        steam_id64: Option<String>,
    },
    Restore {
        game_slug: String,
        snapshot: Utf8PathBuf,
        #[arg(long)]
        steam_id64: Option<String>,
        #[arg(long)]
        dry_run: bool,
    },
    ListSnapshots {
        game_slug: Option<String>,
    },
    ShowSnapshot {
        snapshot: String,
    },
    ShowConfig,
    CheckRunning {
        game_slug: Option<String>,
    },
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let json = cli.json;

    match run(cli) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            if json {
                print_json_error(&error);
            } else {
                eprintln!("error: {error}");
            }
            ExitCode::FAILURE
        }
    }
}

fn run(cli: Cli) -> Result<()> {
    let output = if cli.json {
        OutputMode::Json
    } else {
        OutputMode::Human
    };

    match cli.cmd {
        Cmd::Detect => detect_cmd(output)?,
        Cmd::Backup {
            game_slug,
            out,
            steam_id64,
            dry_run,
        } => backup_cmd(game_slug, out, steam_id64, dry_run, output)?,
        Cmd::PlanRestore {
            game_slug,
            snapshot,
            steam_id64,
        } => plan_restore_cmd(game_slug, snapshot, steam_id64, output)?,
        Cmd::Restore {
            game_slug,
            snapshot,
            steam_id64,
            dry_run,
        } => restore_cmd(game_slug, snapshot, steam_id64, dry_run, output)?,
        Cmd::ListSnapshots { game_slug } => list_snapshots_cmd(game_slug, output)?,
        Cmd::ShowSnapshot { snapshot } => show_snapshot_cmd(snapshot, output)?,
        Cmd::ShowConfig => show_config_cmd(output)?,
        Cmd::CheckRunning { game_slug } => check_running_cmd(game_slug, output)?,
    }

    Ok(())
}

#[derive(Debug, Clone, Copy)]
enum OutputMode {
    Human,
    Json,
}

#[derive(Debug, Serialize)]
struct JsonError<'a> {
    error: JsonErrorBody<'a>,
}

#[derive(Debug, Serialize)]
struct JsonErrorBody<'a> {
    message: &'a str,
}

fn detect_cmd(output: OutputMode) -> Result<()> {
    let result = api::detect_games(DetectOptions::default())?;
    match output {
        OutputMode::Human => println!("{}", serde_json::to_string_pretty(&result.games)?),
        OutputMode::Json => print_json(&result)?,
    }
    Ok(())
}

fn backup_cmd(
    game_slug: String,
    out: Option<Utf8PathBuf>,
    steam_id64: Option<String>,
    dry_run: bool,
    output: OutputMode,
) -> Result<()> {
    let result = api::backup_game(BackupOptions {
        game_slug,
        output_path: out.map(|path| path.to_string()),
        config_path: None,
        backup_root: None,
        steam_id64,
        steam_root_override: None,
        dry_run,
    })?;

    if matches!(output, OutputMode::Json) {
        return print_json(&result);
    }

    if result.dry_run {
        println!("backup plan for {}", result.slug);
    } else {
        println!("backup created for {}", result.slug);
    }
    println!("source: {}", result.source_save_path);
    println!("zip: {}", result.output_zip_path);
    if let Some(metadata_path) = &result.metadata_path {
        println!("metadata: {metadata_path}");
    }
    if let Some(sha256) = &result.sha256 {
        println!("sha256: {sha256}");
    }
    print_activity_warnings(&result.activity_warnings)?;
    println!(
        "candidates considered: {}",
        result.resolved_candidates.len()
    );
    Ok(())
}

fn plan_restore_cmd(
    game_slug: String,
    snapshot: Utf8PathBuf,
    steam_id64: Option<String>,
    output: OutputMode,
) -> Result<()> {
    let result = api::plan_restore(RestorePlanOptions {
        game_slug,
        snapshot_path: snapshot.to_string(),
        config_path: None,
        backup_root: None,
        steam_id64,
        steam_root_override: None,
    })?;

    if matches!(output, OutputMode::Json) {
        return print_json(&result);
    }

    println!("restore plan for {}", result.target_game_slug);
    println!("snapshot: {}", result.snapshot_inventory.payload_zip_path);
    if let Some(destination) = &result.restore_destination_path {
        println!("destination: {destination}");
    } else {
        println!("destination: unresolved");
    }
    println!("files: {}", result.files_to_restore.len());
    println!("target exists: {}", result.target_exists);
    println!(
        "safety backup recommended: {}",
        result.pre_restore_safety_backup.recommended
    );
    if !result.warnings.is_empty() {
        println!("warnings: {}", serde_json::to_string(&result.warnings)?);
    }
    print_activity_warnings(&result.activity_warnings)?;
    Ok(())
}

fn restore_cmd(
    game_slug: String,
    snapshot: Utf8PathBuf,
    steam_id64: Option<String>,
    dry_run: bool,
    output: OutputMode,
) -> Result<()> {
    let result = api::restore_game(RestoreOptions {
        game_slug,
        snapshot_path: snapshot.to_string(),
        config_path: None,
        backup_root: None,
        steam_id64,
        steam_root_override: None,
        dry_run,
    })?;

    if matches!(output, OutputMode::Json) {
        return print_json(&result);
    }

    if result.dry_run {
        println!("restore dry run for {}", result.target_game_slug);
    } else {
        println!("restore completed for {}", result.target_game_slug);
    }
    println!("destination: {}", result.restore_destination_path);
    println!("files: {}", result.restored_file_count);
    if let Some(backup) = &result.safety_backup {
        println!("safety backup: {}", backup.output_zip_path);
    }
    if !result.warnings.is_empty() {
        println!("warnings: {}", serde_json::to_string(&result.warnings)?);
    }
    print_activity_warnings(&result.activity_warnings)?;
    Ok(())
}

fn list_snapshots_cmd(game_slug: Option<String>, output: OutputMode) -> Result<()> {
    let result = api::list_snapshots(ListSnapshotsOptions {
        config_path: None,
        backup_root: None,
        game_slug,
    })?;

    if matches!(output, OutputMode::Json) {
        return print_json(&result);
    }

    for snapshot in result.snapshots {
        println!(
            "{}  {}  {}  files:{}  integrity:{:?}",
            snapshot.id,
            snapshot.game_slug,
            snapshot.created_timestamp,
            snapshot
                .file_count
                .map(|count| count.to_string())
                .unwrap_or_else(|| "unknown".to_string()),
            snapshot.integrity
        );
        if !snapshot.warnings.is_empty() {
            println!("  warnings: {}", serde_json::to_string(&snapshot.warnings)?);
        }
    }
    Ok(())
}

fn show_snapshot_cmd(snapshot: String, output: OutputMode) -> Result<()> {
    let result = api::show_snapshot(ShowSnapshotOptions {
        config_path: None,
        backup_root: None,
        snapshot,
    })?;

    if matches!(output, OutputMode::Json) {
        return print_json(&result);
    }

    println!("snapshot: {}", result.snapshot.id);
    println!("game: {}", result.snapshot.game_slug);
    println!("created: {}", result.snapshot.created_timestamp);
    if let Some(archive_path) = &result.snapshot.archive_path {
        println!("archive: {archive_path}");
    }
    if let Some(metadata_path) = &result.snapshot.metadata_path {
        println!("metadata: {metadata_path}");
    }
    println!("files: {}", result.inventory.len());
    println!("integrity: {:?}", result.snapshot.integrity);
    if !result.snapshot.warnings.is_empty() {
        println!(
            "warnings: {}",
            serde_json::to_string(&result.snapshot.warnings)?
        );
    }
    Ok(())
}

fn show_config_cmd(output: OutputMode) -> Result<()> {
    let result = api::show_effective_settings(EffectiveSettingsOptions::default())?;

    if matches!(output, OutputMode::Json) {
        return print_json(&result);
    }

    if let Some(config_path) = &result.config_path {
        println!("config: {config_path}");
    } else {
        println!("config: unavailable");
    }
    println!("config loaded: {}", result.config_loaded);
    println!("backup root: {}", result.backup_root);
    if let Some(steam_root_override) = &result.steam_root_override {
        println!("steam root override: {steam_root_override}");
    } else {
        println!("steam root override: none");
    }
    Ok(())
}

fn check_running_cmd(game_slug: Option<String>, output: OutputMode) -> Result<()> {
    let result = api::check_game_activity(CheckGameActivityOptions { game_slug })?;

    if matches!(output, OutputMode::Json) {
        return print_json(&result);
    }

    for game in result.games {
        if game.running_processes.is_empty() {
            println!("{}: not running", game.game_slug);
        } else {
            let processes = game
                .running_processes
                .iter()
                .map(|process| format!("{}({})", process.executable, process.pid))
                .collect::<Vec<_>>()
                .join(", ");
            println!("{}: running {processes}", game.game_slug);
        }
    }
    Ok(())
}

fn print_activity_warnings(warnings: &[savecore::types::ActivityWarning]) -> Result<()> {
    if !warnings.is_empty() {
        println!("activity warnings: {}", serde_json::to_string(warnings)?);
    }
    Ok(())
}

fn print_json<T: Serialize>(value: &T) -> Result<()> {
    let stdout = io::stdout();
    serde_json::to_writer_pretty(stdout.lock(), value)?;
    println!();
    Ok(())
}

fn print_json_error(error: &anyhow::Error) {
    let message = error.to_string();
    let stderr = io::stderr();
    let output = JsonError {
        error: JsonErrorBody { message: &message },
    };

    if serde_json::to_writer_pretty(stderr.lock(), &output).is_ok() {
        eprintln!();
    }
}
