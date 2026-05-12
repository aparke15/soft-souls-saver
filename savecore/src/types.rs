use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DetectGame {
    pub name: String,
    pub slug: String,
    pub steam_appid: Option<u32>,
    pub installed_name: Option<String>,
    pub install_dir: Option<String>,
    pub save_candidates: Vec<ResolvedSaveCandidate>,
    #[serde(default)]
    pub activity_warnings: Vec<ActivityWarning>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DetectGamesResult {
    pub games: Vec<DetectGame>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DetectOptions {
    pub config_path: Option<String>,
    pub steam_root_override: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupOptions {
    pub game_slug: String,
    pub output_path: Option<String>,
    pub config_path: Option<String>,
    pub backup_root: Option<String>,
    pub steam_id64: Option<String>,
    pub steam_root_override: Option<String>,
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
    #[serde(default)]
    pub activity_warnings: Vec<ActivityWarning>,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RestorePlanOptions {
    pub game_slug: String,
    pub snapshot_path: String,
    pub config_path: Option<String>,
    pub backup_root: Option<String>,
    pub steam_id64: Option<String>,
    pub steam_root_override: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RestorePlanResult {
    pub target_game_slug: String,
    pub selected_snapshot: BackupResult,
    pub snapshot_inventory: SnapshotInventory,
    pub restore_destination_path: Option<String>,
    pub files_to_restore: Vec<RestoreFile>,
    pub target_exists: bool,
    pub pre_restore_safety_backup: SafetyBackupPlan,
    pub resolved_candidates: Vec<ResolvedSaveCandidate>,
    pub warnings: Vec<RestorePlanWarning>,
    pub blocking_warnings: Vec<RestorePlanWarning>,
    #[serde(default)]
    pub activity_warnings: Vec<ActivityWarning>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotInventory {
    pub metadata_path: Option<String>,
    pub payload_zip_path: String,
    pub file_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RestoreFile {
    pub path: String,
    pub compressed_size: u64,
    pub uncompressed_size: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SafetyBackupPlan {
    pub recommended: bool,
    pub possible: bool,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RestorePlanWarning {
    GameNotCurrentlyDetected,
    NoRestoreTargetResolved,
    SaveDirMissing,
    SnapshotAppearsIncompatible,
    SnapshotIntegrityUnavailable,
    SnapshotIntegrityMismatch,
    MultipleCandidateTargets,
    AmbiguousSteamId,
}

impl RestorePlanWarning {
    pub fn blocks_restore_execution(&self) -> bool {
        match self {
            Self::GameNotCurrentlyDetected
            | Self::NoRestoreTargetResolved
            | Self::SnapshotAppearsIncompatible
            | Self::SnapshotIntegrityMismatch
            | Self::MultipleCandidateTargets
            | Self::AmbiguousSteamId => true,
            Self::SaveDirMissing | Self::SnapshotIntegrityUnavailable => false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RestoreOptions {
    pub game_slug: String,
    pub snapshot_path: String,
    pub config_path: Option<String>,
    pub backup_root: Option<String>,
    pub steam_id64: Option<String>,
    pub steam_root_override: Option<String>,
    pub dry_run: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RestoreResult {
    pub target_game_slug: String,
    pub selected_snapshot: BackupResult,
    pub restore_destination_path: String,
    pub dry_run: bool,
    pub safety_backup: Option<BackupResult>,
    pub restored_files: Vec<RestoreFile>,
    pub restored_file_count: usize,
    pub warnings: Vec<RestorePlanWarning>,
    pub blocking_warnings: Vec<RestorePlanWarning>,
    #[serde(default)]
    pub activity_warnings: Vec<ActivityWarning>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CheckGameActivityOptions {
    pub game_slug: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckGameActivityResult {
    pub games: Vec<GameActivity>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameActivity {
    pub game_slug: String,
    pub game_name: String,
    pub process_hints: Vec<String>,
    pub running_processes: Vec<RunningProcess>,
    pub warnings: Vec<ActivityWarning>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunningProcess {
    pub pid: u32,
    pub executable: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActivityWarning {
    GameAppearsRunning { processes: Vec<RunningProcess> },
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ListSnapshotsOptions {
    pub config_path: Option<String>,
    pub backup_root: Option<String>,
    pub game_slug: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListSnapshotsResult {
    pub backup_root: String,
    pub snapshots: Vec<SnapshotSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShowSnapshotOptions {
    pub config_path: Option<String>,
    pub backup_root: Option<String>,
    pub snapshot: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShowSnapshotResult {
    pub snapshot: SnapshotSummary,
    pub metadata: Option<BackupResult>,
    pub inventory: Vec<RestoreFile>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotSummary {
    pub id: String,
    pub game_slug: String,
    pub created_timestamp: String,
    pub archive_path: Option<String>,
    pub metadata_path: Option<String>,
    pub archive_size: Option<u64>,
    pub steam_id64: Option<String>,
    pub file_count: Option<usize>,
    pub integrity: SnapshotIntegrity,
    pub warnings: Vec<SnapshotWarning>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SnapshotIntegrity {
    Verified,
    Unavailable,
    Mismatch,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SnapshotWarning {
    MissingMetadata,
    MissingArchive,
    InvalidMetadata,
    UnreadableArchive,
    MetadataArchiveMismatch,
    LegacyIntegrityMissing,
    UnsafeArchiveEntry,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EffectiveSettingsOptions {
    pub config_path: Option<String>,
    pub backup_root: Option<String>,
    pub steam_root_override: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EffectiveSettingsResult {
    pub config_path: Option<String>,
    pub config_loaded: bool,
    pub backup_root: String,
    pub steam_root_override: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AppConfig {
    pub paths: Option<ConfigPaths>,
    pub backup: Option<ConfigBackup>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ConfigPaths {
    pub backup_root: Option<String>,
    pub steam_root_override: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ConfigBackup {
    pub compression: Option<String>,
}
