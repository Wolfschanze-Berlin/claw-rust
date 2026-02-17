//! Tool configuration types.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use super::helpers::ToggleConfig;

/// Tool configuration (elevated permissions, agent-to-agent, etc).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ToolsConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub elevated: Option<ElevatedToolsConfig>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_to_agent: Option<ToggleConfig>,

    // --- New fields from OpenClaw config reference ---

    /// Tool profile name (e.g. "standard", "minimal").
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profile: Option<String>,

    /// Global tool allow list.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allow: Option<Vec<String>>,

    /// Global tool deny list.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deny: Option<Vec<String>>,

    /// Per-provider tool overrides.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub by_provider: Option<HashMap<String, ToolsByProviderConfig>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub exec: Option<ExecToolsConfig>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub loop_detection: Option<LoopDetectionConfig>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub web: Option<WebToolsConfig>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub media: Option<MediaToolsConfig>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub sessions: Option<SessionsToolsConfig>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub subagents: Option<SubagentToolsConfig>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub sandbox: Option<SandboxToolsConfig>,

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

/// Execution tool settings (bash, apply_patch, etc).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ExecToolsConfig {
    /// Background process timeout (ms).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub background_ms: Option<u32>,

    /// Foreground execution timeout (seconds).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout_sec: Option<u32>,

    /// Cleanup timeout after process exit (ms).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cleanup_ms: Option<u32>,

    /// Notify user when background process exits.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notify_on_exit: Option<bool>,

    /// apply_patch tool settings (complex, kept as Value).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub apply_patch: Option<serde_json::Value>,

    /// Catch-all for extension fields.
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

/// Tool invocation loop detection.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct LoopDetectionConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub history_size: Option<u32>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub warning_threshold: Option<u32>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub critical_threshold: Option<u32>,

    /// Detector plugins (complex, kept as Value).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detectors: Option<serde_json::Value>,

    /// Catch-all for extension fields.
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

/// Web tool (search, fetch) configuration.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct WebToolsConfig {
    /// Web search settings (complex, kept as Value).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub search: Option<serde_json::Value>,

    /// Web fetch settings (complex, kept as Value).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fetch: Option<serde_json::Value>,

    /// Catch-all for extension fields.
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

/// Media tool (audio, video) configuration.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct MediaToolsConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub concurrency: Option<u32>,

    /// Audio processing settings (complex, kept as Value).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub audio: Option<serde_json::Value>,

    /// Video processing settings (complex, kept as Value).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub video: Option<serde_json::Value>,

    /// Catch-all for extension fields.
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

/// Session tool visibility.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct SessionsToolsConfig {
    /// Visibility: "own", "all".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub visibility: Option<String>,

    /// Catch-all for extension fields.
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

/// Subagent tool access control.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct SubagentToolsConfig {
    /// Tool settings for subagents (complex, kept as Value).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<serde_json::Value>,

    /// Catch-all for extension fields.
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

/// Sandbox tool access control.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct SandboxToolsConfig {
    /// Tool settings for sandboxed execution (complex, kept as Value).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<serde_json::Value>,

    /// Catch-all for extension fields.
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

/// Per-provider tool overrides.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ToolsByProviderConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profile: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub allow: Option<Vec<String>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub deny: Option<Vec<String>>,

    /// Catch-all for extension fields.
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}
