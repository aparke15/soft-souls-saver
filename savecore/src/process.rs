use crate::{GameManifest, types::*};
use camino::Utf8PathBuf;
use std::{ffi::OsStr, fs};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessInfo {
    pub pid: u32,
    pub executable: String,
    pub command_line: Option<String>,
}

pub trait ProcessProvider {
    fn running_processes(&self) -> anyhow::Result<Vec<ProcessInfo>>;
}

#[derive(Debug, Clone, Default)]
pub struct PlatformProcessProvider;

impl ProcessProvider for PlatformProcessProvider {
    fn running_processes(&self) -> anyhow::Result<Vec<ProcessInfo>> {
        running_processes_for_platform()
    }
}

#[derive(Debug, Clone, Default)]
pub struct ProcfsProcessProvider;

impl ProcessProvider for ProcfsProcessProvider {
    fn running_processes(&self) -> anyhow::Result<Vec<ProcessInfo>> {
        running_processes_from_proc(&Utf8PathBuf::from("/proc"))
    }
}

pub fn check_game_activity(
    manifests: &[GameManifest],
    game_slug: Option<&str>,
) -> anyhow::Result<CheckGameActivityResult> {
    check_game_activity_with_provider(manifests, game_slug, &PlatformProcessProvider)
}

pub fn check_game_activity_with_provider(
    manifests: &[GameManifest],
    game_slug: Option<&str>,
    provider: &impl ProcessProvider,
) -> anyhow::Result<CheckGameActivityResult> {
    let running_processes = provider.running_processes()?;
    let games = manifests
        .iter()
        .filter(|manifest| game_slug.is_none_or(|slug| slug == manifest.slug))
        .map(|manifest| game_activity_from_processes(manifest, &running_processes))
        .collect();

    Ok(CheckGameActivityResult { games })
}

pub fn activity_warnings_for_manifest(
    manifest: &GameManifest,
) -> anyhow::Result<Vec<ActivityWarning>> {
    let result = check_game_activity_with_provider(
        std::slice::from_ref(manifest),
        None,
        &PlatformProcessProvider,
    )?;
    Ok(result
        .games
        .into_iter()
        .next()
        .map(|activity| activity.warnings)
        .unwrap_or_default())
}

#[cfg(target_os = "windows")]
fn running_processes_for_platform() -> anyhow::Result<Vec<ProcessInfo>> {
    running_processes_from_sysinfo()
}

#[cfg(not(target_os = "windows"))]
fn running_processes_for_platform() -> anyhow::Result<Vec<ProcessInfo>> {
    running_processes_from_proc(&Utf8PathBuf::from("/proc"))
}

#[cfg(target_os = "windows")]
fn running_processes_from_sysinfo() -> anyhow::Result<Vec<ProcessInfo>> {
    use sysinfo::System;

    let system = System::new_all();
    let processes = system
        .processes()
        .iter()
        .map(|(pid, process)| {
            let executable = process
                .exe()
                .and_then(|path| path.file_name())
                .map(|name| name.to_string_lossy().to_string())
                .filter(|name| !name.is_empty())
                .unwrap_or_else(|| process.name().to_string());
            let command_line = (!process.cmd().is_empty()).then(|| {
                process
                    .cmd()
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(" ")
            });

            ProcessInfo {
                pid: pid.as_u32(),
                executable,
                command_line,
            }
        })
        .collect();

    Ok(processes)
}

pub fn game_activity_from_processes(
    manifest: &GameManifest,
    processes: &[ProcessInfo],
) -> GameActivity {
    let hints = manifest
        .ids
        .as_ref()
        .map(|ids| ids.proc.clone())
        .unwrap_or_default();
    let normalized_hints = hints
        .iter()
        .map(|hint| normalize_process_name(hint))
        .collect::<Vec<_>>();
    let running_processes = processes
        .iter()
        .filter(|process| {
            let executable = normalize_process_name(&process.executable);
            normalized_hints.iter().any(|hint| hint == &executable)
        })
        .map(|process| RunningProcess {
            pid: process.pid,
            executable: process.executable.clone(),
        })
        .collect::<Vec<_>>();
    let warnings = if running_processes.is_empty() {
        Vec::new()
    } else {
        vec![ActivityWarning::GameAppearsRunning {
            processes: running_processes.clone(),
        }]
    };

    GameActivity {
        game_slug: manifest.slug.clone(),
        game_name: manifest.name.clone(),
        process_hints: hints,
        running_processes,
        warnings,
    }
}

fn running_processes_from_proc(proc_root: &Utf8PathBuf) -> anyhow::Result<Vec<ProcessInfo>> {
    let mut processes = Vec::new();

    if !proc_root.is_dir() {
        return Ok(processes);
    }

    for entry in fs::read_dir(proc_root)? {
        let Ok(entry) = entry else {
            continue;
        };
        let file_name = entry.file_name();
        let Some(pid) = file_name.to_string_lossy().parse::<u32>().ok() else {
            continue;
        };
        let path = entry.path();

        if let Some(process) = process_from_proc_dir(pid, &path) {
            processes.push(process);
        }
    }

    Ok(processes)
}

fn process_from_proc_dir(pid: u32, path: &std::path::Path) -> Option<ProcessInfo> {
    let executable = fs::read_link(path.join("exe"))
        .ok()
        .and_then(|path| {
            path.file_name()
                .map(OsStr::to_string_lossy)
                .map(|name| name.to_string())
        })
        .or_else(|| read_comm(path));
    let executable = executable?;
    let command_line = fs::read(path.join("cmdline")).ok().and_then(|bytes| {
        let joined = bytes
            .split(|byte| *byte == 0)
            .filter(|part| !part.is_empty())
            .map(|part| String::from_utf8_lossy(part).to_string())
            .collect::<Vec<_>>()
            .join(" ");
        (!joined.is_empty()).then_some(joined)
    });

    Some(ProcessInfo {
        pid,
        executable,
        command_line,
    })
}

fn read_comm(path: &std::path::Path) -> Option<String> {
    fs::read_to_string(path.join("comm"))
        .ok()
        .map(|name| name.trim().to_string())
        .filter(|name| !name.is_empty())
}

fn normalize_process_name(name: &str) -> String {
    name.rsplit(['/', '\\'])
        .next()
        .unwrap_or(name)
        .to_ascii_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Clone)]
    struct FakeProcessProvider {
        processes: Vec<ProcessInfo>,
    }

    impl ProcessProvider for FakeProcessProvider {
        fn running_processes(&self) -> anyhow::Result<Vec<ProcessInfo>> {
            Ok(self.processes.clone())
        }
    }

    #[test]
    fn game_running_returns_warning() {
        let manifest = manifest_with_hints(vec!["DarkSoulsIII.exe"]);
        let activity = game_activity_from_processes(
            &manifest,
            &[ProcessInfo {
                pid: 42,
                executable: "DarkSoulsIII.exe".to_string(),
                command_line: None,
            }],
        );

        assert_eq!(activity.running_processes.len(), 1);
        assert_eq!(activity.warnings.len(), 1);
    }

    #[test]
    fn game_not_running_returns_no_warning() {
        let manifest = manifest_with_hints(vec!["DarkSoulsIII.exe"]);
        let activity = game_activity_from_processes(
            &manifest,
            &[ProcessInfo {
                pid: 7,
                executable: "other.exe".to_string(),
                command_line: None,
            }],
        );

        assert!(activity.warnings.is_empty());
    }

    #[test]
    fn multiple_process_hints_are_checked() {
        let manifest = manifest_with_hints(vec!["first.exe", "second.exe"]);
        let activity = game_activity_from_processes(
            &manifest,
            &[ProcessInfo {
                pid: 9,
                executable: "second.exe".to_string(),
                command_line: None,
            }],
        );

        assert_eq!(activity.running_processes[0].pid, 9);
    }

    #[test]
    fn windows_style_process_paths_match_by_basename() {
        let manifest = manifest_with_hints(vec!["eldenring.exe"]);
        let activity = game_activity_from_processes(
            &manifest,
            &[ProcessInfo {
                pid: 11,
                executable: "C:\\Program Files (x86)\\Steam\\steamapps\\common\\ELDEN RING\\Game\\eldenring.exe".to_string(),
                command_line: None,
            }],
        );

        assert_eq!(activity.running_processes.len(), 1);
        assert_eq!(
            activity.running_processes[0].executable,
            "C:\\Program Files (x86)\\Steam\\steamapps\\common\\ELDEN RING\\Game\\eldenring.exe"
        );
    }

    #[test]
    fn provider_results_are_reusable_for_check_api() {
        let manifests = vec![
            manifest_with_hints(vec!["DarkSoulsIII.exe"]),
            manifest_with_slug("other"),
        ];
        let provider = FakeProcessProvider {
            processes: vec![ProcessInfo {
                pid: 1,
                executable: "DarkSoulsIII.exe".to_string(),
                command_line: None,
            }],
        };

        let result = check_game_activity_with_provider(&manifests, Some("ds3"), &provider).unwrap();

        assert_eq!(result.games.len(), 1);
        assert_eq!(result.games[0].game_slug, "ds3");
        assert_eq!(result.games[0].warnings.len(), 1);
    }

    #[test]
    fn unreadable_or_malformed_proc_entries_are_ignored() {
        let root = Utf8PathBuf::from_path_buf(std::env::temp_dir().join(format!(
                "savecore-proc-test-{}",
                std::time::SystemTime::now()
                    .duration_since(std::time::SystemTime::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            )))
        .unwrap();
        std::fs::create_dir_all(root.join("not-a-pid")).unwrap();
        std::fs::create_dir_all(root.join("123")).unwrap();

        let processes = running_processes_from_proc(&root).unwrap();

        assert!(processes.is_empty());
        std::fs::remove_dir_all(root).unwrap();
    }

    fn manifest_with_hints(hints: Vec<&str>) -> GameManifest {
        let mut manifest = manifest_with_slug("ds3");
        manifest.ids = Some(crate::GameIds {
            steam: Some(374320),
            proc: hints.into_iter().map(ToOwned::to_owned).collect(),
        });
        manifest
    }

    fn manifest_with_slug(slug: &str) -> GameManifest {
        GameManifest {
            name: slug.to_string(),
            slug: slug.to_string(),
            ids: Some(crate::GameIds {
                steam: None,
                proc: Vec::new(),
            }),
            save_locations: crate::SaveLocations {
                windows: Vec::new(),
                linux: Vec::new(),
            },
            patterns: Vec::new(),
            notes: None,
        }
    }
}
