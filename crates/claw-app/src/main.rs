//! claw-rust application entry point.
//!
//! Loads configuration, initializes subsystems, starts the gateway server
//! and channel manager, then waits for a shutdown signal.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, Result};
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};

use claw_agent_models::anthropic::AnthropicProvider;
use claw_agent_models::catalog::ModelEntry;
use claw_agent_models::ModelCatalog;
use claw_agent_runtime::{AgentRunner, RuntimeDeps, SubscriberConfig, TranscriptStore};
use claw_agent_tools::{PipelineConfig, PolicyEngine, ToolPipeline, ToolRegistry};
use claw_agent_workspace::AgentWorkspace;
use claw_channels::manager::ChannelManager;
use claw_channels::registry::ChannelRegistry;
use claw_config::types::{BindMode, OpenClawConfig};
use claw_config::load_config;
use claw_core::runtime::{RuntimeEnv, init_tracing};
use claw_db::Database;
use claw_claude_code::{ClaudeCodeConfig, ClaudeCodeDispatchContext, SessionManager};
use claw_dispatch::{AgentDispatchContext, CommandQueue};
use claw_gateway::handshake::AuthMode;
use claw_gateway::server::{GatewayBindMode, GatewayServerOptions, start_gateway_server};

#[tokio::main]
async fn main() -> Result<()> {
    // 1. Load .env before anything reads environment variables.
    match dotenvy::dotenv() {
        Ok(path) => eprintln!(".env loaded from {}", path.display()),
        Err(e) if e.not_found() => {} // no .env file is fine
        Err(e) => eprintln!("WARNING: failed to parse .env: {e}"),
    }

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

    // 6. Initialize agent runtime (model provider, catalog, tools, workspace).
    let cancel = CancellationToken::new();
    let agent_ctx = init_agent_runtime(&config)?;
    let dispatch_ready = agent_ctx.is_some();
    if dispatch_ready {
        info!("agent runtime initialized — dispatch pipeline active");
    } else {
        warn!("no ANTHROPIC_API_KEY set — dispatch pipeline disabled (messages will be logged only)");
    }

    // 7. Set up dispatch channel and channel manager.
    let registry = ChannelRegistry::new();
    register_channel_plugins(&registry);
    info!(
        channels = registry.len(),
        "channel plugins registered: {:?}",
        registry.list_ids()
    );

    let (dispatch_tx, dispatch_rx) = tokio::sync::mpsc::unbounded_channel();

    let runtime = RuntimeEnv::new("channels");
    let channel_mgr = ChannelManager::new(registry.clone(), cancel.clone())
        .with_dispatch(dispatch_tx);

    // Start channels from config — iterates config.channels, finds matching
    // registered plugins, and spawns a gateway task per account.
    if let Some(ref channel_configs) = config.channels {
        channel_mgr.start_channels(channel_configs, &runtime);
    } else {
        warn!("no channels configured in config file");
    }

    // 8. Initialize Claude Code dispatcher (if `claude` CLI is available).
    let cc_ctx = init_claude_code_dispatcher(db_path).await;
    if cc_ctx.is_some() {
        info!("Claude Code dispatcher initialized — messages will route through `claude` CLI");
    }

    // 9. Spawn the dispatch loop (bridges inbound messages to agent runtime or Claude Code).
    if let Some(agent_ctx) = agent_ctx {
        let queue = CommandQueue::new();
        let dispatch_cancel = cancel.clone();
        let dispatch_registry = registry.clone();
        tokio::spawn(async move {
            claw_dispatch::run_dispatch_loop(
                dispatch_rx,
                dispatch_registry,
                agent_ctx,
                queue,
                cc_ctx,
                dispatch_cancel,
            )
            .await;
        });
    } else {
        // Drop the receiver so the channel doesn't accumulate messages.
        drop(dispatch_rx);
    }

    info!("application ready — press Ctrl+C to stop");

    // 10. Wait for shutdown signal.
    tokio::signal::ctrl_c()
        .await
        .expect("failed to listen for Ctrl+C");
    info!("received shutdown signal");

    // 11. Graceful shutdown.
    gateway.close("application shutdown").await;
    channel_mgr.stop_all();
    cancel.cancel(); // stops the dispatch loop

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

/// Initialize the agent runtime from environment and config.
///
/// Returns `None` if the required API key is not configured (the app can
/// still run in "logging only" mode without the agent pipeline).
fn init_agent_runtime(config: &OpenClawConfig) -> Result<Option<AgentDispatchContext>> {
    // Resolve API key: env var > config file.
    let api_key = std::env::var("ANTHROPIC_API_KEY").ok().or_else(|| {
        config
            .models
            .as_ref()
            .and_then(|m| m.providers.as_ref())
            .and_then(|p| p.get("anthropic"))
            .and_then(|a| a.auth.clone())
    });

    let Some(api_key) = api_key else {
        return Ok(None);
    };

    if api_key.is_empty() {
        return Ok(None);
    }

    // Build Anthropic provider.
    let provider: Arc<dyn claw_agent_models::provider::ModelProvider> =
        Arc::new(AnthropicProvider::new(api_key));

    // Build model catalog with a default Sonnet entry.
    let mut catalog = ModelCatalog::new();
    catalog.register(ModelEntry {
        id: "claude-sonnet-4-20250514".into(),
        name: "Claude Sonnet 4".into(),
        provider: "anthropic".into(),
        context_window: Some(200_000),
        max_tokens: Some(8192),
        input_modalities: vec![],
        supports_reasoning: false,
        supports_tools: true,
    });
    catalog.set_global_default("claude-sonnet-4-20250514");

    // Register additional models from config if present.
    if let Some(ref models_config) = config.models {
        if let Some(ref default_model) = models_config.default {
            catalog.set_global_default(default_model);
        }
        if let Some(ref providers) = models_config.providers {
            if let Some(anthropic_cfg) = providers.get("anthropic") {
                if let Some(ref models) = anthropic_cfg.models {
                    for entry in models {
                        if let Some(ref id) = entry.id {
                            catalog.register(ModelEntry {
                                id: id.clone(),
                                name: entry.name.clone().unwrap_or_else(|| id.clone()),
                                provider: "anthropic".into(),
                                context_window: entry.context_window,
                                max_tokens: None,
                                input_modalities: vec![],
                                supports_reasoning: entry.reasoning.unwrap_or(false),
                                supports_tools: true,
                            });
                        }
                    }
                }
            }
        }
    }

    // Build tool pipeline (empty registry for now — tools added via config later).
    let tool_pipeline = Arc::new(ToolPipeline::new(
        ToolRegistry::new(),
        PolicyEngine::new(),
        PipelineConfig::default(),
    ));

    // Agent workspace — stores identity/skills/memory files.
    let workspace_dir = Path::new(".data/workspace");
    std::fs::create_dir_all(workspace_dir).context("failed to create workspace directory")?;
    let workspace = AgentWorkspace::new(workspace_dir, "default");

    // In-memory transcript store (will be replaced with DB-backed store later).
    let transcript_store: Arc<dyn TranscriptStore> = Arc::new(InMemoryTranscriptStore::new());

    let deps = Arc::new(RuntimeDeps {
        catalog,
        provider,
        tool_pipeline,
        workspace,
        transcript_store,
        subscriber_config: SubscriberConfig::default(),
        max_tool_iterations: 10,
    });

    Ok(Some(AgentDispatchContext {
        runner: Arc::new(AgentRunner::new()),
        deps,
    }))
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
                BindMode::Localhost | BindMode::Auto | BindMode::Loopback => {
                    GatewayBindMode::Loopback
                }
                BindMode::Lan | BindMode::All | BindMode::Tailnet | BindMode::Custom => {
                    GatewayBindMode::Lan
                }
            };
        }

        // Prefer nested controlUi config, fall back to flat controlUiEnabled.
        let control_ui_enabled = gw
            .control_ui
            .as_ref()
            .and_then(|ui| ui.enabled)
            .or(gw.control_ui_enabled);
        if let Some(enabled) = control_ui_enabled {
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
                    let password = auth
                        .password
                        .clone()
                        .or_else(|| auth.token.clone())
                        .unwrap_or_default();
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

// ---------------------------------------------------------------------------
// Claude Code dispatcher initialization
// ---------------------------------------------------------------------------

/// Initialize the Claude Code dispatcher if the `claude` CLI is available.
///
/// Checks for the CLI binary in PATH and, if found, creates a session
/// manager backed by a separate SQLite connection to the same database.
/// Returns `None` if the binary is not found (the app falls back to the
/// agent runtime API).
async fn init_claude_code_dispatcher(db_path: &Path) -> Option<ClaudeCodeDispatchContext> {
    // Check if `claude` binary is available.
    let config = ClaudeCodeConfig::default();
    let check = tokio::process::Command::new(&config.cli_path)
        .arg("--version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .await;

    match check {
        Ok(status) if status.success() => {
            info!(
                cli = %config.cli_path.display(),
                "Claude Code CLI found"
            );
        }
        _ => {
            info!(
                cli = %config.cli_path.display(),
                "Claude Code CLI not found — Claude Code dispatch disabled"
            );
            return None;
        }
    }

    // Open a dedicated SQLite connection for Claude Code sessions.
    // Uses the same database file but a separate connection to avoid
    // mutex contention with the main Database handle.
    let conn = match rusqlite::Connection::open(db_path) {
        Ok(c) => c,
        Err(e) => {
            warn!(error = %e, "failed to open SQLite connection for Claude Code sessions");
            return None;
        }
    };

    let conn = Arc::new(Mutex::new(conn));
    match claw_claude_code::SqliteSessionStore::new(conn).await {
        Ok(store) => {
            let session_mgr = Arc::new(SessionManager::new(store));
            Some(ClaudeCodeDispatchContext {
                config,
                session_mgr,
            })
        }
        Err(e) => {
            warn!(error = %e, "failed to create Claude Code session store");
            None
        }
    }
}

// ---------------------------------------------------------------------------
// In-memory transcript store
// ---------------------------------------------------------------------------

use async_trait::async_trait;
use claw_agent_models::types::ChatMessage;
use std::collections::HashMap;
use tokio::sync::Mutex;

/// Simple in-memory transcript store for initial deployment.
///
/// Will be replaced with a DB-backed implementation once the transcript
/// bridge from `claw-db` to the `TranscriptStore` trait is built.
struct InMemoryTranscriptStore {
    messages: Mutex<HashMap<String, Vec<ChatMessage>>>,
}

impl InMemoryTranscriptStore {
    fn new() -> Self {
        Self {
            messages: Mutex::new(HashMap::new()),
        }
    }
}

#[async_trait]
impl TranscriptStore for InMemoryTranscriptStore {
    async fn load(&self, session_key: &str) -> std::result::Result<Vec<ChatMessage>, String> {
        let store = self.messages.lock().await;
        Ok(store.get(session_key).cloned().unwrap_or_default())
    }

    async fn save(
        &self,
        session_key: &str,
        messages: &[ChatMessage],
    ) -> std::result::Result<(), String> {
        self.messages
            .lock()
            .await
            .insert(session_key.to_owned(), messages.to_vec());
        Ok(())
    }
}
