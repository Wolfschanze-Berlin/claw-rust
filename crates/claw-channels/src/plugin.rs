//! ChannelPlugin trait and adapter trait definitions.
//!
//! This module defines the core `ChannelPlugin` trait that all channel
//! implementations must satisfy, plus ~20 composable adapter traits that
//! channels implement based on their platform capabilities.
//!
//! The adapter pattern allows each platform (Telegram, Slack, Discord, etc.)
//! to opt in to only the features it supports. For example, a channel that
//! doesn't support threads simply returns `None` from `threading_adapter()`.

use async_trait::async_trait;
use serde_json::Value;

use crate::types::{
    ChannelCapabilities, ChannelError, ChannelGatewayContext, ChannelMeta, ChannelOutboundContext,
    DeliveryMode, OutboundDeliveryResult,
};

// ---------------------------------------------------------------------------
// ChannelPlugin — master trait
// ---------------------------------------------------------------------------

/// The master trait that every channel implementation must satisfy.
///
/// Provides static metadata and capability declarations, plus accessor
/// methods that return optional references to adapter trait objects.
/// A channel only implements the adapters it supports.
pub trait ChannelPlugin: Send + Sync {
    /// Static metadata about this channel (id, label, docs, etc).
    fn meta(&self) -> &ChannelMeta;

    /// Declared capabilities of this channel.
    fn capabilities(&self) -> &ChannelCapabilities;

    // -- Adapter accessors --------------------------------------------------
    // Each returns `None` if the channel doesn't support that capability.

    fn config_adapter(&self) -> Option<&dyn ChannelConfigAdapter> {
        None
    }
    fn gateway_adapter(&self) -> Option<&dyn ChannelGatewayAdapter> {
        None
    }
    fn outbound_adapter(&self) -> Option<&dyn ChannelOutboundAdapter> {
        None
    }
    fn security_adapter(&self) -> Option<&dyn ChannelSecurityAdapter> {
        None
    }
    fn group_adapter(&self) -> Option<&dyn ChannelGroupAdapter> {
        None
    }
    fn mention_adapter(&self) -> Option<&dyn ChannelMentionAdapter> {
        None
    }
    fn status_adapter(&self) -> Option<&dyn ChannelStatusAdapter> {
        None
    }
    fn auth_adapter(&self) -> Option<&dyn ChannelAuthAdapter> {
        None
    }
    fn elevated_adapter(&self) -> Option<&dyn ChannelElevatedAdapter> {
        None
    }
    fn command_adapter(&self) -> Option<&dyn ChannelCommandAdapter> {
        None
    }
    fn streaming_adapter(&self) -> Option<&dyn ChannelStreamingAdapter> {
        None
    }
    fn threading_adapter(&self) -> Option<&dyn ChannelThreadingAdapter> {
        None
    }
    fn messaging_adapter(&self) -> Option<&dyn ChannelMessagingAdapter> {
        None
    }
    fn agent_prompt_adapter(&self) -> Option<&dyn ChannelAgentPromptAdapter> {
        None
    }
    fn directory_adapter(&self) -> Option<&dyn ChannelDirectoryAdapter> {
        None
    }
    fn resolver_adapter(&self) -> Option<&dyn ChannelResolverAdapter> {
        None
    }
    fn message_action_adapter(&self) -> Option<&dyn ChannelMessageActionAdapter> {
        None
    }
    fn heartbeat_adapter(&self) -> Option<&dyn ChannelHeartbeatAdapter> {
        None
    }
    fn onboarding_adapter(&self) -> Option<&dyn ChannelOnboardingAdapter> {
        None
    }
    fn pairing_adapter(&self) -> Option<&dyn ChannelPairingAdapter> {
        None
    }
    fn setup_adapter(&self) -> Option<&dyn ChannelSetupAdapter> {
        None
    }
}

// ---------------------------------------------------------------------------
// Adapter traits
// ---------------------------------------------------------------------------

/// Account configuration and discovery.
///
/// Lists available accounts, resolves account configs, and provides
/// defaults for channels that support multi-account setups.
#[async_trait]
pub trait ChannelConfigAdapter: Send + Sync {
    /// List all configured account IDs for this channel.
    async fn list_account_ids(&self) -> Result<Vec<String>, ChannelError>;

    /// Resolve full account configuration by ID.
    async fn resolve_account(&self, account_id: &str) -> Result<Value, ChannelError>;

    /// Return the default account ID (if any).
    fn default_account_id(&self) -> Option<&str>;
}

/// Inbound message gateway — polling, webhooks, login lifecycle.
///
/// Handles starting/stopping accounts, QR-based logins, and
/// account disconnection.
#[async_trait]
pub trait ChannelGatewayAdapter: Send + Sync {
    /// Start polling/webhook listening for the given account.
    async fn start_account(&self, ctx: ChannelGatewayContext) -> Result<(), ChannelError>;

    /// Stop a running account gracefully.
    async fn stop_account(&self, account_id: &str) -> Result<(), ChannelError>;

    /// Begin a QR-code login flow (for platforms like WhatsApp/WeChat).
    async fn login_with_qr_start(&self, account_id: &str) -> Result<Option<String>, ChannelError> {
        let _ = account_id;
        Ok(None)
    }

    /// Wait for QR-code login completion.
    async fn login_with_qr_wait(&self, account_id: &str) -> Result<bool, ChannelError> {
        let _ = account_id;
        Ok(false)
    }

    /// Logout / disconnect an account.
    async fn logout_account(&self, account_id: &str) -> Result<(), ChannelError> {
        let _ = account_id;
        Ok(())
    }
}

/// Outbound message delivery.
///
/// Sends text, media, polls, and arbitrary payloads to a platform.
#[async_trait]
pub trait ChannelOutboundAdapter: Send + Sync {
    /// The delivery mode this channel uses.
    fn delivery_mode(&self) -> DeliveryMode {
        DeliveryMode::Single
    }

    /// Send an arbitrary JSON payload.
    async fn send_payload(
        &self,
        ctx: &ChannelOutboundContext,
        payload: Value,
    ) -> Result<OutboundDeliveryResult, ChannelError>;

    /// Send a plain text message.
    async fn send_text(
        &self,
        ctx: &ChannelOutboundContext,
        text: &str,
    ) -> Result<OutboundDeliveryResult, ChannelError>;

    /// Send media (images, files, audio, video).
    async fn send_media(
        &self,
        ctx: &ChannelOutboundContext,
        media_url: &str,
        caption: Option<&str>,
    ) -> Result<OutboundDeliveryResult, ChannelError> {
        let _ = (ctx, media_url, caption);
        Err(ChannelError::AdapterNotSupported {
            channel: String::new(),
            adapter: "send_media".into(),
        })
    }

    /// Send a poll.
    async fn send_poll(
        &self,
        ctx: &ChannelOutboundContext,
        question: &str,
        options: &[String],
    ) -> Result<OutboundDeliveryResult, ChannelError> {
        let _ = (ctx, question, options);
        Err(ChannelError::AdapterNotSupported {
            channel: String::new(),
            adapter: "send_poll".into(),
        })
    }
}

/// Security — webhook verification, rate limiting, IP allowlisting.
#[async_trait]
pub trait ChannelSecurityAdapter: Send + Sync {
    /// Verify an inbound webhook signature/payload.
    async fn verify_webhook(&self, headers: &Value, body: &[u8]) -> Result<bool, ChannelError>;

    /// Check if a user/IP is rate-limited.
    async fn check_rate_limit(&self, user_id: &str) -> Result<bool, ChannelError> {
        let _ = user_id;
        Ok(false) // not rate-limited by default
    }
}

/// Group management — admin checks, membership, kicks.
#[async_trait]
pub trait ChannelGroupAdapter: Send + Sync {
    /// Check if a user is an admin in a chat.
    async fn is_admin(&self, account_id: &str, chat_id: &str, user_id: &str) -> Result<bool, ChannelError>;

    /// Get member list for a group chat.
    async fn get_members(&self, account_id: &str, chat_id: &str) -> Result<Vec<Value>, ChannelError> {
        let _ = (account_id, chat_id);
        Ok(vec![])
    }

    /// Get the title/name of a group chat.
    async fn get_chat_title(&self, account_id: &str, chat_id: &str) -> Result<Option<String>, ChannelError> {
        let _ = (account_id, chat_id);
        Ok(None)
    }
}

/// @mention parsing and formatting.
pub trait ChannelMentionAdapter: Send + Sync {
    /// Parse mentions from raw message text, returning user IDs.
    fn parse_mentions(&self, text: &str) -> Vec<String>;

    /// Format a user ID into a platform-native mention string.
    fn format_mention(&self, user_id: &str) -> String;
}

/// Online/offline status and typing indicators.
#[async_trait]
pub trait ChannelStatusAdapter: Send + Sync {
    /// Send a typing indicator to a chat.
    async fn send_typing(&self, account_id: &str, chat_id: &str) -> Result<(), ChannelError>;

    /// Set online/offline status for an account.
    async fn set_online_status(&self, account_id: &str, online: bool) -> Result<(), ChannelError> {
        let _ = (account_id, online);
        Ok(())
    }
}

/// User authentication and identity verification.
#[async_trait]
pub trait ChannelAuthAdapter: Send + Sync {
    /// Validate a user's identity/token from the platform.
    async fn validate_user(&self, account_id: &str, user_id: &str) -> Result<bool, ChannelError>;

    /// Get display name for a user.
    async fn get_user_display_name(
        &self,
        account_id: &str,
        user_id: &str,
    ) -> Result<Option<String>, ChannelError> {
        let _ = (account_id, user_id);
        Ok(None)
    }
}

/// Elevated permissions — admin actions, permission checks.
#[async_trait]
pub trait ChannelElevatedAdapter: Send + Sync {
    /// Check if a user has elevated (admin/operator) permissions.
    async fn has_elevated_access(
        &self,
        account_id: &str,
        user_id: &str,
    ) -> Result<bool, ChannelError>;
}

/// Native slash-command handling.
#[async_trait]
pub trait ChannelCommandAdapter: Send + Sync {
    /// Register native commands with the platform.
    async fn register_commands(
        &self,
        account_id: &str,
        commands: &[Value],
    ) -> Result<(), ChannelError>;

    /// Unregister/clear native commands.
    async fn unregister_commands(&self, account_id: &str) -> Result<(), ChannelError> {
        let _ = account_id;
        Ok(())
    }
}

/// Streaming message delivery (progressive edit-in-place).
#[async_trait]
pub trait ChannelStreamingAdapter: Send + Sync {
    /// Begin a streaming message (returns a handle/message ID).
    async fn stream_start(
        &self,
        ctx: &ChannelOutboundContext,
        initial_text: &str,
    ) -> Result<String, ChannelError>;

    /// Update the streaming message content.
    async fn stream_update(
        &self,
        ctx: &ChannelOutboundContext,
        message_id: &str,
        text: &str,
    ) -> Result<(), ChannelError>;

    /// Finalize the streaming message.
    async fn stream_end(
        &self,
        ctx: &ChannelOutboundContext,
        message_id: &str,
        final_text: &str,
    ) -> Result<(), ChannelError>;
}

/// Thread management.
#[async_trait]
pub trait ChannelThreadingAdapter: Send + Sync {
    /// Create a new thread from a message.
    async fn create_thread(
        &self,
        account_id: &str,
        chat_id: &str,
        message_id: &str,
    ) -> Result<String, ChannelError>;

    /// Get replies in a thread.
    async fn get_thread_replies(
        &self,
        account_id: &str,
        chat_id: &str,
        thread_id: &str,
    ) -> Result<Vec<Value>, ChannelError> {
        let _ = (account_id, chat_id, thread_id);
        Ok(vec![])
    }
}

/// Message lifecycle — editing, deleting, reactions.
#[async_trait]
pub trait ChannelMessagingAdapter: Send + Sync {
    /// Edit an already-sent message.
    async fn edit_message(
        &self,
        account_id: &str,
        chat_id: &str,
        message_id: &str,
        new_text: &str,
    ) -> Result<(), ChannelError>;

    /// Delete/unsend a message.
    async fn delete_message(
        &self,
        account_id: &str,
        chat_id: &str,
        message_id: &str,
    ) -> Result<(), ChannelError>;

    /// Add a reaction to a message.
    async fn add_reaction(
        &self,
        account_id: &str,
        chat_id: &str,
        message_id: &str,
        reaction: &str,
    ) -> Result<(), ChannelError> {
        let _ = (account_id, chat_id, message_id, reaction);
        Err(ChannelError::AdapterNotSupported {
            channel: String::new(),
            adapter: "add_reaction".into(),
        })
    }

    /// Remove a reaction from a message.
    async fn remove_reaction(
        &self,
        account_id: &str,
        chat_id: &str,
        message_id: &str,
        reaction: &str,
    ) -> Result<(), ChannelError> {
        let _ = (account_id, chat_id, message_id, reaction);
        Err(ChannelError::AdapterNotSupported {
            channel: String::new(),
            adapter: "remove_reaction".into(),
        })
    }
}

/// Agent prompt customization per channel.
pub trait ChannelAgentPromptAdapter: Send + Sync {
    /// Return channel-specific system prompt additions.
    fn system_prompt_additions(&self) -> Option<String> {
        None
    }

    /// Return channel-specific instructions for the agent.
    fn channel_instructions(&self) -> Option<String> {
        None
    }
}

/// User/contact directory lookups.
#[async_trait]
pub trait ChannelDirectoryAdapter: Send + Sync {
    /// Look up a user by platform-specific identifier.
    async fn lookup_user(
        &self,
        account_id: &str,
        query: &str,
    ) -> Result<Vec<Value>, ChannelError>;
}

/// Resolve external identifiers (e.g., phone → user ID).
#[async_trait]
pub trait ChannelResolverAdapter: Send + Sync {
    /// Resolve an external identifier to a platform user ID.
    async fn resolve_identifier(
        &self,
        account_id: &str,
        identifier: &str,
    ) -> Result<Option<String>, ChannelError>;
}

/// Message actions (buttons, inline keyboards, callbacks).
#[async_trait]
pub trait ChannelMessageActionAdapter: Send + Sync {
    /// Send a message with action buttons/inline keyboard.
    async fn send_with_actions(
        &self,
        ctx: &ChannelOutboundContext,
        text: &str,
        actions: &[Value],
    ) -> Result<OutboundDeliveryResult, ChannelError>;

    /// Handle a callback/action response from a user.
    async fn handle_action_callback(
        &self,
        account_id: &str,
        callback_data: &Value,
    ) -> Result<(), ChannelError> {
        let _ = (account_id, callback_data);
        Ok(())
    }
}

/// Periodic heartbeat/health-check signals.
#[async_trait]
pub trait ChannelHeartbeatAdapter: Send + Sync {
    /// Send a heartbeat signal for an account.
    async fn send_heartbeat(&self, account_id: &str) -> Result<(), ChannelError>;

    /// Check if an account is still alive/connected.
    async fn is_alive(&self, account_id: &str) -> Result<bool, ChannelError> {
        let _ = account_id;
        Ok(true)
    }
}

/// User onboarding flow (welcome messages, setup wizards).
#[async_trait]
pub trait ChannelOnboardingAdapter: Send + Sync {
    /// Handle a new user's first interaction.
    async fn on_new_user(
        &self,
        account_id: &str,
        user_id: &str,
        chat_id: &str,
    ) -> Result<(), ChannelError>;
}

/// Device/user pairing (linking a chat user to an OpenClaw identity).
#[async_trait]
pub trait ChannelPairingAdapter: Send + Sync {
    /// Initiate pairing for a user.
    async fn initiate_pairing(
        &self,
        account_id: &str,
        user_id: &str,
    ) -> Result<String, ChannelError>;

    /// Confirm a pairing with a code/token.
    async fn confirm_pairing(
        &self,
        account_id: &str,
        user_id: &str,
        code: &str,
    ) -> Result<bool, ChannelError>;
}

/// Channel setup and initialization hooks.
#[async_trait]
pub trait ChannelSetupAdapter: Send + Sync {
    /// Run any one-time setup for the channel (e.g., webhook registration).
    async fn setup(&self, account_id: &str) -> Result<(), ChannelError>;

    /// Tear down channel-specific resources.
    async fn teardown(&self, account_id: &str) -> Result<(), ChannelError> {
        let _ = account_id;
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::ChatType;

    /// A minimal test channel that implements only the required methods.
    struct TestChannel {
        meta: ChannelMeta,
        capabilities: ChannelCapabilities,
    }

    impl TestChannel {
        fn new() -> Self {
            Self {
                meta: ChannelMeta {
                    id: "test".into(),
                    label: "Test Channel".into(),
                    selection_label: None,
                    docs_path: None,
                    blurb: None,
                    order: None,
                    aliases: None,
                },
                capabilities: ChannelCapabilities {
                    chat_types: Some(vec![ChatType::Direct]),
                    ..Default::default()
                },
            }
        }
    }

    impl ChannelPlugin for TestChannel {
        fn meta(&self) -> &ChannelMeta {
            &self.meta
        }

        fn capabilities(&self) -> &ChannelCapabilities {
            &self.capabilities
        }
    }

    #[test]
    fn test_channel_meta() {
        let ch = TestChannel::new();
        assert_eq!(ch.meta().id, "test");
        assert_eq!(ch.meta().label, "Test Channel");
    }

    #[test]
    fn test_channel_capabilities() {
        let ch = TestChannel::new();
        let caps = ch.capabilities();
        assert_eq!(caps.chat_types.as_ref().unwrap(), &[ChatType::Direct]);
        assert!(caps.polls.is_none());
    }

    #[test]
    fn adapters_default_to_none() {
        let ch = TestChannel::new();
        assert!(ch.config_adapter().is_none());
        assert!(ch.gateway_adapter().is_none());
        assert!(ch.outbound_adapter().is_none());
        assert!(ch.security_adapter().is_none());
        assert!(ch.group_adapter().is_none());
        assert!(ch.mention_adapter().is_none());
        assert!(ch.status_adapter().is_none());
        assert!(ch.auth_adapter().is_none());
        assert!(ch.elevated_adapter().is_none());
        assert!(ch.command_adapter().is_none());
        assert!(ch.streaming_adapter().is_none());
        assert!(ch.threading_adapter().is_none());
        assert!(ch.messaging_adapter().is_none());
        assert!(ch.agent_prompt_adapter().is_none());
        assert!(ch.directory_adapter().is_none());
        assert!(ch.resolver_adapter().is_none());
        assert!(ch.message_action_adapter().is_none());
        assert!(ch.heartbeat_adapter().is_none());
        assert!(ch.onboarding_adapter().is_none());
        assert!(ch.pairing_adapter().is_none());
        assert!(ch.setup_adapter().is_none());
    }

    #[test]
    fn channel_plugin_is_object_safe() {
        // Verify ChannelPlugin can be used as a trait object.
        let ch = TestChannel::new();
        let _dyn: &dyn ChannelPlugin = &ch;
        assert_eq!(_dyn.meta().id, "test");
    }
}
