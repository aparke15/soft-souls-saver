use crate::{GameManifest, steam::SteamApp, types::ResolvedSaveCandidate};
use camino::Utf8PathBuf;
use glob::Pattern;
use std::{collections::HashMap, fs};
use walkdir::WalkDir;

const STEAM_ID64_TOKEN: &str = "{steamId64}";
const STEAM_LIBRARY_TOKEN: &str = "{steamLibrary}";

pub fn expand(template: &str, vars: &HashMap<&str, String>) -> Utf8PathBuf {
    expand_for_platform(template, vars, current_platform(), &platform_env_vars())
}

fn expand_for_platform(
    template: &str,
    vars: &HashMap<&str, String>,
    platform: &str,
    env_vars: &HashMap<String, String>,
) -> Utf8PathBuf {
    let mut s = template.to_string();

    for (k, v) in vars {
        s = s.replace(&format!("{{{}}}", k), v);
    }

    if platform == "windows" {
        s = expand_windows_env_vars(&s, env_vars);
    }

    Utf8PathBuf::from(shellexpand::full(&s).unwrap().to_string())
}

pub fn resolve_save_candidates(
    manifest: &GameManifest,
    steam_app: Option<&SteamApp>,
    steam_id64: Option<&str>,
) -> anyhow::Result<Vec<ResolvedSaveCandidate>> {
    resolve_save_candidates_for_platform(
        manifest,
        steam_app,
        steam_id64,
        current_platform(),
        &platform_env_vars(),
    )
}

fn resolve_save_candidates_for_platform(
    manifest: &GameManifest,
    steam_app: Option<&SteamApp>,
    steam_id64: Option<&str>,
    platform: &str,
    env_vars: &HashMap<String, String>,
) -> anyhow::Result<Vec<ResolvedSaveCandidate>> {
    let templates = platform_save_locations_for(manifest, platform);
    let mut candidates = Vec::new();

    for template in templates {
        let Some(template) = template_with_steam_library(template, steam_app) else {
            continue;
        };
        let expanded_template = expand_for_platform(&template, &HashMap::new(), platform, env_vars);
        let expanded_template = expanded_template.to_string();
        let steam_ids = steam_ids_for_template(&expanded_template, steam_id64)?;

        if steam_ids.is_empty() {
            candidates.push(candidate_from_template_for_platform(
                manifest, &template, None, platform, env_vars,
            ));
        } else {
            for steam_id in steam_ids {
                candidates.push(candidate_from_template_for_platform(
                    manifest,
                    &template,
                    Some(&steam_id),
                    platform,
                    env_vars,
                ));
            }
        }
    }

    candidates.sort_by(|left, right| left.path.cmp(&right.path));
    candidates.dedup_by(|left, right| left.path == right.path);

    Ok(candidates)
}

fn template_with_steam_library(template: &str, steam_app: Option<&SteamApp>) -> Option<String> {
    if !template.contains(STEAM_LIBRARY_TOKEN) {
        return Some(template.to_string());
    }

    let steam_app = steam_app?;
    Some(template.replace(STEAM_LIBRARY_TOKEN, steam_app.steamapps_dir.as_str()))
}

pub fn select_best_candidate(
    candidates: &[ResolvedSaveCandidate],
) -> Option<&ResolvedSaveCandidate> {
    candidates
        .iter()
        .filter(|candidate| candidate.exists)
        .max_by_key(|candidate| {
            (
                !candidate.matched_patterns.is_empty(),
                candidate.matched_patterns.len(),
            )
        })
}

fn platform_save_locations_for<'a>(manifest: &'a GameManifest, platform: &str) -> &'a [String] {
    match platform {
        "windows" => &manifest.save_locations.windows,
        _ => &manifest.save_locations.linux,
    }
}

fn steam_ids_for_template(template: &str, steam_id64: Option<&str>) -> anyhow::Result<Vec<String>> {
    if !template.contains(STEAM_ID64_TOKEN) {
        return Ok(Vec::new());
    }

    if let Some(steam_id64) = steam_id64 {
        return Ok(vec![steam_id64.to_string()]);
    }

    infer_steam_id64_dirs(template)
}

fn candidate_from_template_for_platform(
    manifest: &GameManifest,
    template: &str,
    steam_id64: Option<&str>,
    platform: &str,
    env_vars: &HashMap<String, String>,
) -> ResolvedSaveCandidate {
    let vars = steam_id64
        .map(|id| HashMap::from([("steamId64", id.to_string())]))
        .unwrap_or_default();
    let path = expand_for_platform(template, &vars, platform, env_vars);
    let exists = path.is_dir();
    let matched_patterns = if exists {
        matching_patterns(&path, &manifest.patterns)
    } else {
        Vec::new()
    };

    ResolvedSaveCandidate {
        path: path.to_string(),
        source_template: template.to_string(),
        steam_id64: steam_id64.map(ToOwned::to_owned),
        exists,
        matched_patterns,
    }
}

fn platform_env_vars() -> HashMap<String, String> {
    std::env::vars().collect()
}

fn expand_windows_env_vars(template: &str, env_vars: &HashMap<String, String>) -> String {
    let mut output = String::new();
    let mut remainder = template;

    while let Some(start) = remainder.find('%') {
        output.push_str(&remainder[..start]);
        let after_start = &remainder[start + 1..];
        let Some(end) = after_start.find('%') else {
            output.push_str(&remainder[start..]);
            return output;
        };

        let name = &after_start[..end];
        if let Some(value) = windows_env_value(name, env_vars) {
            output.push_str(&value);
        } else {
            output.push('%');
            output.push_str(name);
            output.push('%');
        }
        remainder = &after_start[end + 1..];
    }

    output.push_str(remainder);
    output
}

fn windows_env_value(name: &str, env_vars: &HashMap<String, String>) -> Option<String> {
    let value = env_vars
        .iter()
        .find_map(|(key, value)| key.eq_ignore_ascii_case(name).then(|| value.clone()));

    if value.is_some() {
        return value;
    }

    if name.eq_ignore_ascii_case("documents") {
        return env_vars
            .iter()
            .find_map(|(key, value)| {
                key.eq_ignore_ascii_case("USERPROFILE")
                    .then(|| value.clone())
            })
            .map(|profile| format!("{profile}/Documents"));
    }

    None
}

fn infer_steam_id64_dirs(template: &str) -> anyhow::Result<Vec<String>> {
    let (prefix, suffix) = template
        .split_once(STEAM_ID64_TOKEN)
        .ok_or_else(|| anyhow::anyhow!("template does not contain steam id token"))?;
    let parent = Utf8PathBuf::from(shellexpand::full(prefix.trim_end_matches('/'))?.to_string());

    if !parent.is_dir() {
        return Ok(Vec::new());
    }

    let mut ids = Vec::new();
    for entry in fs::read_dir(parent)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().to_string();
        if !is_steam_id64_dir_name(&name) {
            continue;
        }
        if suffix.is_empty() || entry.path().join(suffix.trim_start_matches('/')).exists() {
            ids.push(name);
        }
    }

    ids.sort();
    Ok(ids)
}

fn matching_patterns(path: &Utf8PathBuf, patterns: &[String]) -> Vec<String> {
    let compiled = patterns
        .iter()
        .filter_map(|pattern| Pattern::new(pattern).ok().map(|glob| (pattern, glob)))
        .collect::<Vec<_>>();
    let mut matches = Vec::new();

    for entry in WalkDir::new(path).into_iter().filter_map(Result::ok) {
        if !entry.file_type().is_file() {
            continue;
        }

        let file_name = entry.file_name().to_string_lossy();
        for (pattern, glob) in &compiled {
            let pattern = pattern.as_str();
            if glob.matches(&file_name) && !matches.iter().any(|matched| matched == pattern) {
                matches.push(pattern.to_string());
            }
        }
    }

    matches
}

pub fn is_steam_id64_dir_name(name: &str) -> bool {
    name.len() == 17 && name.chars().all(|c| c.is_ascii_digit())
}

pub fn current_platform() -> &'static str {
    if cfg!(target_os = "windows") {
        "windows"
    } else {
        "linux"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{fs, time::SystemTime};

    #[test]
    fn expands_tilde_and_template_vars() {
        let path = expand(
            "~/AppData/{steamId64}",
            &HashMap::from([("steamId64", "76561198000000000".to_string())]),
        );

        assert!(path.as_str().contains("76561198000000000"));
        assert!(!path.as_str().contains('{'));
    }

    #[test]
    fn expands_windows_known_env_vars_case_insensitively() {
        let env = HashMap::from([
            (
                "APPDATA".to_string(),
                "/home/test/AppData/Roaming".to_string(),
            ),
            (
                "LOCALAPPDATA".to_string(),
                "/home/test/AppData/Local".to_string(),
            ),
            ("USERPROFILE".to_string(), "/home/test".to_string()),
        ]);

        let path = expand_for_platform(
            "%AppData%/DarkSoulsIII/{steamId64}",
            &HashMap::from([("steamId64", "76561198000000000".to_string())]),
            "windows",
            &env,
        );

        assert_eq!(
            path.as_str(),
            "/home/test/AppData/Roaming/DarkSoulsIII/76561198000000000"
        );
        assert_eq!(
            expand_windows_env_vars("%Documents%/Game", &env),
            "/home/test/Documents/Game"
        );
    }

    #[test]
    fn recognizes_only_17_digit_steam_ids() {
        assert!(is_steam_id64_dir_name("76561198000000000"));
        assert!(!is_steam_id64_dir_name("7656119800000000"));
        assert!(!is_steam_id64_dir_name("7656119800000000a"));
    }

    #[test]
    fn infers_steam_id_dirs_from_template_parent() {
        let root = unique_temp_dir("steam-id-infer");
        fs::create_dir_all(root.join("76561198000000000")).unwrap();
        fs::create_dir_all(root.join("not-a-steam-id")).unwrap();

        let template = format!("{}/{STEAM_ID64_TOKEN}", root);
        let ids = infer_steam_id64_dirs(&template).unwrap();

        assert_eq!(ids, vec!["76561198000000000"]);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn resolves_proton_save_from_detected_steam_library() {
        let steamapps = unique_temp_dir("steam-library-token").join("steamapps");
        let save_dir = steamapps.join("compatdata/374320/pfx/drive_c/users/steamuser/AppData/Roaming/DarkSoulsIII/76561198000000000");
        fs::create_dir_all(&save_dir).unwrap();
        fs::write(save_dir.join("DS30000.sl2"), b"fake save").unwrap();
        let manifest = GameManifest {
            name: "dark souls iii".to_string(),
            slug: "ds3".to_string(),
            ids: None,
            save_locations: crate::SaveLocations {
                windows: Vec::new(),
                linux: vec!["{steamLibrary}/compatdata/374320/pfx/drive_c/users/steamuser/AppData/Roaming/DarkSoulsIII/{steamId64}".to_string()],
            },
            patterns: vec!["DS*.sl2".to_string()],
            notes: None,
        };
        let app = SteamApp {
            appid: 374320,
            name: "DARK SOULS III".to_string(),
            steamapps_dir: steamapps.clone(),
            install_dir: steamapps.join("common/DARK SOULS III"),
        };

        let candidates = resolve_save_candidates(&manifest, Some(&app), None).unwrap();

        assert_eq!(candidates.len(), 1);
        assert_eq!(
            candidates[0].steam_id64.as_deref(),
            Some("76561198000000000")
        );
        assert_eq!(candidates[0].matched_patterns, vec!["DS*.sl2"]);

        fs::remove_dir_all(steamapps.parent().unwrap()).unwrap();
    }

    #[test]
    fn resolves_supported_windows_manifest_paths() {
        let root = unique_temp_dir("windows-saves");
        let appdata = root.join("AppData/Roaming");
        let local_appdata = root.join("AppData/Local");
        let steam_id = "76561198000000000";
        fs::create_dir_all(appdata.join(format!("EldenRing/{steam_id}"))).unwrap();
        fs::write(
            appdata.join(format!("EldenRing/{steam_id}/ER0000.sl2")),
            b"elden ring",
        )
        .unwrap();
        fs::create_dir_all(appdata.join(format!("DarkSoulsIII/{steam_id}"))).unwrap();
        fs::write(
            appdata.join(format!("DarkSoulsIII/{steam_id}/DS30000.sl2")),
            b"ds3",
        )
        .unwrap();
        fs::create_dir_all(local_appdata.join("LiesofP/Saved/SaveGames")).unwrap();
        fs::write(
            local_appdata.join("LiesofP/Saved/SaveGames/SaveData.sav"),
            b"lies of p",
        )
        .unwrap();
        let env = HashMap::from([
            ("APPDATA".to_string(), appdata.to_string()),
            ("LOCALAPPDATA".to_string(), local_appdata.to_string()),
            ("USERPROFILE".to_string(), root.to_string()),
        ]);

        let manifests = crate::load_manifests().unwrap();
        let elden_ring = manifests
            .iter()
            .find(|manifest| manifest.slug == "elden-ring")
            .unwrap();
        let ds3 = manifests
            .iter()
            .find(|manifest| manifest.slug == "ds3")
            .unwrap();
        let lies_of_p = manifests
            .iter()
            .find(|manifest| manifest.slug == "lies-of-p")
            .unwrap();

        let elden_candidates =
            resolve_save_candidates_for_platform(elden_ring, None, None, "windows", &env).unwrap();
        let ds3_candidates =
            resolve_save_candidates_for_platform(ds3, None, None, "windows", &env).unwrap();
        let lies_candidates =
            resolve_save_candidates_for_platform(lies_of_p, None, None, "windows", &env).unwrap();

        assert_eq!(elden_candidates[0].steam_id64.as_deref(), Some(steam_id));
        assert_eq!(elden_candidates[0].matched_patterns, vec!["ER*.sl2"]);
        assert_eq!(ds3_candidates[0].matched_patterns, vec!["DS*.sl2"]);
        assert_eq!(lies_candidates[0].matched_patterns, vec!["*.sav"]);

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn selects_existing_candidate_with_pattern_match() {
        let candidates = vec![
            ResolvedSaveCandidate {
                path: "/tmp/nomatch".to_string(),
                source_template: "a".to_string(),
                steam_id64: None,
                exists: true,
                matched_patterns: Vec::new(),
            },
            ResolvedSaveCandidate {
                path: "/tmp/match".to_string(),
                source_template: "b".to_string(),
                steam_id64: None,
                exists: true,
                matched_patterns: vec!["*.sl2".to_string()],
            },
        ];

        assert_eq!(
            select_best_candidate(&candidates).unwrap().path,
            "/tmp/match"
        );
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
