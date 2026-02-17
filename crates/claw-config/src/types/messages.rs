//! Message configuration types.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Message queue and acknowledgement settings.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct MessagesConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub queue: Option<MessageQueueConfig>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub ack_reaction_scope: Option<String>,

    // --- New fields from OpenClaw config reference ---

    /// Prefix prepended to all bot responses.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_prefix: Option<String>,

    /// Emoji reaction for message acknowledgement.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ack_reaction: Option<String>,

    /// Remove the ack reaction after the bot replies.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remove_ack_after_reply: Option<bool>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub inbound: Option<InboundConfig>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub group_chat: Option<GroupChatConfig>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub tts: Option<TtsConfig>,

    /// Catch-all for extension fields.
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

/// Message queue mode settings.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct MessageQueueConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mode: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub by_channel: Option<HashMap<String, String>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub debounce_ms: Option<u32>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub cap: Option<u32>,

    /// Drop policy: "oldest" or "newest".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub drop: Option<String>,

    /// Catch-all for extension fields.
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

/// Inbound message processing configuration.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct InboundConfig {
    /// Debounce window in ms for rapid inbound messages.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub debounce_ms: Option<u32>,

    /// Per-channel inbound overrides.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub by_channel: Option<HashMap<String, serde_json::Value>>,

    /// Catch-all for extension fields.
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

/// Group chat message handling.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct GroupChatConfig {
    /// Max history messages to include in group context.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub history_limit: Option<u32>,

    /// Catch-all for extension fields.
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

/// Text-to-speech configuration.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct TtsConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auto: Option<bool>,

    /// Mode: "full", "summary".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mode: Option<String>,

    /// Provider: "elevenlabs", "openai".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary_model: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_text_length: Option<u32>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout_ms: Option<u32>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub prefs_path: Option<String>,

    /// ElevenLabs provider config (complex, kept as Value).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub elevenlabs: Option<serde_json::Value>,

    /// OpenAI TTS provider config (complex, kept as Value).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub openai: Option<serde_json::Value>,

    /// Catch-all for extension fields.
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}
