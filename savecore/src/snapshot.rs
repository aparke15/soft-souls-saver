use crate::{config, ops, restore, types::*};
use anyhow::{Context, bail};
use camino::Utf8PathBuf;
use std::fs;

const METADATA_FILE: &str = "metadata.json";
const PAYLOAD_FILE: &str = "payload.zip";

pub fn list_snapshots(options: ListSnapshotsOptions) -> anyhow::Result<ListSnapshotsResult> {
    let settings = config::resolve_effective_settings(config::SettingsInputs {
        config_path: options.config_path,
        backup_root: options.backup_root,
        steam_root_override: None,
    })?;
    let backup_root = config::backup_root_path(&settings.backup_root);
    let mut snapshots = Vec::new();

    if !backup_root.is_dir() {
        return Ok(ListSnapshotsResult {
            backup_root: backup_root.to_string(),
            snapshots,
        });
    }

    for game_entry in fs::read_dir(&backup_root)? {
        let game_entry = game_entry?;
        if !game_entry.file_type()?.is_dir() {
            continue;
        }
        let game_slug = game_entry.file_name().to_string_lossy().to_string();
        if options
            .game_slug
            .as_ref()
            .is_some_and(|filter| filter != &game_slug)
        {
            continue;
        }

        let game_dir = Utf8PathBuf::from_path_buf(game_entry.path())
            .map_err(|path| anyhow::anyhow!("non-utf8 backup path: {}", path.display()))?;
        for snapshot_entry in fs::read_dir(game_dir)? {
            let snapshot_entry = snapshot_entry?;
            if !snapshot_entry.file_type()?.is_dir() {
                continue;
            }
            let snapshot_dir = Utf8PathBuf::from_path_buf(snapshot_entry.path())
                .map_err(|path| anyhow::anyhow!("non-utf8 snapshot path: {}", path.display()))?;
            snapshots.push(summarize_snapshot_dir(
                &backup_root,
                &snapshot_dir,
                &game_slug,
            ));
        }
    }

    snapshots.sort_by(|left, right| {
        right
            .created_timestamp
            .cmp(&left.created_timestamp)
            .then_with(|| right.id.cmp(&left.id))
    });

    Ok(ListSnapshotsResult {
        backup_root: backup_root.to_string(),
        snapshots,
    })
}

pub fn show_snapshot(options: ShowSnapshotOptions) -> anyhow::Result<ShowSnapshotResult> {
    let settings = config::resolve_effective_settings(config::SettingsInputs {
        config_path: options.config_path,
        backup_root: options.backup_root,
        steam_root_override: None,
    })?;
    let backup_root = config::backup_root_path(&settings.backup_root);
    let snapshot_dir = resolve_snapshot_reference(&options.snapshot, Some(&backup_root))?;
    let game_slug = snapshot_dir
        .parent()
        .and_then(|parent| parent.file_name())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| "unknown".to_string());
    let summary = summarize_snapshot_dir(&backup_root, &snapshot_dir, &game_slug);
    let metadata = summary
        .metadata_path
        .as_ref()
        .and_then(|path| fs::read(path).ok())
        .and_then(|bytes| serde_json::from_slice::<BackupResult>(&bytes).ok());
    let inventory = summary
        .archive_path
        .as_ref()
        .and_then(|path| restore::inventory_zip(&Utf8PathBuf::from(path)).ok())
        .unwrap_or_default();

    Ok(ShowSnapshotResult {
        snapshot: summary,
        metadata,
        inventory,
    })
}

pub(crate) fn resolve_snapshot_reference(
    snapshot: &str,
    backup_root: Option<&Utf8PathBuf>,
) -> anyhow::Result<Utf8PathBuf> {
    let raw_path = Utf8PathBuf::from(snapshot);
    if raw_path.exists() {
        return snapshot_dir_from_path(raw_path);
    }

    if let Some((game_slug, timestamp)) = snapshot.split_once('/') {
        let root = backup_root
            .cloned()
            .unwrap_or_else(|| config::backup_root_path("backups"));
        let id_path = root.join(game_slug).join(timestamp);
        if id_path.exists() {
            return Ok(id_path);
        }
    }

    bail!("snapshot not found: {snapshot}")
}

fn snapshot_dir_from_path(path: Utf8PathBuf) -> anyhow::Result<Utf8PathBuf> {
    if path.is_dir() {
        return Ok(path);
    }

    if path.file_name() == Some(METADATA_FILE) || path.file_name() == Some(PAYLOAD_FILE) {
        return path
            .parent()
            .map(ToOwned::to_owned)
            .ok_or_else(|| anyhow::anyhow!("snapshot path has no parent"));
    }

    bail!("snapshot path is not a snapshot directory: {path}")
}

fn summarize_snapshot_dir(
    _backup_root: &Utf8PathBuf,
    snapshot_dir: &Utf8PathBuf,
    fallback_game_slug: &str,
) -> SnapshotSummary {
    let fallback_timestamp = snapshot_dir
        .file_name()
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| "unknown".to_string());
    let metadata_path = snapshot_dir.join(METADATA_FILE);
    let sibling_archive_path = snapshot_dir.join(PAYLOAD_FILE);
    let mut warnings = Vec::new();
    let mut metadata = None;

    if metadata_path.is_file() {
        match fs::read(&metadata_path)
            .with_context(|| format!("failed to read snapshot metadata: {metadata_path}"))
            .and_then(|bytes| Ok(serde_json::from_slice::<BackupResult>(&bytes)?))
        {
            Ok(value) => metadata = Some(value),
            Err(_) => warnings.push(SnapshotWarning::InvalidMetadata),
        }
    } else {
        warnings.push(SnapshotWarning::MissingMetadata);
    }

    let game_slug = metadata
        .as_ref()
        .map(|metadata| metadata.slug.clone())
        .unwrap_or_else(|| fallback_game_slug.to_string());
    let created_timestamp = metadata
        .as_ref()
        .map(|metadata| metadata.timestamp.clone())
        .unwrap_or(fallback_timestamp);
    let id = format!("{game_slug}/{created_timestamp}");
    let metadata_output_archive = metadata
        .as_ref()
        .map(|metadata| Utf8PathBuf::from(&metadata.output_zip_path));
    let archive_path = choose_archive_path(&sibling_archive_path, metadata_output_archive.as_ref());

    if archive_path.is_none() {
        warnings.push(SnapshotWarning::MissingArchive);
    }

    if let (Some(metadata_archive), Some(archive_path)) = (&metadata_output_archive, &archive_path)
        && metadata_archive != archive_path
        && metadata_archive.exists()
    {
        warnings.push(SnapshotWarning::MetadataArchiveMismatch);
    }

    let (file_count, mut archive_warnings) = archive_path
        .as_ref()
        .map(inventory_file_count)
        .unwrap_or((None, Vec::new()));
    warnings.append(&mut archive_warnings);

    let integrity = match (&metadata, &archive_path) {
        (Some(metadata), Some(archive_path)) => match &metadata.sha256 {
            Some(expected) => match ops::sha256_file(archive_path) {
                Ok(actual) if &actual == expected => SnapshotIntegrity::Verified,
                Ok(_) => {
                    warnings.push(SnapshotWarning::MetadataArchiveMismatch);
                    SnapshotIntegrity::Mismatch
                }
                Err(_) => {
                    warnings.push(SnapshotWarning::UnreadableArchive);
                    SnapshotIntegrity::Unknown
                }
            },
            None => {
                warnings.push(SnapshotWarning::LegacyIntegrityMissing);
                SnapshotIntegrity::Unavailable
            }
        },
        _ => SnapshotIntegrity::Unknown,
    };

    SnapshotSummary {
        id,
        game_slug,
        created_timestamp,
        archive_path: archive_path.map(|path| path.to_string()),
        metadata_path: metadata_path.is_file().then(|| metadata_path.to_string()),
        archive_size: archive_path_size(&sibling_archive_path, metadata_output_archive.as_ref()),
        steam_id64: metadata.as_ref().and_then(snapshot_steam_id64),
        file_count,
        integrity,
        warnings,
    }
}

fn choose_archive_path(
    sibling_archive_path: &Utf8PathBuf,
    metadata_archive_path: Option<&Utf8PathBuf>,
) -> Option<Utf8PathBuf> {
    if sibling_archive_path.is_file() {
        return Some(sibling_archive_path.clone());
    }

    metadata_archive_path.filter(|path| path.is_file()).cloned()
}

fn archive_path_size(
    sibling_archive_path: &Utf8PathBuf,
    metadata_archive_path: Option<&Utf8PathBuf>,
) -> Option<u64> {
    choose_archive_path(sibling_archive_path, metadata_archive_path)
        .and_then(|path| fs::metadata(path).ok())
        .map(|metadata| metadata.len())
}

fn inventory_file_count(archive_path: &Utf8PathBuf) -> (Option<usize>, Vec<SnapshotWarning>) {
    match restore::inventory_zip(archive_path) {
        Ok(files) => (Some(files.len()), Vec::new()),
        Err(error) if error.to_string().contains("unsafe") => {
            (None, vec![SnapshotWarning::UnsafeArchiveEntry])
        }
        Err(_) => (None, vec![SnapshotWarning::UnreadableArchive]),
    }
}

fn snapshot_steam_id64(metadata: &BackupResult) -> Option<String> {
    metadata
        .resolved_candidates
        .iter()
        .find(|candidate| candidate.path == metadata.source_save_path)
        .and_then(|candidate| candidate.steam_id64.clone())
        .or_else(|| {
            metadata
                .resolved_candidates
                .iter()
                .find_map(|candidate| candidate.steam_id64.clone())
        })
}
