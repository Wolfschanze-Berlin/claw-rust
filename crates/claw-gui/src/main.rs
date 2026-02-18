//! Claw Control Panel — egui native desktop application.
//!
//! Independent GUI crate for monitoring and controlling the claw-rust gateway.

mod commands;
mod config_manager;
mod logging;
mod state;
mod theme;
mod update_checker;
mod views;
mod widgets;

use std::path::PathBuf;
use std::sync::{Arc, RwLock};

use eframe::egui;
use tokio::sync::mpsc;
use tracing::{info, warn};

use commands::CommandSender;
use config_manager::ConfigManager;
use logging::{GuiLogLayer, LogBuffer};
use state::AppState;

/// Available views in the control panel.
#[derive(Debug, Clone, Copy, PartialEq)]
enum View {
    // -- Monitor section --
    Dashboard,
    Logs,
    // -- Config section --
    Config,
    ConfigChannels,
    ConfigAgents,
    ConfigSkills,
    Bindings,
    // -- About section --
    About,
}

/// Main application struct implementing eframe::App.
struct ClawApp {
    current_view: View,
    state: Arc<RwLock<AppState>>,
    command_sender: CommandSender,
    log_buffer: LogBuffer,
    log_receiver: mpsc::UnboundedReceiver<state::LogEntry>,
    // Config editing state
    config_manager: Option<ConfigManager>,
    config_load_error: Option<String>,
    config_editor_state: views::config_editor::ConfigEditorState,
    channels_view_state: views::channels::ChannelsViewState,
    agents_view_state: views::agents::AgentsViewState,
    skills_view_state: views::skills::SkillsViewState,
    bindings_view_state: views::bindings::BindingsViewState,
    commit_state: views::commit_workflow::CommitWorkflowState,
    about_view_state: views::about::AboutViewState,
    /// Background update checker for GitHub releases.
    update_checker: update_checker::UpdateChecker,
    /// Background update downloader (created on first download request).
    update_downloader: Option<update_checker::UpdateDownloader>,
    /// Tokio runtime handle for spawning async tasks from the sync UI thread.
    tokio_handle: tokio::runtime::Handle,
    /// Whether the user dismissed the update banner for the current version.
    banner_dismissed: bool,
    /// Which version was dismissed, so the banner reappears for newer versions.
    dismissed_version: Option<String>,
    /// Optional channel name filter for the Logs view.
    log_channel_filter: Option<String>,
}

impl ClawApp {
    fn new(
        cc: &eframe::CreationContext<'_>,
        state: Arc<RwLock<AppState>>,
        command_sender: CommandSender,
        log_receiver: mpsc::UnboundedReceiver<state::LogEntry>,
        tokio_handle: tokio::runtime::Handle,
    ) -> Self {
        theme::apply(&cc.egui_ctx);

        // Try to load config from the standard path
        let config_path = PathBuf::from("config/config.json");
        let (config_manager, config_load_error) = match ConfigManager::load(&config_path) {
            Ok(mgr) => {
                info!("config loaded from {}", config_path.display());
                (Some(mgr), None)
            }
            Err(e) => {
                warn!("failed to load config: {e}");
                (None, Some(e.to_string()))
            }
        };

        let mut update_checker = update_checker::UpdateChecker::new(cc.egui_ctx.clone(), tokio_handle.clone());

        // Trigger an automatic update check on launch if the cache has expired
        if update_checker.should_check() {
            update_checker.trigger_check();
        }

        Self {
            current_view: View::Dashboard,
            state,
            command_sender,
            log_buffer: LogBuffer::new(1000),
            log_receiver,
            config_manager,
            config_load_error,
            config_editor_state: Default::default(),
            channels_view_state: Default::default(),
            agents_view_state: Default::default(),
            skills_view_state: Default::default(),
            bindings_view_state: Default::default(),
            commit_state: Default::default(),
            about_view_state: Default::default(),
            update_checker,
            update_downloader: None,
            tokio_handle,
            banner_dismissed: false,
            dismissed_version: None,
            log_channel_filter: None,
        }
    }
}

impl eframe::App for ClawApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Drain log events each frame
        self.log_buffer.drain_receiver(&mut self.log_receiver);

        // Handle manual update check requests from the About view
        if self.about_view_state.check_requested {
            self.about_view_state.check_requested = false;
            self.about_view_state.update_status =
                views::about::UpdateCheckStatus::Checking;
            self.update_checker.trigger_check();
        }

        // Drain update check results
        if let Some(result) = self.update_checker.drain_result() {
            let now = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
            self.about_view_state.last_check = Some(now);
            match result {
                Ok(r) if r.update_available => {
                    let version = r.latest.to_string();
                    // Reset banner dismissal if this is a different version
                    if self.dismissed_version.as_deref() != Some(&version) {
                        self.banner_dismissed = false;
                        self.dismissed_version = None;
                    }
                    self.about_view_state.update_status =
                        views::about::UpdateCheckStatus::Available(version);
                }
                Ok(_) => {
                    self.about_view_state.update_status =
                        views::about::UpdateCheckStatus::UpToDate;
                }
                Err(msg) => {
                    self.about_view_state.update_status =
                        views::about::UpdateCheckStatus::Error(msg);
                }
            }
        }

        // Handle download requests from the About view
        if self.about_view_state.download_requested {
            self.about_view_state.download_requested = false;

            if let views::about::UpdateCheckStatus::Available(ref version) =
                self.about_view_state.update_status
            {
                let handle = self.tokio_handle.clone();
                let downloader = self
                    .update_downloader
                    .get_or_insert_with(|| update_checker::UpdateDownloader::new(ctx.clone(), handle));
                downloader.start_download(version.clone());
                self.about_view_state.download_progress =
                    Some(update_checker::DownloadProgress {
                        bytes_downloaded: 0,
                        total_bytes: None,
                        status: update_checker::DownloadStatus::Downloading,
                    });
            }
        }

        // Drain download progress updates
        if let Some(ref mut downloader) = self.update_downloader {
            if let Some(progress) = downloader.drain_progress() {
                self.about_view_state.download_progress = Some(progress);
            }
        }

        // ── Menu bar ──────────────────────────────────────────────────
        egui::TopBottomPanel::top("menu_bar").show(ctx, |ui| {
            egui::MenuBar::new().ui(ui, |ui| {
                let has_config = self.config_manager.is_some();
                let is_dirty = self.config_manager.as_ref().map_or(false, |m| m.is_dirty());
                let can_undo = self.config_manager.as_ref().map_or(false, |m| m.can_undo());
                let can_redo = self.config_manager.as_ref().map_or(false, |m| m.can_redo());

                // Keyboard shortcuts (consumed once per frame)
                let save_shortcut = egui::KeyboardShortcut::new(egui::Modifiers::CTRL, egui::Key::S);
                let undo_shortcut = egui::KeyboardShortcut::new(egui::Modifiers::CTRL, egui::Key::Z);
                let redo_shortcut = egui::KeyboardShortcut::new(
                    egui::Modifiers::CTRL | egui::Modifiers::SHIFT,
                    egui::Key::Z,
                );

                let save_pressed = ctx.input_mut(|i| i.consume_shortcut(&save_shortcut));
                let undo_pressed = ctx.input_mut(|i| i.consume_shortcut(&undo_shortcut));
                let redo_pressed = ctx.input_mut(|i| i.consume_shortcut(&redo_shortcut));

                // Apply shortcuts
                if save_pressed && has_config && is_dirty {
                    self.commit_state.phase =
                        views::commit_workflow::CommitPhase::Confirming;
                }
                if undo_pressed {
                    if let Some(mgr) = &mut self.config_manager {
                        mgr.undo();
                    }
                }
                if redo_pressed {
                    if let Some(mgr) = &mut self.config_manager {
                        mgr.redo();
                    }
                }

                // ── File ──
                ui.menu_button("File", |ui| {
                    if ui
                        .add_enabled(has_config && is_dirty, egui::Button::new("Save Config\tCtrl+S"))
                        .clicked()
                    {
                        self.commit_state.phase =
                            views::commit_workflow::CommitPhase::Confirming;
                        ui.close();
                    }
                    ui.separator();
                    if ui.button("Exit").clicked() {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                });

                // ── Edit ──
                ui.menu_button("Edit", |ui| {
                    if ui
                        .add_enabled(can_undo, egui::Button::new("Undo\tCtrl+Z"))
                        .clicked()
                    {
                        if let Some(mgr) = &mut self.config_manager {
                            mgr.undo();
                        }
                        ui.close();
                    }
                    if ui
                        .add_enabled(can_redo, egui::Button::new("Redo\tCtrl+Shift+Z"))
                        .clicked()
                    {
                        if let Some(mgr) = &mut self.config_manager {
                            mgr.redo();
                        }
                        ui.close();
                    }
                });

                // ── View ──
                ui.menu_button("View", |ui| {
                    if ui.selectable_label(self.current_view == View::Dashboard, "Dashboard").clicked() {
                        self.current_view = View::Dashboard;
                        ui.close();
                    }
                    if ui.selectable_label(self.current_view == View::Logs, "Logs").clicked() {
                        self.current_view = View::Logs;
                        ui.close();
                    }
                    ui.separator();
                    if ui
                        .add_enabled(
                            has_config,
                            egui::Button::new("Settings").selected(self.current_view == View::Config),
                        )
                        .clicked()
                    {
                        self.current_view = View::Config;
                        ui.close();
                    }
                    if ui
                        .add_enabled(
                            has_config,
                            egui::Button::new("Channels").selected(self.current_view == View::ConfigChannels),
                        )
                        .clicked()
                    {
                        self.current_view = View::ConfigChannels;
                        ui.close();
                    }
                    if ui
                        .add_enabled(
                            has_config,
                            egui::Button::new("Agents").selected(self.current_view == View::ConfigAgents),
                        )
                        .clicked()
                    {
                        self.current_view = View::ConfigAgents;
                        ui.close();
                    }
                    if ui
                        .add_enabled(
                            has_config,
                            egui::Button::new("Skills").selected(self.current_view == View::ConfigSkills),
                        )
                        .clicked()
                    {
                        self.current_view = View::ConfigSkills;
                        ui.close();
                    }
                    if ui
                        .add_enabled(
                            has_config,
                            egui::Button::new("Bindings").selected(self.current_view == View::Bindings),
                        )
                        .clicked()
                    {
                        self.current_view = View::Bindings;
                        ui.close();
                    }
                });

                // ── Help ──
                ui.menu_button("Help", |ui| {
                    if ui.button("Check for Updates").clicked() {
                        self.update_checker.trigger_check();
                        self.about_view_state.update_status =
                            views::about::UpdateCheckStatus::Checking;
                        ui.close();
                    }
                    ui.separator();
                    if ui.selectable_label(self.current_view == View::About, "About").clicked() {
                        self.current_view = View::About;
                        ui.close();
                    }
                });
            });
        });

        // Sidebar navigation
        egui::SidePanel::left("nav_panel")
            .resizable(false)
            .default_width(200.0)
            .show(ctx, |ui| {
                ui.add_space(8.0);
                ui.heading("Claw");
                ui.add_space(16.0);
                ui.separator();
                ui.add_space(8.0);

                // -- Monitor section --
                ui.label(egui::RichText::new("MONITOR").color(theme::OVERLAY).small());
                ui.add_space(4.0);

                if ui
                    .selectable_label(self.current_view == View::Dashboard, "  Dashboard")
                    .clicked()
                {
                    self.current_view = View::Dashboard;
                }
                ui.add_space(2.0);
                if ui
                    .selectable_label(self.current_view == View::Logs, "  Logs")
                    .clicked()
                {
                    self.current_view = View::Logs;
                }

                ui.add_space(12.0);
                ui.separator();
                ui.add_space(8.0);

                // -- Config section --
                ui.label(egui::RichText::new("CONFIGURE").color(theme::OVERLAY).small());
                ui.add_space(4.0);

                let has_config = self.config_manager.is_some();

                let config_nav = |ui: &mut egui::Ui, selected: bool, label: &str| -> bool {
                    ui.add_enabled(
                        has_config,
                        egui::Button::new(label).selected(selected),
                    )
                    .clicked()
                };

                if config_nav(ui, self.current_view == View::Config, "  Settings") {
                    self.current_view = View::Config;
                }
                ui.add_space(2.0);
                if config_nav(ui, self.current_view == View::ConfigChannels, "  Channels") {
                    self.current_view = View::ConfigChannels;
                }
                ui.add_space(2.0);
                if config_nav(ui, self.current_view == View::ConfigAgents, "  Agents") {
                    self.current_view = View::ConfigAgents;
                }
                ui.add_space(2.0);
                if config_nav(ui, self.current_view == View::ConfigSkills, "  Skills") {
                    self.current_view = View::ConfigSkills;
                }
                ui.add_space(2.0);
                if config_nav(ui, self.current_view == View::Bindings, "  Bindings") {
                    self.current_view = View::Bindings;
                }

                // Dirty indicator in sidebar
                if let Some(mgr) = &self.config_manager {
                    if mgr.is_dirty() {
                        ui.add_space(8.0);
                        ui.horizontal(|ui| {
                            let (rect, _) = ui.allocate_exact_size(
                                egui::vec2(8.0, 8.0),
                                egui::Sense::hover(),
                            );
                            ui.painter()
                                .circle_filled(rect.center(), 4.0, theme::YELLOW);
                            ui.label(
                                egui::RichText::new("Unsaved")
                                    .color(theme::YELLOW)
                                    .small(),
                            );
                        });
                    }
                }

                ui.with_layout(egui::Layout::bottom_up(egui::Align::LEFT), |ui| {
                    ui.add_space(8.0);
                    ui.label(egui::RichText::new(
                        format!("v{}", env!("CARGO_PKG_VERSION"))
                    ).small().weak());
                    ui.add_space(4.0);

                    // -- About section --
                    if ui
                        .selectable_label(self.current_view == View::About, "  About")
                        .clicked()
                    {
                        self.current_view = View::About;
                    }
                    ui.add_space(4.0);
                    ui.label(egui::RichText::new("ABOUT").color(theme::OVERLAY).small());
                    ui.add_space(8.0);
                    ui.separator();
                });
            });

        // Update notification banner
        if let views::about::UpdateCheckStatus::Available(ref version) =
            self.about_view_state.update_status
        {
            if !self.banner_dismissed {
                let version = version.clone();
                egui::TopBottomPanel::top("update_banner")
                    .frame(egui::Frame::NONE.fill(theme::BLUE).inner_margin(egui::Margin::symmetric(12, 6)))
                    .show(ctx, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(
                                egui::RichText::new(format!(
                                    "A new version (v{version}) is available!"
                                ))
                                .color(theme::BASE)
                                .strong(),
                            );
                            ui.add_space(8.0);
                            if ui
                                .button(egui::RichText::new("Update Now").color(theme::BASE).strong())
                                .clicked()
                            {
                                self.current_view = View::About;
                            }
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    if ui.button(egui::RichText::new("\u{2715}").color(theme::BASE)).clicked() {
                                        self.banner_dismissed = true;
                                        self.dismissed_version = Some(version.clone());
                                    }
                                },
                            );
                        });
                    });
            }
        }

        // Main content area
        egui::CentralPanel::default().show(ctx, |ui| {
            match self.current_view {
                // -- Monitor views (use runtime AppState) --
                View::Dashboard => {
                    let state = self.state.read().unwrap().clone();
                    views::dashboard::show(ui, &state.gateway);
                }
                View::Logs => {
                    views::logs::show(
                        ui,
                        &self.log_buffer,
                        &mut self.log_channel_filter,
                    );
                }

                // -- About view (standalone, no config dependency) --
                View::About => {
                    views::about::show(ui, &mut self.about_view_state);
                }

                // -- Config views (use ConfigManager) --
                View::Config
                | View::ConfigChannels
                | View::ConfigAgents
                | View::ConfigSkills
                | View::Bindings => {
                    self.show_config_view(ui);
                }
            }
        });
    }
}

impl ClawApp {
    /// Render config editing views with the commit workflow toolbar.
    fn show_config_view(&mut self, ui: &mut egui::Ui) {
        let Some(mgr) = &mut self.config_manager else {
            // Config failed to load — show error
            ui.vertical_centered(|ui| {
                ui.add_space(40.0);
                ui.label(
                    egui::RichText::new("Config not loaded")
                        .color(theme::RED)
                        .size(18.0),
                );
                if let Some(err) = &self.config_load_error {
                    ui.add_space(8.0);
                    ui.label(egui::RichText::new(err).color(theme::SUBTEXT));
                }
            });
            return;
        };

        // Run validation for the current draft
        mgr.validate();
        let validation = claw_config::validate_config(mgr.draft());

        // Commit workflow toolbar (save/discard/undo/redo)
        views::commit_workflow::show(ui, mgr, &mut self.commit_state, &validation);
        ui.add_space(8.0);

        // View-specific content
        match self.current_view {
            View::Config => {
                views::config_editor::show(
                    ui,
                    mgr.draft_mut(),
                    &mut self.config_editor_state,
                    &validation,
                );
            }
            View::ConfigChannels => {
                let action =
                    views::channels::show(ui, mgr, &mut self.channels_view_state);
                if let views::channels::ChannelAction::ViewLogs(channel) = action {
                    self.log_channel_filter = Some(channel);
                    self.current_view = View::Logs;
                }
            }
            View::ConfigAgents => {
                views::agents::show(ui, mgr, &mut self.agents_view_state);
            }
            View::ConfigSkills => {
                views::skills::show(ui, mgr, &mut self.skills_view_state);
            }
            View::Bindings => {
                views::bindings::show(ui, mgr, &mut self.bindings_view_state);
            }
            _ => {}
        }
    }
}

fn main() -> eframe::Result<()> {
    // Load .env before anything else so ${ENV_VAR} substitution in
    // config files can resolve variables defined there.
    // Missing .env is fine — env vars may come from the OS instead.
    let _ = dotenvy::dotenv();

    // Set up log capture for GUI
    let (log_tx, log_rx) = mpsc::unbounded_channel();
    let gui_layer = GuiLogLayer::new(log_tx);

    // Initialize tracing with filtered console + GUI layers.
    //
    // Console: WARN by default (quiet) — override with RUST_LOG env var.
    // GUI panel: INFO for claw_* crates, WARN for noisy dependencies.
    use tracing_subscriber::layer::SubscriberExt;
    use tracing_subscriber::util::SubscriberInitExt;
    use tracing_subscriber::{EnvFilter, Layer};

    let console_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| {
        EnvFilter::new("warn,claw_gui=info,claw_config=info")
    });

    let gui_filter = EnvFilter::new(
        "info,\
         hyper=warn,\
         reqwest=warn,\
         tokio=warn,\
         tungstenite=warn,\
         egui=warn,\
         eframe=warn,\
         winit=warn,\
         wgpu=warn,\
         naga=warn",
    );

    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer().with_filter(console_filter))
        .with(gui_layer.with_filter(gui_filter))
        .init();

    // Shared application state
    let state = Arc::new(RwLock::new(AppState::default()));

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Claw Control Panel")
            .with_inner_size([1024.0, 768.0])
            .with_min_inner_size([800.0, 600.0]),
        ..Default::default()
    };

    let state_clone = state.clone();
    eframe::run_native(
        "Claw Control Panel",
        options,
        Box::new(move |cc| {
            // Spawn tokio runtime for async command handler
            let rt = tokio::runtime::Runtime::new().unwrap();
            let handle = rt.handle().clone();
            let cmd_sender = rt.block_on(async {
                commands::spawn_command_handler(state_clone.clone(), cc.egui_ctx.clone())
            });

            // Keep runtime alive by leaking it (it runs background tasks)
            std::mem::forget(rt);

            Ok(Box::new(ClawApp::new(cc, state_clone, cmd_sender, log_rx, handle)))
        }),
    )
}
