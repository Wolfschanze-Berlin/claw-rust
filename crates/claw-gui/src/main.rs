//! Claw Control Panel — egui native desktop application.
//!
//! Independent GUI crate for monitoring and controlling the claw-rust gateway.

mod commands;
mod config_manager;
mod logging;
mod state;
mod theme;
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
    Bindings,
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
    commit_state: views::commit_workflow::CommitWorkflowState,
}

impl ClawApp {
    fn new(
        cc: &eframe::CreationContext<'_>,
        state: Arc<RwLock<AppState>>,
        command_sender: CommandSender,
        log_receiver: mpsc::UnboundedReceiver<state::LogEntry>,
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
            commit_state: Default::default(),
        }
    }
}

impl eframe::App for ClawApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Drain log events each frame
        self.log_buffer.drain_receiver(&mut self.log_receiver);

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
                    ui.label(egui::RichText::new("v0.1.0").small().weak());
                });
            });

        // Main content area
        egui::CentralPanel::default().show(ctx, |ui| {
            match self.current_view {
                // -- Monitor views (use runtime AppState) --
                View::Dashboard => {
                    let state = self.state.read().unwrap().clone();
                    views::dashboard::show(ui, &state.gateway);
                }
                View::Logs => {
                    views::logs::show(ui, &self.log_buffer);
                }

                // -- Config views (use ConfigManager) --
                View::Config
                | View::ConfigChannels
                | View::ConfigAgents
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
                views::channels::show(ui, mgr, &mut self.channels_view_state);
            }
            View::ConfigAgents => {
                views::agents::show(ui, mgr, &mut self.agents_view_state);
            }
            View::Bindings => {
                views::bindings::show(ui, mgr);
            }
            _ => {}
        }
    }
}

fn main() -> eframe::Result<()> {
    // Set up log capture for GUI
    let (log_tx, log_rx) = mpsc::unbounded_channel();
    let gui_layer = GuiLogLayer::new(log_tx);

    // Initialize tracing with both console and GUI layers
    use tracing_subscriber::layer::SubscriberExt;
    use tracing_subscriber::util::SubscriberInitExt;
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer())
        .with(gui_layer)
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
            let cmd_sender = rt.block_on(async {
                commands::spawn_command_handler(state_clone.clone(), cc.egui_ctx.clone())
            });

            // Keep runtime alive by leaking it (it runs background tasks)
            std::mem::forget(rt);

            Ok(Box::new(ClawApp::new(cc, state_clone, cmd_sender, log_rx)))
        }),
    )
}
