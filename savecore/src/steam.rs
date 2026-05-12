use camino::Utf8PathBuf;
use std::{collections::HashMap, fs};

pub const STEAM_ROOT_ENV: &str = "soft_souls_steam_root";

#[derive(Debug, Clone)]
pub struct SteamInstall {
    pub libraries: Vec<Utf8PathBuf>,
    pub apps: HashMap<u32, SteamApp>,
}

#[derive(Debug, Clone)]
pub struct SteamApp {
    pub appid: u32,
    pub name: String,
    pub steamapps_dir: Utf8PathBuf,
    pub install_dir: Utf8PathBuf,
}

#[derive(Debug, Clone, Default)]
pub struct SteamDiscoveryOptions {
    pub steam_root_override: Option<String>,
}

pub fn find() -> anyhow::Result<SteamInstall> {
    find_with_options(&SteamDiscoveryOptions::default())
}

pub fn find_with_options(options: &SteamDiscoveryOptions) -> anyhow::Result<SteamInstall> {
    let override_root = options
        .steam_root_override
        .clone()
        .or_else(|| std::env::var(STEAM_ROOT_ENV).ok());

    if let Some(root) = override_root {
        return find_from_roots(vec![Utf8PathBuf::from(
            shellexpand::tilde(&root).to_string(),
        )]);
    }

    find_from_roots(default_steam_roots())
}

pub(crate) fn find_from_roots(roots: Vec<Utf8PathBuf>) -> anyhow::Result<SteamInstall> {
    let mut libraries = vec![];
    for root in &roots {
        let libvdf = root.join("steamapps/libraryfolders.vdf");
        if !libvdf.exists() {
            continue;
        }

        libraries.push(root.join("steamapps"));

        let txt = fs::read_to_string(&libvdf)?;
        for path in parse_libraryfolders_paths(&txt) {
            let p = Utf8PathBuf::from(path).join("steamapps");
            if p.exists() {
                libraries.push(p);
            }
        }
    }

    libraries.sort();
    libraries.dedup();

    let mut apps = HashMap::new();

    for lib in &libraries {
        for entry in fs::read_dir(lib)? {
            let entry = entry?;
            let file_name = entry.file_name().to_string_lossy().to_string();

            if !file_name.starts_with("appmanifest_") || !file_name.ends_with(".acf") {
                continue;
            }

            let txt = fs::read_to_string(entry.path())?;

            let Some(appid) = parse_vdf_value(&txt, "appid").and_then(|s| s.parse().ok()) else {
                continue;
            };

            let name = parse_vdf_value(&txt, "name").unwrap_or_default();
            let installdir = parse_vdf_value(&txt, "installdir").unwrap_or_default();

            apps.insert(
                appid,
                SteamApp {
                    appid,
                    name,
                    steamapps_dir: lib.clone(),
                    install_dir: lib.join("common").join(installdir),
                },
            );
        }
    }

    Ok(SteamInstall { libraries, apps })
}

fn default_steam_roots() -> Vec<Utf8PathBuf> {
    platform_steam_roots()
        .into_iter()
        .filter(|path| path.exists())
        .collect()
}

#[cfg(target_os = "windows")]
fn platform_steam_roots() -> Vec<Utf8PathBuf> {
    let mut roots = windows_registry_steam_roots();

    for path in [
        std::env::var("ProgramFiles(x86)")
            .ok()
            .map(|root| Utf8PathBuf::from(root).join("Steam")),
        std::env::var("ProgramFiles")
            .ok()
            .map(|root| Utf8PathBuf::from(root).join("Steam")),
    ]
    .into_iter()
    .flatten()
    {
        roots.push(path);
    }

    roots.sort();
    roots.dedup();
    roots
}

#[cfg(not(target_os = "windows"))]
fn platform_steam_roots() -> Vec<Utf8PathBuf> {
    [
        "~/.local/share/Steam",
        "~/.steam/steam",
        "~/.var/app/com.valvesoftware.Steam/.local/share/Steam",
    ]
    .into_iter()
    .map(|path| Utf8PathBuf::from(shellexpand::tilde(path).to_string()))
    .collect()
}

#[cfg(target_os = "windows")]
fn windows_registry_steam_roots() -> Vec<Utf8PathBuf> {
    use winreg::{RegKey, enums::*};

    let mut roots = Vec::new();
    for (hive, subkey) in [
        (HKEY_CURRENT_USER, "Software\\Valve\\Steam"),
        (HKEY_LOCAL_MACHINE, "Software\\Valve\\Steam"),
        (HKEY_LOCAL_MACHINE, "Software\\WOW6432Node\\Valve\\Steam"),
    ] {
        let key = RegKey::predef(hive);
        let Ok(steam_key) = key.open_subkey(subkey) else {
            continue;
        };
        let Ok(path) = steam_key.get_value::<String, _>("InstallPath") else {
            continue;
        };
        roots.push(Utf8PathBuf::from(path));
    }

    roots
}

fn parse_libraryfolders_paths(txt: &str) -> Vec<String> {
    txt.lines()
        .filter_map(|line| parse_kv_line(line).filter(|(k, _)| k == "path"))
        .map(|(_, v)| normalize_steam_library_path(&v))
        .collect()
}

fn normalize_steam_library_path(path: &str) -> String {
    path.replace('\\', "/")
}

fn parse_vdf_value(txt: &str, key: &str) -> Option<String> {
    txt.lines()
        .filter_map(parse_kv_line)
        .find_map(|(k, v)| (k == key).then_some(v))
}

fn parse_kv_line(line: &str) -> Option<(String, String)> {
    let mut parts = line.split('"').filter(|s| !s.trim().is_empty());

    let key = parts.next()?.trim().to_string();
    let value = parts.next()?.trim().to_string();

    Some((key, value))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{fs, time::SystemTime};

    #[test]
    fn discovers_apps_from_fake_steam_root() {
        let root = unique_temp_dir("fake-steam-root");
        let steamapps = root.join("steamapps");
        fs::create_dir_all(&steamapps).unwrap();
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

        let install = find_with_options(&SteamDiscoveryOptions {
            steam_root_override: Some(root.to_string()),
        })
        .unwrap();
        let app = install.apps.get(&374320).unwrap();

        assert_eq!(app.steamapps_dir, steamapps);
        assert_eq!(app.install_dir, steamapps.join("common/DARK SOULS III"));

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn discovers_apps_from_secondary_libraryfolders_path() {
        let root = unique_temp_dir("fake-steam-root-secondary");
        let primary_steamapps = root.join("steamapps");
        let secondary_root = unique_temp_dir("fake-steam-secondary-library");
        let secondary_steamapps = secondary_root.join("steamapps");

        fs::create_dir_all(&primary_steamapps).unwrap();
        fs::create_dir_all(&secondary_steamapps).unwrap();
        fs::write(
            primary_steamapps.join("libraryfolders.vdf"),
            format!(
                "\"libraryfolders\"\n{{\n  \"1\"\n  {{\n    \"path\" \"{}\"\n  }}\n}}\n",
                secondary_root.as_str().replace('/', "\\")
            ),
        )
        .unwrap();
        fs::write(
            secondary_steamapps.join("appmanifest_374320.acf"),
            "\"AppState\"\n{\n  \"appid\" \"374320\"\n  \"name\" \"DARK SOULS III\"\n  \"installdir\" \"DARK SOULS III\"\n}\n",
        )
        .unwrap();

        let install = find_with_options(&SteamDiscoveryOptions {
            steam_root_override: Some(root.to_string()),
        })
        .unwrap();
        let app = install.apps.get(&374320).unwrap();

        assert!(install.libraries.contains(&primary_steamapps));
        assert!(install.libraries.contains(&secondary_steamapps));
        assert_eq!(app.steamapps_dir, secondary_steamapps);
        assert_eq!(
            app.install_dir,
            secondary_steamapps.join("common/DARK SOULS III")
        );

        fs::remove_dir_all(root).unwrap();
        fs::remove_dir_all(secondary_root).unwrap();
    }

    #[test]
    fn discovers_windows_style_secondary_library_path() {
        let root = unique_temp_dir("fake-windows-steam-root");
        let primary_steamapps = root.join("steamapps");
        let secondary_root = unique_temp_dir("fake-windows-secondary-library");
        let secondary_steamapps = secondary_root.join("steamapps");

        fs::create_dir_all(&primary_steamapps).unwrap();
        fs::create_dir_all(&secondary_steamapps).unwrap();
        fs::write(
            primary_steamapps.join("libraryfolders.vdf"),
            format!(
                "\"libraryfolders\"\n{{\n  \"1\"\n  {{\n    \"path\" \"{}\"\n  }}\n}}\n",
                secondary_root.as_str().replace('/', "\\")
            ),
        )
        .unwrap();
        fs::write(
            secondary_steamapps.join("appmanifest_1245620.acf"),
            "\"AppState\"\n{\n  \"appid\" \"1245620\"\n  \"name\" \"ELDEN RING\"\n  \"installdir\" \"ELDEN RING\"\n}\n",
        )
        .unwrap();

        let install = find_from_roots(vec![root.clone()]).unwrap();
        let app = install.apps.get(&1245620).unwrap();

        assert_eq!(app.steamapps_dir, secondary_steamapps);
        assert_eq!(
            app.install_dir,
            secondary_steamapps.join("common/ELDEN RING")
        );

        fs::remove_dir_all(root).unwrap();
        fs::remove_dir_all(secondary_root).unwrap();
    }

    fn unique_temp_dir(name: &str) -> Utf8PathBuf {
        let nanos = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        Utf8PathBuf::from_path_buf(std::env::temp_dir().join(format!("savecore-{name}-{nanos}")))
            .unwrap()
    }
}
