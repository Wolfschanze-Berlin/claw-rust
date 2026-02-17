//! Channel reply bridge — connects the dispatch pipeline to channel outbound adapters.
//!
//! Provides [`ChannelReplyBridge`], a [`ReplyDispatcher`] implementation that
//! routes replies back through the originating channel's outbound adapter.

use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use tokio_util::sync::CancellationToken;
use tracing::{debug, warn};

use claw_channels::plugin::ChannelPlugin;
use claw_channels::types::{ChannelOutboundContext, InboundMessage};
use claw_channels::{FinalizedMsgContext, MsgContext, ReplyPayload};

use crate::dispatch::ReplyDispatcher;

// ---------------------------------------------------------------------------
// ChannelReplyBridge
// ---------------------------------------------------------------------------

/// A [`ReplyDispatcher`] that routes replies through the originating channel's
/// outbound adapter.
///
/// Created per-message from the inbound message metadata and the channel
/// plugin reference. Extracts the chat_id from the MsgContext so replies
/// go back to the correct conversation.
pub struct ChannelReplyBridge {
    plugin: Arc<dyn ChannelPlugin>,
    account_id: String,
    chat_id: String,
    reply_to_message_id: Option<String>,
    cancel: CancellationToken,
}

impl ChannelReplyBridge {
    /// Create a reply bridge from an inbound message and its channel plugin.
    ///
    /// The `chat_id` is extracted from the message's `from` field (the sender's
    /// chat ID in Telegram) or falls back to the `to` field.
    pub fn from_inbound(msg: &MsgContext, plugin: Arc<dyn ChannelPlugin>, account_id: &str) -> Self {
        // In Telegram, the `from` field contains the chat ID to reply to.
        // For group chats, `to` is the group ID. We use `from` for DMs and `to` for groups.
        let chat_id = msg
            .from
            .as_deref()
            .or(msg.to.as_deref())
            .unwrap_or("unknown")
            .to_owned();

        let reply_to_message_id = msg.message_sid.clone();

        Self {
            plugin,
            account_id: account_id.to_owned(),
            chat_id,
            reply_to_message_id,
            cancel: CancellationToken::new(),
        }
    }

    fn outbound_context(&self) -> ChannelOutboundContext {
        ChannelOutboundContext {
            account_id: self.account_id.clone(),
            chat_id: self.chat_id.clone(),
            thread_id: None,
            reply_to_message_id: self.reply_to_message_id.clone(),
            cancel: self.cancel.clone(),
        }
    }
}

#[async_trait]
impl ReplyDispatcher for ChannelReplyBridge {
    async fn send_typing(&self, session_key: &str) -> Result<()> {
        debug!(session_key, chat_id = %self.chat_id, "sending typing indicator");
        // Telegram typing indicator could be added here via bot.send_chat_action().
        // For now, this is a no-op — typing indicators are optional.
        Ok(())
    }

    async fn send_reply(&self, session_key: &str, payload: &ReplyPayload) -> Result<()> {
        let Some(outbound) = self.plugin.outbound_adapter() else {
            warn!(session_key, "no outbound adapter available for reply");
            return Ok(());
        };

        let ctx = self.outbound_context();

        // Send text if present.
        if let Some(ref text) = payload.text {
            match outbound.send_text(&ctx, text).await {
                Ok(result) => {
                    debug!(
                        session_key,
                        success = ?result.success,
                        message_id = ?result.message_id,
                        "reply sent"
                    );
                }
                Err(e) => {
                    warn!(session_key, error = %e, "failed to send reply text");
                    return Err(e.into());
                }
            }
        }

        // Send media URLs if present.
        if let Some(ref urls) = payload.media_urls {
            for url in urls {
                if let Err(e) = outbound.send_media(&ctx, url, None).await {
                    warn!(session_key, url, error = %e, "failed to send media");
                }
            }
        } else if let Some(ref url) = payload.media_url {
            if let Err(e) = outbound.send_media(&ctx, url, None).await {
                warn!(session_key, url, error = %e, "failed to send media");
            }
        }

        Ok(())
    }

    async fn stop_typing(&self, _session_key: &str) -> Result<()> {
        // Telegram auto-cancels typing on message send.
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Dispatch loop
// ---------------------------------------------------------------------------

/// Run the inbound message dispatch loop.
///
/// Receives [`InboundMessage`]s from the gateway channel and processes each
/// through the full dispatch pipeline (command detection → agent runtime →
/// reply delivery).
///
/// This function runs until the receiver is closed (all senders dropped)
/// or the cancellation token fires.
pub async fn run_dispatch_loop(
    mut rx: tokio::sync::mpsc::UnboundedReceiver<InboundMessage>,
    registry: claw_channels::registry::ChannelRegistry,
    agent_ctx: crate::AgentDispatchContext,
    queue: crate::CommandQueue,
    cancel: CancellationToken,
) {
    use tracing::info;

    info!("dispatch loop started — waiting for inbound messages");

    loop {
        tokio::select! {
            _ = cancel.cancelled() => {
                info!("dispatch loop cancelled");
                break;
            }
            msg = rx.recv() => {
                let Some(inbound) = msg else {
                    info!("dispatch channel closed — all gateways stopped");
                    break;
                };

                let channel_id = inbound.channel_id.clone();
                let account_id = inbound.account_id.clone();

                debug!(
                    channel = %channel_id,
                    account = %account_id,
                    sender = ?inbound.msg.sender_name,
                    body = ?inbound.msg.body,
                    "dispatching inbound message"
                );

                // Look up the channel plugin.
                let dock = registry.get(&channel_id);
                let plugin = dock.as_ref().and_then(|d| d.plugin().cloned());

                let Some(plugin) = plugin else {
                    warn!(
                        channel = %channel_id,
                        "no plugin registered for channel, dropping message"
                    );
                    continue;
                };

                // Create a per-message reply bridge.
                let bridge = ChannelReplyBridge::from_inbound(
                    &inbound.msg,
                    plugin,
                    &account_id,
                );

                // Finalize the MsgContext.
                let finalized = FinalizedMsgContext::from_msg_context(inbound.msg);

                // Build dispatch options.
                let options = crate::GetReplyOptions {
                    run_id: uuid::Uuid::new_v4().to_string(),
                    cancel: CancellationToken::new(),
                    on_partial_reply: None,
                    on_tool_result: None,
                };

                // Run through the dispatch pipeline with agent runtime.
                match crate::dispatch_with_agent(
                    &finalized,
                    &queue,
                    &bridge,
                    &options,
                    &agent_ctx,
                )
                .await
                {
                    Ok(result) => {
                        if let Some(ref err) = result.error {
                            warn!(
                                session_key = %result.session_key,
                                error = %err,
                                "dispatch completed with error"
                            );
                        } else {
                            info!(
                                session_key = %result.session_key,
                                has_reply = result.reply.is_some(),
                                "dispatch completed successfully"
                            );
                        }
                    }
                    Err(e) => {
                        warn!(
                            channel = %channel_id,
                            account = %account_id,
                            error = %e,
                            "dispatch pipeline error"
                        );
                    }
                }
            }
        }
    }

    info!("dispatch loop exited");
}
