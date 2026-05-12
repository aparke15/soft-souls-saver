use crate::{steam::STEAM_ROOT_ENV, types::*};
use camino::Utf8PathBuf;
use std::{env, fs};

const CONFIG_DIR_NAME: &str = "soft-souls";
const CONFIG_FILE_NAME: &str = "config.toml";
const DEFAULT_BACKUP_ROOT: &str = "backups";

pub fn show_effective_settings(
    options: EffectiveSettingsOptions,
) -> anyhow::Result<EffectiveSettingsResult> {
    resolve_effective_settings(SettingsInputs {
        config_path: options.config_path,
        backup_root: options.backup_root,
        steam_root_override: options.steam_root_override,
    })
}

#[derive(Debug, Clone, Default)]
pub struct SettingsInputs {
    pub config_path: Option<String>,
    pub backup_root: Option<String>,
    pub steam_root_override: Option<String>,
}

pub fn resolve_effective_settings(
    inputs: SettingsInputs,
) -> anyhow::Result<EffectiveSettingsResult> {
    resolve_effective_settings_with_env(inputs, env::var(STEAM_ROOT_ENV).ok())
}

pub fn resolve_effective_settings_with_env(
    inputs: SettingsInputs,
    steam_root_env: Option<String>,
) -> anyhow::Result<EffectiveSettingsResult> {
    let config_path = inputs
        .config_path
        .as_deref()
        .map(Utf8PathBuf::from)
        .or_else(default_config_path);
    let (config_loaded, config) = load_config(config_path.as_ref())?;
    let config_paths = config.paths.as_ref();

    let backup_root = inputs
        .backup_root
        .or_else(|| config_paths.and_then(|paths| paths.backup_root.clone()))
        .unwrap_or_else(|| DEFAULT_BACKUP_ROOT.to_string());
    let steam_root_override = inputs
        .steam_root_override
        .or(steam_root_env)
        .or_else(|| config_paths.and_then(|paths| paths.steam_root_override.clone()));

    Ok(EffectiveSettingsResult {
        config_path: config_path.map(|path| path.to_string()),
        config_loaded,
        backup_root,
        steam_root_override,
    })
}

pub fn backup_root_path(backup_root: &str) -> Utf8PathBuf {
    Utf8PathBuf::from(backup_root)
}

pub fn default_config_path() -> Option<Utf8PathBuf> {
    default_config_path_from_env(env::var("XDG_CONFIG_HOME").ok(), env::var("HOME").ok())
}

pub fn default_config_path_from_env(
    xdg_config_home: Option<String>,
    home: Option<String>,
) -> Option<Utf8PathBuf> {
    xdg_config_home
        .map(Utf8PathBuf::from)
        .or_else(|| home.map(|home| Utf8PathBuf::from(home).join(".config")))
        .map(|config_home| config_home.join(CONFIG_DIR_NAME).join(CONFIG_FILE_NAME))
}

fn load_config(config_path: Option<&Utf8PathBuf>) -> anyhow::Result<(bool, AppConfig)> {
    let Some(config_path) = config_path else {
        return Ok((false, AppConfig::default()));
    };

    if !config_path.is_file() {
        return Ok((false, AppConfig::default()));
    }

    let text = fs::read_to_string(config_path)
        .map_err(|error| anyhow::anyhow!("failed to read config {config_path}: {error}"))?;
    let config = toml::from_str::<AppConfig>(&text)
        .map_err(|error| anyhow::anyhow!("failed to parse config {config_path}: {error}"))?;

    Ok((true, config))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{fs, time::SystemTime};

    #[test]
    fn no_config_file_uses_defaults() {
        let config_path = unique_temp_dir("missing-config").join("config.toml");

        let settings = resolve_effective_settings_with_env(
            SettingsInputs {
                config_path: Some(config_path.to_string()),
                ..Default::default()
            },
            None,
        )
        .unwrap();

        assert!(!settings.config_loaded);
        assert_eq!(settings.backup_root, "backups");
        assert_eq!(settings.steam_root_override, None);
    }

    #[test]
    fn config_file_sets_backup_and_steam_roots() {
        let config_path = write_config(
            "config-roots",
            "[paths]\nbackup_root = \"/tmp/backups\"\nsteam_root_override = \"/tmp/steam\"\n",
        );

        let settings = resolve_effective_settings_with_env(
            SettingsInputs {
                config_path: Some(config_path.to_string()),
                ..Default::default()
            },
            None,
        )
        .unwrap();

        assert!(settings.config_loaded);
        assert_eq!(settings.backup_root, "/tmp/backups");
        assert_eq!(settings.steam_root_override.as_deref(), Some("/tmp/steam"));
    }

    #[test]
    fn env_steam_root_overrides_config() {
        let config_path = write_config(
            "config-env-overrides",
            "[paths]\nsteam_root_override = \"/tmp/config-steam\"\n",
        );

        let settings = resolve_effective_settings_with_env(
            SettingsInputs {
                config_path: Some(config_path.to_string()),
                ..Default::default()
            },
            Some("/tmp/env-steam".to_string()),
        )
        .unwrap();

        assert_eq!(
            settings.steam_root_override.as_deref(),
            Some("/tmp/env-steam")
        );
    }

    #[test]
    fn explicit_options_override_env_and_config() {
        let config_path = write_config(
            "config-explicit-overrides",
            "[paths]\nbackup_root = \"/tmp/config-backups\"\nsteam_root_override = \"/tmp/config-steam\"\n",
        );

        let settings = resolve_effective_settings_with_env(
            SettingsInputs {
                config_path: Some(config_path.to_string()),
                backup_root: Some("/tmp/api-backups".to_string()),
                steam_root_override: Some("/tmp/api-steam".to_string()),
            },
            Some("/tmp/env-steam".to_string()),
        )
        .unwrap();

        assert_eq!(settings.backup_root, "/tmp/api-backups");
        assert_eq!(
            settings.steam_root_override.as_deref(),
            Some("/tmp/api-steam")
        );
    }

    #[test]
    fn malformed_config_returns_helpful_error() {
        let config_path = write_config("config-malformed", "[paths\nbackup_root = true\n");

        let err = resolve_effective_settings_with_env(
            SettingsInputs {
                config_path: Some(config_path.to_string()),
                ..Default::default()
            },
            None,
        )
        .unwrap_err();

        assert!(err.to_string().contains("failed to parse config"));
        assert!(err.to_string().contains(config_path.as_str()));
    }

    #[test]
    fn xdg_config_path_resolution_uses_temp_xdg_home() {
        let path = default_config_path_from_env(
            Some("/tmp/xdg-config".to_string()),
            Some("/tmp/home".to_string()),
        )
        .unwrap();

        assert_eq!(path.as_str(), "/tmp/xdg-config/soft-souls/config.toml");
    }

    #[test]
    fn config_path_resolution_falls_back_to_home_config() {
        let path = default_config_path_from_env(None, Some("/tmp/home".to_string())).unwrap();

        assert_eq!(path.as_str(), "/tmp/home/.config/soft-souls/config.toml");
    }

    fn write_config(name: &str, contents: &str) -> Utf8PathBuf {
        let dir = unique_temp_dir(name);
        let path = dir.join("config.toml");
        fs::create_dir_all(&dir).unwrap();
        fs::write(&path, contents).unwrap();
        path
    }

    fn unique_temp_dir(name: &str) -> Utf8PathBuf {
        let nanos = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        Utf8PathBuf::from_path_buf(env::temp_dir().join(format!("savecore-{name}-{nanos}")))
            .unwrap()
    }
}
