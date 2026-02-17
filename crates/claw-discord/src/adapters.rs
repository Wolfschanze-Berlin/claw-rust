//! Discord adapter trait implementations.
//!
//! Thin stubs for MentionAdapter, CommandAdapter, MessageActionAdapter,
//! and ThreadingAdapter. The actual Discord REST API calls are TODO —
//! these stubs establish the wiring so the plugin advertises its capabilities.

use async_trait::async_trait;
use serde_json::Value;
use tracing::info;

use claw_channels::plugin::{
    ChannelCommandAdapter, ChannelMentionAdapter, ChannelMessageActionAdapter,
    ChannelThreadingAdapter,
};
use claw_channels::types::{ChannelError, ChannelOutboundContext, OutboundDeliveryResult};

// ---------------------------------------------------------------------------
// MentionAdapter
// ---------------------------------------------------------------------------

/// Parses and formats Discord <@user_id> mentions.
pub struct DiscordMentionAdapter;

impl DiscordMentionAdapter {
    pub fn new() -> Self {
        Self
    }
}

impl ChannelMentionAdapter for DiscordMentionAdapter {
    fn parse_mentions(&self, text: &str) -> Vec<String> {
        // Discord mentions use the format <@user_id> or <@!user_id> (nickname).
        let mut mentions = Vec::new();
        let mut remaining = text;
        while let Some(start) = remaining.find("<@") {
            let after_prefix = &remaining[start + 2..];
            // Skip optional '!' for nickname mentions.
            let id_start = if after_prefix.starts_with('!') {
                &after_prefix[1..]
            } else {
                after_prefix
            };
            if let Some(end) = id_start.find('>') {
                let id = &id_start[..end];
                if !id.is_empty() && id.chars().all(|c| c.is_ascii_digit()) {
                    mentions.push(id.to_string());
                }
                remaining = &id_start[end + 1..];
            } else {
                break;
            }
        }
        mentions
    }

    fn format_mention(&self, user_id: &str) -> String {
        format!("<@{user_id}>")
    }
}

// ---------------------------------------------------------------------------
// CommandAdapter
// ---------------------------------------------------------------------------

/// Registers Discord application (slash) commands via the REST API.
pub struct DiscordCommandAdapter;

impl DiscordCommandAdapter {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl ChannelCommandAdapter for DiscordCommandAdapter {
    async fn register_commands(
        &self,
        account_id: &str,
        commands: &[Value],
    ) -> Result<(), ChannelError> {
        info!(
            account_id,
            command_count = commands.len(),
            "registering discord slash commands"
        );
        // TODO: PUT to /api/v10/applications/{app_id}/commands with command JSON.
        Ok(())
    }

    async fn unregister_commands(&self, account_id: &str) -> Result<(), ChannelError> {
        info!(account_id, "clearing discord slash commands");
        // TODO: PUT empty array to /api/v10/applications/{app_id}/commands.
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// MessageActionAdapter
// ---------------------------------------------------------------------------

/// Sends messages with Discord components (buttons, select menus).
pub struct DiscordMessageActionAdapter;

impl DiscordMessageActionAdapter {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl ChannelMessageActionAdapter for DiscordMessageActionAdapter {
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
            "sending discord message with components"
        );
        // TODO: POST to /api/v10/channels/{channel_id}/messages with
        // components array containing action rows with buttons/selects.
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
        info!(account_id, "handling discord interaction callback");
        // TODO: POST interaction response to
        // /api/v10/interactions/{id}/{token}/callback
        let _ = callback_data;
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// ThreadingAdapter
// ---------------------------------------------------------------------------

/// Creates and manages Discord threads.
pub struct DiscordThreadingAdapter;

impl DiscordThreadingAdapter {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl ChannelThreadingAdapter for DiscordThreadingAdapter {
    async fn create_thread(
        &self,
        account_id: &str,
        chat_id: &str,
        message_id: &str,
    ) -> Result<String, ChannelError> {
        info!(
            account_id,
            chat_id,
            message_id,
            "creating discord thread from message"
        );
        // TODO: POST to /api/v10/channels/{channel_id}/messages/{message_id}/threads
        Ok("placeholder_thread_id".into())
    }

    async fn get_thread_replies(
        &self,
        account_id: &str,
        chat_id: &str,
        thread_id: &str,
    ) -> Result<Vec<Value>, ChannelError> {
        info!(
            account_id,
            chat_id,
            thread_id,
            "fetching discord thread replies"
        );
        // TODO: GET /api/v10/channels/{thread_id}/messages
        Ok(vec![])
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_mentions_standard() {
        let adapter = DiscordMentionAdapter::new();
        let mentions = adapter.parse_mentions("Hello <@123456> and <@789012>!");
        assert_eq!(mentions, vec!["123456", "789012"]);
    }

    #[test]
    fn parse_mentions_nickname_format() {
        let adapter = DiscordMentionAdapter::new();
        let mentions = adapter.parse_mentions("Hey <@!555666>");
        assert_eq!(mentions, vec!["555666"]);
    }

    #[test]
    fn parse_mentions_empty() {
        let adapter = DiscordMentionAdapter::new();
        let mentions = adapter.parse_mentions("no mentions here");
        assert!(mentions.is_empty());
    }

    #[test]
    fn parse_mentions_ignores_role_mentions() {
        let adapter = DiscordMentionAdapter::new();
        // Role mentions use <@&role_id> — the '&' means the id part starts
        // with a non-digit, so it won't be collected.
        let mentions = adapter.parse_mentions("<@&999>");
        assert!(mentions.is_empty());
    }

    #[test]
    fn format_mention() {
        let adapter = DiscordMentionAdapter::new();
        assert_eq!(adapter.format_mention("123456"), "<@123456>");
    }
}
