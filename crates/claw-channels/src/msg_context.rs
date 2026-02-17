//! MsgContext, ReplyPayload, and FinalizedMsgContext types.
//!
//! Ports OpenClaw's central message envelope from `src/auto-reply/templating.ts`
//! and reply payload from `src/auto-reply/types.ts`.
//!
//! MsgContext flows through the entire dispatch pipeline — from inbound
//! channel message to agent processing to outbound delivery.
//!
//! All fields use `Option<T>` because different channels populate different
//! subsets. Field names use PascalCase serde renames for exact wire
//! compatibility with the TypeScript implementation.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// CommandArgs
// ---------------------------------------------------------------------------

/// Parsed command arguments from inbound messages.
///
/// Maps to OpenClaw's `CommandArgs` from `commands-registry.types.ts`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CommandArgs {
    /// Raw argument string (everything after the command name).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub raw: Option<String>,

    /// Parsed argument values keyed by name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub values: Option<HashMap<String, serde_json::Value>>,
}

// ---------------------------------------------------------------------------
// StickerMetadata
// ---------------------------------------------------------------------------

/// Telegram sticker metadata (emoji, set name, file IDs, cached description).
///
/// Maps to OpenClaw's `StickerMetadata` from `telegram/bot/types.ts`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct StickerMetadata {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub emoji: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub set_name: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_id: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_unique_id: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

// ---------------------------------------------------------------------------
// MediaUnderstandingOutput / MediaUnderstandingDecision
// ---------------------------------------------------------------------------

/// Output from media understanding (image/audio analysis).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct MediaUnderstandingOutput {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub media_type: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_json::Value>,
}

/// Decision from the media understanding pipeline.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct MediaUnderstandingDecision {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub action: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_json::Value>,
}

// ---------------------------------------------------------------------------
// MsgContext
// ---------------------------------------------------------------------------

/// The unified message context envelope.
///
/// Carries all metadata about an inbound message through the dispatch
/// pipeline. Fields are optional because each channel populates only
/// the subset it supports.
///
/// Wire format uses PascalCase field names to match OpenClaw's TypeScript
/// `MsgContext` interface in `src/auto-reply/templating.ts`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct MsgContext {
    // -- Message body variants ----------------------------------------------

    /// The processed message body (may have commands stripped).
    #[serde(rename = "Body", skip_serializing_if = "Option::is_none")]
    pub body: Option<String>,

    /// Agent prompt body (may include envelope/history/context).
    /// Should use real newlines, not escaped `\\n`.
    #[serde(rename = "BodyForAgent", skip_serializing_if = "Option::is_none")]
    pub body_for_agent: Option<String>,

    /// Raw message body without structural context (history, sender labels).
    /// Legacy alias for CommandBody.
    #[serde(rename = "RawBody", skip_serializing_if = "Option::is_none")]
    pub raw_body: Option<String>,

    /// Preferred for command detection over RawBody.
    #[serde(rename = "CommandBody", skip_serializing_if = "Option::is_none")]
    pub command_body: Option<String>,

    /// Clean text for command parsing (no history/sender context).
    /// Prefer over CommandBody/RawBody when set.
    #[serde(rename = "BodyForCommands", skip_serializing_if = "Option::is_none")]
    pub body_for_commands: Option<String>,

    /// Parsed command arguments.
    #[serde(rename = "CommandArgs", skip_serializing_if = "Option::is_none")]
    pub command_args: Option<CommandArgs>,

    // -- Addressing ---------------------------------------------------------

    /// Sender identifier (platform-specific).
    #[serde(rename = "From", skip_serializing_if = "Option::is_none")]
    pub from: Option<String>,

    /// Recipient/destination identifier.
    #[serde(rename = "To", skip_serializing_if = "Option::is_none")]
    pub to: Option<String>,

    /// Session key for this conversation.
    #[serde(rename = "SessionKey", skip_serializing_if = "Option::is_none")]
    pub session_key: Option<String>,

    /// Provider account ID (multi-account support).
    #[serde(rename = "AccountId", skip_serializing_if = "Option::is_none")]
    pub account_id: Option<String>,

    /// Parent session key (for sub-sessions).
    #[serde(rename = "ParentSessionKey", skip_serializing_if = "Option::is_none")]
    pub parent_session_key: Option<String>,

    // -- Message IDs --------------------------------------------------------

    /// Provider-specific message SID (may be shortened).
    #[serde(rename = "MessageSid", skip_serializing_if = "Option::is_none")]
    pub message_sid: Option<String>,

    /// Full provider-specific message ID when MessageSid is a shortened alias.
    #[serde(rename = "MessageSidFull", skip_serializing_if = "Option::is_none")]
    pub message_sid_full: Option<String>,

    /// Array of message SIDs (for multi-message contexts).
    #[serde(rename = "MessageSids", skip_serializing_if = "Option::is_none")]
    pub message_sids: Option<Vec<String>>,

    /// First message SID in a sequence.
    #[serde(rename = "MessageSidFirst", skip_serializing_if = "Option::is_none")]
    pub message_sid_first: Option<String>,

    /// Last message SID in a sequence.
    #[serde(rename = "MessageSidLast", skip_serializing_if = "Option::is_none")]
    pub message_sid_last: Option<String>,

    // -- Reply context ------------------------------------------------------

    /// ID of the message being replied to.
    #[serde(rename = "ReplyToId", skip_serializing_if = "Option::is_none")]
    pub reply_to_id: Option<String>,

    /// Full provider-specific reply-to ID.
    #[serde(rename = "ReplyToIdFull", skip_serializing_if = "Option::is_none")]
    pub reply_to_id_full: Option<String>,

    /// Body of the message being replied to.
    #[serde(rename = "ReplyToBody", skip_serializing_if = "Option::is_none")]
    pub reply_to_body: Option<String>,

    /// Sender of the message being replied to.
    #[serde(rename = "ReplyToSender", skip_serializing_if = "Option::is_none")]
    pub reply_to_sender: Option<String>,

    /// Whether the reply is a quote-reply.
    #[serde(rename = "ReplyToIsQuote", skip_serializing_if = "Option::is_none")]
    pub reply_to_is_quote: Option<bool>,

    // -- Forwarded message context ------------------------------------------

    /// Original sender of a forwarded message.
    #[serde(rename = "ForwardedFrom", skip_serializing_if = "Option::is_none")]
    pub forwarded_from: Option<String>,

    #[serde(rename = "ForwardedFromType", skip_serializing_if = "Option::is_none")]
    pub forwarded_from_type: Option<String>,

    #[serde(rename = "ForwardedFromId", skip_serializing_if = "Option::is_none")]
    pub forwarded_from_id: Option<String>,

    #[serde(rename = "ForwardedFromUsername", skip_serializing_if = "Option::is_none")]
    pub forwarded_from_username: Option<String>,

    #[serde(rename = "ForwardedFromTitle", skip_serializing_if = "Option::is_none")]
    pub forwarded_from_title: Option<String>,

    #[serde(rename = "ForwardedFromSignature", skip_serializing_if = "Option::is_none")]
    pub forwarded_from_signature: Option<String>,

    #[serde(rename = "ForwardedFromChatType", skip_serializing_if = "Option::is_none")]
    pub forwarded_from_chat_type: Option<String>,

    #[serde(rename = "ForwardedFromMessageId", skip_serializing_if = "Option::is_none")]
    pub forwarded_from_message_id: Option<i64>,

    #[serde(rename = "ForwardedDate", skip_serializing_if = "Option::is_none")]
    pub forwarded_date: Option<i64>,

    // -- Thread context -----------------------------------------------------

    /// Body of the thread starter message.
    #[serde(rename = "ThreadStarterBody", skip_serializing_if = "Option::is_none")]
    pub thread_starter_body: Option<String>,

    /// Thread label/title.
    #[serde(rename = "ThreadLabel", skip_serializing_if = "Option::is_none")]
    pub thread_label: Option<String>,

    // -- Media --------------------------------------------------------------

    /// Local path to downloaded media file.
    #[serde(rename = "MediaPath", skip_serializing_if = "Option::is_none")]
    pub media_path: Option<String>,

    /// URL of the media attachment.
    #[serde(rename = "MediaUrl", skip_serializing_if = "Option::is_none")]
    pub media_url: Option<String>,

    /// MIME type of the media.
    #[serde(rename = "MediaType", skip_serializing_if = "Option::is_none")]
    pub media_type: Option<String>,

    /// Directory for media downloads.
    #[serde(rename = "MediaDir", skip_serializing_if = "Option::is_none")]
    pub media_dir: Option<String>,

    /// Multiple media file paths.
    #[serde(rename = "MediaPaths", skip_serializing_if = "Option::is_none")]
    pub media_paths: Option<Vec<String>>,

    /// Multiple media URLs.
    #[serde(rename = "MediaUrls", skip_serializing_if = "Option::is_none")]
    pub media_urls: Option<Vec<String>>,

    /// Multiple media MIME types.
    #[serde(rename = "MediaTypes", skip_serializing_if = "Option::is_none")]
    pub media_types: Option<Vec<String>>,

    /// Telegram sticker metadata.
    #[serde(rename = "Sticker", skip_serializing_if = "Option::is_none")]
    pub sticker: Option<StickerMetadata>,

    /// Output directory for generated files.
    #[serde(rename = "OutputDir", skip_serializing_if = "Option::is_none")]
    pub output_dir: Option<String>,

    /// Output base filename.
    #[serde(rename = "OutputBase", skip_serializing_if = "Option::is_none")]
    pub output_base: Option<String>,

    /// Remote host for SCP when media lives on a different machine.
    #[serde(rename = "MediaRemoteHost", skip_serializing_if = "Option::is_none")]
    pub media_remote_host: Option<String>,

    /// Transcription of audio/video media.
    #[serde(rename = "Transcript", skip_serializing_if = "Option::is_none")]
    pub transcript: Option<String>,

    /// Media understanding analysis outputs.
    #[serde(rename = "MediaUnderstanding", skip_serializing_if = "Option::is_none")]
    pub media_understanding: Option<Vec<MediaUnderstandingOutput>>,

    /// Media understanding pipeline decisions.
    #[serde(rename = "MediaUnderstandingDecisions", skip_serializing_if = "Option::is_none")]
    pub media_understanding_decisions: Option<Vec<MediaUnderstandingDecision>>,

    /// Extracted link understanding results.
    #[serde(rename = "LinkUnderstanding", skip_serializing_if = "Option::is_none")]
    pub link_understanding: Option<Vec<String>>,

    // -- Prompt / display ---------------------------------------------------

    /// System prompt override.
    #[serde(rename = "Prompt", skip_serializing_if = "Option::is_none")]
    pub prompt: Option<String>,

    /// Maximum characters for response.
    #[serde(rename = "MaxChars", skip_serializing_if = "Option::is_none")]
    pub max_chars: Option<u32>,

    /// Type of chat (direct, group, channel, thread) — kept as String
    /// for wire compatibility (TS uses a plain string).
    #[serde(rename = "ChatType", skip_serializing_if = "Option::is_none")]
    pub chat_type: Option<String>,

    /// Human label for conversation headers (not sender).
    #[serde(rename = "ConversationLabel", skip_serializing_if = "Option::is_none")]
    pub conversation_label: Option<String>,

    // -- Group context ------------------------------------------------------

    /// Group chat subject/title.
    #[serde(rename = "GroupSubject", skip_serializing_if = "Option::is_none")]
    pub group_subject: Option<String>,

    /// Group channel name (e.g. #general, #support).
    #[serde(rename = "GroupChannel", skip_serializing_if = "Option::is_none")]
    pub group_channel: Option<String>,

    /// Group workspace/space name.
    #[serde(rename = "GroupSpace", skip_serializing_if = "Option::is_none")]
    pub group_space: Option<String>,

    /// Comma-separated group member list.
    #[serde(rename = "GroupMembers", skip_serializing_if = "Option::is_none")]
    pub group_members: Option<String>,

    /// Group-specific system prompt addition.
    #[serde(rename = "GroupSystemPrompt", skip_serializing_if = "Option::is_none")]
    pub group_system_prompt: Option<String>,

    /// Untrusted metadata that must not be treated as system instructions.
    #[serde(rename = "UntrustedContext", skip_serializing_if = "Option::is_none")]
    pub untrusted_context: Option<Vec<String>>,

    // -- Sender identity ----------------------------------------------------

    /// Display name of the sender.
    #[serde(rename = "SenderName", skip_serializing_if = "Option::is_none")]
    pub sender_name: Option<String>,

    /// Platform user ID of the sender.
    #[serde(rename = "SenderId", skip_serializing_if = "Option::is_none")]
    pub sender_id: Option<String>,

    /// Username of the sender (e.g. @handle).
    #[serde(rename = "SenderUsername", skip_serializing_if = "Option::is_none")]
    pub sender_username: Option<String>,

    /// Tag for the sender (platform-specific label).
    #[serde(rename = "SenderTag", skip_serializing_if = "Option::is_none")]
    pub sender_tag: Option<String>,

    /// E.164 phone number of the sender.
    #[serde(rename = "SenderE164", skip_serializing_if = "Option::is_none")]
    pub sender_e164: Option<String>,

    // -- Timestamps & provider info -----------------------------------------

    /// Unix timestamp of the message.
    #[serde(rename = "Timestamp", skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<i64>,

    /// Provider label (e.g. "telegram", "whatsapp").
    #[serde(rename = "Provider", skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,

    /// Provider surface label. Prefer over `provider` when available.
    #[serde(rename = "Surface", skip_serializing_if = "Option::is_none")]
    pub surface: Option<String>,

    // -- Flags & authorization ----------------------------------------------

    /// Whether the bot was @mentioned in this message.
    #[serde(rename = "WasMentioned", skip_serializing_if = "Option::is_none")]
    pub was_mentioned: Option<bool>,

    /// Whether the sender is authorized to run commands.
    #[serde(rename = "CommandAuthorized", skip_serializing_if = "Option::is_none")]
    pub command_authorized: Option<bool>,

    /// Source of the command: "text" or "native".
    #[serde(rename = "CommandSource", skip_serializing_if = "Option::is_none")]
    pub command_source: Option<String>,

    /// Target session key for command routing.
    #[serde(rename = "CommandTargetSessionKey", skip_serializing_if = "Option::is_none")]
    pub command_target_session_key: Option<String>,

    /// Gateway client scopes when originating from the gateway.
    #[serde(rename = "GatewayClientScopes", skip_serializing_if = "Option::is_none")]
    pub gateway_client_scopes: Option<Vec<String>>,

    // -- Threading ----------------------------------------------------------

    /// Thread identifier (Telegram topic ID or Matrix thread event ID).
    /// Can be string or number in TS, so we use a JSON Value.
    #[serde(rename = "MessageThreadId", skip_serializing_if = "Option::is_none")]
    pub message_thread_id: Option<serde_json::Value>,

    /// Whether this is a Telegram forum supergroup.
    #[serde(rename = "IsForum", skip_serializing_if = "Option::is_none")]
    pub is_forum: Option<bool>,

    // -- Routing ------------------------------------------------------------

    /// Originating channel for reply routing.
    /// When set, replies should be routed back to this provider
    /// instead of using lastChannel from the session.
    #[serde(rename = "OriginatingChannel", skip_serializing_if = "Option::is_none")]
    pub originating_channel: Option<String>,

    /// Originating destination (chat/channel/user ID) for reply routing.
    #[serde(rename = "OriginatingTo", skip_serializing_if = "Option::is_none")]
    pub originating_to: Option<String>,

    /// Messages from hooks to include in the response.
    #[serde(rename = "HookMessages", skip_serializing_if = "Option::is_none")]
    pub hook_messages: Option<Vec<String>>,
}

// ---------------------------------------------------------------------------
// FinalizedMsgContext
// ---------------------------------------------------------------------------

/// A finalized MsgContext with guaranteed `command_authorized` value.
///
/// In OpenClaw's TypeScript this is:
/// ```text
/// Omit<MsgContext, "CommandAuthorized"> & { CommandAuthorized: boolean }
/// ```
///
/// We use `#[serde(flatten)]` on the inner MsgContext and override the
/// `CommandAuthorized` field so it serializes as a non-optional boolean.
/// Default-deny: `false` when unset.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FinalizedMsgContext {
    /// All MsgContext fields except CommandAuthorized.
    #[serde(flatten)]
    inner: MsgContext,

    /// Always set after finalization. Default-deny: false.
    #[serde(rename = "CommandAuthorized")]
    pub command_authorized: bool,
}

impl FinalizedMsgContext {
    /// Finalize a MsgContext, defaulting `command_authorized` to `false` if unset.
    pub fn from_msg_context(mut ctx: MsgContext) -> Self {
        let authorized = ctx.command_authorized.unwrap_or(false);
        // Clear the optional field so it doesn't conflict with our explicit bool.
        ctx.command_authorized = None;
        Self {
            inner: ctx,
            command_authorized: authorized,
        }
    }

    /// Access the underlying MsgContext fields.
    pub fn context(&self) -> &MsgContext {
        &self.inner
    }

    /// Consume and return the inner MsgContext (with command_authorized restored).
    pub fn into_msg_context(self) -> MsgContext {
        let mut ctx = self.inner;
        ctx.command_authorized = Some(self.command_authorized);
        ctx
    }
}

impl std::ops::Deref for FinalizedMsgContext {
    type Target = MsgContext;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

// ---------------------------------------------------------------------------
// ReplyPayload
// ---------------------------------------------------------------------------

/// Outbound reply payload sent back to the channel.
///
/// Matches OpenClaw's `ReplyPayload` type from `src/auto-reply/types.ts`.
/// Uses camelCase field names for wire compatibility.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ReplyPayload {
    /// Reply text content.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,

    /// Single media URL attachment.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub media_url: Option<String>,

    /// Multiple media URL attachments.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub media_urls: Option<Vec<String>>,

    /// Message ID to reply to.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reply_to_id: Option<String>,

    /// Whether to tag the reply target.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reply_to_tag: Option<bool>,

    /// True when `[[reply_to_current]]` was present but not yet mapped to a message ID.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reply_to_current: Option<bool>,

    /// Send audio as voice message (bubble) instead of audio file.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub audio_as_voice: Option<bool>,

    /// Whether this reply represents an error message.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_error: Option<bool>,

    /// Channel-specific payload data (per-channel envelope).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub channel_data: Option<HashMap<String, serde_json::Value>>,
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn msg_context_default_serializes_to_empty() {
        let ctx = MsgContext::default();
        let json = serde_json::to_value(&ctx).unwrap();
        assert_eq!(json, serde_json::json!({}));
    }

    #[test]
    fn msg_context_pascal_case_field_names() {
        let ctx = MsgContext {
            body: Some("hello".into()),
            from: Some("+1234".into()),
            session_key: Some("agent:gpt:telegram:main".into()),
            chat_type: Some("direct".into()),
            was_mentioned: Some(true),
            ..Default::default()
        };
        let json = serde_json::to_value(&ctx).unwrap();
        let obj = json.as_object().unwrap();

        assert_eq!(json["Body"], "hello");
        assert_eq!(json["From"], "+1234");
        assert_eq!(json["SessionKey"], "agent:gpt:telegram:main");
        assert_eq!(json["ChatType"], "direct");
        assert_eq!(json["WasMentioned"], true);

        // snake_case keys should NOT appear
        assert!(!obj.contains_key("body"));
        assert!(!obj.contains_key("from"));
        assert!(!obj.contains_key("chat_type"));
    }

    #[test]
    fn msg_context_deserialize_from_pascal_case_json() {
        let json = r#"{
            "Body": "hello world",
            "From": "user-1",
            "ChatType": "group",
            "WasMentioned": true,
            "Timestamp": 1700000000,
            "MediaUrls": ["https://example.com/img.png"]
        }"#;
        let ctx: MsgContext = serde_json::from_str(json).unwrap();
        assert_eq!(ctx.body.as_deref(), Some("hello world"));
        assert_eq!(ctx.from.as_deref(), Some("user-1"));
        assert_eq!(ctx.chat_type.as_deref(), Some("group"));
        assert_eq!(ctx.was_mentioned, Some(true));
        assert_eq!(ctx.timestamp, Some(1700000000));
        assert_eq!(ctx.media_urls.as_ref().unwrap().len(), 1);
    }

    #[test]
    fn msg_context_serde_roundtrip() {
        let ctx = MsgContext {
            body: Some("test message".into()),
            to: Some("bot-123".into()),
            account_id: Some("acc-1".into()),
            message_sid: Some("msg-42".into()),
            media_urls: Some(vec!["https://example.com/a.jpg".into()]),
            sender_name: Some("Alice".into()),
            sender_id: Some("user-1".into()),
            timestamp: Some(1700000000),
            provider: Some("telegram".into()),
            command_authorized: Some(false),
            ..Default::default()
        };
        let json_str = serde_json::to_string(&ctx).unwrap();
        let parsed: MsgContext = serde_json::from_str(&json_str).unwrap();
        assert_eq!(parsed.body, ctx.body);
        assert_eq!(parsed.to, ctx.to);
        assert_eq!(parsed.account_id, ctx.account_id);
        assert_eq!(parsed.media_urls, ctx.media_urls);
        assert_eq!(parsed.timestamp, ctx.timestamp);
    }

    #[test]
    fn msg_context_ignores_unknown_fields() {
        let json = r#"{"Body": "x", "UnknownField": 42}"#;
        let ctx: MsgContext = serde_json::from_str(json).unwrap();
        assert_eq!(ctx.body.as_deref(), Some("x"));
    }

    #[test]
    fn msg_context_forwarded_fields() {
        let ctx = MsgContext {
            forwarded_from: Some("Alice".into()),
            forwarded_from_type: Some("user".into()),
            forwarded_from_message_id: Some(42),
            forwarded_date: Some(1700000000),
            ..Default::default()
        };
        let json = serde_json::to_value(&ctx).unwrap();
        assert_eq!(json["ForwardedFrom"], "Alice");
        assert_eq!(json["ForwardedFromType"], "user");
        assert_eq!(json["ForwardedFromMessageId"], 42);
        assert_eq!(json["ForwardedDate"], 1700000000);
    }

    #[test]
    fn msg_context_media_fields() {
        let ctx = MsgContext {
            media_path: Some("/tmp/img.jpg".into()),
            media_url: Some("https://cdn.example.com/img.jpg".into()),
            media_type: Some("image/jpeg".into()),
            sticker: Some(StickerMetadata {
                emoji: Some("\u{1f600}".into()),
                set_name: Some("HappyStickers".into()),
                ..Default::default()
            }),
            ..Default::default()
        };
        let json = serde_json::to_value(&ctx).unwrap();
        assert_eq!(json["MediaPath"], "/tmp/img.jpg");
        assert_eq!(json["Sticker"]["emoji"], "\u{1f600}");
        assert_eq!(json["Sticker"]["setName"], "HappyStickers");
    }

    #[test]
    fn msg_context_command_args_struct() {
        let ctx = MsgContext {
            command_args: Some(CommandArgs {
                raw: Some("/reset gpt-4".into()),
                values: Some(HashMap::from([
                    ("model".into(), serde_json::json!("gpt-4")),
                ])),
            }),
            ..Default::default()
        };
        let json = serde_json::to_value(&ctx).unwrap();
        assert_eq!(json["CommandArgs"]["raw"], "/reset gpt-4");
        assert_eq!(json["CommandArgs"]["values"]["model"], "gpt-4");
    }

    #[test]
    fn msg_context_body_variants() {
        let ctx = MsgContext {
            body: Some("hello".into()),
            body_for_agent: Some("context: hello".into()),
            raw_body: Some("hello".into()),
            command_body: Some("hello".into()),
            body_for_commands: Some("hello".into()),
            ..Default::default()
        };
        let json = serde_json::to_value(&ctx).unwrap();
        assert_eq!(json["Body"], "hello");
        assert_eq!(json["BodyForAgent"], "context: hello");
        assert_eq!(json["RawBody"], "hello");
        assert_eq!(json["CommandBody"], "hello");
        assert_eq!(json["BodyForCommands"], "hello");
    }

    #[test]
    fn finalized_msg_context_defaults_to_false() {
        let ctx = MsgContext::default();
        let finalized = FinalizedMsgContext::from_msg_context(ctx);
        assert!(!finalized.command_authorized);
    }

    #[test]
    fn finalized_msg_context_preserves_true() {
        let ctx = MsgContext {
            command_authorized: Some(true),
            body: Some("cmd".into()),
            ..Default::default()
        };
        let finalized = FinalizedMsgContext::from_msg_context(ctx);
        assert!(finalized.command_authorized);
        assert_eq!(finalized.context().body.as_deref(), Some("cmd"));
    }

    #[test]
    fn finalized_msg_context_serde_roundtrip() {
        let ctx = MsgContext {
            body: Some("test".into()),
            command_authorized: Some(true),
            ..Default::default()
        };
        let finalized = FinalizedMsgContext::from_msg_context(ctx);
        let json = serde_json::to_value(&finalized).unwrap();
        assert_eq!(json["CommandAuthorized"], true);
        assert_eq!(json["Body"], "test");
    }

    #[test]
    fn finalized_into_msg_context_roundtrip() {
        let original = MsgContext {
            body: Some("round-trip".into()),
            command_authorized: Some(true),
            sender_name: Some("Bob".into()),
            ..Default::default()
        };
        let finalized = FinalizedMsgContext::from_msg_context(original);
        let restored = finalized.into_msg_context();
        assert_eq!(restored.body.as_deref(), Some("round-trip"));
        assert_eq!(restored.command_authorized, Some(true));
        assert_eq!(restored.sender_name.as_deref(), Some("Bob"));
    }

    #[test]
    fn reply_payload_camel_case_fields() {
        let payload = ReplyPayload {
            text: Some("reply text".into()),
            media_url: Some("https://example.com/img.png".into()),
            reply_to_id: Some("msg-1".into()),
            reply_to_tag: Some(true),
            audio_as_voice: Some(false),
            is_error: Some(false),
            ..Default::default()
        };
        let json = serde_json::to_value(&payload).unwrap();
        assert_eq!(json["text"], "reply text");
        assert_eq!(json["mediaUrl"], "https://example.com/img.png");
        assert_eq!(json["replyToId"], "msg-1");
        assert_eq!(json["replyToTag"], true);
        assert_eq!(json["audioAsVoice"], false);
        assert_eq!(json["isError"], false);
    }

    #[test]
    fn reply_payload_serde_roundtrip() {
        let payload = ReplyPayload {
            text: Some("hello".into()),
            media_urls: Some(vec!["a.jpg".into(), "b.jpg".into()]),
            reply_to_current: Some(true),
            ..Default::default()
        };
        let json_str = serde_json::to_string(&payload).unwrap();
        let parsed: ReplyPayload = serde_json::from_str(&json_str).unwrap();
        assert_eq!(parsed.text, payload.text);
        assert_eq!(parsed.media_urls, payload.media_urls);
        assert_eq!(parsed.reply_to_current, Some(true));
    }

    #[test]
    fn reply_payload_default_is_empty() {
        let payload = ReplyPayload::default();
        let json = serde_json::to_value(&payload).unwrap();
        assert_eq!(json, serde_json::json!({}));
    }

    #[test]
    fn reply_payload_channel_data() {
        let mut channel_data = HashMap::new();
        channel_data.insert("telegram".into(), serde_json::json!({"parseMode": "HTML"}));
        let payload = ReplyPayload {
            text: Some("hi".into()),
            channel_data: Some(channel_data),
            ..Default::default()
        };
        let json = serde_json::to_value(&payload).unwrap();
        assert_eq!(json["channelData"]["telegram"]["parseMode"], "HTML");
    }
}
