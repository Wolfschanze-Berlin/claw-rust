//! Claw Control Panel — egui native desktop application.
//!
//! Independent GUI crate for monitoring and controlling the claw-rust gateway.

mod commands;
mod logging;
mod state;
mod theme;
mod views;

use std::sync::{Arc, RwLock};

use eframe::egui;
use tokio::sync::mpsc;

use commands::CommandSender;
use logging::{GuiLogLayer, LogBuffer};
use state::AppState;

/// Available views in the control panel.
#[derive(Debug, Clone, Copy, PartialEq)]
enum View {
    Dashboard,
    Channels,
    Logs,
}

/// Main application struct implementing eframe::App.
struct ClawApp {
    current_view: View,
    state: Arc<RwLock<AppState>>,
    command_sender: CommandSender,
    log_buffer: LogBuffer,
    log_receiver: mpsc::UnboundedReceiver<state::LogEntry>,
}

impl ClawApp {
    fn new(
        cc: &eframe::CreationContext<'_>,
        state: Arc<RwLock<AppState>>,
        command_sender: CommandSender,
        log_receiver: mpsc::UnboundedReceiver<state::LogEntry>,
    ) -> Self {
        theme::apply(&cc.egui_ctx);
        Self {
            current_view: View::Dashboard,
            state,
            command_sender,
            log_buffer: LogBuffer::new(1000),
            log_receiver,
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

                if ui
                    .selectable_label(self.current_view == View::Dashboard, "📊  Dashboard")
                    .clicked()
                {
                    self.current_view = View::Dashboard;
                }
                ui.add_space(4.0);
                if ui
                    .selectable_label(self.current_view == View::Channels, "📡  Channels")
                    .clicked()
                {
                    self.current_view = View::Channels;
                }
                ui.add_space(4.0);
                if ui
                    .selectable_label(self.current_view == View::Logs, "📋  Logs")
                    .clicked()
                {
                    self.current_view = View::Logs;
                }

                ui.with_layout(egui::Layout::bottom_up(egui::Align::LEFT), |ui| {
                    ui.add_space(8.0);
                    ui.label(egui::RichText::new("v0.1.0").small().weak());
                });
            });

        // Main content area
        egui::CentralPanel::default().show(ctx, |ui| {
            let state = self.state.read().unwrap().clone();
            match self.current_view {
                View::Dashboard => views::dashboard::show(ui, &state.gateway),
                View::Channels => {
                    views::channels::show(ui, &state.channels, &self.command_sender)
                }
                View::Logs => views::logs::show(ui, &self.log_buffer),
            }
        });
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
            let cmd_sender =
                rt.block_on(async { commands::spawn_command_handler(state_clone.clone(), cc.egui_ctx.clone()) });

            // Keep runtime alive by leaking it (it runs background tasks)
            std::mem::forget(rt);

            Ok(Box::new(ClawApp::new(cc, state_clone, cmd_sender, log_rx)))
        }),
    )
}
