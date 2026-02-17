//! Inbound message dispatch pipeline.
//!
//! Receives a finalized inbound message, detects commands, enqueues
//! processing on the appropriate [`CommandQueue`] lane, and drives the
//! typing → reply → stop-typing lifecycle through a [`ReplyDispatcher`].

use std::sync::{Arc, Mutex};

use anyhow::Result;
use async_trait::async_trait;
use tokio_util::sync::CancellationToken;
use tracing::{debug, warn};

use claw_channels::{FinalizedMsgContext, ReplyPayload};

use crate::command_queue::{CommandQueue, MAIN_LANE};

// ---------------------------------------------------------------------------
// GetReplyOptions
// ---------------------------------------------------------------------------

/// Options governing reply generation for a single dispatch cycle.
#[derive(Clone)]
pub struct GetReplyOptions {
    /// Unique run identifier for tracing / idempotency.
    pub run_id: String,

    /// Cancellation token — aborting this cancels the in-flight reply.
    pub cancel: CancellationToken,

    /// Called with partial reply text during streaming.
    pub on_partial_reply: Option<Arc<dyn Fn(&str) + Send + Sync>>,

    /// Called with tool/function-call results during agent execution.
    pub on_tool_result: Option<Arc<dyn Fn(&str) + Send + Sync>>,
}

// ---------------------------------------------------------------------------
// DispatchInboundResult
// ---------------------------------------------------------------------------

/// The outcome of dispatching a single inbound message.
#[derive(Debug, Clone)]
pub struct DispatchInboundResult {
    /// Session key that was processed.
    pub session_key: String,

    /// Agent that handled the message.
    pub agent_id: String,

    /// The reply payload (if the agent produced one).
    pub reply: Option<ReplyPayload>,

    /// Error message (if the dispatch failed non-fatally).
    pub error: Option<String>,
}

// ---------------------------------------------------------------------------
// ReplyDispatcher trait
// ---------------------------------------------------------------------------

/// Abstracts the channel-side reply lifecycle (typing indicators + delivery).
///
/// Each channel implementation (Telegram, WhatsApp, Gateway WS) provides its
/// own `ReplyDispatcher` so the dispatch pipeline is channel-agnostic.
#[async_trait]
pub trait ReplyDispatcher: Send + Sync {
    /// Show a typing indicator to the user.
    async fn send_typing(&self, session_key: &str) -> Result<()>;

    /// Deliver a reply payload to the user.
    async fn send_reply(&self, session_key: &str, payload: &ReplyPayload) -> Result<()>;

    /// Remove the typing indicator.
    async fn stop_typing(&self, session_key: &str) -> Result<()>;
}

// ---------------------------------------------------------------------------
// BufferedReplyDispatcher
// ---------------------------------------------------------------------------

/// A test-oriented dispatcher that captures replies in memory.
///
/// Useful for unit/integration tests that need to inspect what the dispatch
/// pipeline would have sent without involving a real channel transport.
pub struct BufferedReplyDispatcher {
    replies: Arc<Mutex<Vec<ReplyPayload>>>,
}

impl BufferedReplyDispatcher {
    pub fn new() -> Self {
        Self {
            replies: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Drain and return all captured replies.
    pub fn take_replies(&self) -> Vec<ReplyPayload> {
        let mut guard = self.replies.lock().expect("BufferedReplyDispatcher poisoned");
        std::mem::take(&mut *guard)
    }
}

impl Default for BufferedReplyDispatcher {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ReplyDispatcher for BufferedReplyDispatcher {
    async fn send_typing(&self, _session_key: &str) -> Result<()> {
        Ok(())
    }

    async fn send_reply(&self, _session_key: &str, payload: &ReplyPayload) -> Result<()> {
        let mut guard = self.replies.lock().expect("BufferedReplyDispatcher poisoned");
        guard.push(payload.clone());
        Ok(())
    }

    async fn stop_typing(&self, _session_key: &str) -> Result<()> {
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Command detection
// ---------------------------------------------------------------------------

/// A command detected in the message body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DetectedCommand {
    /// Command name without the prefix (e.g. "reset", "help").
    pub name: String,

    /// Everything after the command name (trimmed). Empty if no args.
    pub args: String,

    /// `true` for Telegram-native `/` commands, `false` for `!`-prefixed.
    pub is_native: bool,
}

/// Detect a slash (`/`) or bang (`!`) command at the start of a message body.
///
/// Returns `None` for regular (non-command) messages.
///
/// # Examples
///
/// ```
/// use claw_dispatch::dispatch::detect_command;
///
/// let cmd = detect_command("/reset gpt-4").unwrap();
/// assert_eq!(cmd.name, "reset");
/// assert_eq!(cmd.args, "gpt-4");
/// assert!(cmd.is_native);
///
/// assert!(detect_command("hello world").is_none());
/// ```
pub fn detect_command(body: &str) -> Option<DetectedCommand> {
    let trimmed = body.trim();
    if trimmed.is_empty() {
        return None;
    }

    let first_char = trimmed.chars().next()?;
    let is_native = match first_char {
        '/' => true,
        '!' => false,
        _ => return None,
    };

    // Strip the prefix and split into name + args.
    let rest = &trimmed[first_char.len_utf8()..];
    if rest.is_empty() || !rest.starts_with(|c: char| c.is_ascii_alphanumeric()) {
        // Bare "/" or "!" with nothing after, or non-alpha start — not a command.
        return None;
    }

    let (name, args) = match rest.find(|c: char| c.is_ascii_whitespace()) {
        Some(pos) => (&rest[..pos], rest[pos..].trim()),
        None => (rest, ""),
    };

    Some(DetectedCommand {
        name: name.to_lowercase(),
        args: args.to_owned(),
        is_native,
    })
}

// ---------------------------------------------------------------------------
// dispatch_inbound_message
// ---------------------------------------------------------------------------

/// Drive the inbound dispatch pipeline for a single message.
///
/// Pipeline steps:
/// 1. Extract `session_key` and `agent_id` from the finalized context
/// 2. Detect commands (`/` or `!` prefixed)
/// 3. Enqueue processing on the command queue lane
/// 4. Within the lane: typing → generate reply → stop typing → send reply
/// 5. Return [`DispatchInboundResult`]
///
/// The actual LLM/agent call is **stubbed** — this builds the pipeline
/// skeleton that will be wired to the agent runtime later.
pub async fn dispatch_inbound_message(
    msg: &FinalizedMsgContext,
    queue: &CommandQueue,
    dispatcher: &dyn ReplyDispatcher,
    options: &GetReplyOptions,
) -> Result<DispatchInboundResult> {
    let session_key = msg
        .session_key
        .as_deref()
        .unwrap_or("unknown")
        .to_owned();

    // Use the provider as a stand-in for agent_id until routing is wired.
    let agent_id = msg
        .provider
        .as_deref()
        .unwrap_or("default")
        .to_owned();

    // Determine the body to use for command detection.
    let body = msg
        .body_for_commands
        .as_deref()
        .or(msg.command_body.as_deref())
        .or(msg.raw_body.as_deref())
        .or(msg.body.as_deref())
        .unwrap_or("");

    let detected = detect_command(body);
    if let Some(ref cmd) = detected {
        debug!(
            session_key = %session_key,
            command = %cmd.name,
            args = %cmd.args,
            native = cmd.is_native,
            "command detected"
        );
    }

    // Clone values for the closure (must be 'static + Send).
    let sk = session_key.clone();
    let aid = agent_id.clone();
    let cancel = options.cancel.clone();
    let detected_clone = detected.clone();

    // We need to pass the dispatcher into the queue closure, but
    // `&dyn ReplyDispatcher` isn't 'static. Wrap the lifecycle calls
    // here at the top level instead — the queue only sequences execution.
    //
    // Send typing before entering the queue so the user gets immediate
    // feedback even if the lane is busy.
    if let Err(e) = dispatcher.send_typing(&session_key).await {
        warn!(session_key = %session_key, error = %e, "failed to send typing indicator");
    }

    let result = queue
        .enqueue_command_in_lane(MAIN_LANE, move || async move {
            // Check cancellation before doing work.
            if cancel.is_cancelled() {
                return Ok(DispatchInboundResult {
                    session_key: sk,
                    agent_id: aid,
                    reply: None,
                    error: Some("cancelled before processing".into()),
                });
            }

            // --- Stub: generate reply ---
            // In a real implementation this calls the agent runtime.
            // For now, produce a stub reply or handle detected commands.
            let reply = if let Some(cmd) = detected_clone {
                ReplyPayload {
                    text: Some(format!("Command '{}' received (args: '{}')", cmd.name, cmd.args)),
                    ..Default::default()
                }
            } else {
                ReplyPayload {
                    text: Some("[stub] reply placeholder".into()),
                    ..Default::default()
                }
            };

            Ok(DispatchInboundResult {
                session_key: sk,
                agent_id: aid,
                reply: Some(reply),
                error: None,
            })
        })
        .await?;

    // Stop typing and send the reply.
    if let Err(e) = dispatcher.stop_typing(&session_key).await {
        warn!(session_key = %session_key, error = %e, "failed to stop typing indicator");
    }

    if let Some(ref payload) = result.reply {
        if let Err(e) = dispatcher.send_reply(&session_key, payload).await {
            warn!(session_key = %session_key, error = %e, "failed to send reply");
        }
    }

    Ok(result)
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use claw_channels::MsgContext;

    // -- detect_command -----------------------------------------------------

    #[test]
    fn detect_slash_command_with_args() {
        let cmd = detect_command("/reset gpt-4").unwrap();
        assert_eq!(cmd.name, "reset");
        assert_eq!(cmd.args, "gpt-4");
        assert!(cmd.is_native);
    }

    #[test]
    fn detect_slash_command_without_args() {
        let cmd = detect_command("/help").unwrap();
        assert_eq!(cmd.name, "help");
        assert_eq!(cmd.args, "");
        assert!(cmd.is_native);
    }

    #[test]
    fn detect_bang_command() {
        let cmd = detect_command("!status check all").unwrap();
        assert_eq!(cmd.name, "status");
        assert_eq!(cmd.args, "check all");
        assert!(!cmd.is_native);
    }

    #[test]
    fn detect_command_trims_whitespace() {
        let cmd = detect_command("  /ping  ").unwrap();
        assert_eq!(cmd.name, "ping");
        assert_eq!(cmd.args, "");
    }

    #[test]
    fn regular_message_returns_none() {
        assert!(detect_command("hello world").is_none());
    }

    #[test]
    fn empty_message_returns_none() {
        assert!(detect_command("").is_none());
        assert!(detect_command("   ").is_none());
    }

    #[test]
    fn bare_slash_returns_none() {
        assert!(detect_command("/").is_none());
        assert!(detect_command("!").is_none());
    }

    #[test]
    fn command_name_is_lowercased() {
        let cmd = detect_command("/Reset").unwrap();
        assert_eq!(cmd.name, "reset");
    }

    #[test]
    fn slash_with_special_char_not_command() {
        // "/!" or "/#" should not be a command
        assert!(detect_command("/#tag").is_none());
    }

    // -- BufferedReplyDispatcher --------------------------------------------

    #[tokio::test]
    async fn buffered_dispatcher_collects_replies() {
        let dispatcher = BufferedReplyDispatcher::new();

        dispatcher
            .send_reply(
                "key1",
                &ReplyPayload {
                    text: Some("hello".into()),
                    ..Default::default()
                },
            )
            .await
            .unwrap();

        dispatcher
            .send_reply(
                "key2",
                &ReplyPayload {
                    text: Some("world".into()),
                    ..Default::default()
                },
            )
            .await
            .unwrap();

        let replies = dispatcher.take_replies();
        assert_eq!(replies.len(), 2);
        assert_eq!(replies[0].text.as_deref(), Some("hello"));
        assert_eq!(replies[1].text.as_deref(), Some("world"));

        // After take_replies, buffer should be empty.
        assert!(dispatcher.take_replies().is_empty());
    }

    #[tokio::test]
    async fn buffered_dispatcher_typing_is_noop() {
        let dispatcher = BufferedReplyDispatcher::new();
        dispatcher.send_typing("key").await.unwrap();
        dispatcher.stop_typing("key").await.unwrap();
        assert!(dispatcher.take_replies().is_empty());
    }

    // -- DispatchInboundResult ----------------------------------------------

    #[test]
    fn dispatch_result_construction() {
        let result = DispatchInboundResult {
            session_key: "agent:gpt:telegram:main".into(),
            agent_id: "gpt-4".into(),
            reply: Some(ReplyPayload {
                text: Some("hi".into()),
                ..Default::default()
            }),
            error: None,
        };
        assert_eq!(result.session_key, "agent:gpt:telegram:main");
        assert!(result.error.is_none());
        assert!(result.reply.is_some());
    }

    #[test]
    fn dispatch_result_with_error() {
        let result = DispatchInboundResult {
            session_key: "s1".into(),
            agent_id: "a1".into(),
            reply: None,
            error: Some("timeout".into()),
        };
        assert_eq!(result.error.as_deref(), Some("timeout"));
        assert!(result.reply.is_none());
    }

    // -- dispatch_inbound_message -------------------------------------------

    #[tokio::test]
    async fn dispatch_regular_message() {
        let ctx = MsgContext {
            body: Some("hello world".into()),
            session_key: Some("agent:gpt:telegram:main".into()),
            provider: Some("telegram".into()),
            ..Default::default()
        };
        let finalized = FinalizedMsgContext::from_msg_context(ctx);
        let queue = CommandQueue::new();
        let dispatcher = BufferedReplyDispatcher::new();
        let options = GetReplyOptions {
            run_id: "run-1".into(),
            cancel: CancellationToken::new(),
            on_partial_reply: None,
            on_tool_result: None,
        };

        let result = dispatch_inbound_message(&finalized, &queue, &dispatcher, &options)
            .await
            .unwrap();

        assert_eq!(result.session_key, "agent:gpt:telegram:main");
        assert_eq!(result.agent_id, "telegram");
        assert!(result.error.is_none());
        assert!(result.reply.is_some());

        // Dispatcher should have received the reply.
        let replies = dispatcher.take_replies();
        assert_eq!(replies.len(), 1);
    }

    #[tokio::test]
    async fn dispatch_command_message() {
        let ctx = MsgContext {
            body: Some("/reset gpt-4".into()),
            session_key: Some("s1".into()),
            ..Default::default()
        };
        let finalized = FinalizedMsgContext::from_msg_context(ctx);
        let queue = CommandQueue::new();
        let dispatcher = BufferedReplyDispatcher::new();
        let options = GetReplyOptions {
            run_id: "run-2".into(),
            cancel: CancellationToken::new(),
            on_partial_reply: None,
            on_tool_result: None,
        };

        let result = dispatch_inbound_message(&finalized, &queue, &dispatcher, &options)
            .await
            .unwrap();

        let text = result.reply.as_ref().unwrap().text.as_deref().unwrap();
        assert!(text.contains("reset"));
        assert!(text.contains("gpt-4"));
    }

    #[tokio::test]
    async fn dispatch_with_cancellation() {
        let ctx = MsgContext {
            body: Some("hi".into()),
            session_key: Some("s1".into()),
            ..Default::default()
        };
        let finalized = FinalizedMsgContext::from_msg_context(ctx);
        let queue = CommandQueue::new();
        let dispatcher = BufferedReplyDispatcher::new();
        let cancel = CancellationToken::new();
        cancel.cancel(); // pre-cancel

        let options = GetReplyOptions {
            run_id: "run-3".into(),
            cancel,
            on_partial_reply: None,
            on_tool_result: None,
        };

        let result = dispatch_inbound_message(&finalized, &queue, &dispatcher, &options)
            .await
            .unwrap();

        // Should report cancellation.
        assert!(result.error.is_some());
        assert!(result.error.as_deref().unwrap().contains("cancelled"));
    }

    #[tokio::test]
    async fn dispatch_defaults_for_missing_fields() {
        let ctx = MsgContext::default();
        let finalized = FinalizedMsgContext::from_msg_context(ctx);
        let queue = CommandQueue::new();
        let dispatcher = BufferedReplyDispatcher::new();
        let options = GetReplyOptions {
            run_id: "run-4".into(),
            cancel: CancellationToken::new(),
            on_partial_reply: None,
            on_tool_result: None,
        };

        let result = dispatch_inbound_message(&finalized, &queue, &dispatcher, &options)
            .await
            .unwrap();

        assert_eq!(result.session_key, "unknown");
        assert_eq!(result.agent_id, "default");
    }
}
