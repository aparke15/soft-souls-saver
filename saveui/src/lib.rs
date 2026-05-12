use eframe::egui;
use savecore::{
    api,
    types::{
        ActivityWarning, BackupOptions, BackupResult, DetectGame, DetectOptions,
        EffectiveSettingsOptions, EffectiveSettingsResult, ListSnapshotsOptions, RestoreOptions,
        RestorePlanOptions, RestorePlanResult, RestorePlanWarning, ShowSnapshotOptions,
        ShowSnapshotResult, SnapshotSummary,
    },
};
use std::sync::mpsc::{self, Receiver, Sender};

pub struct SaveUiApp {
    state: UiState,
    worker: Worker,
}

impl SaveUiApp {
    pub fn new(_creation_context: &eframe::CreationContext<'_>) -> Self {
        let mut app = Self {
            state: UiState::default(),
            worker: Worker::new(),
        };
        app.refresh_all();
        app
    }

    fn refresh_all(&mut self) {
        self.state.begin_operation("Refreshing games and snapshots");
        self.worker.spawn(UiRequest::Refresh);
    }

    fn create_backup(&mut self) {
        let Some(game_slug) = self.state.selected_game_slug.clone() else {
            self.state.set_error("Select a detected game first");
            return;
        };

        self.state.begin_operation("Creating backup");
        self.worker.spawn(UiRequest::Backup { game_slug });
    }

    fn plan_restore(&mut self) {
        let Some(game_slug) = self.state.selected_game_slug.clone() else {
            self.state.set_error("Select a detected game first");
            return;
        };
        let Some(snapshot_id) = self.state.selected_snapshot_id.clone() else {
            self.state.set_error("Select a snapshot first");
            return;
        };

        self.state.begin_operation("Previewing restore");
        self.worker.spawn(UiRequest::PlanRestore {
            game_slug,
            snapshot_id,
        });
    }

    fn restore_selected_snapshot(&mut self) {
        if !self.state.restore_confirmation_ready() {
            self.state
                .set_error("Preview restore and confirm before restoring");
            return;
        }
        let Some(game_slug) = self.state.selected_game_slug.clone() else {
            self.state.set_error("Select a detected game first");
            return;
        };
        let Some(snapshot_id) = self.state.confirm_restore_snapshot_id.clone() else {
            self.state.set_error("Select a snapshot first");
            return;
        };

        self.state.begin_operation("Restoring snapshot");
        self.worker.spawn(UiRequest::Restore {
            game_slug,
            snapshot_id,
        });
    }

    fn poll_worker(&mut self, context: &egui::Context) {
        while let Ok(message) = self.worker.receiver.try_recv() {
            self.state.pending = false;
            match message {
                UiResponse::Refresh(result) => match result {
                    Ok(result) => self.state.apply_refresh(result),
                    Err(error) => self.state.set_error(error),
                },
                UiResponse::Backup(result) => match result {
                    Ok(result) => {
                        let game_slug = result.slug.clone();
                        self.state.last_backup = Some(result);
                        self.state.status = "Backup created".to_string();
                        self.state.confirm_restore = false;
                        self.state.confirm_restore_snapshot_id = None;
                        self.state.begin_operation("Refreshing snapshots");
                        self.worker.spawn(UiRequest::RefreshSnapshots { game_slug });
                    }
                    Err(error) => self.state.set_error(error),
                },
                UiResponse::RefreshSnapshots(result) => match result {
                    Ok(snapshots) => {
                        self.state.snapshots = snapshots;
                        self.state.retain_snapshot_selection();
                        self.state.pending = false;
                    }
                    Err(error) => self.state.set_error(error),
                },
                UiResponse::PlanRestore(result) => match result {
                    Ok(plan) => self.state.apply_restore_plan(plan),
                    Err(error) => self.state.set_error(error),
                },
                UiResponse::Restore(result) => match result {
                    Ok(summary) => {
                        self.state.status = summary;
                        self.state.confirm_restore = false;
                        self.state.confirm_restore_snapshot_id = None;
                    }
                    Err(error) => self.state.set_error(error),
                },
                UiResponse::SnapshotDetails(result) => match result {
                    Ok(details) => {
                        self.state.snapshot_details = Some(details);
                        self.state.status = "Snapshot details loaded".to_string();
                    }
                    Err(error) => self.state.set_error(error),
                },
            }
            context.request_repaint();
        }
    }
}

impl eframe::App for SaveUiApp {
    fn update(&mut self, context: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll_worker(context);

        egui::TopBottomPanel::top("top_bar").show(context, |ui| {
            ui.horizontal(|ui| {
                ui.heading("Soft Souls Saver");
                ui.separator();
                if ui
                    .add_enabled(!self.state.pending, egui::Button::new("Refresh"))
                    .clicked()
                {
                    self.refresh_all();
                }
                if self.state.pending {
                    ui.label("Working...");
                }
            });
        });

        egui::SidePanel::left("games")
            .resizable(true)
            .default_width(260.0)
            .show(context, |ui| {
                ui.heading("Detected Games");
                ui.add_space(6.0);
                if self.state.detected_games.is_empty() {
                    ui.label("No supported Steam games detected.");
                }
                let games = self.state.detected_games.clone();
                for game in games {
                    let selected = self.state.selected_game_slug.as_deref() == Some(&game.slug);
                    if ui.selectable_label(selected, &game.name).clicked() {
                        self.state.select_game(game.slug.clone());
                        self.state.begin_operation("Loading snapshots");
                        self.worker.spawn(UiRequest::RefreshSnapshots {
                            game_slug: game.slug,
                        });
                    }
                    if !game.activity_warnings.is_empty() {
                        ui.small("Activity warning");
                    }
                }

                ui.separator();
                ui.heading("Config");
                if let Some(config) = &self.state.config {
                    ui.label(format!("Backup root: {}", config.backup_root));
                    ui.label(format!(
                        "Steam root: {}",
                        config.steam_root_override.as_deref().unwrap_or("auto")
                    ));
                } else {
                    ui.label("Config not loaded.");
                }
            });

        egui::CentralPanel::default().show(context, |ui| {
            ui.heading("Snapshots");
            ui.add_space(6.0);

            ui.horizontal(|ui| {
                if ui
                    .add_enabled(
                        !self.state.pending && self.state.selected_game_slug.is_some(),
                        egui::Button::new("Backup"),
                    )
                    .clicked()
                {
                    self.create_backup();
                }
                if ui
                    .add_enabled(
                        !self.state.pending && self.state.can_plan_restore(),
                        egui::Button::new("Plan restore"),
                    )
                    .clicked()
                {
                    self.plan_restore();
                }
                ui.checkbox(&mut self.state.confirm_restore, "Confirm restore");
                if ui
                    .add_enabled(
                        !self.state.pending && self.state.restore_confirmation_ready(),
                        egui::Button::new("Restore"),
                    )
                    .clicked()
                {
                    self.restore_selected_snapshot();
                }
            });

            ui.separator();
            ui.columns(2, |columns| {
                columns[0].heading("Snapshot List");
                egui::ScrollArea::vertical()
                    .id_salt("snapshot_list")
                    .show(&mut columns[0], |ui| {
                        if self.state.snapshots.is_empty() {
                            ui.label("No snapshots for the selected game.");
                        }
                        let snapshots = self.state.snapshots.clone();
                        for snapshot in snapshots {
                            let selected =
                                self.state.selected_snapshot_id.as_deref() == Some(&snapshot.id);
                            let label = format!(
                                "{}  files:{}  {:?}",
                                snapshot.created_timestamp,
                                snapshot
                                    .file_count
                                    .map(|count| count.to_string())
                                    .unwrap_or_else(|| "unknown".to_string()),
                                snapshot.integrity
                            );
                            if ui.selectable_label(selected, label).clicked() {
                                self.state.select_snapshot(snapshot.id.clone());
                                self.state.begin_operation("Loading snapshot details");
                                self.worker.spawn(UiRequest::SnapshotDetails {
                                    snapshot_id: snapshot.id,
                                });
                            }
                        }
                    });

                columns[1].heading("Details");
                egui::ScrollArea::vertical()
                    .id_salt("details")
                    .show(&mut columns[1], |ui| {
                        self.render_snapshot_details(ui);
                        ui.separator();
                        self.render_restore_plan(ui);
                    });
            });
        });

        egui::TopBottomPanel::bottom("status").show(context, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label(format!("Status: {}", self.state.status));
                if let Some(error) = &self.state.error {
                    ui.colored_label(
                        egui::Color32::from_rgb(180, 32, 32),
                        format!("Error: {error}"),
                    );
                }
            });
            self.render_warnings(ui);
        });
    }
}

impl SaveUiApp {
    fn render_snapshot_details(&self, ui: &mut egui::Ui) {
        let Some(details) = &self.state.snapshot_details else {
            ui.label("Select a snapshot to inspect it.");
            return;
        };

        ui.label(format!("Snapshot: {}", details.snapshot.id));
        ui.label(format!("Game: {}", details.snapshot.game_slug));
        if let Some(path) = &details.snapshot.archive_path {
            ui.label(format!("Archive: {path}"));
        }
        ui.label(format!("Files: {}", details.inventory.len()));
        if !details.snapshot.warnings.is_empty() {
            ui.colored_label(
                egui::Color32::from_rgb(180, 112, 24),
                format!("Snapshot warnings: {:?}", details.snapshot.warnings),
            );
        }
    }

    fn render_restore_plan(&self, ui: &mut egui::Ui) {
        let Some(plan) = &self.state.restore_plan else {
            ui.label("Plan a restore to preview the destination and safety checks.");
            return;
        };

        ui.heading("Restore Preview");
        ui.label(format!("Target: {}", plan.target_game_slug));
        ui.label(format!(
            "Destination: {}",
            plan.restore_destination_path
                .as_deref()
                .unwrap_or("unresolved")
        ));
        ui.label(format!("Files: {}", plan.files_to_restore.len()));
        ui.label(format!("Target exists: {}", plan.target_exists));
        ui.label(format!(
            "Safety backup: {}",
            if plan.pre_restore_safety_backup.recommended {
                "recommended"
            } else {
                "not needed"
            }
        ));

        if !plan.blocking_warnings.is_empty() {
            ui.colored_label(
                egui::Color32::from_rgb(180, 32, 32),
                format!("Restore blocked: {:?}", plan.blocking_warnings),
            );
        }
        if !plan.activity_warnings.is_empty() {
            ui.colored_label(
                egui::Color32::from_rgb(180, 112, 24),
                format!("Activity warnings: {:?}", plan.activity_warnings),
            );
        }
    }

    fn render_warnings(&self, ui: &mut egui::Ui) {
        if let Some(game) = self.state.selected_game() {
            for warning in &game.activity_warnings {
                ui.colored_label(
                    egui::Color32::from_rgb(180, 112, 24),
                    format!("Activity: {}", activity_warning_text(warning)),
                );
            }
        }
        if let Some(plan) = &self.state.restore_plan {
            for warning in &plan.warnings {
                ui.colored_label(
                    warning_color(warning),
                    format!("Restore warning: {warning:?}"),
                );
            }
        }
    }
}

#[derive(Debug, Default)]
pub struct UiState {
    detected_games: Vec<DetectGame>,
    selected_game_slug: Option<String>,
    snapshots: Vec<SnapshotSummary>,
    selected_snapshot_id: Option<String>,
    snapshot_details: Option<ShowSnapshotResult>,
    restore_plan: Option<RestorePlanResult>,
    last_backup: Option<BackupResult>,
    config: Option<EffectiveSettingsResult>,
    status: String,
    error: Option<String>,
    pending: bool,
    confirm_restore: bool,
    confirm_restore_snapshot_id: Option<String>,
}

impl UiState {
    pub fn new_for_tests() -> Self {
        Self::default()
    }

    fn begin_operation(&mut self, status: &str) {
        self.status = status.to_string();
        self.error = None;
        self.pending = true;
    }

    fn set_error(&mut self, error: impl Into<String>) {
        self.error = Some(error.into());
        self.pending = false;
    }

    fn apply_refresh(&mut self, result: RefreshResult) {
        self.detected_games = result.detected_games;
        self.config = Some(result.config);
        self.snapshots = result.snapshots;
        self.pending = false;
        self.status = "Ready".to_string();
        if self.selected_game_slug.is_none() {
            self.selected_game_slug = self.detected_games.first().map(|game| game.slug.clone());
        }
        self.retain_snapshot_selection();
    }

    fn select_game(&mut self, game_slug: String) {
        if self.selected_game_slug.as_deref() == Some(&game_slug) {
            return;
        }
        self.selected_game_slug = Some(game_slug);
        self.snapshots.clear();
        self.selected_snapshot_id = None;
        self.snapshot_details = None;
        self.restore_plan = None;
        self.confirm_restore = false;
        self.confirm_restore_snapshot_id = None;
    }

    fn select_snapshot(&mut self, snapshot_id: String) {
        if self.selected_snapshot_id.as_deref() == Some(&snapshot_id) {
            return;
        }
        self.selected_snapshot_id = Some(snapshot_id);
        self.restore_plan = None;
        self.confirm_restore = false;
        self.confirm_restore_snapshot_id = None;
    }

    fn apply_restore_plan(&mut self, plan: RestorePlanResult) {
        self.confirm_restore = false;
        self.confirm_restore_snapshot_id = Some(plan.selected_snapshot_id());
        self.status = if plan.blocking_warnings.is_empty() {
            "Restore preview ready".to_string()
        } else {
            "Restore preview has blocking warnings".to_string()
        };
        self.restore_plan = Some(plan);
    }

    fn retain_snapshot_selection(&mut self) {
        if self.selected_snapshot_id.as_ref().is_some_and(|selected| {
            self.snapshots
                .iter()
                .any(|snapshot| &snapshot.id == selected)
        }) {
            return;
        }
        self.selected_snapshot_id = self.snapshots.first().map(|snapshot| snapshot.id.clone());
    }

    fn selected_game(&self) -> Option<&DetectGame> {
        self.selected_game_slug
            .as_ref()
            .and_then(|slug| self.detected_games.iter().find(|game| &game.slug == slug))
    }

    fn can_plan_restore(&self) -> bool {
        self.selected_game_slug.is_some() && self.selected_snapshot_id.is_some()
    }

    pub fn restore_confirmation_ready(&self) -> bool {
        self.confirm_restore
            && self.restore_plan.as_ref().is_some_and(|plan| {
                plan.blocking_warnings.is_empty()
                    && Some(plan.selected_snapshot_id()) == self.confirm_restore_snapshot_id
                    && Some(plan.selected_snapshot_id()) == self.selected_snapshot_id
            })
    }
}

trait RestorePlanExt {
    fn selected_snapshot_id(&self) -> String;
}

impl RestorePlanExt for RestorePlanResult {
    fn selected_snapshot_id(&self) -> String {
        format!(
            "{}/{}",
            self.selected_snapshot.slug, self.selected_snapshot.timestamp
        )
    }
}

#[derive(Debug)]
enum UiRequest {
    Refresh,
    RefreshSnapshots {
        game_slug: String,
    },
    SnapshotDetails {
        snapshot_id: String,
    },
    Backup {
        game_slug: String,
    },
    PlanRestore {
        game_slug: String,
        snapshot_id: String,
    },
    Restore {
        game_slug: String,
        snapshot_id: String,
    },
}

#[derive(Debug)]
enum UiResponse {
    Refresh(Result<RefreshResult, String>),
    RefreshSnapshots(Result<Vec<SnapshotSummary>, String>),
    SnapshotDetails(Result<ShowSnapshotResult, String>),
    Backup(Result<BackupResult, String>),
    PlanRestore(Result<RestorePlanResult, String>),
    Restore(Result<String, String>),
}

#[derive(Debug)]
struct RefreshResult {
    detected_games: Vec<DetectGame>,
    snapshots: Vec<SnapshotSummary>,
    config: EffectiveSettingsResult,
}

struct Worker {
    sender: Sender<UiResponse>,
    receiver: Receiver<UiResponse>,
}

impl Worker {
    fn new() -> Self {
        let (sender, receiver) = mpsc::channel();
        Self { sender, receiver }
    }

    fn spawn(&self, request: UiRequest) {
        let sender = self.sender.clone();
        std::thread::spawn(move || {
            let response = handle_request(request);
            let _ = sender.send(response);
        });
    }
}

fn handle_request(request: UiRequest) -> UiResponse {
    match request {
        UiRequest::Refresh => UiResponse::Refresh(refresh_data().map_err(error_message)),
        UiRequest::RefreshSnapshots { game_slug } => {
            UiResponse::RefreshSnapshots(list_snapshots(Some(game_slug)).map_err(error_message))
        }
        UiRequest::SnapshotDetails { snapshot_id } => UiResponse::SnapshotDetails(
            api::show_snapshot(ShowSnapshotOptions {
                config_path: None,
                backup_root: None,
                snapshot: snapshot_id,
            })
            .map_err(error_message),
        ),
        UiRequest::Backup { game_slug } => UiResponse::Backup(
            api::backup_game(BackupOptions {
                game_slug,
                output_path: None,
                config_path: None,
                backup_root: None,
                steam_id64: None,
                steam_root_override: None,
                dry_run: false,
            })
            .map_err(error_message),
        ),
        UiRequest::PlanRestore {
            game_slug,
            snapshot_id,
        } => UiResponse::PlanRestore(
            api::plan_restore(RestorePlanOptions {
                game_slug,
                snapshot_path: snapshot_id,
                config_path: None,
                backup_root: None,
                steam_id64: None,
                steam_root_override: None,
            })
            .map_err(error_message),
        ),
        UiRequest::Restore {
            game_slug,
            snapshot_id,
        } => {
            let plan = api::plan_restore(RestorePlanOptions {
                game_slug: game_slug.clone(),
                snapshot_path: snapshot_id.clone(),
                config_path: None,
                backup_root: None,
                steam_id64: None,
                steam_root_override: None,
            });
            match plan {
                Ok(plan) if plan.blocking_warnings.is_empty() => UiResponse::Restore(
                    api::restore_game(RestoreOptions {
                        game_slug,
                        snapshot_path: snapshot_id,
                        config_path: None,
                        backup_root: None,
                        steam_id64: None,
                        steam_root_override: None,
                        dry_run: false,
                    })
                    .map(|result| format!("Restored {} files", result.restored_file_count))
                    .map_err(error_message),
                ),
                Ok(plan) => UiResponse::Restore(Err(format!(
                    "restore blocked by {:?}",
                    plan.blocking_warnings
                ))),
                Err(error) => UiResponse::Restore(Err(error_message(error))),
            }
        }
    }
}

fn refresh_data() -> anyhow::Result<RefreshResult> {
    let detected_games = api::detect_games(DetectOptions::default())?.games;
    let selected_slug = detected_games.first().map(|game| game.slug.clone());
    let snapshots = if selected_slug.is_some() {
        list_snapshots(selected_slug)?
    } else {
        Vec::new()
    };
    let config = api::show_effective_settings(EffectiveSettingsOptions::default())?;

    Ok(RefreshResult {
        detected_games,
        snapshots,
        config,
    })
}

fn list_snapshots(game_slug: Option<String>) -> anyhow::Result<Vec<SnapshotSummary>> {
    Ok(api::list_snapshots(ListSnapshotsOptions {
        config_path: None,
        backup_root: None,
        game_slug,
    })?
    .snapshots)
}

fn error_message(error: anyhow::Error) -> String {
    error.to_string()
}

fn warning_color(warning: &RestorePlanWarning) -> egui::Color32 {
    if warning.blocks_restore_execution() {
        egui::Color32::from_rgb(180, 32, 32)
    } else {
        egui::Color32::from_rgb(180, 112, 24)
    }
}

fn activity_warning_text(warning: &ActivityWarning) -> String {
    match warning {
        ActivityWarning::GameAppearsRunning { processes } => {
            let names = processes
                .iter()
                .map(|process| format!("{}({})", process.executable, process.pid))
                .collect::<Vec<_>>()
                .join(", ");
            format!("game appears running: {names}")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initial_state_is_not_restore_ready() {
        let state = UiState::new_for_tests();

        assert!(!state.restore_confirmation_ready());
    }

    #[test]
    fn selecting_game_clears_restore_state() {
        let mut state = UiState::new_for_tests();
        state.selected_game_slug = Some("ds3".to_string());
        state.selected_snapshot_id = Some("ds3/snapshot".to_string());
        state.confirm_restore = true;
        state.confirm_restore_snapshot_id = Some("ds3/snapshot".to_string());

        state.select_game("elden-ring".to_string());

        assert_eq!(state.selected_game_slug.as_deref(), Some("elden-ring"));
        assert!(state.selected_snapshot_id.is_none());
        assert!(!state.confirm_restore);
        assert!(state.confirm_restore_snapshot_id.is_none());
    }

    #[test]
    fn activity_warning_text_lists_processes() {
        let text = activity_warning_text(&ActivityWarning::GameAppearsRunning {
            processes: vec![savecore::types::RunningProcess {
                pid: 42,
                executable: "DarkSoulsIII.exe".to_string(),
            }],
        });

        assert!(text.contains("DarkSoulsIII.exe(42)"));
    }
}
