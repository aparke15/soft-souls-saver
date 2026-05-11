use crate::{GameManifest, ops, resolve, types::*};
use anyhow::Context;
use camino::Utf8PathBuf;
use chrono::Utc;
use std::fs;

pub fn backup_game(
    manifest: &GameManifest,
    options: BackupOptions,
) -> anyhow::Result<BackupResult> {
    let timestamp = Utc::now().format("%Y%m%dT%H%M%SZ").to_string();
    let candidates = resolve::resolve_save_candidates(manifest, options.steam_id64.as_deref())?;
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
    let output_zip = output_zip_path(manifest, &timestamp, options.output_path.as_deref())?;
    let metadata_path = default_metadata_path(manifest, &timestamp, options.output_path.as_deref());
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
) -> anyhow::Result<Utf8PathBuf> {
    if let Some(output_path) = output_path {
        let path = Utf8PathBuf::from(output_path);
        if path.extension() == Some("zip") {
            return Ok(path);
        }
        return Ok(path.join("payload.zip"));
    }

    Ok(default_backup_dir(manifest, timestamp).join("payload.zip"))
}

fn default_metadata_path(
    manifest: &GameManifest,
    timestamp: &str,
    output_path: Option<&str>,
) -> Option<Utf8PathBuf> {
    output_path
        .is_none()
        .then(|| default_backup_dir(manifest, timestamp).join("metadata.json"))
}

fn default_backup_dir(manifest: &GameManifest, timestamp: &str) -> Utf8PathBuf {
    Utf8PathBuf::from("backups")
        .join(&manifest.slug)
        .join(timestamp)
}
