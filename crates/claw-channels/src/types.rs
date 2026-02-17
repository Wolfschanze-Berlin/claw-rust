//! Channel system data types.
//!
//! Defines ChatType, ChannelMeta, ChannelCapabilities, context structs,
//! and delivery result types used by the channel plugin system.

use serde::{Deserialize, Serialize};
use tokio_util::sync::CancellationToken;

use claw_config::ChannelConfig;
use claw_core::RuntimeEnv;

// ---------------------------------------------------------------------------
// ChatType
// ---------------------------------------------------------------------------

/// The type of conversation context for a message.
///
/// Maps to OpenClaw's discriminated `ChatType` union. Used for routing
/// decisions and determining which adapter capabilities apply.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ChatType {
    /// One-to-one direct/private message.
    Direct,
    /// Group conversation with multiple participants.
    Group,
    /// Broadcast channel (one-to-many, read-only for most participants).
    Channel,
    /// Threaded reply within a conversation.
    Thread,
}

impl Default for ChatType {
    fn default() -> Self {
        Self::Direct
    }
}

impl std::fmt::Display for ChatType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Direct => write!(f, "direct"),
            Self::Group => write!(f, "group"),
            Self::Channel => write!(f, "channel"),
            Self::Thread => write!(f, "thread"),
        }
    }
}

// ---------------------------------------------------------------------------
// ChannelMeta
// ---------------------------------------------------------------------------

/// Static metadata describing a channel plugin.
///
/// Populated once during plugin registration and used for discovery,
/// UI labeling, and documentation references.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChannelMeta {
    /// Unique identifier for this channel (e.g. "telegram", "discord").
    pub id: String,

    /// Human-readable display label.
    pub label: String,

    /// Short label shown in selection UIs.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub selection_label: Option<String>,

    /// Path to documentation for this channel.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub docs_path: Option<String>,

    /// Short description / tagline.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blurb: Option<String>,

    /// Sort order for UI display.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub order: Option<i32>,

    /// Alternative names that can also match this channel.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub aliases: Option<Vec<String>>,
}

// ---------------------------------------------------------------------------
// ChannelCapabilities
// ---------------------------------------------------------------------------

/// Declares which features a channel plugin supports.
///
/// All fields are optional booleans — `None` means the channel hasn't
/// declared support (treated as unsupported). This lets each platform
/// advertise only what it actually implements.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ChannelCapabilities {
    /// Which conversation types this channel supports.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chat_types: Option<Vec<ChatType>>,

    /// Supports creating/managing polls.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub polls: Option<bool>,

    /// Supports emoji reactions on messages.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reactions: Option<bool>,

    /// Supports editing sent messages.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub edit: Option<bool>,

    /// Supports deleting/unsending messages.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unsend: Option<bool>,

    /// Supports replying to specific messages.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reply: Option<bool>,

    /// Supports message effects/animations.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub effects: Option<bool>,

    /// Supports group management (admin, kick, ban).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub group_management: Option<bool>,

    /// Supports threaded conversations.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub threads: Option<bool>,

    /// Supports media file upload/download.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub media: Option<bool>,

    /// Supports native slash commands.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub native_commands: Option<bool>,

    /// Whether streaming should be blocked for this channel.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub block_streaming: Option<bool>,
}

// ---------------------------------------------------------------------------
// Outbound delivery mode
// ---------------------------------------------------------------------------

/// How a channel delivers outbound messages.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DeliveryMode {
    /// Send the complete message once.
    Single,
    /// Stream the message incrementally (edit-in-place).
    Stream,
}

impl Default for DeliveryMode {
    fn default() -> Self {
        Self::Single
    }
}

// ---------------------------------------------------------------------------
// OutboundDeliveryResult
// ---------------------------------------------------------------------------

/// Result returned from an outbound message delivery attempt.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct OutboundDeliveryResult {
    /// Whether delivery succeeded.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub success: Option<bool>,

    /// Platform-assigned message ID for the delivered message.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message_id: Option<String>,

    /// Error message if delivery failed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,

    /// Platform-specific metadata about the delivery.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_json::Value>,
}

// ---------------------------------------------------------------------------
// ChannelGatewayContext
// ---------------------------------------------------------------------------

/// Context provided to a channel's gateway adapter when starting polling.
///
/// Carries the runtime environment, account identity, configuration snapshot,
/// and a cancellation token for cooperative shutdown.
#[derive(Clone)]
pub struct ChannelGatewayContext {
    /// The account ID being started (channels may have multiple accounts).
    pub account_id: String,

    /// Per-account configuration snapshot.
    pub account_config: serde_json::Value,

    /// Channel-level configuration.
    pub channel_config: ChannelConfig,

    /// Shared runtime environment (env vars, tracing, etc).
    pub runtime: RuntimeEnv,

    /// Cancellation token — cancel to signal this account should stop polling.
    pub cancel: CancellationToken,
}

impl std::fmt::Debug for ChannelGatewayContext {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ChannelGatewayContext")
            .field("account_id", &self.account_id)
            .finish_non_exhaustive()
    }
}

// ---------------------------------------------------------------------------
// ChannelOutboundContext
// ---------------------------------------------------------------------------

/// Context provided when sending an outbound message through a channel.
#[derive(Debug, Clone)]
pub struct ChannelOutboundContext {
    /// The account ID to send from.
    pub account_id: String,

    /// Target chat/conversation ID.
    pub chat_id: String,

    /// Optional thread ID for threaded replies.
    pub thread_id: Option<String>,

    /// Optional message ID to reply to.
    pub reply_to_message_id: Option<String>,

    /// Cancellation token for this delivery attempt.
    pub cancel: CancellationToken,
}

// ---------------------------------------------------------------------------
// ChannelError
// ---------------------------------------------------------------------------

/// Errors specific to the channel system.
#[derive(Debug, thiserror::Error)]
pub enum ChannelError {
    #[error("channel not found: {0}")]
    NotFound(String),

    #[error("channel not enabled: {0}")]
    NotEnabled(String),

    #[error("adapter not supported: {adapter} on channel {channel}")]
    AdapterNotSupported { channel: String, adapter: String },

    #[error("account not found: {account_id} on channel {channel}")]
    AccountNotFound { channel: String, account_id: String },

    #[error("delivery failed: {0}")]
    DeliveryFailed(String),

    #[error("gateway error: {0}")]
    GatewayError(String),

    #[error("configuration error: {0}")]
    ConfigError(String),

    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chat_type_serde_roundtrip() {
        let ct = ChatType::Group;
        let json = serde_json::to_string(&ct).unwrap();
        assert_eq!(json, r#""group""#);
        let parsed: ChatType = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, ct);
    }

    #[test]
    fn chat_type_display() {
        assert_eq!(ChatType::Direct.to_string(), "direct");
        assert_eq!(ChatType::Thread.to_string(), "thread");
    }

    #[test]
    fn capabilities_default_is_all_none() {
        let caps = ChannelCapabilities::default();
        assert!(caps.polls.is_none());
        assert!(caps.reactions.is_none());
        assert!(caps.chat_types.is_none());
    }

    #[test]
    fn capabilities_serde_omits_none() {
        let caps = ChannelCapabilities {
            reactions: Some(true),
            ..Default::default()
        };
        let json = serde_json::to_value(&caps).unwrap();
        let obj = json.as_object().unwrap();
        assert!(obj.contains_key("reactions"));
        assert!(!obj.contains_key("polls"));
        assert!(!obj.contains_key("chatTypes"));
    }

    #[test]
    fn delivery_result_serde() {
        let result = OutboundDeliveryResult {
            success: Some(true),
            message_id: Some("msg123".into()),
            ..Default::default()
        };
        let json = serde_json::to_value(&result).unwrap();
        assert_eq!(json["success"], true);
        assert_eq!(json["messageId"], "msg123");
    }

    #[test]
    fn channel_meta_serde() {
        let meta = ChannelMeta {
            id: "telegram".into(),
            label: "Telegram".into(),
            selection_label: Some("TG".into()),
            docs_path: None,
            blurb: Some("Telegram Bot API".into()),
            order: Some(1),
            aliases: Some(vec!["tg".into()]),
        };
        let json = serde_json::to_value(&meta).unwrap();
        assert_eq!(json["id"], "telegram");
        assert_eq!(json["selectionLabel"], "TG");
        assert_eq!(json["aliases"][0], "tg");
    }

    #[test]
    fn delivery_mode_serde() {
        let mode = DeliveryMode::Stream;
        let json = serde_json::to_string(&mode).unwrap();
        assert_eq!(json, r#""stream""#);
    }

    #[test]
    fn channel_error_display() {
        let err = ChannelError::NotFound("slack".into());
        assert_eq!(err.to_string(), "channel not found: slack");

        let err = ChannelError::AdapterNotSupported {
            channel: "discord".into(),
            adapter: "threading".into(),
        };
        assert_eq!(
            err.to_string(),
            "adapter not supported: threading on channel discord"
        );
    }
}
