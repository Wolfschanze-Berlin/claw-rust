//! Channel reply bridge — connects the dispatch pipeline to channel outbound adapters.
//!
//! Provides [`ChannelReplyBridge`], a [`ReplyDispatcher`] implementation that
//! routes replies back through the originating channel's outbound adapter.
//! Also hosts the inbound message dispatch loop with dual-path routing
//! (agent runtime API vs Claude Code CLI subprocess).

use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use tokio_util::sync::CancellationToken;
use tracing::{debug, warn};

use claw_channels::plugin::ChannelPlugin;
use claw_channels::types::{ChannelOutboundContext, InboundMessage};
use claw_channels::{FinalizedMsgContext, MsgContext, ReplyPayload};
use claw_claude_code::ClaudeCodeDispatchContext;

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
/// When `cc_ctx` is `Some`, messages are routed through the Claude Code CLI
/// subprocess instead of the direct Anthropic API agent runtime. The Claude
/// Code path handles `/reset` and `/status` commands natively.
///
/// This function runs until the receiver is closed (all senders dropped)
/// or the cancellation token fires.
pub async fn run_dispatch_loop(
    mut rx: tokio::sync::mpsc::UnboundedReceiver<InboundMessage>,
    registry: claw_channels::registry::ChannelRegistry,
    agent_ctx: crate::AgentDispatchContext,
    queue: crate::CommandQueue,
    cc_ctx: Option<ClaudeCodeDispatchContext>,
    cancel: CancellationToken,
) {
    use tracing::info;

    let mode = if cc_ctx.is_some() {
        "claude-code"
    } else {
        "agent-runtime"
    };
    info!(mode, "dispatch loop started — waiting for inbound messages");

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

                // Route based on dispatch mode.
                if let Some(ref cc) = cc_ctx {
                    dispatch_via_claude_code(
                        cc,
                        &inbound,
                        &bridge,
                        cancel.clone(),
                    )
                    .await;
                } else {
                    dispatch_via_agent_runtime(
                        &inbound,
                        &bridge,
                        &agent_ctx,
                        &queue,
                    )
                    .await;
                }
            }
        }
    }

    info!("dispatch loop exited");
}

/// Dispatch a message through the Claude Code CLI subprocess.
///
/// Handles `/reset` and `/status` slash commands; everything else is sent
/// as a prompt to Claude Code.
async fn dispatch_via_claude_code(
    cc_ctx: &ClaudeCodeDispatchContext,
    inbound: &InboundMessage,
    bridge: &ChannelReplyBridge,
    cancel: CancellationToken,
) {
    use tracing::info;

    let body = inbound.msg.body.as_deref().unwrap_or("").trim();

    // Build a session key from channel+account+chat.
    let chat_id = inbound
        .msg
        .from
        .as_deref()
        .or(inbound.msg.to.as_deref())
        .unwrap_or("unknown");
    let session_key = format!("{}:{}:{}", inbound.channel_id, inbound.account_id, chat_id);

    // Handle slash commands.
    let reply = if body.eq_ignore_ascii_case("/reset") {
        match claw_claude_code::dispatch::handle_reset(cc_ctx, &session_key).await {
            Ok(payload) => Some(payload),
            Err(e) => {
                warn!(error = %e, "Claude Code /reset failed");
                Some(ReplyPayload {
                    text: Some(format!("Error resetting session: {e}")),
                    ..Default::default()
                })
            }
        }
    } else if body.eq_ignore_ascii_case("/status") {
        match claw_claude_code::dispatch::handle_status(cc_ctx, &session_key).await {
            Ok(payload) => Some(payload),
            Err(e) => {
                warn!(error = %e, "Claude Code /status failed");
                Some(ReplyPayload {
                    text: Some(format!("Error getting status: {e}")),
                    ..Default::default()
                })
            }
        }
    } else if body.is_empty() {
        debug!("empty message body, skipping Claude Code dispatch");
        None
    } else {
        // Regular message → send to Claude Code.
        match claw_claude_code::dispatch::run_claude_code(
            cc_ctx,
            &session_key,
            body,
            cancel,
        )
        .await
        {
            Ok(result) => {
                info!(
                    session_key,
                    session_id = ?result.session_id,
                    turns = ?result.num_turns,
                    cost = ?result.cost_usd,
                    "Claude Code dispatch complete"
                );
                if result.response_text.is_empty() {
                    None
                } else {
                    Some(ReplyPayload {
                        text: Some(result.response_text),
                        ..Default::default()
                    })
                }
            }
            Err(e) => {
                warn!(session_key, error = %e, "Claude Code dispatch error");
                Some(ReplyPayload {
                    text: Some(format!("Error: {e}")),
                    ..Default::default()
                })
            }
        }
    };

    // Send the reply back through the channel.
    if let Some(ref payload) = reply {
        if let Err(e) = bridge.send_reply(&session_key, payload).await {
            warn!(error = %e, "failed to send Claude Code reply");
        }
    }
}

/// Dispatch a message through the agent runtime API (existing path).
async fn dispatch_via_agent_runtime(
    inbound: &InboundMessage,
    bridge: &ChannelReplyBridge,
    agent_ctx: &crate::AgentDispatchContext,
    queue: &crate::CommandQueue,
) {
    use tracing::info;

    let channel_id = &inbound.channel_id;
    let account_id = &inbound.account_id;

    // Finalize the MsgContext.
    let finalized = FinalizedMsgContext::from_msg_context(inbound.msg.clone());

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
        queue,
        bridge,
        &options,
        agent_ctx,
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
