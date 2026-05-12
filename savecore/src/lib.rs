use camino::Utf8PathBuf;
use serde::{Deserialize, Serialize};

pub mod api;
pub mod backup;
pub mod config;
pub mod ops;
pub mod process;
pub mod resolve;
pub mod restore;
pub mod snapshot;
pub mod steam;
pub mod types;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameManifest {
    pub name: String,
    pub slug: String,
    pub ids: Option<GameIds>,
    pub save_locations: SaveLocations,
    pub patterns: Vec<String>,
    pub notes: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameIds {
    pub steam: Option<u32>,
    #[serde(default)]
    pub proc: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SaveLocations {
    pub windows: Vec<String>,
    pub linux: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct DetectedGame {
    pub manifest: GameManifest,
    pub steam_appid: Option<u32>,
    pub install_name: Option<String>,
    pub candidate_paths: Vec<Utf8PathBuf>,
}

pub fn load_manifests() -> anyhow::Result<Vec<GameManifest>> {
    let s = include_str!("../assets/games.json");
    Ok(serde_json::from_str(s)?)
}
