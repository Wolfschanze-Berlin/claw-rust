//! Channel configuration types (shared + platform-specific).

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

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

    /// Directory for storing downloaded files.
    /// Default: system temp dir + "/claw-files/".
    /// Files are stored as `{file_storage_dir}/{account_id}/{chat_id}/{filename}`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_storage_dir: Option<String>,

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
