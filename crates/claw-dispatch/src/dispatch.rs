//! Inbound message dispatch pipeline.
//!
//! Receives a finalized inbound message, detects commands, enqueues
//! processing on the appropriate [`CommandQueue`] lane, and drives the
//! typing → reply → stop-typing lifecycle through a [`ReplyDispatcher`].

use std::sync::{Arc, Mutex};

use anyhow::Result;
use async_trait::async_trait;
use tokio_util::sync::CancellationToken;
use tracing::{debug, info, warn};

use claw_channels::{FinalizedMsgContext, ReplyPayload};

use claw_agent_runtime::{
    AgentRunner, DeliveryError, ResponseSink, RunContext, RuntimeDeps, UserMessage,
};

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

    // Clean up downloaded media file after dispatch completes.
    if let Some(ref media_path) = msg.media_path {
        if let Err(e) = tokio::fs::remove_file(media_path).await {
            debug!(
                session_key = %session_key,
                media_path = %media_path,
                error = %e,
                "media file cleanup failed (may already be deleted)"
            );
        } else {
            debug!(session_key = %session_key, media_path = %media_path, "media file cleaned up");
        }
    }

    Ok(result)
}

// ---------------------------------------------------------------------------
// Agent dispatch context
// ---------------------------------------------------------------------------

/// Dependencies for dispatching messages through the agent runtime.
///
/// Wraps the runner and its deps in `Arc`s so they can be shared across
/// async tasks and moved into `'static` queue closures.
#[derive(Clone)]
pub struct AgentDispatchContext {
    /// The agent runner instance (manages concurrent sessions).
    pub runner: Arc<AgentRunner>,
    /// Runtime dependencies (catalog, provider, pipeline, workspace, etc.).
    pub deps: Arc<RuntimeDeps>,
}

// ---------------------------------------------------------------------------
// CollectingSink
// ---------------------------------------------------------------------------

/// A [`ResponseSink`] that buffers all text into a `String`.
///
/// Used to collect the full agent response inside the queue closure,
/// then convert it to a `ReplyPayload` for dispatch.
struct CollectingSink {
    buffer: Arc<tokio::sync::Mutex<String>>,
    on_partial: Option<Arc<dyn Fn(&str) + Send + Sync>>,
}

impl CollectingSink {
    fn new(on_partial: Option<Arc<dyn Fn(&str) + Send + Sync>>) -> Self {
        Self {
            buffer: Arc::new(tokio::sync::Mutex::new(String::new())),
            on_partial,
        }
    }

    async fn take_text(&self) -> String {
        std::mem::take(&mut *self.buffer.lock().await)
    }
}

#[async_trait]
impl ResponseSink for CollectingSink {
    async fn send_text(&self, text: &str) -> Result<(), DeliveryError> {
        self.buffer.lock().await.push_str(text);
        if let Some(ref cb) = self.on_partial {
            cb(text);
        }
        Ok(())
    }

    async fn finish(&self) -> Result<(), DeliveryError> {
        Ok(())
    }

    fn max_message_size(&self) -> usize {
        4096
    }
}

// ---------------------------------------------------------------------------
// dispatch_with_agent
// ---------------------------------------------------------------------------

/// Drive the inbound dispatch pipeline with full agent runtime integration.
///
/// Replaces the stub in [`dispatch_inbound_message`] with actual agent calls.
/// Pipeline steps:
/// 1. Extract `session_key` and `agent_id` from the finalized context
/// 2. Detect commands (`/` or `!` prefixed) — handle internally
/// 3. Enqueue processing on the command queue lane
/// 4. Within the lane: call `run_agent()` with streaming collection
/// 5. Typing → reply → stop-typing lifecycle via dispatcher
/// 6. Return [`DispatchInboundResult`]
pub async fn dispatch_with_agent(
    msg: &FinalizedMsgContext,
    queue: &CommandQueue,
    dispatcher: &dyn ReplyDispatcher,
    options: &GetReplyOptions,
    agent_ctx: &AgentDispatchContext,
) -> Result<DispatchInboundResult> {
    let session_key = msg
        .session_key
        .as_deref()
        .unwrap_or("unknown")
        .to_owned();

    let agent_id = msg
        .provider
        .as_deref()
        .unwrap_or("default")
        .to_owned();

    // Determine the body for command detection and agent input.
    let body = msg
        .body_for_commands
        .as_deref()
        .or(msg.command_body.as_deref())
        .or(msg.raw_body.as_deref())
        .or(msg.body.as_deref())
        .unwrap_or("")
        .to_owned();

    let detected = detect_command(&body);
    if let Some(ref cmd) = detected {
        debug!(
            session_key = %session_key,
            command = %cmd.name,
            args = %cmd.args,
            native = cmd.is_native,
            "command detected"
        );
    }

    // Send typing indicator immediately.
    if let Err(e) = dispatcher.send_typing(&session_key).await {
        warn!(session_key = %session_key, error = %e, "failed to send typing indicator");
    }

    // Build a UserMessage with optional media attachments.
    let user_message = {
        let attachments = msg
            .media_path
            .as_ref()
            .filter(|p| !p.is_empty())
            .map(|path| {
                let mime_type = msg
                    .media_mime_type
                    .as_deref()
                    .unwrap_or("application/octet-stream")
                    .to_owned();
                let file_name = msg.media_file_name.clone();
                vec![claw_agent_models::Attachment {
                    file_path: path.clone(),
                    mime_type,
                    file_name,
                }]
            });
        UserMessage {
            text: body.clone(),
            attachments,
        }
    };

    // Clone for the closure ('static + Send).
    let sk = session_key.clone();
    let aid = agent_id.clone();
    let cancel = options.cancel.clone();
    let detected_clone = detected.clone();
    let runner = Arc::clone(&agent_ctx.runner);
    let deps = Arc::clone(&agent_ctx.deps);
    let on_partial = options.on_partial_reply.clone();
    let user_id = msg.sender_id.clone();
    let channel = msg
        .provider
        .as_deref()
        .unwrap_or("unknown")
        .to_owned();

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

            // Handle detected commands directly (without agent runtime).
            if let Some(cmd) = detected_clone {
                let reply = ReplyPayload {
                    text: Some(format!(
                        "Command '{}' received (args: '{}')",
                        cmd.name, cmd.args
                    )),
                    ..Default::default()
                };
                return Ok(DispatchInboundResult {
                    session_key: sk,
                    agent_id: aid,
                    reply: Some(reply),
                    error: None,
                });
            }

            // Build the run context for the agent runtime.
            let run_context = RunContext {
                agent_id: aid.clone(),
                channel,
                user_id,
                model_override: None,
            };

            // Create a collecting sink to buffer the response.
            let sink = CollectingSink::new(on_partial);

            // Run the agent through its 11-phase lifecycle.
            let run_result = runner
                .run_agent(&sk, user_message, run_context, &deps, &sink)
                .await;

            match run_result {
                Ok(()) => {
                    let response_text = sink.take_text().await;
                    let reply = if response_text.is_empty() {
                        None
                    } else {
                        Some(ReplyPayload {
                            text: Some(response_text),
                            ..Default::default()
                        })
                    };

                    info!(
                        session_key = %sk,
                        has_reply = reply.is_some(),
                        "agent run completed"
                    );

                    Ok(DispatchInboundResult {
                        session_key: sk,
                        agent_id: aid,
                        reply,
                        error: None,
                    })
                }
                Err(err) => {
                    warn!(
                        session_key = %sk,
                        error = %err,
                        "agent run failed"
                    );

                    Ok(DispatchInboundResult {
                        session_key: sk,
                        agent_id: aid,
                        reply: None,
                        error: Some(err.to_string()),
                    })
                }
            }
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

    // Clean up downloaded media file after dispatch completes.
    if let Some(ref media_path) = msg.media_path {
        if let Err(e) = tokio::fs::remove_file(media_path).await {
            debug!(
                session_key = %session_key,
                media_path = %media_path,
                error = %e,
                "media file cleanup failed (may already be deleted)"
            );
        } else {
            debug!(session_key = %session_key, media_path = %media_path, "media file cleaned up");
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

    // -- dispatch_with_agent -----------------------------------------------

    use claw_agent_models::types::{ChatRequest, ChatResponse, ChatMessage, StreamChunk, Usage};
    use claw_agent_models::provider::ChatStream;
    use claw_agent_models::catalog::ModelEntry;
    use claw_agent_runtime::{
        AgentRunner, CompactionConfig, ContextConfig, PruningConfig, RuntimeDeps, SubscriberConfig,
        TranscriptStore,
    };
    use claw_agent_tools::{PolicyEngine, ToolRegistry, PipelineConfig, ToolPipeline};
    use claw_agent_workspace::AgentWorkspace;
    use claw_agent_models::ModelProvider;
    use std::sync::atomic::{AtomicUsize, Ordering};

    // --- Mock ModelProvider ---

    struct MockModelProvider {
        responses: tokio::sync::Mutex<Vec<String>>,
        call_count: AtomicUsize,
    }

    impl MockModelProvider {
        fn new(responses: Vec<String>) -> Arc<Self> {
            Arc::new(Self {
                responses: tokio::sync::Mutex::new(responses),
                call_count: AtomicUsize::new(0),
            })
        }
    }

    #[async_trait]
    impl ModelProvider for MockModelProvider {
        fn provider_name(&self) -> &str {
            "mock"
        }
        fn supports_tools(&self) -> bool {
            true
        }
        fn max_context_window(&self) -> u64 {
            200_000
        }
        async fn chat_completion(
            &self,
            _request: &ChatRequest,
        ) -> Result<ChatResponse, claw_agent_models::ModelError> {
            unimplemented!("use streaming")
        }
        async fn chat_completion_stream(
            &self,
            _request: &ChatRequest,
        ) -> Result<ChatStream, claw_agent_models::ModelError> {
            self.call_count.fetch_add(1, Ordering::Relaxed);
            let mut resps = self.responses.lock().await;
            let text = if resps.is_empty() {
                "default".to_owned()
            } else {
                resps.remove(0)
            };
            Ok(Box::pin(futures_util::stream::iter(vec![
                Ok(StreamChunk::ContentDelta(text)),
                Ok(StreamChunk::Done(Usage {
                    input_tokens: 10,
                    output_tokens: 5,
                })),
            ])))
        }
    }

    // --- Mock TranscriptStore ---

    struct MockTranscriptStore;

    #[async_trait]
    impl TranscriptStore for MockTranscriptStore {
        async fn load(&self, _session_key: &str) -> std::result::Result<Vec<ChatMessage>, String> {
            Ok(vec![])
        }
        async fn save(
            &self,
            _session_key: &str,
            _messages: &[ChatMessage],
        ) -> std::result::Result<(), String> {
            Ok(())
        }
    }

    async fn test_agent_ctx(provider: Arc<dyn ModelProvider>) -> AgentDispatchContext {
        let tmp = tempfile::tempdir().unwrap();
        let workspace = AgentWorkspace::new(tmp.path(), "test");
        workspace.dir().ensure_dirs().await.unwrap();

        let mut catalog = claw_agent_models::ModelCatalog::new();
        catalog.register(ModelEntry {
            id: "test-model".into(),
            name: "Test Model".into(),
            provider: "mock".into(),
            context_window: Some(200_000),
            max_tokens: Some(4096),
            input_modalities: vec![],
            supports_reasoning: false,
            supports_tools: true,
        });
        catalog.set_global_default("test-model");

        AgentDispatchContext {
            runner: Arc::new(AgentRunner::new()),
            deps: Arc::new(RuntimeDeps {
                catalog,
                provider,
                tool_pipeline: Arc::new(ToolPipeline::new(
                    ToolRegistry::new(),
                    PolicyEngine::new(),
                    PipelineConfig::default(),
                )),
                workspace,
                transcript_store: Arc::new(MockTranscriptStore),
                subscriber_config: SubscriberConfig::default(),
                max_tool_iterations: 10,
                context_config: ContextConfig::default(),
                compaction_config: CompactionConfig::default(),
                pruning_config: PruningConfig::default(),
            }),
        }
    }

    #[tokio::test]
    async fn dispatch_with_agent_regular_message() {
        let provider = MockModelProvider::new(vec!["Hello from agent!".into()]);
        let agent_ctx = test_agent_ctx(provider).await;
        let ctx = MsgContext {
            body: Some("hello world".into()),
            session_key: Some("test-session".into()),
            provider: Some("telegram".into()),
            ..Default::default()
        };
        let finalized = FinalizedMsgContext::from_msg_context(ctx);
        let queue = CommandQueue::new();
        let dispatcher = BufferedReplyDispatcher::new();
        let options = GetReplyOptions {
            run_id: "run-agent-1".into(),
            cancel: CancellationToken::new(),
            on_partial_reply: None,
            on_tool_result: None,
        };

        let result = dispatch_with_agent(&finalized, &queue, &dispatcher, &options, &agent_ctx)
            .await
            .unwrap();

        assert_eq!(result.session_key, "test-session");
        assert!(result.error.is_none());
        assert!(result.reply.is_some());
        let text = result.reply.as_ref().unwrap().text.as_deref().unwrap();
        assert_eq!(text, "Hello from agent!");

        // Dispatcher should have received the reply.
        let replies = dispatcher.take_replies();
        assert_eq!(replies.len(), 1);
    }

    #[tokio::test]
    async fn dispatch_with_agent_command_bypasses_runtime() {
        let provider = MockModelProvider::new(vec![]);
        let agent_ctx = test_agent_ctx(provider).await;
        let ctx = MsgContext {
            body: Some("/help me".into()),
            session_key: Some("cmd-session".into()),
            ..Default::default()
        };
        let finalized = FinalizedMsgContext::from_msg_context(ctx);
        let queue = CommandQueue::new();
        let dispatcher = BufferedReplyDispatcher::new();
        let options = GetReplyOptions {
            run_id: "run-cmd".into(),
            cancel: CancellationToken::new(),
            on_partial_reply: None,
            on_tool_result: None,
        };

        let result = dispatch_with_agent(&finalized, &queue, &dispatcher, &options, &agent_ctx)
            .await
            .unwrap();

        // Command should be handled without calling the agent runtime.
        let text = result.reply.as_ref().unwrap().text.as_deref().unwrap();
        assert!(text.contains("help"));
    }

    #[tokio::test]
    async fn dispatch_cleans_up_media_file() {
        // Create a real temp file to verify cleanup.
        let tmp = tempfile::NamedTempFile::new().unwrap();
        let media_path = tmp.path().to_string_lossy().into_owned();
        // Keep the file open — into_temp_path() releases the handle.
        let tmp_path = tmp.into_temp_path();
        assert!(std::path::Path::new(&media_path).exists());

        let ctx = MsgContext {
            body: Some("file attached".into()),
            session_key: Some("media-session".into()),
            media_path: Some(media_path.clone()),
            ..Default::default()
        };
        let finalized = FinalizedMsgContext::from_msg_context(ctx);
        let queue = CommandQueue::new();
        let dispatcher = BufferedReplyDispatcher::new();
        let options = GetReplyOptions {
            run_id: "run-media".into(),
            cancel: CancellationToken::new(),
            on_partial_reply: None,
            on_tool_result: None,
        };

        let result = dispatch_inbound_message(&finalized, &queue, &dispatcher, &options)
            .await
            .unwrap();

        assert!(result.error.is_none());
        // The file should have been deleted by the cleanup logic.
        assert!(
            !std::path::Path::new(&media_path).exists(),
            "media file should be cleaned up after dispatch"
        );

        // Prevent tmp_path destructor from failing on missing file.
        let _ = tmp_path;
    }

    #[tokio::test]
    async fn dispatch_no_media_path_no_cleanup_error() {
        // Dispatch without media_path should succeed without errors.
        let ctx = MsgContext {
            body: Some("no media".into()),
            session_key: Some("no-media-session".into()),
            ..Default::default()
        };
        let finalized = FinalizedMsgContext::from_msg_context(ctx);
        let queue = CommandQueue::new();
        let dispatcher = BufferedReplyDispatcher::new();
        let options = GetReplyOptions {
            run_id: "run-no-media".into(),
            cancel: CancellationToken::new(),
            on_partial_reply: None,
            on_tool_result: None,
        };

        let result = dispatch_inbound_message(&finalized, &queue, &dispatcher, &options)
            .await
            .unwrap();

        assert!(result.error.is_none());
    }

    #[tokio::test]
    async fn dispatch_missing_media_file_does_not_fail() {
        // If the media file doesn't exist (already deleted), dispatch should not fail.
        let ctx = MsgContext {
            body: Some("stale path".into()),
            session_key: Some("stale-session".into()),
            media_path: Some("/nonexistent/path/file.jpg".into()),
            ..Default::default()
        };
        let finalized = FinalizedMsgContext::from_msg_context(ctx);
        let queue = CommandQueue::new();
        let dispatcher = BufferedReplyDispatcher::new();
        let options = GetReplyOptions {
            run_id: "run-stale".into(),
            cancel: CancellationToken::new(),
            on_partial_reply: None,
            on_tool_result: None,
        };

        let result = dispatch_inbound_message(&finalized, &queue, &dispatcher, &options)
            .await
            .unwrap();

        // Should succeed — missing file is logged at debug level, not an error.
        assert!(result.error.is_none());
    }

    #[tokio::test]
    async fn dispatch_with_agent_cancellation() {
        let provider = MockModelProvider::new(vec![]);
        let agent_ctx = test_agent_ctx(provider).await;
        let ctx = MsgContext {
            body: Some("test".into()),
            session_key: Some("cancel-session".into()),
            ..Default::default()
        };
        let finalized = FinalizedMsgContext::from_msg_context(ctx);
        let queue = CommandQueue::new();
        let dispatcher = BufferedReplyDispatcher::new();
        let cancel = CancellationToken::new();
        cancel.cancel(); // pre-cancel

        let options = GetReplyOptions {
            run_id: "run-cancel".into(),
            cancel,
            on_partial_reply: None,
            on_tool_result: None,
        };

        let result = dispatch_with_agent(&finalized, &queue, &dispatcher, &options, &agent_ctx)
            .await
            .unwrap();

        assert!(result.error.is_some());
        assert!(result.error.as_deref().unwrap().contains("cancelled"));
    }
}
