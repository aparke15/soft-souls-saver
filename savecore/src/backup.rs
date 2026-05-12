use crate::{GameManifest, config, ops, resolve, steam::SteamApp, types::*};
use anyhow::Context;
use camino::Utf8PathBuf;
use chrono::Utc;
use std::fs;

pub fn backup_game(
    manifest: &GameManifest,
    steam_app: Option<&SteamApp>,
    options: BackupOptions,
) -> anyhow::Result<BackupResult> {
    let timestamp = Utc::now().format("%Y%m%dT%H%M%S%.fZ").to_string();
    let candidates =
        resolve::resolve_save_candidates(manifest, steam_app, options.steam_id64.as_deref())?;
    let selected = resolve::select_best_candidate(&candidates).with_context(|| {
        let attempted = candidates
            .iter()
            .map(|candidate| candidate.path.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        format!(
            "no save path found for '{}'; attempted: {}",
            manifest.slug,
            if attempted.is_empty() {
                "none"
            } else {
                &attempted
            }
        )
    })?;

    let source_save_path = selected.path.clone();
    let source = Utf8PathBuf::from(&source_save_path);
    let backup_root = config::backup_root_path(options.backup_root.as_deref().unwrap_or("backups"));
    let output_zip = output_zip_path(
        manifest,
        &timestamp,
        options.output_path.as_deref(),
        &backup_root,
    )?;
    let metadata_path = default_metadata_path(
        manifest,
        &timestamp,
        options.output_path.as_deref(),
        &backup_root,
    );
    let selected_pattern_list = if selected.matched_patterns.is_empty() {
        manifest.patterns.clone()
    } else {
        selected.matched_patterns.clone()
    };

    let mut result = BackupResult {
        game_name: manifest.name.clone(),
        slug: manifest.slug.clone(),
        timestamp,
        source_save_path,
        resolved_candidates: candidates,
        selected_pattern_list,
        output_zip_path: output_zip.to_string(),
        metadata_path: metadata_path.as_ref().map(ToString::to_string),
        sha256: None,
        dry_run: options.dry_run,
        activity_warnings: Vec::new(),
    };

    if options.dry_run {
        return Ok(result);
    }

    ops::zip_dir(&source, &output_zip)?;
    result.sha256 = Some(ops::sha256_file(&output_zip)?);

    if let Some(metadata_path) = metadata_path {
        fs::create_dir_all(
            metadata_path
                .parent()
                .ok_or_else(|| anyhow::anyhow!("no parent for metadata path"))?,
        )?;
        fs::write(&metadata_path, serde_json::to_vec_pretty(&result)?)?;
    }

    Ok(result)
}

fn output_zip_path(
    manifest: &GameManifest,
    timestamp: &str,
    output_path: Option<&str>,
    backup_root: &Utf8PathBuf,
) -> anyhow::Result<Utf8PathBuf> {
    if let Some(output_path) = output_path {
        let path = Utf8PathBuf::from(output_path);
        if path.extension() == Some("zip") {
            return Ok(path);
        }
        return Ok(path.join("payload.zip"));
    }

    Ok(default_backup_dir(manifest, timestamp, backup_root).join("payload.zip"))
}

fn default_metadata_path(
    manifest: &GameManifest,
    timestamp: &str,
    output_path: Option<&str>,
    backup_root: &Utf8PathBuf,
) -> Option<Utf8PathBuf> {
    output_path
        .is_none()
        .then(|| default_backup_dir(manifest, timestamp, backup_root).join("metadata.json"))
}

fn default_backup_dir(
    manifest: &GameManifest,
    timestamp: &str,
    backup_root: &Utf8PathBuf,
) -> Utf8PathBuf {
    backup_root.join(&manifest.slug).join(timestamp)
}
