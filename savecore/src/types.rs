use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DetectGame {
    pub name: String,
    pub slug: String,
    pub steam_appid: Option<u32>,
    pub installed_name: Option<String>,
    pub install_dir: Option<String>,
    pub save_candidates: Vec<ResolvedSaveCandidate>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DetectGamesResult {
    pub games: Vec<DetectGame>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupOptions {
    pub game_slug: String,
    pub output_path: Option<String>,
    pub steam_id64: Option<String>,
    pub dry_run: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupResult {
    pub game_name: String,
    pub slug: String,
    pub timestamp: String,
    pub source_save_path: String,
    pub resolved_candidates: Vec<ResolvedSaveCandidate>,
    pub selected_pattern_list: Vec<String>,
    pub output_zip_path: String,
    pub metadata_path: Option<String>,
    pub sha256: Option<String>,
    pub dry_run: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResolvedSaveCandidate {
    pub path: String,
    pub source_template: String,
    pub steam_id64: Option<String>,
    pub exists: bool,
    pub matched_patterns: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupPlan {
    pub source_save_path: String,
    pub output_zip_path: String,
    pub metadata_path: Option<String>,
    pub selected_pattern_list: Vec<String>,
}
