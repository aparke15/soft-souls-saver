use camino::Utf8PathBuf;
use std::{collections::HashMap, fs};

#[derive(Debug, Clone)]
pub struct SteamInstall {
    pub libraries: Vec<Utf8PathBuf>,
    pub apps: HashMap<u32, SteamApp>,
}

#[derive(Debug, Clone)]
pub struct SteamApp {
    pub appid: u32,
    pub name: String,
    pub install_dir: Utf8PathBuf,
}

pub fn find() -> anyhow::Result<SteamInstall> {
    let candidates = [
        "~/.local/share/Steam",
        "~/.steam/steam",
        "~/.var/app/com.valvesoftware.Steam/.local/share/Steam",
    ];

    let mut roots = vec![];
    for c in candidates {
        let p = Utf8PathBuf::from(shellexpand::tilde(c).to_string());
        if p.exists() {
            roots.push(p);
        }
    }

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
                    install_dir: lib.join("common").join(installdir),
                },
            );
        }
    }

    Ok(SteamInstall { libraries, apps })
}

fn parse_libraryfolders_paths(txt: &str) -> Vec<String> {
    txt.lines()
        .filter_map(|line| parse_kv_line(line).filter(|(k, _)| k == "path"))
        .map(|(_, v)| v.replace("\\\\", "/"))
        .collect()
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
