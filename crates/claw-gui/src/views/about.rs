//! About panel showing version info and update check controls.

use eframe::egui;

use crate::theme;
use crate::update_checker::{DownloadProgress, DownloadStatus};

/// Status of an update check operation.
#[derive(Debug, Clone, Default)]
pub enum UpdateCheckStatus {
    /// No check has been performed or initiated.
    #[default]
    Idle,
    /// A check is currently in progress.
    Checking,
    /// An update is available at the given version.
    Available(String),
    /// The current version is up to date.
    UpToDate,
    /// The check failed with the given error message.
    Error(String),
}

/// Persistent state for the About view.
#[derive(Debug, Clone, Default)]
pub struct AboutViewState {
    /// Whether the user has requested an update check this frame.
    pub check_requested: bool,
    /// Whether the user has requested a download this frame.
    pub download_requested: bool,
    /// Timestamp of the last completed update check.
    pub last_check: Option<String>,
    /// Current status of the update check.
    pub update_status: UpdateCheckStatus,
    /// Latest download progress snapshot (set by the main update loop).
    pub download_progress: Option<DownloadProgress>,
}

/// Render the About panel.
pub fn show(ui: &mut egui::Ui, state: &mut AboutViewState) {
    ui.heading("About");
    ui.add_space(16.0);

    // App identity card
    egui::Frame::default()
        .fill(theme::SURFACE0)
        .corner_radius(egui::CornerRadius::same(8))
        .inner_margin(egui::Margin::same(16))
        .show(ui, |ui| {
            ui.label(egui::RichText::new("Claw").size(28.0).color(theme::TEXT));
            ui.add_space(4.0);
            ui.label(
                egui::RichText::new("Multi-channel AI chatbot gateway")
                    .color(theme::SUBTEXT),
            );

            ui.add_space(12.0);
            ui.separator();
            ui.add_space(12.0);

            egui::Grid::new("about_info")
                .spacing([40.0, 8.0])
                .show(ui, |ui| {
                    ui.label(egui::RichText::new("Version").color(theme::SUBTEXT));
                    ui.label(
                        egui::RichText::new(format!("v{}", env!("CARGO_PKG_VERSION")))
                            .color(theme::TEXT),
                    );
                    ui.end_row();

                    ui.label(egui::RichText::new("Build date").color(theme::SUBTEXT));
                    ui.label(
                        egui::RichText::new(build_date()).color(theme::TEXT),
                    );
                    ui.end_row();

                    ui.label(egui::RichText::new("Repository").color(theme::SUBTEXT));
                    ui.hyperlink_to(
                        "github.com/Wolfschanze-Berlin/claw-rust",
                        "https://github.com/Wolfschanze-Berlin/claw-rust",
                    );
                    ui.end_row();
                });
        });

    ui.add_space(16.0);

    // Update check card
    egui::Frame::default()
        .fill(theme::SURFACE0)
        .corner_radius(egui::CornerRadius::same(8))
        .inner_margin(egui::Margin::same(16))
        .show(ui, |ui| {
            ui.label(
                egui::RichText::new("Updates")
                    .size(16.0)
                    .color(theme::TEXT),
            );
            ui.add_space(8.0);

            // Status display
            match &state.update_status {
                UpdateCheckStatus::Idle => {
                    ui.label(
                        egui::RichText::new("No update check performed yet.")
                            .color(theme::SUBTEXT),
                    );
                }
                UpdateCheckStatus::Checking => {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label(
                            egui::RichText::new("Checking for updates...")
                                .color(theme::YELLOW),
                        );
                    });
                }
                UpdateCheckStatus::Available(version) => {
                    ui.label(
                        egui::RichText::new(format!("Update available: v{version}"))
                            .color(theme::GREEN),
                    );
                }
                UpdateCheckStatus::UpToDate => {
                    ui.label(
                        egui::RichText::new("You are running the latest version.")
                            .color(theme::GREEN),
                    );
                }
                UpdateCheckStatus::Error(msg) => {
                    ui.label(
                        egui::RichText::new(format!("Check failed: {msg}"))
                            .color(theme::RED),
                    );
                }
            }

            ui.add_space(8.0);

            // Download progress display
            let is_downloading = show_download_progress(ui, &state.download_progress);

            ui.add_space(8.0);

            let is_checking = matches!(state.update_status, UpdateCheckStatus::Checking);
            let is_available = matches!(state.update_status, UpdateCheckStatus::Available(_));

            ui.horizontal(|ui| {
                ui.add_enabled_ui(!is_checking && !is_downloading, |ui| {
                    if ui.button("Check for Updates").clicked() {
                        state.check_requested = true;
                    }
                });

                // Show "Download & Install" when an update is available and not already downloading
                if is_available {
                    ui.add_enabled_ui(!is_downloading, |ui| {
                        if ui
                            .button(
                                egui::RichText::new("Download & Install")
                                    .color(theme::BASE)
                                    .strong(),
                            )
                            .clicked()
                        {
                            state.download_requested = true;
                        }
                    });
                }

                // Show retry button on download failure
                if let Some(DownloadProgress {
                    status: DownloadStatus::Failed(_),
                    ..
                }) = &state.download_progress
                {
                    if ui.button("Retry Download").clicked() {
                        state.download_progress = None;
                        state.download_requested = true;
                    }
                }
            });

            if let Some(ts) = &state.last_check {
                ui.add_space(4.0);
                ui.label(
                    egui::RichText::new(format!("Last checked: {ts}"))
                        .small()
                        .color(theme::OVERLAY),
                );
            }
        });
}

/// Render download progress UI.  Returns `true` if a download is actively in
/// progress (used to disable other buttons).
fn show_download_progress(
    ui: &mut egui::Ui,
    progress: &Option<DownloadProgress>,
) -> bool {
    let Some(p) = progress else {
        return false;
    };
    match &p.status {
        DownloadStatus::Downloading => {
            let fraction = match p.total_bytes {
                Some(total) if total > 0 => p.bytes_downloaded as f32 / total as f32,
                _ => 0.0,
            };
            let label = match p.total_bytes {
                Some(total) => format!(
                    "Downloading: {:.1} / {:.1} MB",
                    p.bytes_downloaded as f64 / 1_048_576.0,
                    total as f64 / 1_048_576.0,
                ),
                None => format!(
                    "Downloading: {:.1} MB",
                    p.bytes_downloaded as f64 / 1_048_576.0,
                ),
            };
            ui.add(
                egui::ProgressBar::new(fraction)
                    .text(label)
                    .animate(true),
            );
            true
        }
        DownloadStatus::Completed(path) => {
            ui.label(
                egui::RichText::new(format!(
                    "Download complete. Launching installer from {}",
                    path.display()
                ))
                .color(theme::GREEN),
            );
            false
        }
        DownloadStatus::Failed(msg) => {
            ui.label(
                egui::RichText::new(format!("Download failed: {msg}"))
                    .color(theme::RED),
            );
            false
        }
    }
}

/// Return a compile-time build date string.
///
/// Uses the `SOURCE_DATE` environment variable if set at build time (for
/// reproducible builds), otherwise falls back to a placeholder.
fn build_date() -> &'static str {
    option_env!("SOURCE_DATE").unwrap_or("dev build")
}
