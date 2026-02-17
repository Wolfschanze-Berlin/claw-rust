//! claw-rust application entry point.
//!
//! Loads configuration, initializes subsystems, starts the gateway server
//! and channel manager, then waits for a shutdown signal.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use tokio_util::sync::CancellationToken;
use tracing::{error, info, warn};

use claw_channels::registry::ChannelRegistry;
use claw_config::types::{BindMode, OpenClawConfig};
use claw_config::load_config;
use claw_core::backoff::BackoffPolicy;
use claw_core::runtime::{RuntimeEnv, init_tracing};
use claw_db::Database;
use claw_gateway::channel_manager::ChannelManager;
use claw_gateway::handshake::AuthMode;
use claw_gateway::server::{GatewayBindMode, GatewayServerOptions, start_gateway_server};

#[tokio::main]
async fn main() -> Result<()> {
    // 1. Load .env before anything reads environment variables.
    dotenvy::dotenv().ok();

    // 2. Initialize tracing (respects RUST_LOG from .env).
    init_tracing();

    info!("claw-rust v{} starting", env!("CARGO_PKG_VERSION"));

    // 3. Load configuration.
    let config_path = config_path_from_args();
    let config = load_config(&config_path)
        .with_context(|| format!("failed to load config from {}", config_path.display()))?;
    info!(path = %config_path.display(), "config loaded");

    // 4. Initialize database.
    let db_path = Path::new(".data/claw.db");
    if let Some(parent) = db_path.parent() {
        std::fs::create_dir_all(parent).context("failed to create database directory")?;
    }
    let db = Database::open(db_path).context("failed to open database")?;
    db.initialize().context("failed to initialize database")?;
    info!(path = %db_path.display(), "database initialized");

    // 5. Start gateway server.
    let gateway_opts = build_gateway_options(&config);
    let gateway = start_gateway_server(gateway_opts)
        .await
        .context("failed to start gateway server")?;
    info!(
        addr = %gateway.bind_addr,
        port = gateway.port,
        "gateway server listening"
    );

    // 6. Set up channel manager with registered plugins.
    let cancel = CancellationToken::new();
    let registry = ChannelRegistry::new();
    register_channel_plugins(&registry);
    info!(
        channels = registry.len(),
        "channel plugins registered: {:?}",
        registry.list_ids()
    );
    let channel_mgr = ChannelManager::new(
        registry,
        BackoffPolicy::default(),
        RuntimeEnv::new("channels"),
        cancel.clone(),
    );
    if let Err(e) = channel_mgr.start_channels().await {
        error!(error = %e, "failed to start channels");
    }

    info!("application ready — press Ctrl+C to stop");

    // 7. Wait for shutdown signal.
    tokio::signal::ctrl_c()
        .await
        .expect("failed to listen for Ctrl+C");
    info!("received shutdown signal");

    // 8. Graceful shutdown.
    gateway.close("application shutdown").await;
    channel_mgr.shutdown().await;

    info!("shutdown complete");
    Ok(())
}

/// Determine the config file path from CLI args or fall back to default.
fn config_path_from_args() -> PathBuf {
    std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("config/config.json"))
}

/// Register channel plugins that are compiled in via feature flags.
fn register_channel_plugins(registry: &ChannelRegistry) {
    #[cfg(feature = "telegram")]
    {
        let plugin = std::sync::Arc::new(claw_telegram::TelegramPlugin::new());
        registry.register_plugin(plugin);
        info!("registered telegram channel plugin");
    }

    #[cfg(feature = "whatsapp")]
    {
        let plugin = std::sync::Arc::new(claw_whatsapp::WhatsAppPlugin::new());
        registry.register_plugin(plugin);
        info!("registered whatsapp channel plugin");
    }

    #[cfg(feature = "discord")]
    {
        let plugin = std::sync::Arc::new(claw_discord::DiscordPlugin::new());
        registry.register_plugin(plugin);
        info!("registered discord channel plugin");
    }
}

/// Map [`OpenClawConfig`] gateway settings to [`GatewayServerOptions`].
fn build_gateway_options(config: &OpenClawConfig) -> GatewayServerOptions {
    let mut opts = GatewayServerOptions::default();

    if let Some(ref gw) = config.gateway {
        if let Some(port) = gw.port {
            opts.port = port;
        }

        if let Some(ref bind) = gw.bind {
            opts.bind_mode = match bind {
                BindMode::Localhost => GatewayBindMode::Loopback,
                BindMode::Lan | BindMode::All => GatewayBindMode::Lan,
            };
        }

        if let Some(enabled) = gw.control_ui_enabled {
            opts.control_ui_enabled = enabled;
        }

        if let Some(ref auth) = gw.auth {
            opts.auth_mode = match auth.mode.as_deref() {
                Some("token") => {
                    let token = auth.token.clone().unwrap_or_default();
                    if token.is_empty() {
                        warn!("auth mode is 'token' but no token configured");
                    }
                    AuthMode::Token { token }
                }
                Some("password") => {
                    let password = auth.token.clone().unwrap_or_default();
                    AuthMode::Password { password }
                }
                Some("trusted-proxy") => AuthMode::TrustedProxy,
                Some(other) => {
                    warn!(mode = other, "unknown auth mode, falling back to none");
                    AuthMode::None
                }
                None => AuthMode::None,
            };
        }
    }

    opts
}
