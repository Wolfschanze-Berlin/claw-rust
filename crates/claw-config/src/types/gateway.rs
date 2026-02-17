//! Gateway server configuration types.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

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

    /// Deprecated flat boolean — prefer `control_ui.enabled`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub control_ui_enabled: Option<bool>,

    /// Nested control UI configuration (overrides `control_ui_enabled`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub control_ui: Option<ControlUiConfig>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub auth: Option<GatewayAuthConfig>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub tailscale: Option<TailscaleConfig>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub remote: Option<RemoteConfig>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub trusted_proxies: Option<Vec<String>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<GatewayToolsConfig>,

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
    /// Auto-detect the best interface.
    Auto,
    /// Bind to localhost only (127.0.0.1).
    Localhost,
    /// Bind to loopback (alias for localhost).
    Loopback,
    /// Bind to LAN interfaces.
    Lan,
    /// Bind to Tailscale tailnet interface.
    Tailnet,
    /// Bind to all interfaces (0.0.0.0).
    All,
    /// Custom bind address (specified elsewhere).
    Custom,
}

/// Gateway authentication configuration.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct GatewayAuthConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mode: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub token: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub password: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub trusted_proxy: Option<TrustedProxyConfig>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub allow_tailscale: Option<bool>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub rate_limit: Option<RateLimitConfig>,

    /// Catch-all for extension fields.
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

// ---------------------------------------------------------------------------
// Gateway sub-configs
// ---------------------------------------------------------------------------

/// Control UI configuration.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ControlUiConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_path: Option<String>,

    /// Catch-all for extension fields.
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

/// Tailscale integration configuration.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct TailscaleConfig {
    /// Mode: "off", "serve", or "funnel".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mode: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub reset_on_exit: Option<bool>,

    /// Catch-all for extension fields.
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

/// Remote gateway connection configuration.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct RemoteConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,

    /// Transport: "ssh" or "direct".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transport: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub token: Option<String>,

    /// Catch-all for extension fields.
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

/// Gateway-level tool allow/deny lists.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct GatewayToolsConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allow: Option<Vec<String>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub deny: Option<Vec<String>>,

    /// Catch-all for extension fields.
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

/// Trusted proxy authentication settings.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct TrustedProxyConfig {
    /// Header name containing the authenticated user identity.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_header: Option<String>,

    /// Catch-all for extension fields.
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

/// Authentication rate limiting configuration.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct RateLimitConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_attempts: Option<u32>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub window_ms: Option<u64>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub lockout_ms: Option<u64>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub exempt_loopback: Option<bool>,

    /// Catch-all for extension fields.
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}
