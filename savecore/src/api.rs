use crate::{backup, config, load_manifests, process, resolve, restore, snapshot, steam, types::*};
use anyhow::{Context, bail};

// Shared application boundary for savectl now, and future saveui/savehost adapters later.
pub fn detect_games(options: DetectOptions) -> anyhow::Result<DetectGamesResult> {
    detect_games_with_process_provider(options, &process::PlatformProcessProvider)
}

fn detect_games_with_process_provider(
    options: DetectOptions,
    process_provider: &impl process::ProcessProvider,
) -> anyhow::Result<DetectGamesResult> {
    let manifests = load_manifests()?;
    let settings = config::resolve_effective_settings(config::SettingsInputs {
        config_path: options.config_path,
        backup_root: None,
        steam_root_override: options.steam_root_override,
    })?;
    let steam_install = steam::find_with_options(&steam::SteamDiscoveryOptions {
        steam_root_override: settings.steam_root_override,
    })?;
    let mut games = Vec::new();

    for manifest in manifests {
        let steam_appid = manifest.ids.as_ref().and_then(|ids| ids.steam);
        let installed = steam_appid.and_then(|id| steam_install.apps.get(&id));

        let Some(app) = installed else {
            continue;
        };

        let save_candidates = resolve::resolve_save_candidates(&manifest, Some(app), None)?;
        let activity_warnings = process::check_game_activity_with_provider(
            std::slice::from_ref(&manifest),
            Some(&manifest.slug),
            process_provider,
        )?
        .games
        .into_iter()
        .next()
        .map(|activity| activity.warnings)
        .unwrap_or_default();

        games.push(DetectGame {
            name: manifest.name,
            slug: manifest.slug,
            steam_appid,
            installed_name: Some(app.name.clone()),
            install_dir: Some(app.install_dir.to_string()),
            save_candidates,
            activity_warnings,
        });
    }

    Ok(DetectGamesResult { games })
}

pub fn backup_game(options: BackupOptions) -> anyhow::Result<BackupResult> {
    backup_game_with_process_provider(options, &process::PlatformProcessProvider)
}

fn backup_game_with_process_provider(
    mut options: BackupOptions,
    process_provider: &impl process::ProcessProvider,
) -> anyhow::Result<BackupResult> {
    let manifests = load_manifests()?;
    let settings = config::resolve_effective_settings(config::SettingsInputs {
        config_path: options.config_path.clone(),
        backup_root: options.backup_root.clone(),
        steam_root_override: options.steam_root_override.clone(),
    })?;
    options.backup_root = Some(settings.backup_root.clone());
    options.steam_root_override = settings.steam_root_override.clone();
    let available_slugs = manifests
        .iter()
        .map(|manifest| manifest.slug.as_str())
        .collect::<Vec<_>>();

    let manifest = manifests
        .iter()
        .find(|manifest| manifest.slug == options.game_slug)
        .with_context(|| {
            format!(
                "unknown game slug '{}'; available slugs: {}",
                options.game_slug,
                available_slugs.join(", ")
            )
        })?;
    let activity_warnings = activity_warnings_for_manifest(manifest, process_provider)?;

    if let Some(appid) = manifest.ids.as_ref().and_then(|ids| ids.steam) {
        let steam_install = steam::find_with_options(&steam::SteamDiscoveryOptions {
            steam_root_override: settings.steam_root_override.clone(),
        })?;
        let Some(app) = steam_install.apps.get(&appid) else {
            bail!(
                "'{}' is known as '{}' but Steam app {} was not detected as installed",
                options.game_slug,
                manifest.name,
                appid
            );
        };

        let mut result = backup::backup_game(manifest, Some(app), options)?;
        result.activity_warnings = activity_warnings;
        return Ok(result);
    }

    let mut result = backup::backup_game(manifest, None, options)?;
    result.activity_warnings = activity_warnings;
    Ok(result)
}

pub fn plan_restore(options: RestorePlanOptions) -> anyhow::Result<RestorePlanResult> {
    plan_restore_with_process_provider(options, &process::PlatformProcessProvider)
}

fn plan_restore_with_process_provider(
    mut options: RestorePlanOptions,
    process_provider: &impl process::ProcessProvider,
) -> anyhow::Result<RestorePlanResult> {
    let manifests = load_manifests()?;
    let settings = config::resolve_effective_settings(config::SettingsInputs {
        config_path: options.config_path.clone(),
        backup_root: options.backup_root.clone(),
        steam_root_override: options.steam_root_override.clone(),
    })?;
    options.backup_root = Some(settings.backup_root.clone());
    options.steam_root_override = settings.steam_root_override.clone();
    let available_slugs = manifests
        .iter()
        .map(|manifest| manifest.slug.as_str())
        .collect::<Vec<_>>();

    let manifest = manifests
        .iter()
        .find(|manifest| manifest.slug == options.game_slug)
        .with_context(|| {
            format!(
                "unknown game slug '{}'; available slugs: {}",
                options.game_slug,
                available_slugs.join(", ")
            )
        })?;

    let mut detected = true;
    let steam_install = steam::find_with_options(&steam::SteamDiscoveryOptions {
        steam_root_override: settings.steam_root_override.clone(),
    })?;
    let steam_app = manifest
        .ids
        .as_ref()
        .and_then(|ids| ids.steam)
        .and_then(|appid| steam_install.apps.get(&appid));

    if manifest.ids.as_ref().and_then(|ids| ids.steam).is_some() && steam_app.is_none() {
        detected = false;
    }

    let mut result = restore::plan_restore(manifest, steam_app, options, detected)?;
    result.activity_warnings = activity_warnings_for_manifest(manifest, process_provider)?;
    Ok(result)
}

pub fn check_game_activity(
    options: CheckGameActivityOptions,
) -> anyhow::Result<CheckGameActivityResult> {
    let manifests = load_manifests()?;
    let available_slugs = manifests
        .iter()
        .map(|manifest| manifest.slug.as_str())
        .collect::<Vec<_>>();

    if let Some(game_slug) = &options.game_slug
        && !manifests.iter().any(|manifest| &manifest.slug == game_slug)
    {
        bail!(
            "unknown game slug '{}'; available slugs: {}",
            game_slug,
            available_slugs.join(", ")
        );
    }

    process::check_game_activity(&manifests, options.game_slug.as_deref())
}

pub fn list_snapshots(options: ListSnapshotsOptions) -> anyhow::Result<ListSnapshotsResult> {
    snapshot::list_snapshots(options)
}

pub fn show_snapshot(options: ShowSnapshotOptions) -> anyhow::Result<ShowSnapshotResult> {
    snapshot::show_snapshot(options)
}

pub fn show_effective_settings(
    options: EffectiveSettingsOptions,
) -> anyhow::Result<EffectiveSettingsResult> {
    config::show_effective_settings(options)
}

pub fn restore_game(options: RestoreOptions) -> anyhow::Result<RestoreResult> {
    restore_game_with_process_provider(options, &process::PlatformProcessProvider)
}

fn restore_game_with_process_provider(
    mut options: RestoreOptions,
    process_provider: &impl process::ProcessProvider,
) -> anyhow::Result<RestoreResult> {
    let manifests = load_manifests()?;
    let settings = config::resolve_effective_settings(config::SettingsInputs {
        config_path: options.config_path.clone(),
        backup_root: options.backup_root.clone(),
        steam_root_override: options.steam_root_override.clone(),
    })?;
    options.backup_root = Some(settings.backup_root.clone());
    options.steam_root_override = settings.steam_root_override.clone();
    let available_slugs = manifests
        .iter()
        .map(|manifest| manifest.slug.as_str())
        .collect::<Vec<_>>();

    let manifest = manifests
        .iter()
        .find(|manifest| manifest.slug == options.game_slug)
        .with_context(|| {
            format!(
                "unknown game slug '{}'; available slugs: {}",
                options.game_slug,
                available_slugs.join(", ")
            )
        })?;

    let mut detected = true;
    let steam_install = steam::find_with_options(&steam::SteamDiscoveryOptions {
        steam_root_override: settings.steam_root_override.clone(),
    })?;
    let steam_app = manifest
        .ids
        .as_ref()
        .and_then(|ids| ids.steam)
        .and_then(|appid| steam_install.apps.get(&appid));

    if manifest.ids.as_ref().and_then(|ids| ids.steam).is_some() && steam_app.is_none() {
        detected = false;
    }

    let mut result = restore::restore_game(manifest, steam_app, options, detected)?;
    result.activity_warnings = activity_warnings_for_manifest(manifest, process_provider)?;
    Ok(result)
}

fn activity_warnings_for_manifest(
    manifest: &crate::GameManifest,
    process_provider: &impl process::ProcessProvider,
) -> anyhow::Result<Vec<ActivityWarning>> {
    Ok(process::check_game_activity_with_provider(
        std::slice::from_ref(manifest),
        Some(&manifest.slug),
        process_provider,
    )?
    .games
    .into_iter()
    .next()
    .map(|activity| activity.warnings)
    .unwrap_or_default())
}

#[cfg(test)]
mod tests {
    use super::*;
    use camino::Utf8PathBuf;
    use std::{
        fs,
        io::{Read, Write},
        time::SystemTime,
    };
    use zip::{ZipArchive, ZipWriter, write::FileOptions};

    #[test]
    fn backup_dry_run_uses_fake_steam_tree() {
        let root = fake_ds3_steam_root("api-dry-run");

        let result = backup_game(BackupOptions {
            game_slug: "ds3".to_string(),
            output_path: None,
            config_path: None,
            backup_root: None,
            steam_id64: None,
            steam_root_override: Some(root.to_string()),
            dry_run: true,
        })
        .unwrap();

        assert!(result.dry_run);
        assert!(
            result
                .source_save_path
                .contains("/steamapps/compatdata/374320/")
        );
        assert_eq!(result.selected_pattern_list, vec!["DS*.sl2"]);

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn backup_dry_run_uses_secondary_steam_library() {
        let root = unique_temp_dir("api-dry-run-primary-library");
        let primary_steamapps = root.join("steamapps");
        let secondary_root = unique_temp_dir("api-dry-run-secondary-library");
        let secondary_steamapps = secondary_root.join("steamapps");
        let save_dir = secondary_steamapps.join("compatdata/374320/pfx/drive_c/users/steamuser/AppData/Roaming/DarkSoulsIII/76561198000000000");

        fs::create_dir_all(&primary_steamapps).unwrap();
        fs::create_dir_all(&save_dir).unwrap();
        fs::write(
            primary_steamapps.join("libraryfolders.vdf"),
            format!(
                "\"libraryfolders\"\n{{\n  \"1\"\n  {{\n    \"path\" \"{}\"\n  }}\n}}\n",
                secondary_root
            ),
        )
        .unwrap();
        fs::write(
            secondary_steamapps.join("appmanifest_374320.acf"),
            "\"AppState\"\n{\n  \"appid\" \"374320\"\n  \"name\" \"DARK SOULS III\"\n  \"installdir\" \"DARK SOULS III\"\n}\n",
        )
        .unwrap();
        fs::write(save_dir.join("DS30000.sl2"), b"fake save").unwrap();

        let result = backup_game(BackupOptions {
            game_slug: "ds3".to_string(),
            output_path: None,
            config_path: None,
            backup_root: None,
            steam_id64: None,
            steam_root_override: Some(root.to_string()),
            dry_run: true,
        })
        .unwrap();

        assert!(result.dry_run);
        assert!(
            result
                .source_save_path
                .starts_with(secondary_steamapps.as_str())
        );
        assert!(
            !result
                .source_save_path
                .starts_with(primary_steamapps.as_str())
        );
        assert_eq!(result.resolved_candidates.len(), 1);
        assert_eq!(result.selected_pattern_list, vec!["DS*.sl2"]);

        fs::remove_dir_all(root).unwrap();
        fs::remove_dir_all(secondary_root).unwrap();
    }

    #[test]
    fn backup_writes_zip_payload_and_metadata() {
        let root = fake_ds3_steam_root("api-real-backup");

        let result = backup_game(BackupOptions {
            game_slug: "ds3".to_string(),
            output_path: None,
            config_path: None,
            backup_root: None,
            steam_id64: None,
            steam_root_override: Some(root.to_string()),
            dry_run: false,
        })
        .unwrap();

        assert!(!result.dry_run);
        assert!(
            result
                .sha256
                .as_deref()
                .is_some_and(|hash| hash.starts_with("sha256:"))
        );

        let zip_path = Utf8PathBuf::from(&result.output_zip_path);
        let metadata_path = Utf8PathBuf::from(result.metadata_path.as_ref().unwrap());

        assert!(zip_path.is_file());
        assert!(metadata_path.is_file());

        let zip_file = fs::File::open(&zip_path).unwrap();
        let mut archive = ZipArchive::new(zip_file).unwrap();
        let mut save_file = archive.by_name("DS30000.sl2").unwrap();
        let mut save_contents = String::new();
        save_file.read_to_string(&mut save_contents).unwrap();
        assert_eq!(save_contents, "fake save");

        let metadata = fs::read(&metadata_path).unwrap();
        let metadata: BackupResult = serde_json::from_slice(&metadata).unwrap();
        assert_eq!(metadata.slug, "ds3");
        assert_eq!(metadata.output_zip_path, result.output_zip_path);
        assert_eq!(metadata.sha256, result.sha256);
        assert_eq!(metadata.selected_pattern_list, vec!["DS*.sl2"]);
        assert_eq!(metadata.resolved_candidates.len(), 1);

        fs::remove_dir_all(root).unwrap();
        fs::remove_dir_all(metadata_path.parent().unwrap()).unwrap();
    }

    #[test]
    fn plan_restore_succeeds_from_real_backup_artifacts() {
        let root = fake_ds3_steam_root("api-plan-restore");
        let backup = backup_game(BackupOptions {
            game_slug: "ds3".to_string(),
            output_path: None,
            config_path: None,
            backup_root: None,
            steam_id64: None,
            steam_root_override: Some(root.to_string()),
            dry_run: false,
        })
        .unwrap();
        let snapshot_dir = Utf8PathBuf::from(backup.metadata_path.as_ref().unwrap())
            .parent()
            .unwrap()
            .to_path_buf();

        let plan = plan_restore(RestorePlanOptions {
            game_slug: "ds3".to_string(),
            snapshot_path: snapshot_dir.to_string(),
            config_path: None,
            backup_root: None,
            steam_id64: None,
            steam_root_override: Some(root.to_string()),
        })
        .unwrap();

        assert_eq!(plan.target_game_slug, "ds3");
        assert_eq!(plan.selected_snapshot.slug, "ds3");
        assert_eq!(plan.snapshot_inventory.file_count, 1);
        assert_eq!(plan.files_to_restore[0].path, "DS30000.sl2");
        assert_eq!(
            plan.restore_destination_path.as_deref(),
            Some(backup.source_save_path.as_str())
        );
        assert!(plan.target_exists);
        assert!(plan.pre_restore_safety_backup.recommended);
        assert!(plan.warnings.is_empty());

        fs::remove_dir_all(root).unwrap();
        fs::remove_dir_all(snapshot_dir).unwrap();
    }

    #[test]
    fn plan_restore_errors_when_snapshot_is_missing() {
        let err = plan_restore(RestorePlanOptions {
            game_slug: "ds3".to_string(),
            snapshot_path: unique_temp_dir("missing-snapshot").to_string(),
            config_path: None,
            backup_root: None,
            steam_id64: None,
            steam_root_override: None,
        })
        .unwrap_err();

        assert!(err.to_string().contains("snapshot not found"));
    }

    #[test]
    fn plan_restore_warns_when_game_is_not_detected() {
        let root = fake_ds3_steam_root("api-plan-game-not-detected");
        let backup = backup_game(BackupOptions {
            game_slug: "ds3".to_string(),
            output_path: None,
            config_path: None,
            backup_root: None,
            steam_id64: None,
            steam_root_override: Some(root.to_string()),
            dry_run: false,
        })
        .unwrap();
        let snapshot_dir = Utf8PathBuf::from(backup.metadata_path.as_ref().unwrap())
            .parent()
            .unwrap()
            .to_path_buf();
        let missing_root = unique_temp_dir("empty-steam-root");
        fs::create_dir_all(missing_root.join("steamapps")).unwrap();
        fs::write(
            missing_root.join("steamapps/libraryfolders.vdf"),
            "\"libraryfolders\"\n{\n}\n",
        )
        .unwrap();

        let plan = plan_restore(RestorePlanOptions {
            game_slug: "ds3".to_string(),
            snapshot_path: snapshot_dir.to_string(),
            config_path: None,
            backup_root: None,
            steam_id64: None,
            steam_root_override: Some(missing_root.to_string()),
        })
        .unwrap();

        assert!(
            plan.warnings
                .contains(&RestorePlanWarning::GameNotCurrentlyDetected)
        );
        assert!(
            plan.warnings
                .contains(&RestorePlanWarning::NoRestoreTargetResolved)
        );
        assert!(plan.restore_destination_path.is_none());

        fs::remove_dir_all(root).unwrap();
        fs::remove_dir_all(snapshot_dir).unwrap();
        fs::remove_dir_all(missing_root).unwrap();
    }

    #[test]
    fn plan_restore_warns_for_ambiguous_restore_target() {
        let root = fake_ds3_steam_root("api-plan-ambiguous");
        let steamapps = root.join("steamapps");
        let second_save_dir = steamapps.join("compatdata/374320/pfx/drive_c/users/steamuser/AppData/Roaming/DarkSoulsIII/76561198000000001");
        fs::create_dir_all(&second_save_dir).unwrap();
        fs::write(second_save_dir.join("DS30001.sl2"), b"second fake save").unwrap();
        let backup = backup_game(BackupOptions {
            game_slug: "ds3".to_string(),
            output_path: None,
            config_path: None,
            backup_root: None,
            steam_id64: Some("76561198000000000".to_string()),
            steam_root_override: Some(root.to_string()),
            dry_run: false,
        })
        .unwrap();
        let snapshot_dir = Utf8PathBuf::from(backup.metadata_path.as_ref().unwrap())
            .parent()
            .unwrap()
            .to_path_buf();

        let plan = plan_restore(RestorePlanOptions {
            game_slug: "ds3".to_string(),
            snapshot_path: snapshot_dir.to_string(),
            config_path: None,
            backup_root: None,
            steam_id64: None,
            steam_root_override: Some(root.to_string()),
        })
        .unwrap();

        assert!(
            plan.warnings
                .contains(&RestorePlanWarning::MultipleCandidateTargets)
        );
        assert!(
            plan.warnings
                .contains(&RestorePlanWarning::AmbiguousSteamId)
        );
        assert_eq!(plan.resolved_candidates.len(), 2);

        fs::remove_dir_all(root).unwrap();
        fs::remove_dir_all(snapshot_dir).unwrap();
    }

    #[test]
    fn plan_restore_warns_when_explicit_target_is_missing() {
        let root = fake_ds3_steam_root("api-plan-missing-target");
        let backup = backup_game(BackupOptions {
            game_slug: "ds3".to_string(),
            output_path: None,
            config_path: None,
            backup_root: None,
            steam_id64: None,
            steam_root_override: Some(root.to_string()),
            dry_run: false,
        })
        .unwrap();
        let snapshot_dir = Utf8PathBuf::from(backup.metadata_path.as_ref().unwrap())
            .parent()
            .unwrap()
            .to_path_buf();

        let plan = plan_restore(RestorePlanOptions {
            game_slug: "ds3".to_string(),
            snapshot_path: snapshot_dir.to_string(),
            config_path: None,
            backup_root: None,
            steam_id64: Some("76561198000000001".to_string()),
            steam_root_override: Some(root.to_string()),
        })
        .unwrap();

        assert!(plan.warnings.contains(&RestorePlanWarning::SaveDirMissing));
        assert!(!plan.target_exists);
        assert!(
            plan.restore_destination_path
                .as_deref()
                .is_some_and(|path| path.ends_with("76561198000000001"))
        );

        fs::remove_dir_all(root).unwrap();
        fs::remove_dir_all(snapshot_dir).unwrap();
    }

    #[test]
    fn restore_existing_target_creates_safety_backup_and_restores_snapshot() {
        let root = fake_ds3_steam_root("api-restore-existing");
        let backup = backup_game(BackupOptions {
            game_slug: "ds3".to_string(),
            output_path: None,
            config_path: None,
            backup_root: None,
            steam_id64: None,
            steam_root_override: Some(root.to_string()),
            dry_run: false,
        })
        .unwrap();
        let snapshot_dir = Utf8PathBuf::from(backup.metadata_path.as_ref().unwrap())
            .parent()
            .unwrap()
            .to_path_buf();
        let save_dir = Utf8PathBuf::from(&backup.source_save_path);
        fs::write(save_dir.join("DS30000.sl2"), b"current save").unwrap();

        let result = restore_game(RestoreOptions {
            game_slug: "ds3".to_string(),
            snapshot_path: snapshot_dir.to_string(),
            config_path: None,
            backup_root: None,
            steam_id64: None,
            steam_root_override: Some(root.to_string()),
            dry_run: false,
        })
        .unwrap();

        assert!(!result.dry_run);
        assert_eq!(result.restored_file_count, 1);
        assert_eq!(
            fs::read_to_string(save_dir.join("DS30000.sl2")).unwrap(),
            "fake save"
        );
        assert!(result.restore_destination_path.starts_with(root.as_str()));
        let safety_backup = result.safety_backup.as_ref().unwrap();
        let mut safety_zip =
            ZipArchive::new(fs::File::open(&safety_backup.output_zip_path).unwrap()).unwrap();
        let mut saved_before_restore = String::new();
        safety_zip
            .by_name("DS30000.sl2")
            .unwrap()
            .read_to_string(&mut saved_before_restore)
            .unwrap();
        assert_eq!(saved_before_restore, "current save");

        fs::remove_dir_all(root).unwrap();
        fs::remove_dir_all(snapshot_dir).unwrap();
        fs::remove_dir_all(
            Utf8PathBuf::from(safety_backup.metadata_path.as_ref().unwrap())
                .parent()
                .unwrap(),
        )
        .unwrap();
    }

    #[test]
    fn restore_creates_missing_resolved_target() {
        let root = fake_ds3_steam_root("api-restore-missing-target");
        let backup = backup_game(BackupOptions {
            game_slug: "ds3".to_string(),
            output_path: None,
            config_path: None,
            backup_root: None,
            steam_id64: None,
            steam_root_override: Some(root.to_string()),
            dry_run: false,
        })
        .unwrap();
        let snapshot_dir = Utf8PathBuf::from(backup.metadata_path.as_ref().unwrap())
            .parent()
            .unwrap()
            .to_path_buf();
        let save_dir = Utf8PathBuf::from(&backup.source_save_path);
        fs::remove_dir_all(&save_dir).unwrap();

        let result = restore_game(RestoreOptions {
            game_slug: "ds3".to_string(),
            snapshot_path: snapshot_dir.to_string(),
            config_path: None,
            backup_root: None,
            steam_id64: Some("76561198000000000".to_string()),
            steam_root_override: Some(root.to_string()),
            dry_run: false,
        })
        .unwrap();

        assert!(
            result
                .warnings
                .contains(&RestorePlanWarning::SaveDirMissing)
        );
        assert!(result.safety_backup.is_none());
        assert_eq!(
            fs::read_to_string(save_dir.join("DS30000.sl2")).unwrap(),
            "fake save"
        );

        fs::remove_dir_all(root).unwrap();
        fs::remove_dir_all(snapshot_dir).unwrap();
    }

    #[test]
    fn restore_dry_run_performs_no_writes() {
        let root = fake_ds3_steam_root("api-restore-dry-run");
        let backup = backup_game(BackupOptions {
            game_slug: "ds3".to_string(),
            output_path: None,
            config_path: None,
            backup_root: None,
            steam_id64: None,
            steam_root_override: Some(root.to_string()),
            dry_run: false,
        })
        .unwrap();
        let snapshot_dir = Utf8PathBuf::from(backup.metadata_path.as_ref().unwrap())
            .parent()
            .unwrap()
            .to_path_buf();
        let save_dir = Utf8PathBuf::from(&backup.source_save_path);
        fs::write(save_dir.join("DS30000.sl2"), b"current save").unwrap();

        let result = restore_game(RestoreOptions {
            game_slug: "ds3".to_string(),
            snapshot_path: snapshot_dir.to_string(),
            config_path: None,
            backup_root: None,
            steam_id64: None,
            steam_root_override: Some(root.to_string()),
            dry_run: true,
        })
        .unwrap();

        assert!(result.dry_run);
        assert!(result.safety_backup.is_none());
        assert_eq!(
            fs::read_to_string(save_dir.join("DS30000.sl2")).unwrap(),
            "current save"
        );

        fs::remove_dir_all(root).unwrap();
        fs::remove_dir_all(snapshot_dir).unwrap();
    }

    #[test]
    fn restore_validation_failure_does_not_mutate_live_target() {
        let root = fake_ds3_steam_root("api-restore-validation-failure");
        let backup = backup_game(BackupOptions {
            game_slug: "ds3".to_string(),
            output_path: None,
            config_path: None,
            backup_root: None,
            steam_id64: None,
            steam_root_override: Some(root.to_string()),
            dry_run: false,
        })
        .unwrap();
        let snapshot_dir = Utf8PathBuf::from(backup.metadata_path.as_ref().unwrap())
            .parent()
            .unwrap()
            .to_path_buf();
        write_zip_entries(
            &Utf8PathBuf::from(&backup.output_zip_path),
            &[("notes.txt", b"not a save")],
        );
        let save_dir = Utf8PathBuf::from(&backup.source_save_path);
        fs::write(save_dir.join("DS30000.sl2"), b"live save").unwrap();

        let err = restore_game(RestoreOptions {
            game_slug: "ds3".to_string(),
            snapshot_path: snapshot_dir.to_string(),
            config_path: None,
            backup_root: None,
            steam_id64: None,
            steam_root_override: Some(root.to_string()),
            dry_run: false,
        })
        .unwrap_err();

        assert!(err.to_string().contains("SnapshotAppearsIncompatible"));
        assert_eq!(
            fs::read_to_string(save_dir.join("DS30000.sl2")).unwrap(),
            "live save"
        );

        fs::remove_dir_all(root).unwrap();
        fs::remove_dir_all(snapshot_dir).unwrap();
    }

    #[test]
    fn restore_ambiguous_target_without_steam_id_fails_safely() {
        let root = fake_ds3_steam_root("api-restore-ambiguous");
        let steamapps = root.join("steamapps");
        let second_save_dir = steamapps.join("compatdata/374320/pfx/drive_c/users/steamuser/AppData/Roaming/DarkSoulsIII/76561198000000001");
        fs::create_dir_all(&second_save_dir).unwrap();
        fs::write(second_save_dir.join("DS30001.sl2"), b"second live save").unwrap();
        let backup = backup_game(BackupOptions {
            game_slug: "ds3".to_string(),
            output_path: None,
            config_path: None,
            backup_root: None,
            steam_id64: Some("76561198000000000".to_string()),
            steam_root_override: Some(root.to_string()),
            dry_run: false,
        })
        .unwrap();
        let snapshot_dir = Utf8PathBuf::from(backup.metadata_path.as_ref().unwrap())
            .parent()
            .unwrap()
            .to_path_buf();

        let err = restore_game(RestoreOptions {
            game_slug: "ds3".to_string(),
            snapshot_path: snapshot_dir.to_string(),
            config_path: None,
            backup_root: None,
            steam_id64: None,
            steam_root_override: Some(root.to_string()),
            dry_run: false,
        })
        .unwrap_err();

        assert!(err.to_string().contains("MultipleCandidateTargets"));
        assert_eq!(
            fs::read_to_string(second_save_dir.join("DS30001.sl2")).unwrap(),
            "second live save"
        );

        fs::remove_dir_all(root).unwrap();
        fs::remove_dir_all(snapshot_dir).unwrap();
    }

    #[test]
    fn restore_rejects_archive_path_traversal_without_mutating_target() {
        let root = fake_ds3_steam_root("api-restore-traversal");
        let backup = backup_game(BackupOptions {
            game_slug: "ds3".to_string(),
            output_path: None,
            config_path: None,
            backup_root: None,
            steam_id64: None,
            steam_root_override: Some(root.to_string()),
            dry_run: false,
        })
        .unwrap();
        let snapshot_dir = Utf8PathBuf::from(backup.metadata_path.as_ref().unwrap())
            .parent()
            .unwrap()
            .to_path_buf();
        write_zip_entries(
            &Utf8PathBuf::from(&backup.output_zip_path),
            &[("../escape.sl2", b"escape")],
        );
        let save_dir = Utf8PathBuf::from(&backup.source_save_path);
        fs::write(save_dir.join("DS30000.sl2"), b"live save").unwrap();

        let err = restore_game(RestoreOptions {
            game_slug: "ds3".to_string(),
            snapshot_path: snapshot_dir.to_string(),
            config_path: None,
            backup_root: None,
            steam_id64: None,
            steam_root_override: Some(root.to_string()),
            dry_run: false,
        })
        .unwrap_err();

        assert!(err.to_string().contains("unsafe zip entry path"));
        assert_eq!(
            fs::read_to_string(save_dir.join("DS30000.sl2")).unwrap(),
            "live save"
        );

        fs::remove_dir_all(root).unwrap();
        fs::remove_dir_all(snapshot_dir).unwrap();
    }

    #[test]
    fn backup_surfaces_running_game_activity_warning() {
        let root = fake_ds3_steam_root("api-activity-backup");
        let provider = FakeProcessProvider::new("DarkSoulsIII.exe");

        let result = backup_game_with_process_provider(
            BackupOptions {
                game_slug: "ds3".to_string(),
                output_path: None,
                config_path: None,
                backup_root: None,
                steam_id64: None,
                steam_root_override: Some(root.to_string()),
                dry_run: true,
            },
            &provider,
        )
        .unwrap();

        assert_eq!(result.activity_warnings.len(), 1);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn restore_planning_surfaces_running_game_activity_warning() {
        let root = fake_ds3_steam_root("api-activity-plan");
        let backup = backup_game(BackupOptions {
            game_slug: "ds3".to_string(),
            output_path: None,
            config_path: None,
            backup_root: None,
            steam_id64: None,
            steam_root_override: Some(root.to_string()),
            dry_run: false,
        })
        .unwrap();
        let snapshot_dir = Utf8PathBuf::from(backup.metadata_path.as_ref().unwrap())
            .parent()
            .unwrap()
            .to_path_buf();
        let provider = FakeProcessProvider::new("DarkSoulsIII.exe");

        let plan = plan_restore_with_process_provider(
            RestorePlanOptions {
                game_slug: "ds3".to_string(),
                snapshot_path: snapshot_dir.to_string(),
                config_path: None,
                backup_root: None,
                steam_id64: None,
                steam_root_override: Some(root.to_string()),
            },
            &provider,
        )
        .unwrap();

        assert_eq!(plan.activity_warnings.len(), 1);
        assert!(plan.blocking_warnings.is_empty());

        fs::remove_dir_all(root).unwrap();
        fs::remove_dir_all(snapshot_dir).unwrap();
    }

    #[test]
    fn restore_execution_surfaces_activity_warning_without_blocking() {
        let root = fake_ds3_steam_root("api-activity-restore");
        let backup = backup_game(BackupOptions {
            game_slug: "ds3".to_string(),
            output_path: None,
            config_path: None,
            backup_root: None,
            steam_id64: None,
            steam_root_override: Some(root.to_string()),
            dry_run: false,
        })
        .unwrap();
        let snapshot_dir = Utf8PathBuf::from(backup.metadata_path.as_ref().unwrap())
            .parent()
            .unwrap()
            .to_path_buf();
        let provider = FakeProcessProvider::new("DarkSoulsIII.exe");

        let result = restore_game_with_process_provider(
            RestoreOptions {
                game_slug: "ds3".to_string(),
                snapshot_path: snapshot_dir.to_string(),
                config_path: None,
                backup_root: None,
                steam_id64: None,
                steam_root_override: Some(root.to_string()),
                dry_run: true,
            },
            &provider,
        )
        .unwrap();

        assert!(result.dry_run);
        assert_eq!(result.activity_warnings.len(), 1);
        assert!(result.blocking_warnings.is_empty());

        fs::remove_dir_all(root).unwrap();
        fs::remove_dir_all(snapshot_dir).unwrap();
    }

    #[test]
    fn backup_uses_configured_backup_root_and_steam_root() {
        let root = fake_ds3_steam_root("api-config-backup-steam-root");
        let backup_root = unique_temp_dir("api-config-backup-root");
        let config_path = write_config_file(
            "api-config-backup",
            &format!(
                "[paths]\nbackup_root = \"{}\"\nsteam_root_override = \"{}\"\n",
                backup_root, root
            ),
        );

        let result = backup_game(BackupOptions {
            game_slug: "ds3".to_string(),
            output_path: None,
            config_path: Some(config_path.to_string()),
            backup_root: None,
            steam_id64: None,
            steam_root_override: None,
            dry_run: false,
        })
        .unwrap();

        assert!(result.output_zip_path.starts_with(backup_root.as_str()));
        assert!(result.source_save_path.starts_with(root.as_str()));

        fs::remove_dir_all(root).unwrap();
        fs::remove_dir_all(backup_root).unwrap();
        fs::remove_dir_all(config_path.parent().unwrap()).unwrap();
    }

    #[test]
    fn restore_planning_and_restore_use_configured_backup_root() {
        let root = fake_ds3_steam_root("api-config-restore-root");
        let backup_root = unique_temp_dir("api-config-restore-backups");
        let config_path = write_config_file(
            "api-config-restore",
            &format!(
                "[paths]\nbackup_root = \"{}\"\nsteam_root_override = \"{}\"\n",
                backup_root, root
            ),
        );
        let backup = backup_game(BackupOptions {
            game_slug: "ds3".to_string(),
            output_path: None,
            config_path: Some(config_path.to_string()),
            backup_root: None,
            steam_id64: None,
            steam_root_override: None,
            dry_run: false,
        })
        .unwrap();
        let snapshot_id = format!("{}/{}", backup.slug, backup.timestamp);
        let save_dir = Utf8PathBuf::from(&backup.source_save_path);
        fs::write(save_dir.join("DS30000.sl2"), b"changed save").unwrap();

        let plan = plan_restore(RestorePlanOptions {
            game_slug: "ds3".to_string(),
            snapshot_path: snapshot_id.clone(),
            config_path: Some(config_path.to_string()),
            backup_root: None,
            steam_id64: None,
            steam_root_override: None,
        })
        .unwrap();
        assert!(
            plan.snapshot_inventory
                .payload_zip_path
                .starts_with(backup_root.as_str())
        );

        restore_game(RestoreOptions {
            game_slug: "ds3".to_string(),
            snapshot_path: snapshot_id,
            config_path: Some(config_path.to_string()),
            backup_root: None,
            steam_id64: None,
            steam_root_override: None,
            dry_run: false,
        })
        .unwrap();

        assert_eq!(
            fs::read_to_string(save_dir.join("DS30000.sl2")).unwrap(),
            "fake save"
        );

        fs::remove_dir_all(root).unwrap();
        fs::remove_dir_all(backup_root).unwrap();
        fs::remove_dir_all(config_path.parent().unwrap()).unwrap();
    }

    #[test]
    fn snapshot_listing_uses_configured_backup_root() {
        let backup_root = unique_temp_dir("api-config-list-backups");
        let config_path = write_config_file(
            "api-config-list",
            &format!("[paths]\nbackup_root = \"{}\"\n", backup_root),
        );
        write_snapshot_fixture(&backup_root, "ds3", "20000101T000000Z", true, true, true);

        let result = list_snapshots(ListSnapshotsOptions {
            config_path: Some(config_path.to_string()),
            backup_root: None,
            game_slug: None,
        })
        .unwrap();

        assert_eq!(result.backup_root, backup_root.to_string());
        assert_eq!(result.snapshots[0].id, "ds3/20000101T000000Z");

        fs::remove_dir_all(backup_root).unwrap();
        fs::remove_dir_all(config_path.parent().unwrap()).unwrap();
    }

    #[test]
    fn list_snapshots_filters_single_game_from_backup_root() {
        let root = fake_ds3_steam_root("api-snapshots-single");
        let backup_root = unique_temp_dir("snapshot-root-single");
        let backup = backup_game(BackupOptions {
            game_slug: "ds3".to_string(),
            output_path: None,
            config_path: None,
            backup_root: Some(backup_root.to_string()),
            steam_id64: None,
            steam_root_override: Some(root.to_string()),
            dry_run: false,
        })
        .unwrap();
        write_snapshot_fixture(
            &backup_root,
            "elden-ring",
            "20000101T000000Z",
            true,
            true,
            true,
        );

        let result = list_snapshots(ListSnapshotsOptions {
            config_path: None,
            backup_root: Some(backup_root.to_string()),
            game_slug: Some("ds3".to_string()),
        })
        .unwrap();

        assert_eq!(result.snapshots.len(), 1);
        assert_eq!(result.snapshots[0].game_slug, "ds3");
        assert_eq!(
            result.snapshots[0].archive_size,
            fs::metadata(&backup.output_zip_path)
                .ok()
                .map(|metadata| metadata.len())
        );
        assert_eq!(result.snapshots[0].integrity, SnapshotIntegrity::Verified);

        fs::remove_dir_all(root).unwrap();
        fs::remove_dir_all(backup_root).unwrap();
    }

    #[test]
    fn list_snapshots_across_games_newest_first() {
        let backup_root = unique_temp_dir("snapshot-root-all");
        write_snapshot_fixture(&backup_root, "ds3", "20000101T000000Z", true, true, true);
        write_snapshot_fixture(
            &backup_root,
            "elden-ring",
            "20010101T000000Z",
            true,
            true,
            true,
        );

        let result = list_snapshots(ListSnapshotsOptions {
            config_path: None,
            backup_root: Some(backup_root.to_string()),
            game_slug: None,
        })
        .unwrap();

        assert_eq!(result.snapshots.len(), 2);
        assert_eq!(result.snapshots[0].id, "elden-ring/20010101T000000Z");
        assert_eq!(result.snapshots[1].id, "ds3/20000101T000000Z");

        fs::remove_dir_all(backup_root).unwrap();
    }

    #[test]
    fn show_snapshot_returns_metadata_and_inventory() {
        let backup_root = unique_temp_dir("snapshot-root-show");
        write_snapshot_fixture(&backup_root, "ds3", "20000101T000000Z", true, true, true);

        let result = show_snapshot(ShowSnapshotOptions {
            config_path: None,
            backup_root: Some(backup_root.to_string()),
            snapshot: "ds3/20000101T000000Z".to_string(),
        })
        .unwrap();

        assert_eq!(result.snapshot.id, "ds3/20000101T000000Z");
        assert_eq!(result.metadata.as_ref().unwrap().slug, "ds3");
        assert_eq!(result.inventory.len(), 1);
        assert_eq!(result.inventory[0].path, "DS30000.sl2");

        fs::remove_dir_all(backup_root).unwrap();
    }

    #[test]
    fn restore_accepts_listed_snapshot_id() {
        let root = fake_ds3_steam_root("api-snapshot-restore-id");
        let backup_root = unique_temp_dir("snapshot-root-restore-id");
        let backup = backup_game(BackupOptions {
            game_slug: "ds3".to_string(),
            output_path: None,
            config_path: None,
            backup_root: Some(backup_root.to_string()),
            steam_id64: None,
            steam_root_override: Some(root.to_string()),
            dry_run: false,
        })
        .unwrap();
        let list = list_snapshots(ListSnapshotsOptions {
            config_path: None,
            backup_root: Some(backup_root.to_string()),
            game_slug: Some("ds3".to_string()),
        })
        .unwrap();
        let save_dir = Utf8PathBuf::from(&backup.source_save_path);
        fs::write(save_dir.join("DS30000.sl2"), b"changed save").unwrap();

        restore_game(RestoreOptions {
            game_slug: "ds3".to_string(),
            snapshot_path: list.snapshots[0].id.clone(),
            config_path: None,
            backup_root: Some(backup_root.to_string()),
            steam_id64: None,
            steam_root_override: Some(root.to_string()),
            dry_run: false,
        })
        .unwrap();

        assert_eq!(
            fs::read_to_string(save_dir.join("DS30000.sl2")).unwrap(),
            "fake save"
        );

        fs::remove_dir_all(root).unwrap();
        fs::remove_dir_all(backup_root).unwrap();
    }

    #[test]
    fn list_snapshot_with_missing_archive_reports_warning() {
        let backup_root = unique_temp_dir("snapshot-root-missing-archive");
        write_snapshot_fixture(&backup_root, "ds3", "20000101T000000Z", true, false, true);

        let result = list_snapshots(ListSnapshotsOptions {
            config_path: None,
            backup_root: Some(backup_root.to_string()),
            game_slug: None,
        })
        .unwrap();

        assert!(
            result.snapshots[0]
                .warnings
                .contains(&SnapshotWarning::MissingArchive)
        );
        assert_eq!(result.snapshots[0].integrity, SnapshotIntegrity::Unknown);

        fs::remove_dir_all(backup_root).unwrap();
    }

    #[test]
    fn list_snapshot_with_missing_metadata_reports_warning() {
        let backup_root = unique_temp_dir("snapshot-root-missing-metadata");
        write_snapshot_fixture(&backup_root, "ds3", "20000101T000000Z", false, true, true);

        let result = list_snapshots(ListSnapshotsOptions {
            config_path: None,
            backup_root: Some(backup_root.to_string()),
            game_slug: None,
        })
        .unwrap();

        assert!(
            result.snapshots[0]
                .warnings
                .contains(&SnapshotWarning::MissingMetadata)
        );
        assert_eq!(result.snapshots[0].file_count, Some(1));

        fs::remove_dir_all(backup_root).unwrap();
    }

    #[test]
    fn invalid_metadata_does_not_crash_listing() {
        let backup_root = unique_temp_dir("snapshot-root-invalid-metadata");
        let snapshot_dir = backup_root.join("ds3/20000101T000000Z");
        fs::create_dir_all(&snapshot_dir).unwrap();
        fs::write(snapshot_dir.join("metadata.json"), b"not json").unwrap();
        write_zip_entries(
            &snapshot_dir.join("payload.zip"),
            &[("DS30000.sl2", b"fake save")],
        );

        let result = list_snapshots(ListSnapshotsOptions {
            config_path: None,
            backup_root: Some(backup_root.to_string()),
            game_slug: None,
        })
        .unwrap();

        assert!(
            result.snapshots[0]
                .warnings
                .contains(&SnapshotWarning::InvalidMetadata)
        );
        assert_eq!(result.snapshots[0].id, "ds3/20000101T000000Z");

        fs::remove_dir_all(backup_root).unwrap();
    }

    #[test]
    fn legacy_snapshot_without_integrity_still_lists_with_warning() {
        let backup_root = unique_temp_dir("snapshot-root-legacy");
        write_snapshot_fixture(&backup_root, "ds3", "20000101T000000Z", true, true, false);

        let result = list_snapshots(ListSnapshotsOptions {
            config_path: None,
            backup_root: Some(backup_root.to_string()),
            game_slug: None,
        })
        .unwrap();

        assert_eq!(
            result.snapshots[0].integrity,
            SnapshotIntegrity::Unavailable
        );
        assert!(
            result.snapshots[0]
                .warnings
                .contains(&SnapshotWarning::LegacyIntegrityMissing)
        );

        fs::remove_dir_all(backup_root).unwrap();
    }

    fn fake_ds3_steam_root(name: &str) -> Utf8PathBuf {
        let root = unique_temp_dir(name);
        let steamapps = root.join("steamapps");
        let save_dir = steamapps.join("compatdata/374320/pfx/drive_c/users/steamuser/AppData/Roaming/DarkSoulsIII/76561198000000000");
        fs::create_dir_all(&save_dir).unwrap();
        fs::write(
            steamapps.join("libraryfolders.vdf"),
            "\"libraryfolders\"\n{\n}\n",
        )
        .unwrap();
        fs::write(
            steamapps.join("appmanifest_374320.acf"),
            "\"AppState\"\n{\n  \"appid\" \"374320\"\n  \"name\" \"DARK SOULS III\"\n  \"installdir\" \"DARK SOULS III\"\n}\n",
        )
        .unwrap();
        fs::write(save_dir.join("DS30000.sl2"), b"fake save").unwrap();
        root
    }

    fn unique_temp_dir(name: &str) -> Utf8PathBuf {
        let nanos = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        Utf8PathBuf::from_path_buf(std::env::temp_dir().join(format!("savecore-{name}-{nanos}")))
            .unwrap()
    }

    fn write_zip_entries(zip_path: &Utf8PathBuf, entries: &[(&str, &[u8])]) {
        let file = fs::File::create(zip_path).unwrap();
        let mut zip = ZipWriter::new(file);
        let opts = FileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        for (name, contents) in entries {
            zip.start_file(*name, opts).unwrap();
            zip.write_all(contents).unwrap();
        }
        zip.finish().unwrap();
    }

    fn write_snapshot_fixture(
        backup_root: &Utf8PathBuf,
        game_slug: &str,
        timestamp: &str,
        include_metadata: bool,
        include_archive: bool,
        include_hash: bool,
    ) {
        let snapshot_dir = backup_root.join(game_slug).join(timestamp);
        fs::create_dir_all(&snapshot_dir).unwrap();
        let archive_path = snapshot_dir.join("payload.zip");
        if include_archive {
            write_zip_entries(&archive_path, &[("DS30000.sl2", b"fake save")]);
        }
        if include_metadata {
            let sha256 = include_hash
                .then(|| savecore_hash_for_test(&archive_path))
                .flatten();
            let metadata = serde_json::json!({
                "game_name": game_slug,
                "slug": game_slug,
                "timestamp": timestamp,
                "source_save_path": "/tmp/source-save",
                "resolved_candidates": [
                    {
                        "path": "/tmp/source-save",
                        "source_template": "/tmp/source-save",
                        "steam_id64": "76561198000000000",
                        "exists": true,
                        "matched_patterns": ["DS*.sl2"]
                    }
                ],
                "selected_pattern_list": ["DS*.sl2"],
                "output_zip_path": archive_path.to_string(),
                "metadata_path": snapshot_dir.join("metadata.json").to_string(),
                "sha256": sha256,
                "dry_run": false
            });
            fs::write(
                snapshot_dir.join("metadata.json"),
                serde_json::to_vec_pretty(&metadata).unwrap(),
            )
            .unwrap();
        }
    }

    fn savecore_hash_for_test(path: &Utf8PathBuf) -> Option<String> {
        crate::ops::sha256_file(path).ok()
    }

    fn write_config_file(name: &str, contents: &str) -> Utf8PathBuf {
        let dir = unique_temp_dir(name);
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.toml");
        fs::write(&path, contents).unwrap();
        path
    }

    struct FakeProcessProvider {
        processes: Vec<crate::process::ProcessInfo>,
    }

    impl FakeProcessProvider {
        fn new(executable: &str) -> Self {
            Self {
                processes: vec![crate::process::ProcessInfo {
                    pid: 4242,
                    executable: executable.to_string(),
                    command_line: None,
                }],
            }
        }
    }

    impl crate::process::ProcessProvider for FakeProcessProvider {
        fn running_processes(&self) -> anyhow::Result<Vec<crate::process::ProcessInfo>> {
            Ok(self.processes.clone())
        }
    }
}
