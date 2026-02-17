//! Configuration type definitions for OpenClaw.
//!
//! Ports the config schema from OpenClaw's TypeScript types to Rust structs.
//! All fields use `Option<T>` with serde defaults since configs are highly
//! flexible and most fields are optional.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ---------------------------------------------------------------------------
// Top-level config
// ---------------------------------------------------------------------------

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
    pub auth: Option<serde_json::Value>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub env: Option<EnvConfig>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub logging: Option<serde_json::Value>,

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
    pub cron: Option<serde_json::Value>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub hooks: Option<serde_json::Value>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub gateway: Option<GatewayConfig>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub plugins: Option<PluginsConfig>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub skills: Option<SkillsConfig>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub discovery: Option<serde_json::Value>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub browser: Option<serde_json::Value>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub talk: Option<serde_json::Value>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub memory: Option<serde_json::Value>,

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

// ---------------------------------------------------------------------------
// Agents
// ---------------------------------------------------------------------------

/// Agent configuration containing defaults and a list of agent entries.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct AgentsConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub defaults: Option<AgentDefaults>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub list: Option<Vec<AgentEntry>>,
}

/// Default settings applied to all agents unless overridden.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct AgentDefaults {
    /// Model overrides keyed by model name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub models: Option<HashMap<String, serde_json::Value>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub time_format: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub memory_search: Option<ToggleConfig>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub context_pruning: Option<serde_json::Value>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub compaction: Option<serde_json::Value>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub thinking_default: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub verbose_default: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub elevated_default: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub typing_mode: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub heartbeat: Option<serde_json::Value>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_concurrent: Option<u32>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub subagents: Option<SubagentDefaults>,

    /// Catch-all for extension fields.
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

/// Subagent concurrency defaults.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct SubagentDefaults {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_concurrent: Option<u32>,
}

/// A single agent entry.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct AgentEntry {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub workspace: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_dir: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub subagents: Option<SubagentConfig>,

    /// Whether this is the default agent (fallback when no binding matches).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default: Option<bool>,

    /// Catch-all for extension fields.
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

/// Per-agent subagent settings.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct SubagentConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allow_agents: Option<Vec<String>>,
}

// ---------------------------------------------------------------------------
// Bindings
// ---------------------------------------------------------------------------

/// Maps an agent to a channel/account via pattern matching.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct BindingEntry {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<String>,

    #[serde(rename = "match", skip_serializing_if = "Option::is_none")]
    pub match_rule: Option<BindingMatch>,
}

/// Match criteria for a binding rule.
///
/// All specified fields use AND semantics — every non-`None` field
/// must match the inbound message for the binding to apply.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct BindingMatch {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub channel: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub account_id: Option<String>,

    /// Exact peer (DM/group/channel) match.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub peer: Option<BindingPeer>,

    /// Discord guild (server) ID.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub guild_id: Option<String>,

    /// Discord roles required for this binding (AND with guild_id).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub roles: Option<Vec<String>>,

    /// Slack team (workspace) ID.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub team_id: Option<String>,
}

/// Peer identifier in a binding match rule.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct BindingPeer {
    /// Chat type kind (e.g. "direct", "group", "channel").
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,

    /// Peer/chat identifier.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
}

// ---------------------------------------------------------------------------
// Tools
// ---------------------------------------------------------------------------

/// Tool configuration (elevated permissions, agent-to-agent, etc).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ToolsConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub elevated: Option<ElevatedToolsConfig>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_to_agent: Option<ToggleConfig>,

    /// Catch-all for extension fields.
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

/// Elevated tool permissions.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ElevatedToolsConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub allow_from: Option<HashMap<String, Vec<String>>>,
}

// ---------------------------------------------------------------------------
// Messages
// ---------------------------------------------------------------------------

/// Message queue and acknowledgement settings.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct MessagesConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub queue: Option<MessageQueueConfig>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub ack_reaction_scope: Option<String>,
}

/// Message queue mode settings.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct MessageQueueConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mode: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub by_channel: Option<HashMap<String, String>>,
}

// ---------------------------------------------------------------------------
// Commands
// ---------------------------------------------------------------------------

/// Native command toggles.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct CommandsConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub native: Option<bool>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub native_skills: Option<bool>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub bash: Option<bool>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub config: Option<bool>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub debug: Option<bool>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub restart: Option<bool>,
}

// ---------------------------------------------------------------------------
// Channels
// ---------------------------------------------------------------------------

/// Per-channel configuration (telegram, discord, etc).
///
/// Uses camelCase field renaming to match the OpenClaw JSON schema.
/// Highly dynamic -- most fields are optional and channel-specific
/// fields are captured in the `extra` map.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ChannelConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub dm_policy: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub group_policy: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream_mode: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub markdown: Option<serde_json::Value>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub accounts: Option<HashMap<String, ChannelAccountConfig>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub actions: Option<serde_json::Value>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub reaction_level: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub heartbeat: Option<serde_json::Value>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub link_preview: Option<bool>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub allow_from: Option<Vec<String>>,

    /// Catch-all for channel-specific extension fields.
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

/// Per-account settings within a channel (e.g. bot tokens).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ChannelAccountConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bot_token: Option<String>,

    /// Catch-all for provider-specific fields.
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

// ---------------------------------------------------------------------------
// Session
// ---------------------------------------------------------------------------

/// Session management configuration.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct SessionConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ttl: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_history: Option<u32>,

    /// Catch-all for extension fields.
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

// ---------------------------------------------------------------------------
// Gateway
// ---------------------------------------------------------------------------

/// Gateway server configuration.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct GatewayConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mode: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub port: Option<u16>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub bind: Option<BindMode>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub control_ui_enabled: Option<bool>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub auth: Option<GatewayAuthConfig>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub openai_compat: Option<serde_json::Value>,

    /// Catch-all for extension fields.
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

/// Gateway bind mode -- which interfaces to listen on.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum BindMode {
    /// Bind to localhost only (127.0.0.1).
    Localhost,
    /// Bind to LAN interfaces.
    Lan,
    /// Bind to all interfaces (0.0.0.0).
    All,
}

/// Gateway authentication configuration.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct GatewayAuthConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mode: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub token: Option<String>,

    /// Catch-all for extension fields.
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

// ---------------------------------------------------------------------------
// Plugins
// ---------------------------------------------------------------------------

/// Plugin configuration with entries.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct PluginsConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entries: Option<HashMap<String, PluginEntry>>,
}

/// A single plugin entry.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct PluginEntry {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,

    /// Catch-all for plugin-specific fields.
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

// ---------------------------------------------------------------------------
// Skills
// ---------------------------------------------------------------------------

/// Skills loading, installation, and entries configuration.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct SkillsConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub load: Option<SkillsLoadConfig>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub install: Option<SkillsInstallConfig>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub entries: Option<HashMap<String, SkillEntry>>,
}

/// Skills file-watching configuration.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct SkillsLoadConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub watch: Option<bool>,
}

/// Skills installation preferences.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct SkillsInstallConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prefer_brew: Option<bool>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub node_manager: Option<String>,
}

/// A single skill entry.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct SkillEntry {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,

    /// Catch-all for skill-specific fields.
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

// ---------------------------------------------------------------------------
// Models
// ---------------------------------------------------------------------------

/// Model provider and registry configuration.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ModelsConfig {
    /// Default model identifier (e.g. "anthropic/claude-sonnet-4-20250514").
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default: Option<String>,

    /// Providers keyed by provider name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub providers: Option<HashMap<String, ModelProvider>>,

    /// Catch-all for extension fields.
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

/// A model provider (e.g. "anthropic", "openai").
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ModelProvider {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_url: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub auth: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub api: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub models: Option<Vec<ModelEntry>>,

    /// Catch-all for extension fields.
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

/// A single model definition within a provider.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ModelEntry {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub input: Option<Vec<String>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<bool>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub context_window: Option<u64>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u64>,

    /// Catch-all for extension fields.
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

// ---------------------------------------------------------------------------
// Platform-specific channel account configs
// ---------------------------------------------------------------------------

/// Telegram-specific account configuration.
///
/// Telegram bots authenticate with a single token from BotFather.
/// The gateway supports either webhook (HTTP POST) or long-polling mode.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct TelegramAccountConfig {
    /// Bot token issued by BotFather.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bot_token: Option<String>,

    /// Webhook URL for webhook mode. When `None`, long polling is used.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub webhook_url: Option<String>,

    /// Webhook secret token for validating incoming requests.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub webhook_secret: Option<String>,

    /// Types of updates to receive (e.g. "message", "callback_query").
    /// If `None`, all update types are received.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowed_updates: Option<Vec<String>>,

    /// Long-poll timeout in seconds (default: 30).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub poll_timeout_secs: Option<u32>,

    /// Catch-all for extension fields.
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

/// WhatsApp-specific account configuration.
///
/// WhatsApp uses session-based auth (QR code scan or pairing code).
/// Sessions persist via a local store (SQLite) and can be invalidated
/// remotely by the mobile app at any time.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct WhatsAppAccountConfig {
    /// Path to the SQLite session store.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub store_path: Option<String>,

    /// Phone number for pairing-code login (alternative to QR scan).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub phone_number: Option<String>,

    /// Whether to use pairing code mode instead of QR.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub use_pairing_code: Option<bool>,

    /// Heartbeat interval in seconds for connection health checks.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub heartbeat_interval_secs: Option<u32>,

    /// Maximum reconnection attempts before giving up.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_reconnect_attempts: Option<u32>,

    /// Catch-all for extension fields.
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

/// Discord-specific account configuration.
///
/// Discord bots authenticate via a bot token from the developer portal.
/// The gateway uses WebSocket shards; slash commands require an application ID.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct DiscordAccountConfig {
    /// Bot token from the Discord developer portal.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bot_token: Option<String>,

    /// Application ID (required for slash command registration).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub application_id: Option<String>,

    /// Gateway intents bitmask. If `None`, defaults to non-privileged intents.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub intents: Option<u64>,

    /// Number of shards. If `None`, Discord auto-selects.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shard_count: Option<u32>,

    /// Whether to sync slash commands on startup.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sync_commands: Option<bool>,

    /// Catch-all for extension fields.
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

// --- Conversion from generic ChannelAccountConfig ---

impl TryFrom<&ChannelAccountConfig> for TelegramAccountConfig {
    type Error = serde_json::Error;

    fn try_from(generic: &ChannelAccountConfig) -> Result<Self, Self::Error> {
        let value = serde_json::to_value(generic)?;
        serde_json::from_value(value)
    }
}

impl TryFrom<&ChannelAccountConfig> for WhatsAppAccountConfig {
    type Error = serde_json::Error;

    fn try_from(generic: &ChannelAccountConfig) -> Result<Self, Self::Error> {
        let value = serde_json::to_value(generic)?;
        serde_json::from_value(value)
    }
}

impl TryFrom<&ChannelAccountConfig> for DiscordAccountConfig {
    type Error = serde_json::Error;

    fn try_from(generic: &ChannelAccountConfig) -> Result<Self, Self::Error> {
        let value = serde_json::to_value(generic)?;
        serde_json::from_value(value)
    }
}

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

/// Simple toggle config used in several places (enabled: bool).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ToggleConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_config_deserializes() {
        let config: OpenClawConfig = serde_json::from_str("{}").unwrap();
        assert!(config.agents.is_none());
        assert!(config.gateway.is_none());
        assert!(config.channels.is_none());
    }

    #[test]
    fn default_config_serializes_to_empty_object() {
        let config = OpenClawConfig::default();
        let json = serde_json::to_value(&config).unwrap();
        assert_eq!(json, serde_json::json!({}));
    }

    #[test]
    fn gateway_bind_mode_roundtrip() {
        let mode = BindMode::Lan;
        let json = serde_json::to_string(&mode).unwrap();
        assert_eq!(json, r#""lan""#);
        let parsed: BindMode = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, BindMode::Lan);
    }

    #[test]
    fn gateway_config_parses() {
        let json = r#"{
            "mode": "local",
            "bind": "lan",
            "auth": {
                "mode": "token",
                "token": "secret123"
            }
        }"#;
        let gw: GatewayConfig = serde_json::from_str(json).unwrap();
        assert_eq!(gw.mode.as_deref(), Some("local"));
        assert_eq!(gw.bind, Some(BindMode::Lan));
        assert_eq!(gw.auth.as_ref().unwrap().mode.as_deref(), Some("token"));
    }

    #[test]
    fn channel_config_with_accounts() {
        let json = r#"{
            "enabled": true,
            "dmPolicy": "pairing",
            "accounts": {
                "main": { "botToken": "tok123" }
            }
        }"#;
        let ch: ChannelConfig = serde_json::from_str(json).unwrap();
        assert_eq!(ch.enabled, Some(true));
        assert_eq!(ch.dm_policy.as_deref(), Some("pairing"));
        let accts = ch.accounts.unwrap();
        assert_eq!(
            accts.get("main").unwrap().bot_token.as_deref(),
            Some("tok123")
        );
    }

    #[test]
    fn binding_entry_parses() {
        let json = r#"{
            "agentId": "main",
            "match": {
                "channel": "telegram",
                "accountId": "main"
            }
        }"#;
        let b: BindingEntry = serde_json::from_str(json).unwrap();
        assert_eq!(b.agent_id.as_deref(), Some("main"));
        let m = b.match_rule.unwrap();
        assert_eq!(m.channel.as_deref(), Some("telegram"));
        assert_eq!(m.account_id.as_deref(), Some("main"));
    }

    #[test]
    fn agent_entry_parses() {
        let json = r#"{
            "id": "einstein",
            "name": "einstein",
            "model": "anthropic/claude-haiku-4-5-20251001",
            "subagents": { "allowAgents": [] }
        }"#;
        let a: AgentEntry = serde_json::from_str(json).unwrap();
        assert_eq!(a.id.as_deref(), Some("einstein"));
        assert_eq!(a.model.as_deref(), Some("anthropic/claude-haiku-4-5-20251001"));
        assert!(a.subagents.unwrap().allow_agents.unwrap().is_empty());
    }

    #[test]
    fn model_entry_parses() {
        let json = r#"{
            "id": "claude-opus-4-6",
            "name": "anthropic/claude-opus-4-6",
            "input": ["text", "image"],
            "reasoning": true,
            "contextWindow": 200000,
            "maxTokens": 128000
        }"#;
        let m: ModelEntry = serde_json::from_str(json).unwrap();
        assert_eq!(m.id.as_deref(), Some("claude-opus-4-6"));
        assert_eq!(m.reasoning, Some(true));
        assert_eq!(m.context_window, Some(200000));
        assert_eq!(m.max_tokens, Some(128000));
    }

    #[test]
    fn full_config_roundtrip() {
        let json = include_str!("../../../config/config.json");
        let config: OpenClawConfig = serde_json::from_str(json).unwrap();
        assert!(config.agents.is_some());
        assert!(config.channels.is_some());
        assert!(config.gateway.is_some());
        assert!(config.models.is_some());
        assert!(config.bindings.is_some());

        // Round-trip
        let serialized = serde_json::to_string(&config).unwrap();
        let reparsed: OpenClawConfig = serde_json::from_str(&serialized).unwrap();
        assert!(reparsed.agents.is_some());
    }

    #[test]
    fn extra_fields_preserved() {
        let json = r#"{"customField": 42, "anotherOne": "hello"}"#;
        let config: OpenClawConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.extra.get("customField").unwrap(), &serde_json::json!(42));
        assert_eq!(config.extra.get("anotherOne").unwrap(), &serde_json::json!("hello"));
    }

    #[test]
    fn telegram_account_config_parses() {
        let json = r#"{
            "botToken": "123:ABC",
            "webhookUrl": "https://example.com/hook",
            "webhookSecret": "sec123",
            "allowedUpdates": ["message", "callback_query"],
            "pollTimeoutSecs": 60
        }"#;
        let cfg: TelegramAccountConfig = serde_json::from_str(json).unwrap();
        assert_eq!(cfg.bot_token.as_deref(), Some("123:ABC"));
        assert_eq!(cfg.webhook_url.as_deref(), Some("https://example.com/hook"));
        assert_eq!(cfg.webhook_secret.as_deref(), Some("sec123"));
        assert_eq!(cfg.allowed_updates.as_ref().unwrap().len(), 2);
        assert_eq!(cfg.poll_timeout_secs, Some(60));
    }

    #[test]
    fn telegram_account_from_generic() {
        let json = r#"{"botToken": "123:ABC", "webhookUrl": "https://example.com"}"#;
        let generic: ChannelAccountConfig = serde_json::from_str(json).unwrap();
        let tg = TelegramAccountConfig::try_from(&generic).unwrap();
        assert_eq!(tg.bot_token.as_deref(), Some("123:ABC"));
        assert_eq!(tg.webhook_url.as_deref(), Some("https://example.com"));
    }

    #[test]
    fn whatsapp_account_config_parses() {
        let json = r#"{
            "storePath": "/data/wa-store.db",
            "phoneNumber": "+1234567890",
            "usePairingCode": true,
            "heartbeatIntervalSecs": 15,
            "maxReconnectAttempts": 5
        }"#;
        let cfg: WhatsAppAccountConfig = serde_json::from_str(json).unwrap();
        assert_eq!(cfg.store_path.as_deref(), Some("/data/wa-store.db"));
        assert_eq!(cfg.phone_number.as_deref(), Some("+1234567890"));
        assert_eq!(cfg.use_pairing_code, Some(true));
        assert_eq!(cfg.heartbeat_interval_secs, Some(15));
        assert_eq!(cfg.max_reconnect_attempts, Some(5));
    }

    #[test]
    fn whatsapp_account_from_generic() {
        let json = r#"{"storePath": "/data/wa.db"}"#;
        let generic: ChannelAccountConfig = serde_json::from_str(json).unwrap();
        let wa = WhatsAppAccountConfig::try_from(&generic).unwrap();
        assert_eq!(wa.store_path.as_deref(), Some("/data/wa.db"));
        assert!(wa.phone_number.is_none());
    }

    #[test]
    fn discord_account_config_parses() {
        let json = r#"{
            "botToken": "discord-tok",
            "applicationId": "1234567890",
            "intents": 3276799,
            "shardCount": 2,
            "syncCommands": true
        }"#;
        let cfg: DiscordAccountConfig = serde_json::from_str(json).unwrap();
        assert_eq!(cfg.bot_token.as_deref(), Some("discord-tok"));
        assert_eq!(cfg.application_id.as_deref(), Some("1234567890"));
        assert_eq!(cfg.intents, Some(3276799));
        assert_eq!(cfg.shard_count, Some(2));
        assert_eq!(cfg.sync_commands, Some(true));
    }

    #[test]
    fn discord_account_from_generic() {
        let json = r#"{"botToken": "disc-tok", "applicationId": "app123"}"#;
        let generic: ChannelAccountConfig = serde_json::from_str(json).unwrap();
        let dc = DiscordAccountConfig::try_from(&generic).unwrap();
        assert_eq!(dc.bot_token.as_deref(), Some("disc-tok"));
        assert_eq!(dc.application_id.as_deref(), Some("app123"));
    }

    #[test]
    fn platform_config_defaults_are_empty() {
        let tg = TelegramAccountConfig::default();
        let json = serde_json::to_value(&tg).unwrap();
        assert_eq!(json, serde_json::json!({}));

        let wa = WhatsAppAccountConfig::default();
        let json = serde_json::to_value(&wa).unwrap();
        assert_eq!(json, serde_json::json!({}));

        let dc = DiscordAccountConfig::default();
        let json = serde_json::to_value(&dc).unwrap();
        assert_eq!(json, serde_json::json!({}));
    }
}
