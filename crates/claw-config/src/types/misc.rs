//! Miscellaneous configuration sections (browser, hooks, cron, logging, etc).

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use super::helpers::ToggleConfig;

// ---------------------------------------------------------------------------
// Browser
// ---------------------------------------------------------------------------

/// Headless browser configuration for web tools and canvas.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct BrowserConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub evaluate_enabled: Option<bool>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_profile: Option<String>,

    /// Browser profiles (complex, kept as Value).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profiles: Option<serde_json::Value>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub headless: Option<bool>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub no_sandbox: Option<bool>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub executable_path: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub attach_only: Option<bool>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,

    /// Catch-all for extension fields.
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

// ---------------------------------------------------------------------------
// Hooks
// ---------------------------------------------------------------------------

/// Webhook/event hooks configuration.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct HooksConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub token: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_body_bytes: Option<u64>,

    /// Hook event->action mappings (complex, kept as Value).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mappings: Option<serde_json::Value>,

    /// Gmail integration (complex, kept as Value).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gmail: Option<serde_json::Value>,

    /// Catch-all for extension fields.
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

// ---------------------------------------------------------------------------
// Cron
// ---------------------------------------------------------------------------

/// Scheduled task (cron) configuration.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct CronConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_concurrent_runs: Option<u32>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub webhook: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub webhook_token: Option<String>,

    /// Duration string for session data retention (e.g. "7d").
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_retention: Option<String>,

    /// Catch-all for extension fields.
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

// ---------------------------------------------------------------------------
// Discovery
// ---------------------------------------------------------------------------

/// Service discovery configuration.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct DiscoveryConfig {
    /// mDNS discovery settings (complex, kept as Value).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mdns: Option<serde_json::Value>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub wide_area: Option<ToggleConfig>,

    /// Catch-all for extension fields.
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

// ---------------------------------------------------------------------------
// Talk (voice)
// ---------------------------------------------------------------------------

/// Voice interaction (talk) configuration.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct TalkConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub voice_id: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_id: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_format: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub interrupt_on_speech: Option<bool>,

    /// Catch-all for extension fields.
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

// ---------------------------------------------------------------------------
// Logging
// ---------------------------------------------------------------------------

/// Logging configuration.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct LoggingConfig {
    /// Log level: "trace", "debug", "info", "warn", "error".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub level: Option<String>,

    /// Log file path.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,

    /// Console log level override.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub console_level: Option<String>,

    /// Console output style: "json", "pretty", "compact".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub console_style: Option<String>,

    /// Redact sensitive values in logs.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub redact_sensitive: Option<bool>,

    /// Regex patterns to redact from log output.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub redact_patterns: Option<Vec<String>>,

    /// Catch-all for extension fields.
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

// ---------------------------------------------------------------------------
// Auth (top-level, distinct from gateway auth)
// ---------------------------------------------------------------------------

/// Top-level authentication profiles.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct AuthConfig {
    /// Named auth profiles (complex, kept as Value).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profiles: Option<serde_json::Value>,

    /// Profile evaluation order (complex, kept as Value).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub order: Option<serde_json::Value>,

    /// Catch-all for extension fields.
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

// ---------------------------------------------------------------------------
// UI
// ---------------------------------------------------------------------------

/// Control UI appearance configuration.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct UiConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seam_color: Option<String>,

    /// Assistant panel settings (complex, kept as Value).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub assistant: Option<serde_json::Value>,

    /// Catch-all for extension fields.
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

// ---------------------------------------------------------------------------
// Canvas host
// ---------------------------------------------------------------------------

/// Canvas hosting configuration.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct CanvasHostConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub root: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub live_reload: Option<bool>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,

    /// Catch-all for extension fields.
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

// ---------------------------------------------------------------------------
// Web channel
// ---------------------------------------------------------------------------

/// Web channel (browser-based chat) configuration.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct WebChannelConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub heartbeat_seconds: Option<u32>,

    /// Reconnection settings (complex, kept as Value).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reconnect: Option<serde_json::Value>,

    /// Catch-all for extension fields.
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}
