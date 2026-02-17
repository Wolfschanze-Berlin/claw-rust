//! Top-level configuration and metadata types.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use super::agents::AgentsConfig;
use super::bindings::BindingEntry;
use super::channels::ChannelConfig;
use super::commands::CommandsConfig;
use super::gateway::GatewayConfig;
use super::messages::MessagesConfig;
use super::misc::{
    AuthConfig, BrowserConfig, CanvasHostConfig, CronConfig, DiscoveryConfig, HooksConfig,
    LoggingConfig, TalkConfig, UiConfig, WebChannelConfig,
};
use super::models::ModelsConfig;
use super::plugins::PluginsConfig;
use super::plugins::SkillsConfig;
use super::session::SessionConfig;
use super::tools::ToolsConfig;

/// Root configuration matching OpenClaw's config file schema.
///
/// Nearly every field is optional to support partial configs, config layering,
/// and `$include` composition.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct OpenClawConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub meta: Option<MetaConfig>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub auth: Option<AuthConfig>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub env: Option<EnvConfig>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub logging: Option<LoggingConfig>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub wizard: Option<WizardConfig>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub update: Option<UpdateConfig>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub models: Option<ModelsConfig>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub agents: Option<AgentsConfig>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<ToolsConfig>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub bindings: Option<Vec<BindingEntry>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub session: Option<SessionConfig>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub channels: Option<HashMap<String, ChannelConfig>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub messages: Option<MessagesConfig>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub commands: Option<CommandsConfig>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub cron: Option<CronConfig>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub hooks: Option<HooksConfig>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub gateway: Option<GatewayConfig>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub plugins: Option<PluginsConfig>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub skills: Option<SkillsConfig>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub discovery: Option<DiscoveryConfig>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub browser: Option<BrowserConfig>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub talk: Option<TalkConfig>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub memory: Option<serde_json::Value>,

    // --- New top-level fields ---

    #[serde(skip_serializing_if = "Option::is_none")]
    pub ui: Option<UiConfig>,

    #[serde(rename = "canvasHost", skip_serializing_if = "Option::is_none")]
    pub canvas_host: Option<CanvasHostConfig>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub web: Option<WebChannelConfig>,

    /// Catch-all for unknown/extension fields.
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

// ---------------------------------------------------------------------------
// Meta & lifecycle
// ---------------------------------------------------------------------------

/// Metadata about the config file itself (version tracking, timestamps).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct MetaConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_touched_version: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_touched_at: Option<String>,
}

/// Environment configuration (shell env passthrough, etc).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct EnvConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shell_env: Option<ShellEnvConfig>,
}

/// Shell environment passthrough settings.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ShellEnvConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
}

/// Setup wizard tracking.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct WizardConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_run_at: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_run_version: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_run_command: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_run_mode: Option<String>,
}

/// Auto-update channel configuration.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct UpdateConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub channel: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub check_on_start: Option<bool>,
}
