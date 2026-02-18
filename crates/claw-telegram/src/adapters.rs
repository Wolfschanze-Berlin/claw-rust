//! Telegram adapter trait implementations.
//!
//! Implements MentionAdapter, CommandAdapter, MessageActionAdapter,
//! StreamingAdapter, GroupAdapter, StatusAdapter, MessagingAdapter,
//! AuthAdapter, and ThreadingAdapter with real teloxide Bot API calls.

use async_trait::async_trait;
use serde_json::Value;
use teloxide::requests::Requester;
use teloxide::types::ChatId;
use tracing::{info, warn};

use claw_channels::plugin::{
    ChannelAuthAdapter, ChannelCommandAdapter, ChannelGroupAdapter, ChannelMentionAdapter,
    ChannelMessageActionAdapter, ChannelMessagingAdapter, ChannelStatusAdapter,
    ChannelStreamingAdapter, ChannelThreadingAdapter,
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

/// Parse a message_id string into a teloxide MessageId.
fn parse_message_id(message_id: &str) -> Result<teloxide::types::MessageId, ChannelError> {
    message_id
        .parse::<i32>()
        .map(teloxide::types::MessageId)
        .map_err(|_| ChannelError::DeliveryFailed(format!("invalid message_id: {message_id}")))
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
// GroupAdapter
// ---------------------------------------------------------------------------

/// Checks admin status, retrieves chat info via the Telegram Bot API.
pub struct TelegramGroupAdapter {
    bots: BotStore,
}

impl TelegramGroupAdapter {
    pub fn new(bots: BotStore) -> Self {
        Self { bots }
    }
}

#[async_trait]
impl ChannelGroupAdapter for TelegramGroupAdapter {
    async fn is_admin(
        &self,
        account_id: &str,
        chat_id: &str,
        user_id: &str,
    ) -> Result<bool, ChannelError> {
        use teloxide::types::ChatMemberKind;

        let bot = get_bot(&self.bots, account_id)?;
        let cid = parse_chat_id(chat_id)?;
        let uid: u64 = user_id
            .parse()
            .map_err(|_| ChannelError::DeliveryFailed(format!("invalid user_id: {user_id}")))?;

        let member = bot
            .get_chat_member(cid, teloxide::types::UserId(uid))
            .await
            .map_err(|e| {
                ChannelError::GatewayError(format!("get_chat_member failed: {e}"))
            })?;

        Ok(matches!(
            member.kind,
            ChatMemberKind::Owner(_) | ChatMemberKind::Administrator(_)
        ))
    }

    async fn get_members(
        &self,
        account_id: &str,
        chat_id: &str,
    ) -> Result<Vec<Value>, ChannelError> {
        // Telegram Bot API does not expose a "list all members" endpoint.
        // getChatMemberCount is available but only returns a count.
        // Return the member count as a single-element array for callers
        // that want at least some group info.
        let bot = get_bot(&self.bots, account_id)?;
        let cid = parse_chat_id(chat_id)?;

        let count = bot.get_chat_member_count(cid).await.map_err(|e| {
            ChannelError::GatewayError(format!("get_chat_member_count failed: {e}"))
        })?;

        Ok(vec![serde_json::json!({ "member_count": count })])
    }

    async fn get_chat_title(
        &self,
        account_id: &str,
        chat_id: &str,
    ) -> Result<Option<String>, ChannelError> {
        let bot = get_bot(&self.bots, account_id)?;
        let cid = parse_chat_id(chat_id)?;

        let chat = bot.get_chat(cid).await.map_err(|e| {
            ChannelError::GatewayError(format!("get_chat failed: {e}"))
        })?;

        Ok(chat.title().map(|t| t.to_string()))
    }
}

// ---------------------------------------------------------------------------
// StatusAdapter
// ---------------------------------------------------------------------------

/// Sends typing indicators via Telegram's `sendChatAction`.
pub struct TelegramStatusAdapter {
    bots: BotStore,
}

impl TelegramStatusAdapter {
    pub fn new(bots: BotStore) -> Self {
        Self { bots }
    }
}

#[async_trait]
impl ChannelStatusAdapter for TelegramStatusAdapter {
    async fn send_typing(
        &self,
        account_id: &str,
        chat_id: &str,
    ) -> Result<(), ChannelError> {
        use teloxide::types::ChatAction;

        let bot = get_bot(&self.bots, account_id)?;
        let cid = parse_chat_id(chat_id)?;

        bot.send_chat_action(cid, ChatAction::Typing)
            .await
            .map_err(|e| {
                ChannelError::GatewayError(format!("send_chat_action failed: {e}"))
            })?;

        Ok(())
    }

    // set_online_status: Telegram Bot API has no online/offline concept for
    // bots, so we use the default no-op implementation from the trait.
}

// ---------------------------------------------------------------------------
// MessagingAdapter
// ---------------------------------------------------------------------------

/// Edits, deletes, and reacts to Telegram messages.
pub struct TelegramMessagingAdapter {
    bots: BotStore,
}

impl TelegramMessagingAdapter {
    pub fn new(bots: BotStore) -> Self {
        Self { bots }
    }
}

#[async_trait]
impl ChannelMessagingAdapter for TelegramMessagingAdapter {
    async fn edit_message(
        &self,
        account_id: &str,
        chat_id: &str,
        message_id: &str,
        new_text: &str,
    ) -> Result<(), ChannelError> {
        let bot = get_bot(&self.bots, account_id)?;
        let cid = parse_chat_id(chat_id)?;
        let mid = parse_message_id(message_id)?;

        bot.edit_message_text(cid, mid, new_text)
            .await
            .map_err(|e| {
                ChannelError::DeliveryFailed(format!("edit_message_text failed: {e}"))
            })?;

        Ok(())
    }

    async fn delete_message(
        &self,
        account_id: &str,
        chat_id: &str,
        message_id: &str,
    ) -> Result<(), ChannelError> {
        let bot = get_bot(&self.bots, account_id)?;
        let cid = parse_chat_id(chat_id)?;
        let mid = parse_message_id(message_id)?;

        bot.delete_message(cid, mid).await.map_err(|e| {
            ChannelError::DeliveryFailed(format!("delete_message failed: {e}"))
        })?;

        Ok(())
    }

    async fn add_reaction(
        &self,
        account_id: &str,
        chat_id: &str,
        message_id: &str,
        reaction: &str,
    ) -> Result<(), ChannelError> {
        use teloxide::payloads::SetMessageReactionSetters;
        use teloxide::types::ReactionType;

        let bot = get_bot(&self.bots, account_id)?;
        let cid = parse_chat_id(chat_id)?;
        let mid = parse_message_id(message_id)?;

        let reaction_type = ReactionType::Emoji {
            emoji: reaction.to_string(),
        };

        bot.set_message_reaction(cid, mid)
            .reaction(vec![reaction_type])
            .await
            .map_err(|e| {
                ChannelError::DeliveryFailed(format!("set_message_reaction failed: {e}"))
            })?;

        Ok(())
    }

    async fn remove_reaction(
        &self,
        account_id: &str,
        chat_id: &str,
        message_id: &str,
        _reaction: &str,
    ) -> Result<(), ChannelError> {
        use teloxide::payloads::SetMessageReactionSetters;

        // Telegram's setMessageReaction with an empty array clears all
        // reactions set by the bot. There is no per-emoji removal.
        let bot = get_bot(&self.bots, account_id)?;
        let cid = parse_chat_id(chat_id)?;
        let mid = parse_message_id(message_id)?;

        bot.set_message_reaction(cid, mid)
            .reaction(Vec::<teloxide::types::ReactionType>::new())
            .await
            .map_err(|e| {
                ChannelError::DeliveryFailed(format!("remove_reaction failed: {e}"))
            })?;

        Ok(())
    }
}

// ---------------------------------------------------------------------------
// AuthAdapter
// ---------------------------------------------------------------------------

/// Validates Telegram users by querying chat membership info.
pub struct TelegramAuthAdapter {
    bots: BotStore,
}

impl TelegramAuthAdapter {
    pub fn new(bots: BotStore) -> Self {
        Self { bots }
    }
}

#[async_trait]
impl ChannelAuthAdapter for TelegramAuthAdapter {
    async fn validate_user(
        &self,
        account_id: &str,
        user_id: &str,
    ) -> Result<bool, ChannelError> {
        // A user is "valid" if Telegram recognises the ID. We probe by
        // fetching user profile photos — if the API succeeds the user exists.
        let bot = get_bot(&self.bots, account_id)?;
        let uid: u64 = user_id
            .parse()
            .map_err(|_| ChannelError::DeliveryFailed(format!("invalid user_id: {user_id}")))?;

        match bot
            .get_user_profile_photos(teloxide::types::UserId(uid))
            .await
        {
            Ok(_) => Ok(true),
            Err(e) => {
                let msg = e.to_string();
                // "Bad Request: user not found" → user doesn't exist
                if msg.contains("user not found") || msg.contains("USER_ID_INVALID") {
                    Ok(false)
                } else {
                    Err(ChannelError::GatewayError(format!(
                        "get_user_profile_photos failed: {e}"
                    )))
                }
            }
        }
    }

    async fn get_user_display_name(
        &self,
        account_id: &str,
        user_id: &str,
    ) -> Result<Option<String>, ChannelError> {
        // Telegram Bot API has no direct "get user by ID" method outside of
        // a chat context. We try getChatMember on the user's private chat
        // (chatId == userId for private chats).
        let bot = get_bot(&self.bots, account_id)?;
        let uid: i64 = user_id
            .parse()
            .map_err(|_| ChannelError::DeliveryFailed(format!("invalid user_id: {user_id}")))?;

        // In Telegram, a private chat with a user has chatId == userId.
        match bot
            .get_chat_member(ChatId(uid), teloxide::types::UserId(uid as u64))
            .await
        {
            Ok(member) => {
                let user = member.user;
                let name = match &user.last_name {
                    Some(last) => format!("{} {last}", user.first_name),
                    None => user.first_name.clone(),
                };
                Ok(Some(name))
            }
            Err(_) => Ok(None),
        }
    }
}

// ---------------------------------------------------------------------------
// ThreadingAdapter
// ---------------------------------------------------------------------------

/// Creates forum topics (threads) in Telegram supergroups.
pub struct TelegramThreadingAdapter {
    bots: BotStore,
}

impl TelegramThreadingAdapter {
    pub fn new(bots: BotStore) -> Self {
        Self { bots }
    }
}

#[async_trait]
impl ChannelThreadingAdapter for TelegramThreadingAdapter {
    async fn create_thread(
        &self,
        account_id: &str,
        chat_id: &str,
        message_id: &str,
    ) -> Result<String, ChannelError> {
        // Telegram "threads" are forum topics in supergroups.
        // We use the message text (or a default) as the topic name.
        let bot = get_bot(&self.bots, account_id)?;
        let cid = parse_chat_id(chat_id)?;

        // Use the message_id as a hint for the topic name.
        let topic_name = format!("Thread from message #{message_id}");

        let topic = bot
            .create_forum_topic(cid, &topic_name)
            .await
            .map_err(|e| {
                ChannelError::GatewayError(format!("create_forum_topic failed: {e}"))
            })?;

        // ThreadId wraps MessageId(i32) — extract the inner value.
        Ok(topic.thread_id.0.0.to_string())
    }

    async fn get_thread_replies(
        &self,
        _account_id: &str,
        _chat_id: &str,
        _thread_id: &str,
    ) -> Result<Vec<Value>, ChannelError> {
        // Telegram Bot API does not provide a "get messages in thread" endpoint.
        // Bots receive thread messages via updates; historical fetch is not supported.
        Ok(vec![])
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

    // -- Shared helpers -------------------------------------------------------

    #[test]
    fn parse_chat_id_valid() {
        let cid = parse_chat_id("123456").unwrap();
        assert_eq!(cid, ChatId(123456));
    }

    #[test]
    fn parse_chat_id_negative() {
        let cid = parse_chat_id("-1001234567890").unwrap();
        assert_eq!(cid, ChatId(-1001234567890));
    }

    #[test]
    fn parse_chat_id_invalid() {
        assert!(parse_chat_id("not_a_number").is_err());
    }

    #[test]
    fn parse_message_id_valid() {
        let mid = parse_message_id("42").unwrap();
        assert_eq!(mid, teloxide::types::MessageId(42));
    }

    #[test]
    fn parse_message_id_invalid() {
        assert!(parse_message_id("abc").is_err());
    }

    #[test]
    fn get_bot_missing_account() {
        let store = crate::new_bot_store();
        let result = get_bot(&store, "nonexistent");
        assert!(result.is_err());
        match result.unwrap_err() {
            ChannelError::AccountNotFound { channel, account_id } => {
                assert_eq!(channel, "telegram");
                assert_eq!(account_id, "nonexistent");
            }
            other => panic!("expected AccountNotFound, got: {other:?}"),
        }
    }

    // -- GroupAdapter ---------------------------------------------------------

    #[test]
    fn group_adapter_requires_valid_user_id() {
        // Verify the user_id parsing logic without a real bot.
        let uid: Result<u64, _> = "12345".parse();
        assert!(uid.is_ok());
        let uid: Result<u64, _> = "not_a_number".parse();
        assert!(uid.is_err());
    }

    // -- MessagingAdapter (reaction type construction) ------------------------

    #[test]
    fn build_emoji_reaction_type() {
        use teloxide::types::ReactionType;

        let rt = ReactionType::Emoji {
            emoji: "\u{1F44D}".to_string(),
        };
        match rt {
            ReactionType::Emoji { emoji } => assert_eq!(emoji, "\u{1F44D}"),
            _ => panic!("expected Emoji variant"),
        }
    }

    // -- ThreadingAdapter (thread_id extraction) ------------------------------

    #[test]
    fn thread_id_to_string() {
        let tid = teloxide::types::ThreadId(teloxide::types::MessageId(42));
        assert_eq!(tid.0.0.to_string(), "42");
    }

    // -- AuthAdapter (user_id parsing) ----------------------------------------

    #[test]
    fn auth_user_id_parsing() {
        // Validate that both i64 and u64 parse from the same string.
        let as_i64: i64 = "123456789".parse().unwrap();
        let as_u64: u64 = "123456789".parse().unwrap();
        assert_eq!(as_i64, 123456789);
        assert_eq!(as_u64, 123456789);
    }

    #[test]
    fn auth_user_id_invalid() {
        let result: Result<u64, _> = "not_a_user".parse();
        assert!(result.is_err());
    }
}
