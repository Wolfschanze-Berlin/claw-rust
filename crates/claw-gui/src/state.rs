//! Core application state types for the control panel.
//!
//! All types derive Clone for immediate-mode UI reads.

use std::time::Duration;

/// Channel lifecycle state machine.
///
/// Transitions: Stopped → Starting → Running, Running → Stopping → Stopped, any → Error(String)
#[derive(Debug, Clone, PartialEq)]
pub enum ChannelStatus {
    Stopped,
    Starting,
    Running,
    Stopping,
    Error(String),
}

/// Information about a single channel.
#[derive(Debug, Clone)]
pub struct ChannelInfo {
    pub name: String,
    pub channel_type: String,
    pub status: ChannelStatus,
}

/// Gateway health status.
#[derive(Debug, Clone)]
pub struct GatewayStatus {
    pub running: bool,
    pub uptime: Duration,
    pub port: u16,
    pub connected_clients: usize,
}

impl Default for GatewayStatus {
    fn default() -> Self {
        Self {
            running: false,
            uptime: Duration::ZERO,
            port: 18789,
            connected_clients: 0,
        }
    }
}

/// A single log entry for the GUI log viewer.
#[derive(Debug, Clone)]
pub struct LogEntry {
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub level: tracing::Level,
    pub target: String,
    pub message: String,
}

/// Central application state read by all views.
#[derive(Debug, Clone)]
pub struct AppState {
    pub gateway: GatewayStatus,
    pub channels: Vec<ChannelInfo>,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            gateway: GatewayStatus::default(),
            channels: vec![ChannelInfo {
                name: "telegram".into(),
                channel_type: "Telegram".into(),
                status: ChannelStatus::Stopped,
            }],
        }
    }
}
