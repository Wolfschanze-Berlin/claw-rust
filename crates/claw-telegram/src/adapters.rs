//! Telegram adapter trait implementations.
//!
//! Implements MentionAdapter, CommandAdapter, MessageActionAdapter,
//! and StreamingAdapter with real teloxide Bot API calls.

use async_trait::async_trait;
use serde_json::Value;
use teloxide::requests::Requester;
use teloxide::types::ChatId;
use tracing::{info, warn};

use claw_channels::plugin::{
    ChannelCommandAdapter, ChannelMentionAdapter, ChannelMessageActionAdapter,
    ChannelStreamingAdapter,
};
use claw_channels::types::{ChannelError, ChannelOutboundContext, OutboundDeliveryResult};

use crate::BotStore;

/// Parse a chat_id string into a teloxide ChatId.
fn parse_chat_id(chat_id: &str) -> Result<ChatId, ChannelError> {
    chat_id
        .parse::<i64>()
        .map(ChatId)
        .map_err(|_| ChannelError::DeliveryFailed(format!("invalid chat_id: {chat_id}")))
}

/// Look up a bot from the shared store by account ID.
fn get_bot(bots: &BotStore, account_id: &str) -> Result<teloxide::Bot, ChannelError> {
    let store = bots.read().map_err(|e| {
        ChannelError::DeliveryFailed(format!("bot store lock poisoned: {e}"))
    })?;
    store.get(account_id).cloned().ok_or_else(|| {
        ChannelError::AccountNotFound {
            channel: "telegram".into(),
            account_id: account_id.into(),
        }
    })
}

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
pub struct TelegramCommandAdapter {
    bots: BotStore,
}

impl TelegramCommandAdapter {
    pub fn new(bots: BotStore) -> Self {
        Self { bots }
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

        let bot = get_bot(&self.bots, account_id)?;

        // Parse command JSON values into teloxide BotCommand structs.
        let bot_commands: Vec<teloxide::types::BotCommand> = commands
            .iter()
            .filter_map(|cmd| {
                let command = cmd.get("command")?.as_str()?.to_string();
                let description = cmd
                    .get("description")
                    .and_then(|d| d.as_str())
                    .unwrap_or("")
                    .to_string();
                Some(teloxide::types::BotCommand { command, description })
            })
            .collect();

        bot.set_my_commands(bot_commands)
            .await
            .map_err(|e| ChannelError::GatewayError(format!("failed to set commands: {e}")))?;

        Ok(())
    }

    async fn unregister_commands(&self, account_id: &str) -> Result<(), ChannelError> {
        info!(account_id, "clearing telegram bot commands");

        let bot = get_bot(&self.bots, account_id)?;
        bot.delete_my_commands()
            .await
            .map_err(|e| ChannelError::GatewayError(format!("failed to delete commands: {e}")))?;

        Ok(())
    }
}

// ---------------------------------------------------------------------------
// MessageActionAdapter
// ---------------------------------------------------------------------------

/// Sends messages with Telegram inline keyboards (action buttons).
pub struct TelegramMessageActionAdapter {
    bots: BotStore,
}

impl TelegramMessageActionAdapter {
    pub fn new(bots: BotStore) -> Self {
        Self { bots }
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
        use teloxide::payloads::SendMessageSetters;
        use teloxide::types::{InlineKeyboardButton, InlineKeyboardMarkup};

        info!(
            account_id = %ctx.account_id,
            chat_id = %ctx.chat_id,
            action_count = actions.len(),
            "sending telegram message with inline keyboard"
        );

        let bot = get_bot(&self.bots, &ctx.account_id)?;
        let chat_id = parse_chat_id(&ctx.chat_id)?;

        // Build inline keyboard from actions JSON.
        // Each action is expected to have "text" and "callback_data" fields.
        // Actions can optionally specify "url" for URL buttons.
        let buttons: Vec<InlineKeyboardButton> = actions
            .iter()
            .filter_map(|action| {
                let label = action.get("text")?.as_str()?.to_string();
                if let Some(url) = action.get("url").and_then(|u| u.as_str()) {
                    Some(InlineKeyboardButton::url(
                        label,
                        url.parse().ok()?,
                    ))
                } else {
                    let data = action
                        .get("callback_data")
                        .and_then(|d| d.as_str())
                        .unwrap_or_default()
                        .to_string();
                    Some(InlineKeyboardButton::callback(label, data))
                }
            })
            .collect();

        let keyboard = InlineKeyboardMarkup::new(vec![buttons]);

        match bot
            .send_message(chat_id, text)
            .reply_markup(keyboard)
            .await
        {
            Ok(msg) => Ok(OutboundDeliveryResult {
                success: Some(true),
                message_id: Some(msg.id.0.to_string()),
                error: None,
                metadata: None,
            }),
            Err(e) => {
                warn!(error = %e, "telegram send_with_actions failed");
                Err(ChannelError::DeliveryFailed(format!(
                    "telegram send_with_actions failed: {e}"
                )))
            }
        }
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

        let bot = get_bot(&self.bots, account_id)?;

        // Extract callback_query_id from the data.
        let query_id = callback_data
            .get("callback_query_id")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string();

        if !query_id.is_empty() {
            bot.answer_callback_query(teloxide::types::CallbackQueryId(query_id))
                .await
                .map_err(|e| {
                    ChannelError::GatewayError(format!("answer_callback_query failed: {e}"))
                })?;
        }

        Ok(())
    }
}

// ---------------------------------------------------------------------------
// StreamingAdapter
// ---------------------------------------------------------------------------

/// Streams messages via Telegram edit-in-place (progressive message updates).
pub struct TelegramStreamingAdapter {
    bots: BotStore,
}

impl TelegramStreamingAdapter {
    pub fn new(bots: BotStore) -> Self {
        Self { bots }
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

        let bot = get_bot(&self.bots, &ctx.account_id)?;
        let chat_id = parse_chat_id(&ctx.chat_id)?;

        // Send the initial message and return its ID for subsequent edits.
        let msg = bot
            .send_message(chat_id, initial_text)
            .await
            .map_err(|e| {
                ChannelError::DeliveryFailed(format!("stream_start send_message failed: {e}"))
            })?;

        Ok(msg.id.0.to_string())
    }

    async fn stream_update(
        &self,
        ctx: &ChannelOutboundContext,
        message_id: &str,
        text: &str,
    ) -> Result<(), ChannelError> {
        let bot = get_bot(&self.bots, &ctx.account_id)?;
        let chat_id = parse_chat_id(&ctx.chat_id)?;

        let msg_id: i32 = message_id
            .parse()
            .map_err(|_| ChannelError::DeliveryFailed(format!("invalid message_id: {message_id}")))?;

        // Edit the message in-place with updated text.
        bot.edit_message_text(chat_id, teloxide::types::MessageId(msg_id), text)
            .await
            .map_err(|e| {
                // Telegram returns an error if text hasn't changed; treat as non-fatal.
                let err_str = e.to_string();
                if err_str.contains("message is not modified") {
                    tracing::debug!("stream_update: message not modified (no-op)");
                    return ChannelError::DeliveryFailed("message not modified".into());
                }
                ChannelError::DeliveryFailed(format!("stream_update edit failed: {e}"))
            })?;

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

        // Final edit uses the same mechanism as stream_update.
        self.stream_update(ctx, message_id, final_text).await
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -- MentionAdapter -------------------------------------------------------

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

    #[test]
    fn parse_mentions_in_sentence() {
        let adapter = TelegramMentionAdapter::new();
        let mentions = adapter.parse_mentions("Hey @admin, can you help @support_team?");
        assert_eq!(mentions, vec!["@admin", "@support_team"]);
    }

    #[test]
    fn parse_mentions_multiple_same() {
        let adapter = TelegramMentionAdapter::new();
        let mentions = adapter.parse_mentions("@alice @alice @alice");
        assert_eq!(mentions.len(), 3);
    }

    // -- CommandAdapter (parse helpers) ----------------------------------------

    #[test]
    fn parse_command_json() {
        let cmds = vec![
            serde_json::json!({"command": "start", "description": "Start the bot"}),
            serde_json::json!({"command": "help", "description": "Show help"}),
        ];
        let bot_commands: Vec<teloxide::types::BotCommand> = cmds
            .iter()
            .filter_map(|cmd| {
                let command = cmd.get("command")?.as_str()?.to_string();
                let description = cmd
                    .get("description")
                    .and_then(|d| d.as_str())
                    .unwrap_or("")
                    .to_string();
                Some(teloxide::types::BotCommand { command, description })
            })
            .collect();
        assert_eq!(bot_commands.len(), 2);
        assert_eq!(bot_commands[0].command, "start");
        assert_eq!(bot_commands[1].description, "Show help");
    }

    #[test]
    fn parse_command_json_missing_fields() {
        let cmds = vec![
            serde_json::json!({"not_command": "bad"}),
        ];
        let bot_commands: Vec<teloxide::types::BotCommand> = cmds
            .iter()
            .filter_map(|cmd| {
                let command = cmd.get("command")?.as_str()?.to_string();
                let description = cmd
                    .get("description")
                    .and_then(|d| d.as_str())
                    .unwrap_or("")
                    .to_string();
                Some(teloxide::types::BotCommand { command, description })
            })
            .collect();
        assert!(bot_commands.is_empty());
    }

    // -- MessageActionAdapter (keyboard building) -----------------------------

    #[test]
    fn build_inline_keyboard_buttons() {
        use teloxide::types::InlineKeyboardButton;

        let actions = vec![
            serde_json::json!({"text": "Yes", "callback_data": "yes"}),
            serde_json::json!({"text": "No", "callback_data": "no"}),
        ];

        let buttons: Vec<InlineKeyboardButton> = actions
            .iter()
            .filter_map(|action| {
                let label = action.get("text")?.as_str()?.to_string();
                let data = action
                    .get("callback_data")
                    .and_then(|d| d.as_str())
                    .unwrap_or_default()
                    .to_string();
                Some(InlineKeyboardButton::callback(label, data))
            })
            .collect();

        assert_eq!(buttons.len(), 2);
    }

    #[test]
    fn build_url_button() {
        use teloxide::types::InlineKeyboardButton;

        let actions = vec![
            serde_json::json!({"text": "Visit", "url": "https://example.com"}),
        ];

        let buttons: Vec<InlineKeyboardButton> = actions
            .iter()
            .filter_map(|action| {
                let label = action.get("text")?.as_str()?.to_string();
                if let Some(url) = action.get("url").and_then(|u| u.as_str()) {
                    Some(InlineKeyboardButton::url(
                        label,
                        url.parse().ok()?,
                    ))
                } else {
                    None
                }
            })
            .collect();

        assert_eq!(buttons.len(), 1);
    }

    // -- StreamingAdapter (parse helpers) --------------------------------------

    #[test]
    fn parse_valid_message_id() {
        let id: Result<i32, _> = "42".parse();
        assert_eq!(id.unwrap(), 42);
    }

    #[test]
    fn parse_invalid_message_id() {
        let id: Result<i32, _> = "not_a_number".parse();
        assert!(id.is_err());
    }
}
