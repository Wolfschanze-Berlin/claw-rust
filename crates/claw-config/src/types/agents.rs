//! Agent configuration types.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use super::helpers::ToggleConfig;

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
    pub context_pruning: Option<ContextPruningConfig>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub compaction: Option<CompactionConfig>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub thinking_default: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub verbose_default: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub elevated_default: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub typing_mode: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub heartbeat: Option<HeartbeatConfig>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_concurrent: Option<u32>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub subagents: Option<SubagentDefaults>,

    // --- New fields from OpenClaw config reference ---

    #[serde(skip_serializing_if = "Option::is_none")]
    pub workspace: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub repo_root: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub skip_bootstrap: Option<bool>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub bootstrap_max_chars: Option<u32>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub bootstrap_total_max_chars: Option<u32>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_timezone: Option<String>,

    /// Primary model selection.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<AgentModelConfig>,

    /// Image generation model identifier.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image_model: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout_seconds: Option<u32>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub media_max_mb: Option<u32>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub context_tokens: Option<u32>,

    /// CLI backends list (e.g. ["openai", "anthropic"]).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cli_backends: Option<Vec<String>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub block_streaming_default: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub block_streaming_break: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub block_streaming_chunk: Option<BlockStreamingChunkConfig>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub block_streaming_coalesce: Option<BlockStreamingCoalesceConfig>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub human_delay: Option<HumanDelayConfig>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub typing_interval_seconds: Option<f64>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub sandbox: Option<SandboxConfig>,

    /// Catch-all for extension fields.
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

/// Primary/fallback model configuration.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct AgentModelConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub primary: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub fallbacks: Option<Vec<String>>,

    /// Catch-all for extension fields.
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

/// Heartbeat (keep-alive) configuration for long-running agent sessions.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct HeartbeatConfig {
    /// Interval string (e.g. "30s", "5m").
    #[serde(skip_serializing_if = "Option::is_none")]
    pub every: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub include_reasoning: Option<bool>,

    /// Session key for heartbeat context.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session: Option<String>,

    /// Target channel/account for heartbeat messages.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub to: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub ack_max_chars: Option<u32>,

    /// Catch-all for extension fields.
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

/// Context compaction (summarization) configuration.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct CompactionConfig {
    /// Mode: "auto", "manual", "off".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mode: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub reserve_tokens_floor: Option<u32>,

    /// Memory flush settings (complex, kept as Value).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub memory_flush: Option<serde_json::Value>,

    /// Catch-all for extension fields.
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

/// Context pruning configuration.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ContextPruningConfig {
    /// Mode: "auto", "aggressive", "off".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mode: Option<String>,

    /// TTL duration string (e.g. "30m").
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ttl: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub keep_last_assistants: Option<u32>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub soft_trim_ratio: Option<f64>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub hard_clear_ratio: Option<f64>,

    /// Catch-all for extension fields.
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

/// Human-like typing delay configuration.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct HumanDelayConfig {
    /// Mode: "off", "fixed", "variable".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mode: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_ms: Option<u32>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_ms: Option<u32>,

    /// Catch-all for extension fields.
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

/// Sandbox (isolated execution) configuration.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct SandboxConfig {
    /// Mode: "off", "docker", "nsjail".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mode: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub scope: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub workspace_access: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub workspace_root: Option<String>,

    /// Docker-specific settings (complex, kept as Value).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub docker: Option<serde_json::Value>,

    /// Browser-in-sandbox settings (complex, kept as Value).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub browser: Option<serde_json::Value>,

    /// Prune/cleanup settings (complex, kept as Value).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prune: Option<serde_json::Value>,

    /// Catch-all for extension fields.
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

/// Block streaming chunk size configuration.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct BlockStreamingChunkConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_chars: Option<u32>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_chars: Option<u32>,

    /// Catch-all for extension fields.
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

/// Block streaming coalesce timing.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct BlockStreamingCoalesceConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub idle_ms: Option<u32>,

    /// Catch-all for extension fields.
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

// ---------------------------------------------------------------------------
// Per-agent types
// ---------------------------------------------------------------------------

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

    // --- New per-agent fields ---

    #[serde(skip_serializing_if = "Option::is_none")]
    pub identity: Option<AgentIdentityConfig>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub group_chat: Option<AgentGroupChatConfig>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<AgentToolsConfig>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub heartbeat: Option<HeartbeatConfig>,

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

/// Agent identity/appearance configuration.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct AgentIdentityConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub theme: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub emoji: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub avatar: Option<String>,

    /// Catch-all for extension fields.
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

/// Agent group chat behavior.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct AgentGroupChatConfig {
    /// Regex patterns that trigger this agent in group chats.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mention_patterns: Option<Vec<String>>,

    /// Catch-all for extension fields.
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

/// Per-agent tool permissions.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct AgentToolsConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profile: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub allow: Option<Vec<String>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub deny: Option<Vec<String>>,

    /// Elevated tool settings (complex, kept as Value).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub elevated: Option<serde_json::Value>,

    /// Loop detection settings (complex, kept as Value).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub loop_detection: Option<serde_json::Value>,

    /// Catch-all for extension fields.
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}
