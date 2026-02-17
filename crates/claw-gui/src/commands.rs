//! Async command handler bridging UI actions to backend operations.
//!
//! UI sends commands via mpsc channel. Backend processes them and updates AppState.

use std::sync::{Arc, RwLock};

use eframe::egui;
use tokio::sync::mpsc;

use crate::state::{AppState, ChannelStatus};

/// Commands the UI can send to the backend.
#[derive(Debug, Clone)]
pub enum Command {
    StartChannel(String),
    StopChannel(String),
}

/// Handle for the UI to send commands (non-blocking).
#[derive(Clone)]
pub struct CommandSender {
    tx: mpsc::Sender<Command>,
}

impl CommandSender {
    pub fn new(tx: mpsc::Sender<Command>) -> Self {
        Self { tx }
    }

    /// Send a command without blocking the UI thread.
    pub fn send(&self, cmd: Command) {
        let _ = self.tx.try_send(cmd);
    }
}

/// Create a command channel and spawn the backend handler task.
pub fn spawn_command_handler(
    state: Arc<RwLock<AppState>>,
    repaint_signal: egui::Context,
) -> CommandSender {
    let (tx, rx) = mpsc::channel::<Command>(100);
    tokio::spawn(process_commands(rx, state, repaint_signal));
    CommandSender::new(tx)
}

async fn process_commands(
    mut rx: mpsc::Receiver<Command>,
    state: Arc<RwLock<AppState>>,
    ctx: egui::Context,
) {
    while let Some(cmd) = rx.recv().await {
        match cmd {
            Command::StartChannel(name) => {
                update_channel_status(&state, &name, ChannelStatus::Starting);
                ctx.request_repaint();

                // Simulate async startup (replace with real channel start later)
                tokio::time::sleep(std::time::Duration::from_millis(500)).await;

                update_channel_status(&state, &name, ChannelStatus::Running);
                ctx.request_repaint();
            }
            Command::StopChannel(name) => {
                update_channel_status(&state, &name, ChannelStatus::Stopping);
                ctx.request_repaint();

                tokio::time::sleep(std::time::Duration::from_millis(300)).await;

                update_channel_status(&state, &name, ChannelStatus::Stopped);
                ctx.request_repaint();
            }
        }
    }
}

fn update_channel_status(state: &Arc<RwLock<AppState>>, name: &str, status: ChannelStatus) {
    if let Ok(mut state) = state.write() {
        if let Some(channel) = state.channels.iter_mut().find(|c| c.name == name) {
            channel.status = status;
        }
    }
}
