use anyhow::Result;
use camino::Utf8PathBuf;
use clap::{Parser, Subcommand};
use savecore::{api, types::BackupOptions};

#[derive(Parser)]
#[command(name = "savectl", version, about = "souls-like save manager")]
struct Cli {
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
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.cmd {
        Cmd::Detect => detect_cmd()?,
        Cmd::Backup {
            game_slug,
            out,
            steam_id64,
            dry_run,
        } => backup_cmd(game_slug, out, steam_id64, dry_run)?,
    }

    Ok(())
}

fn detect_cmd() -> Result<()> {
    let result = api::detect_games()?;
    println!("{}", serde_json::to_string_pretty(&result.games)?);
    Ok(())
}

fn backup_cmd(
    game_slug: String,
    out: Option<Utf8PathBuf>,
    steam_id64: Option<String>,
    dry_run: bool,
) -> Result<()> {
    let result = api::backup_game(BackupOptions {
        game_slug,
        output_path: out.map(|path| path.to_string()),
        steam_id64,
        dry_run,
    })?;

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
    println!(
        "candidates considered: {}",
        result.resolved_candidates.len()
    );
    Ok(())
}
