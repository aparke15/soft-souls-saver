use crate::{backup, load_manifests, resolve, steam, types::*};
use anyhow::{Context, bail};

// Shared application boundary for savectl now, and future saveui/savehost adapters later.
pub fn detect_games() -> anyhow::Result<DetectGamesResult> {
    let manifests = load_manifests()?;
    let steam_install = steam::find()?;
    let mut games = Vec::new();

    for manifest in manifests {
        let steam_appid = manifest.ids.as_ref().and_then(|ids| ids.steam);
        let installed = steam_appid.and_then(|id| steam_install.apps.get(&id));

        let Some(app) = installed else {
            continue;
        };

        let save_candidates = resolve::resolve_save_candidates(&manifest, None)?;

        games.push(DetectGame {
            name: manifest.name,
            slug: manifest.slug,
            steam_appid,
            installed_name: Some(app.name.clone()),
            install_dir: Some(app.install_dir.to_string()),
            save_candidates,
        });
    }

    Ok(DetectGamesResult { games })
}

pub fn backup_game(options: BackupOptions) -> anyhow::Result<BackupResult> {
    let manifests = load_manifests()?;
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

    if let Some(appid) = manifest.ids.as_ref().and_then(|ids| ids.steam) {
        let steam_install = steam::find()?;
        if !steam_install.apps.contains_key(&appid) {
            bail!(
                "'{}' is known as '{}' but Steam app {} was not detected as installed",
                options.game_slug,
                manifest.name,
                appid
            );
        }
    }

    backup::backup_game(manifest, options)
}
