//! Session management configuration types.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Session management configuration.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct SessionConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ttl: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_history: Option<u32>,

    /// Session scope: "per-sender" etc.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scope: Option<String>,

    /// DM session scope: "main", "per-peer", "per-channel-peer", "per-account-channel-peer".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dm_scope: Option<String>,

    /// Identity links: canonical -> [aliases] mapping.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub identity_links: Option<serde_json::Value>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub reset: Option<SessionResetConfig>,

    /// Per-chat-type reset overrides (keyed by type name).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reset_by_type: Option<serde_json::Value>,

    /// Reset trigger phrases.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reset_triggers: Option<Vec<String>>,

    /// Session store backend identifier.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub store: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub maintenance: Option<SessionMaintenanceConfig>,

    /// Custom main session key template.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub main_key: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_to_agent: Option<SessionAgentToAgentConfig>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub send_policy: Option<SendPolicyConfig>,

    /// Catch-all for extension fields.
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

/// Session reset configuration.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct SessionResetConfig {
    /// Reset mode: "daily", "idle".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mode: Option<String>,

    /// Hour of day for daily reset (0-23).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub at_hour: Option<u32>,

    /// Idle timeout in minutes before reset.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub idle_minutes: Option<u32>,

    /// Catch-all for extension fields.
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

/// Session maintenance (pruning, rotation) configuration.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct SessionMaintenanceConfig {
    /// Mode: "warn" or "enforce".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mode: Option<String>,

    /// Duration string after which sessions are pruned (e.g. "30d").
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prune_after: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_entries: Option<u32>,

    /// Byte threshold string for rotation (e.g. "10MB").
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rotate_bytes: Option<String>,

    /// Catch-all for extension fields.
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

/// Agent-to-agent session configuration.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct SessionAgentToAgentConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_ping_pong_turns: Option<u32>,

    /// Catch-all for extension fields.
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

/// Message send policy configuration.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct SendPolicyConfig {
    /// Policy rules (complex structure, kept as Value).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rules: Option<serde_json::Value>,

    /// Default policy action: "allow" or "deny".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default: Option<String>,

    /// Catch-all for extension fields.
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}
