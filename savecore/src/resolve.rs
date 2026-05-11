use crate::{GameManifest, types::ResolvedSaveCandidate};
use camino::Utf8PathBuf;
use glob::Pattern;
use std::{collections::HashMap, fs};
use walkdir::WalkDir;

const STEAM_ID64_TOKEN: &str = "{steamId64}";

pub fn expand(template: &str, vars: &HashMap<&str, String>) -> Utf8PathBuf {
    let mut s = template.to_string();

    for (k, v) in vars {
        s = s.replace(&format!("{{{}}}", k), v);
    }

    Utf8PathBuf::from(shellexpand::full(&s).unwrap().to_string())
}

pub fn resolve_save_candidates(
    manifest: &GameManifest,
    steam_id64: Option<&str>,
) -> anyhow::Result<Vec<ResolvedSaveCandidate>> {
    let templates = platform_save_locations(manifest);
    let mut candidates = Vec::new();

    for template in templates {
        let steam_ids = steam_ids_for_template(template, steam_id64)?;

        if steam_ids.is_empty() {
            candidates.push(candidate_from_template(manifest, template, None));
        } else {
            for steam_id in steam_ids {
                candidates.push(candidate_from_template(manifest, template, Some(&steam_id)));
            }
        }
    }

    candidates.sort_by(|left, right| left.path.cmp(&right.path));
    candidates.dedup_by(|left, right| left.path == right.path);

    Ok(candidates)
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

fn platform_save_locations(manifest: &GameManifest) -> &[String] {
    match current_platform() {
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

fn candidate_from_template(
    manifest: &GameManifest,
    template: &str,
    steam_id64: Option<&str>,
) -> ResolvedSaveCandidate {
    let vars = steam_id64
        .map(|id| HashMap::from([("steamId64", id.to_string())]))
        .unwrap_or_default();
    let path = expand(template, &vars);
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
