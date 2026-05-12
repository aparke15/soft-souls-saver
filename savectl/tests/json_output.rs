use serde_json::Value;
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
    time::{SystemTime, UNIX_EPOCH},
};

#[test]
fn detect_json_outputs_typed_result() {
    let fixture = Fixture::new("detect-json");

    let output = fixture.run(["detect", "--json"]);
    let json = assert_json_success(output);

    assert_eq!(json["games"][0]["slug"], "ds3");
    assert!(json["games"][0]["save_candidates"].is_array());
}

#[test]
fn backup_dry_run_json_outputs_typed_result() {
    let fixture = Fixture::new("backup-json");

    let output = fixture.run(["backup", "ds3", "--dry-run", "--json"]);
    let json = assert_json_success(output);

    assert_eq!(json["slug"], "ds3");
    assert_eq!(json["dry_run"], true);
    assert!(
        json["source_save_path"]
            .as_str()
            .unwrap()
            .ends_with(STEAM_ID)
    );
    assert!(
        json["output_zip_path"]
            .as_str()
            .unwrap()
            .contains("backups/ds3")
    );
}

#[test]
fn plan_restore_json_outputs_resolved_paths() {
    let fixture = Fixture::new("plan-json");
    let snapshot = fixture.create_snapshot();

    let output = fixture.run(["plan-restore", "ds3", snapshot.as_str(), "--json"]);
    let json = assert_json_success(output);

    assert_eq!(json["target_game_slug"], "ds3");
    assert!(
        json["snapshot_inventory"]["payload_zip_path"]
            .as_str()
            .unwrap()
            .contains("payload.zip")
    );
    assert!(
        json["restore_destination_path"]
            .as_str()
            .unwrap()
            .ends_with(STEAM_ID)
    );
}

#[test]
fn restore_dry_run_json_outputs_typed_result() {
    let fixture = Fixture::new("restore-json");
    let snapshot = fixture.create_snapshot();

    let output = fixture.run(["restore", "ds3", snapshot.as_str(), "--dry-run", "--json"]);
    let json = assert_json_success(output);

    assert_eq!(json["target_game_slug"], "ds3");
    assert_eq!(json["dry_run"], true);
    assert_eq!(json["restored_file_count"], 1);
    assert!(
        json["restore_destination_path"]
            .as_str()
            .unwrap()
            .ends_with(STEAM_ID)
    );
}

#[test]
fn list_snapshots_json_outputs_catalog() {
    let fixture = Fixture::new("list-json");
    fixture.create_snapshot();

    let output = fixture.run(["list-snapshots", "--json"]);
    let json = assert_json_success(output);

    assert_eq!(json["backup_root"], "backups");
    assert_eq!(json["snapshots"][0]["game_slug"], "ds3");
    assert!(
        json["snapshots"][0]["id"]
            .as_str()
            .unwrap()
            .starts_with("ds3/")
    );
}

#[test]
fn show_snapshot_json_outputs_snapshot_and_inventory() {
    let fixture = Fixture::new("show-json");
    let snapshot = fixture.create_snapshot();

    let output = fixture.run(["show-snapshot", snapshot.as_str(), "--json"]);
    let json = assert_json_success(output);

    assert_eq!(json["snapshot"]["game_slug"], "ds3");
    assert!(
        json["inventory"]
            .as_array()
            .unwrap()
            .iter()
            .any(|entry| { entry["path"].as_str() == Some("DS30000.sl2") })
    );
}

#[test]
fn show_config_json_outputs_effective_settings() {
    let fixture = Fixture::new("config-json");

    let output = fixture.run(["show-config", "--json"]);
    let json = assert_json_success(output);

    assert_eq!(json["config_loaded"], false);
    assert_eq!(json["backup_root"], "backups");
    assert!(
        json["config_path"]
            .as_str()
            .unwrap()
            .ends_with("soft-souls/config.toml")
    );
}

#[test]
fn check_running_json_outputs_activity_result() {
    let fixture = Fixture::new("running-json");

    let output = fixture.run(["check-running", "ds3", "--json"]);
    let json = assert_json_success(output);

    assert_eq!(json["games"][0]["game_slug"], "ds3");
    assert_eq!(json["games"][0]["process_hints"][0], "DarkSoulsIII.exe");
    assert!(json["games"][0]["running_processes"].is_array());
}

#[test]
fn json_errors_are_structured_on_stderr() {
    let fixture = Fixture::new("error-json");

    let output = fixture.run(["backup", "unknown", "--dry-run", "--json"]);

    assert!(!output.status.success());
    assert!(String::from_utf8(output.stdout).unwrap().is_empty());
    let json: Value = serde_json::from_slice(&output.stderr).unwrap();
    assert!(
        json["error"]["message"]
            .as_str()
            .unwrap()
            .contains("unknown game slug 'unknown'")
    );
}

const STEAM_ID: &str = "76561198000000000";

struct Fixture {
    work_dir: PathBuf,
    steam_root: PathBuf,
    config_home: PathBuf,
    home: PathBuf,
}

impl Fixture {
    fn new(name: &str) -> Self {
        let root = unique_temp_dir(name);
        let work_dir = root.join("work");
        let steam_root = root.join("steam");
        let config_home = root.join("xdg");
        let home = root.join("home");
        fs::create_dir_all(&work_dir).unwrap();
        fs::create_dir_all(&config_home).unwrap();
        fs::create_dir_all(&home).unwrap();
        write_fake_ds3_steam_root(&steam_root);

        Self {
            work_dir,
            steam_root,
            config_home,
            home,
        }
    }

    fn run<const N: usize>(&self, args: [&str; N]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_savectl"))
            .args(args)
            .current_dir(&self.work_dir)
            .env("soft_souls_steam_root", &self.steam_root)
            .env("XDG_CONFIG_HOME", &self.config_home)
            .env("HOME", &self.home)
            .output()
            .unwrap()
    }

    fn create_snapshot(&self) -> String {
        let output = self.run(["backup", "ds3", "--json"]);
        let json = assert_json_success(output);
        let metadata_path = PathBuf::from(json["metadata_path"].as_str().unwrap());
        metadata_path
            .parent()
            .unwrap()
            .to_string_lossy()
            .to_string()
    }
}

fn assert_json_success(output: Output) -> Value {
    assert!(
        output.status.success(),
        "command failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stderr).is_empty());
    serde_json::from_slice(&output.stdout).unwrap()
}

fn write_fake_ds3_steam_root(root: &Path) {
    let steamapps = root.join("steamapps");
    let save_dir = steamapps
        .join("compatdata/374320/pfx/drive_c/users/steamuser/AppData/Roaming/DarkSoulsIII")
        .join(STEAM_ID);
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
}

fn unique_temp_dir(name: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!("savectl-{name}-{nanos}"))
}
