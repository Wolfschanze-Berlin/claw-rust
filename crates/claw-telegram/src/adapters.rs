//! Telegram adapter trait implementations.
//!
//! Thin stubs for MentionAdapter, CommandAdapter, MessageActionAdapter,
//! and StreamingAdapter. The actual Telegram API calls are TODO — these
//! stubs establish the wiring so the plugin advertises its capabilities.

use async_trait::async_trait;
use serde_json::Value;
use tracing::info;

use claw_channels::plugin::{
    ChannelCommandAdapter, ChannelMentionAdapter, ChannelMessageActionAdapter,
    ChannelStreamingAdapter,
};
use claw_channels::types::{ChannelError, ChannelOutboundContext, OutboundDeliveryResult};

// ---------------------------------------------------------------------------
// MentionAdapter
// ---------------------------------------------------------------------------

/// Parses and formats Telegram @username mentions.
pub struct TelegramMentionAdapter;

impl TelegramMentionAdapter {
    pub fn new() -> Self {
        Self
    }
}

impl ChannelMentionAdapter for TelegramMentionAdapter {
    fn parse_mentions(&self, text: &str) -> Vec<String> {
        // Telegram mentions are @username — extract them with a simple scan.
        text.split_whitespace()
            .filter_map(|word| {
                let trimmed = word.trim_matches(|c: char| !c.is_alphanumeric() && c != '@' && c != '_');
                if trimmed.starts_with('@') && trimmed.len() > 1 {
                    Some(trimmed.to_string())
                } else {
                    None
                }
            })
            .collect()
    }

    fn format_mention(&self, user_id: &str) -> String {
        // Telegram supports both @username and tg://user?id=<id> deep links.
        // For simplicity, use the text-based @mention when we have a username.
        if user_id.starts_with('@') {
            user_id.to_string()
        } else {
            format!("@{user_id}")
        }
    }
}

// ---------------------------------------------------------------------------
// CommandAdapter
// ---------------------------------------------------------------------------

/// Registers and manages Telegram Bot Commands (slash commands like /start).
pub struct TelegramCommandAdapter;

impl TelegramCommandAdapter {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl ChannelCommandAdapter for TelegramCommandAdapter {
    async fn register_commands(
        &self,
        account_id: &str,
        commands: &[Value],
    ) -> Result<(), ChannelError> {
        info!(
            account_id,
            command_count = commands.len(),
            "registering telegram bot commands"
        );
        // TODO: call teloxide Bot::set_my_commands with BotCommand list.
        Ok(())
    }

    async fn unregister_commands(&self, account_id: &str) -> Result<(), ChannelError> {
        info!(account_id, "clearing telegram bot commands");
        // TODO: call teloxide Bot::delete_my_commands.
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// MessageActionAdapter
// ---------------------------------------------------------------------------

/// Sends messages with Telegram inline keyboards (action buttons).
pub struct TelegramMessageActionAdapter;

impl TelegramMessageActionAdapter {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl ChannelMessageActionAdapter for TelegramMessageActionAdapter {
    async fn send_with_actions(
        &self,
        ctx: &ChannelOutboundContext,
        text: &str,
        actions: &[Value],
    ) -> Result<OutboundDeliveryResult, ChannelError> {
        info!(
            account_id = %ctx.account_id,
            chat_id = %ctx.chat_id,
            action_count = actions.len(),
            "sending telegram message with inline keyboard"
        );
        // TODO: build InlineKeyboardMarkup from actions JSON and send via
        // teloxide Bot::send_message(...).reply_markup(keyboard).
        let _ = text;
        Ok(OutboundDeliveryResult {
            success: Some(true),
            message_id: None,
            error: None,
            metadata: None,
        })
    }

    async fn handle_action_callback(
        &self,
        account_id: &str,
        callback_data: &Value,
    ) -> Result<(), ChannelError> {
        info!(
            account_id,
            "handling telegram callback query"
        );
        // TODO: call teloxide Bot::answer_callback_query.
        let _ = callback_data;
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// StreamingAdapter
// ---------------------------------------------------------------------------

/// Streams messages via Telegram edit-in-place (progressive message updates).
pub struct TelegramStreamingAdapter;

impl TelegramStreamingAdapter {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl ChannelStreamingAdapter for TelegramStreamingAdapter {
    async fn stream_start(
        &self,
        ctx: &ChannelOutboundContext,
        initial_text: &str,
    ) -> Result<String, ChannelError> {
        info!(
            account_id = %ctx.account_id,
            chat_id = %ctx.chat_id,
            "starting telegram streaming message"
        );
        // TODO: send initial message via teloxide, return message_id.
        let _ = initial_text;
        Ok("placeholder_msg_id".into())
    }

    async fn stream_update(
        &self,
        ctx: &ChannelOutboundContext,
        message_id: &str,
        text: &str,
    ) -> Result<(), ChannelError> {
        info!(
            account_id = %ctx.account_id,
            chat_id = %ctx.chat_id,
            message_id,
            text_len = text.len(),
            "updating telegram streaming message"
        );
        // TODO: call teloxide Bot::edit_message_text.
        Ok(())
    }

    async fn stream_end(
        &self,
        ctx: &ChannelOutboundContext,
        message_id: &str,
        final_text: &str,
    ) -> Result<(), ChannelError> {
        info!(
            account_id = %ctx.account_id,
            chat_id = %ctx.chat_id,
            message_id,
            "finalizing telegram streaming message"
        );
        // Final edit to set the completed text.
        // TODO: call teloxide Bot::edit_message_text with final content.
        let _ = final_text;
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_mentions_basic() {
        let adapter = TelegramMentionAdapter::new();
        let mentions = adapter.parse_mentions("Hello @alice and @bob_123!");
        assert_eq!(mentions, vec!["@alice", "@bob_123"]);
    }

    #[test]
    fn parse_mentions_no_at() {
        let adapter = TelegramMentionAdapter::new();
        let mentions = adapter.parse_mentions("Hello world");
        assert!(mentions.is_empty());
    }

    #[test]
    fn parse_mentions_ignores_lone_at() {
        let adapter = TelegramMentionAdapter::new();
        let mentions = adapter.parse_mentions("@ is not a mention");
        assert!(mentions.is_empty());
    }

    #[test]
    fn format_mention_with_at() {
        let adapter = TelegramMentionAdapter::new();
        assert_eq!(adapter.format_mention("@alice"), "@alice");
    }

    #[test]
    fn format_mention_without_at() {
        let adapter = TelegramMentionAdapter::new();
        assert_eq!(adapter.format_mention("alice"), "@alice");
    }
}
