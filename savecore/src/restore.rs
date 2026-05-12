use crate::{GameManifest, backup, config, ops, resolve, snapshot, steam::SteamApp, types::*};
use anyhow::{Context, bail};
use camino::Utf8PathBuf;
use glob::Pattern;
use std::{
    fs, io,
    path::{Component, Path, PathBuf},
    time::SystemTime,
};
use zip::{ZipArchive, read::ZipFile};

pub fn plan_restore(
    manifest: &GameManifest,
    steam_app: Option<&SteamApp>,
    options: RestorePlanOptions,
    game_detected: bool,
) -> anyhow::Result<RestorePlanResult> {
    let snapshot = load_snapshot(&options.snapshot_path, options.backup_root.as_deref())?;
    let candidates =
        resolve::resolve_save_candidates(manifest, steam_app, options.steam_id64.as_deref())?;
    let files_to_restore = inventory_zip(&snapshot.payload_zip_path)?;
    let restore_target = choose_restore_target(&candidates);
    let mut warnings = Vec::new();

    if !game_detected {
        warnings.push(RestorePlanWarning::GameNotCurrentlyDetected);
    }

    if snapshot.metadata.slug != manifest.slug
        || !snapshot_matches_manifest(&files_to_restore, manifest)
    {
        warnings.push(RestorePlanWarning::SnapshotAppearsIncompatible);
    }

    match &snapshot.metadata.sha256 {
        Some(expected_hash) if ops::sha256_file(&snapshot.payload_zip_path)? != *expected_hash => {
            warnings.push(RestorePlanWarning::SnapshotIntegrityMismatch);
        }
        Some(_) => {}
        None => warnings.push(RestorePlanWarning::SnapshotIntegrityUnavailable),
    }

    let existing_candidates = candidates
        .iter()
        .filter(|candidate| candidate.exists)
        .collect::<Vec<_>>();
    if existing_candidates.len() > 1 {
        warnings.push(RestorePlanWarning::MultipleCandidateTargets);
        if options.steam_id64.is_none() {
            warnings.push(RestorePlanWarning::AmbiguousSteamId);
        }
    }

    let restore_destination_path = restore_target.map(|candidate| candidate.path.clone());
    let target_exists = restore_target.is_some_and(|candidate| candidate.exists);

    if restore_destination_path.is_none() {
        warnings.push(RestorePlanWarning::NoRestoreTargetResolved);
    } else if !target_exists {
        warnings.push(RestorePlanWarning::SaveDirMissing);
    }

    let pre_restore_safety_backup = SafetyBackupPlan {
        recommended: target_exists,
        possible: target_exists,
        reason: if target_exists {
            Some("restore target currently exists".to_string())
        } else {
            Some("restore target is missing or unresolved".to_string())
        },
    };
    let blocking_warnings = warnings
        .iter()
        .filter(|warning| warning.blocks_restore_execution())
        .cloned()
        .collect::<Vec<_>>();

    Ok(RestorePlanResult {
        target_game_slug: manifest.slug.clone(),
        selected_snapshot: snapshot.metadata,
        snapshot_inventory: SnapshotInventory {
            metadata_path: snapshot.metadata_path.map(|path| path.to_string()),
            payload_zip_path: snapshot.payload_zip_path.to_string(),
            file_count: files_to_restore.len(),
        },
        restore_destination_path,
        files_to_restore,
        target_exists,
        pre_restore_safety_backup,
        resolved_candidates: candidates,
        warnings,
        blocking_warnings,
        activity_warnings: Vec::new(),
    })
}

pub fn restore_game(
    manifest: &GameManifest,
    steam_app: Option<&SteamApp>,
    options: RestoreOptions,
    game_detected: bool,
) -> anyhow::Result<RestoreResult> {
    let plan = plan_restore(
        manifest,
        steam_app,
        RestorePlanOptions {
            game_slug: options.game_slug.clone(),
            snapshot_path: options.snapshot_path.clone(),
            config_path: options.config_path.clone(),
            backup_root: options.backup_root.clone(),
            steam_id64: options.steam_id64.clone(),
            steam_root_override: options.steam_root_override.clone(),
        },
        game_detected,
    )?;

    if !plan.blocking_warnings.is_empty() {
        bail!(
            "restore blocked by planning warnings: {}",
            warning_list(&plan.blocking_warnings)
        );
    }

    let destination = plan
        .restore_destination_path
        .clone()
        .ok_or_else(|| anyhow::anyhow!("restore destination was not resolved"))?;

    if options.dry_run {
        return Ok(RestoreResult {
            target_game_slug: plan.target_game_slug,
            selected_snapshot: plan.selected_snapshot,
            restore_destination_path: destination,
            dry_run: true,
            safety_backup: None,
            restored_file_count: plan.files_to_restore.len(),
            restored_files: plan.files_to_restore,
            warnings: plan.warnings,
            blocking_warnings: plan.blocking_warnings,
            activity_warnings: plan.activity_warnings,
        });
    }

    let destination_path = Utf8PathBuf::from(&destination);
    let destination_parent = destination_path
        .parent()
        .ok_or_else(|| anyhow::anyhow!("restore destination has no parent"))?;
    fs::create_dir_all(destination_parent)?;

    let staging_dir = unique_sibling_dir(&destination_path, "restore-staging");
    let rollback_dir = unique_sibling_dir(&destination_path, "restore-rollback");

    extract_zip_to_staging(
        &Utf8PathBuf::from(&plan.snapshot_inventory.payload_zip_path),
        &staging_dir,
    )?;
    validate_staged_contents(&staging_dir, &plan.files_to_restore)?;

    let safety_backup = if plan.target_exists {
        Some(backup::backup_game(
            manifest,
            steam_app,
            BackupOptions {
                game_slug: manifest.slug.clone(),
                output_path: None,
                config_path: options.config_path.clone(),
                backup_root: options.backup_root.clone(),
                steam_id64: options.steam_id64.clone(),
                steam_root_override: options.steam_root_override.clone(),
                dry_run: false,
            },
        )?)
    } else {
        None
    };

    replace_destination(
        &staging_dir,
        &destination_path,
        plan.target_exists,
        &rollback_dir,
    )?;

    Ok(RestoreResult {
        target_game_slug: plan.target_game_slug,
        selected_snapshot: plan.selected_snapshot,
        restore_destination_path: destination,
        dry_run: false,
        safety_backup,
        restored_file_count: plan.files_to_restore.len(),
        restored_files: plan.files_to_restore,
        warnings: plan.warnings,
        blocking_warnings: plan.blocking_warnings,
        activity_warnings: plan.activity_warnings,
    })
}

struct LoadedSnapshot {
    metadata: BackupResult,
    metadata_path: Option<Utf8PathBuf>,
    payload_zip_path: Utf8PathBuf,
}

fn load_snapshot(snapshot_path: &str, backup_root: Option<&str>) -> anyhow::Result<LoadedSnapshot> {
    let backup_root = config::backup_root_path(backup_root.unwrap_or("backups"));
    let snapshot_path = snapshot::resolve_snapshot_reference(snapshot_path, Some(&backup_root))?;

    let (metadata_path, inferred_zip_path) = if snapshot_path.is_dir() {
        (
            snapshot_path.join("metadata.json"),
            snapshot_path.join("payload.zip"),
        )
    } else if snapshot_path.file_name() == Some("metadata.json") {
        let parent = snapshot_path
            .parent()
            .ok_or_else(|| anyhow::anyhow!("metadata path has no parent"))?;
        (snapshot_path.clone(), parent.join("payload.zip"))
    } else {
        let parent = snapshot_path
            .parent()
            .ok_or_else(|| anyhow::anyhow!("snapshot path has no parent"))?;
        (parent.join("metadata.json"), snapshot_path.clone())
    };

    if !metadata_path.is_file() {
        bail!("snapshot metadata not found: {}", metadata_path);
    }

    let metadata: BackupResult = serde_json::from_slice(&fs::read(&metadata_path)?)
        .with_context(|| format!("failed to read snapshot metadata: {metadata_path}"))?;
    let metadata_zip_path = Utf8PathBuf::from(&metadata.output_zip_path);
    let payload_zip_path = if inferred_zip_path.is_file() {
        inferred_zip_path
    } else {
        metadata_zip_path
    };

    if !payload_zip_path.is_file() {
        bail!("snapshot payload not found: {}", payload_zip_path);
    }

    Ok(LoadedSnapshot {
        metadata,
        metadata_path: Some(metadata_path),
        payload_zip_path,
    })
}

pub(crate) fn inventory_zip(zip_path: &Utf8PathBuf) -> anyhow::Result<Vec<RestoreFile>> {
    let file = fs::File::open(zip_path)?;
    let mut archive = ZipArchive::new(file)?;
    let mut files = Vec::new();

    for index in 0..archive.len() {
        let entry = archive.by_index(index)?;
        validate_zip_entry(&entry)?;
        if entry.is_dir() {
            continue;
        }
        let path = sanitized_zip_entry_path(entry.name())?;

        files.push(RestoreFile {
            path: path.to_string_lossy().replace('\\', "/"),
            compressed_size: entry.compressed_size(),
            uncompressed_size: entry.size(),
        });
    }

    Ok(files)
}

fn choose_restore_target(candidates: &[ResolvedSaveCandidate]) -> Option<&ResolvedSaveCandidate> {
    resolve::select_best_candidate(candidates)
        .or_else(|| (candidates.len() == 1).then(|| &candidates[0]))
}

fn snapshot_matches_manifest(files: &[RestoreFile], manifest: &GameManifest) -> bool {
    let patterns = manifest
        .patterns
        .iter()
        .filter_map(|pattern| Pattern::new(pattern).ok())
        .collect::<Vec<_>>();

    files.iter().any(|file| {
        let file_name = file.path.rsplit('/').next().unwrap_or(file.path.as_str());
        patterns.iter().any(|pattern| pattern.matches(file_name))
    })
}

fn extract_zip_to_staging(zip_path: &Utf8PathBuf, staging_dir: &Utf8PathBuf) -> anyhow::Result<()> {
    if staging_dir.exists() {
        fs::remove_dir_all(staging_dir)?;
    }
    fs::create_dir_all(staging_dir)?;
    let staging_root = staging_dir.canonicalize()?;

    let file = fs::File::open(zip_path)?;
    let mut archive = ZipArchive::new(file)?;

    for index in 0..archive.len() {
        let mut entry = archive.by_index(index)?;
        validate_zip_entry(&entry)?;
        let relative_path = sanitized_zip_entry_path(entry.name())?;
        let output_path = staging_dir.join(relative_path.to_string_lossy().as_ref());

        if entry.is_dir() {
            fs::create_dir_all(&output_path)?;
            continue;
        }

        let output_parent = output_path
            .parent()
            .ok_or_else(|| anyhow::anyhow!("staged file has no parent"))?;
        fs::create_dir_all(output_parent)?;
        let canonical_parent = output_parent.canonicalize()?;
        if !canonical_parent.starts_with(&staging_root) {
            bail!(
                "unsafe zip entry escapes staging directory: {}",
                entry.name()
            );
        }

        let mut output = fs::File::create(&output_path)?;
        io::copy(&mut entry, &mut output)?;
    }

    Ok(())
}

fn validate_staged_contents(
    staging_dir: &Utf8PathBuf,
    expected_files: &[RestoreFile],
) -> anyhow::Result<()> {
    for expected in expected_files {
        let relative_path = sanitized_zip_entry_path(&expected.path)?;
        let staged_file = staging_dir.join(relative_path.to_string_lossy().as_ref());
        let metadata = fs::metadata(&staged_file)
            .with_context(|| format!("staged file missing: {}", expected.path))?;
        if !metadata.is_file() {
            bail!("staged entry is not a file: {}", expected.path);
        }
        if metadata.len() != expected.uncompressed_size {
            bail!("staged file size mismatch: {}", expected.path);
        }
    }

    Ok(())
}

fn replace_destination(
    staging_dir: &Utf8PathBuf,
    destination_path: &Utf8PathBuf,
    target_exists: bool,
    rollback_dir: &Utf8PathBuf,
) -> anyhow::Result<()> {
    if target_exists {
        fs::rename(destination_path, rollback_dir)?;
        if let Err(error) = fs::rename(staging_dir, destination_path) {
            let _ = fs::rename(rollback_dir, destination_path);
            return Err(error.into());
        }
        fs::remove_dir_all(rollback_dir)?;
    } else {
        fs::rename(staging_dir, destination_path)?;
    }

    Ok(())
}

fn validate_zip_entry(entry: &ZipFile<'_>) -> anyhow::Result<()> {
    if let Some(mode) = entry.unix_mode() {
        let file_type = mode & 0o170000;
        if file_type != 0 && file_type != 0o100000 && file_type != 0o040000 {
            bail!("unsupported zip entry type: {}", entry.name());
        }
    }

    sanitized_zip_entry_path(entry.name()).map(|_| ())
}

fn sanitized_zip_entry_path(name: &str) -> anyhow::Result<PathBuf> {
    if name.is_empty() || name.contains('\\') {
        bail!("unsafe zip entry path: {name}");
    }

    let path = Path::new(name);
    if path.is_absolute() {
        bail!("unsafe absolute zip entry path: {name}");
    }

    let mut sanitized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Normal(part) => sanitized.push(part),
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                bail!("unsafe zip entry path: {name}");
            }
        }
    }

    if sanitized.as_os_str().is_empty() {
        bail!("unsafe empty zip entry path: {name}");
    }

    Ok(sanitized)
}

fn unique_sibling_dir(destination_path: &Utf8PathBuf, label: &str) -> Utf8PathBuf {
    let nanos = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    let name = destination_path
        .file_name()
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| "restore-target".to_string());
    destination_path.with_file_name(format!(".{name}.{label}-{nanos}"))
}

fn warning_list(warnings: &[RestorePlanWarning]) -> String {
    warnings
        .iter()
        .map(|warning| format!("{warning:?}"))
        .collect::<Vec<_>>()
        .join(", ")
}
