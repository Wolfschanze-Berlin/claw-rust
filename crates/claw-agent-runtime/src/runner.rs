//! Core agent execution engine.
//!
//! [`AgentRunner`] manages the lifecycle of agent runs — one per session key
//! at a time. While a run is active, incoming messages are buffered in a
//! per-session [`MessageQueue`] and drained when the run completes.
//!
//! The execution loop follows an 11-phase lifecycle:
//! 1. Resolve session key
//! 2. Acquire session write lock (at-most-one-run per session)
//! 3. Load conversation history from transcript store
//! 4. Check context window budget (trigger compaction if exceeded)
//! 5. Construct system prompt via `PromptBuilder`
//! 6. Select model via `ModelCatalog`
//! 7. Make streaming API call
//! 8. Process tool call loop (execute tools, append results, re-call)
//! 9. Deliver response chunks via `StreamSubscriber`
//! 10. Persist transcript
//! 11. Release lock and drain queue

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use anyhow::Result;
use async_trait::async_trait;
use tokio::sync::{Mutex, Notify};
use tokio::time::Instant;
use tokio_util::sync::CancellationToken;
use tracing::{debug, info};

use claw_agent_models::{
    ChatMessage, ChatRequest, ModelCatalog, ModelProvider, Role,
    SelectionContext, ToolDefinition,
};
use claw_agent_tools::{PolicyContext, ToolPipeline};
use claw_agent_workspace::AgentWorkspace;

use crate::error::RuntimeError;
use crate::prompt::{PromptBuilder, PromptContext};
use crate::queue::MessageQueue;
use crate::subscriber::{ResponseSink, StreamSubscriber, SubscriberConfig};

// ---------------------------------------------------------------------------
// Transcript store trait
// ---------------------------------------------------------------------------

/// Persistence backend for conversation transcripts.
///
/// Abstracts over the actual storage mechanism (file-based, SQLite, etc.)
/// so the runner can load/save transcripts without knowing the backend.
#[async_trait]
pub trait TranscriptStore: Send + Sync {
    /// Load the transcript for a session. Returns an empty vec if none exists.
    async fn load(&self, session_key: &str) -> Result<Vec<ChatMessage>, String>;
    /// Save the transcript for a session, replacing any existing content.
    async fn save(&self, session_key: &str, messages: &[ChatMessage]) -> Result<(), String>;
}

// ---------------------------------------------------------------------------
// Supporting types
// ---------------------------------------------------------------------------

/// Context passed into each agent run describing *which* agent, channel, etc.
#[derive(Debug, Clone)]
pub struct RunContext {
    /// Identifier of the agent definition to execute.
    pub agent_id: String,
    /// Channel the message originated from (e.g. "telegram", "discord").
    pub channel: String,
    /// Optional user identifier within the channel.
    pub user_id: Option<String>,
    /// Optional model override for this run.
    pub model_override: Option<String>,
}

/// A message waiting in the per-session queue while a run is already active.
#[derive(Debug, Clone)]
pub struct QueuedMessage {
    /// The textual content of the message.
    pub content: String,
    /// Opaque channel-specific context (adapter payload, metadata, etc.).
    pub channel_context: serde_json::Value,
    /// When this message was enqueued.
    pub queued_at: Instant,
}

/// Tracks the state of a single in-flight agent run.
#[derive(Debug)]
struct RunState {
    /// Session key this run belongs to.
    #[allow(dead_code)]
    session_key: String,
    /// Token to cancel this run cooperatively.
    cancel_token: CancellationToken,
    /// Whether the run is currently streaming a response.
    is_streaming: AtomicBool,
    /// When the run started.
    #[allow(dead_code)]
    started_at: Instant,
}

/// Dependencies injected into the agent runner for execution.
///
/// Bundles all the services the 11-phase loop needs so `AgentRunner`
/// doesn't grow a massive constructor signature.
pub struct RuntimeDeps {
    /// Model catalog for resolving which model to use.
    pub catalog: ModelCatalog,
    /// Model provider (may be a `FailoverChain`).
    pub provider: Arc<dyn ModelProvider>,
    /// Tool execution pipeline.
    pub tool_pipeline: Arc<ToolPipeline>,
    /// Agent workspace for loading identity/skills/memory files.
    pub workspace: AgentWorkspace,
    /// Transcript persistence backend.
    pub transcript_store: Arc<dyn TranscriptStore>,
    /// Streaming subscriber configuration.
    pub subscriber_config: SubscriberConfig,
    /// Maximum tool call loop iterations before aborting.
    pub max_tool_iterations: usize,
}

/// Configuration for context window management.
#[derive(Debug, Clone)]
pub struct ContextConfig {
    /// Maximum fraction of context window to use (0.0–1.0).
    pub budget_fraction: f64,
}

impl Default for ContextConfig {
    fn default() -> Self {
        Self {
            budget_fraction: 0.8,
        }
    }
}

// ---------------------------------------------------------------------------
// AgentRunner
// ---------------------------------------------------------------------------

/// Manages agent run lifecycles — at most one active run per session key.
///
/// # Concurrency model
///
/// - Each session key can have at most one active run at a time.
/// - Messages arriving while a run is active are buffered in a [`MessageQueue`].
/// - Callers can `abort_run` to cooperatively cancel via [`CancellationToken`].
/// - `wait_for_run_end` blocks until a session's active run finishes.
pub struct AgentRunner {
    /// Active runs keyed by session key.
    active_runs: Arc<Mutex<HashMap<String, RunState>>>,
    /// Per-session message queues for buffering during active runs.
    message_queues: Arc<Mutex<HashMap<String, MessageQueue>>>,
    /// Notifier signalled whenever a run completes (any session).
    run_ended: Arc<Notify>,
}

impl AgentRunner {
    /// Create a new `AgentRunner` with no active runs.
    pub fn new() -> Self {
        Self {
            active_runs: Arc::new(Mutex::new(HashMap::new())),
            message_queues: Arc::new(Mutex::new(HashMap::new())),
            run_ended: Arc::new(Notify::new()),
        }
    }

    /// Start an agent run for the given session.
    ///
    /// Executes the full 11-phase lifecycle: load history, build prompt,
    /// call model, process tool calls, deliver response, persist transcript.
    ///
    /// Returns [`RuntimeError::SessionBusy`] if the session already has an
    /// active run.
    pub async fn run_agent(
        &self,
        session_key: &str,
        message: &str,
        context: RunContext,
        deps: &RuntimeDeps,
        sink: &dyn ResponseSink,
    ) -> Result<(), RuntimeError> {
        // Phase 1–2: Resolve session key and acquire lock.
        {
            let runs = self.active_runs.lock().await;
            if runs.contains_key(session_key) {
                return Err(RuntimeError::SessionBusy {
                    session_key: session_key.to_owned(),
                });
            }
        }

        let cancel_token = CancellationToken::new();
        let state = RunState {
            session_key: session_key.to_owned(),
            cancel_token: cancel_token.clone(),
            is_streaming: AtomicBool::new(false),
            started_at: Instant::now(),
        };

        self.active_runs
            .lock()
            .await
            .insert(session_key.to_owned(), state);

        info!(
            session_key,
            agent_id = %context.agent_id,
            channel = %context.channel,
            "agent run started"
        );
        debug!(session_key, message, "run message content");

        // Execute the lifecycle, ensuring cleanup on any outcome.
        let result = self
            .execute_lifecycle(session_key, message, &context, deps, sink, &cancel_token)
            .await;

        // Phase 11: Release lock and notify waiters.
        self.active_runs.lock().await.remove(session_key);
        self.run_ended.notify_waiters();

        result
    }

    /// The core 11-phase execution lifecycle.
    async fn execute_lifecycle(
        &self,
        session_key: &str,
        message: &str,
        context: &RunContext,
        deps: &RuntimeDeps,
        sink: &dyn ResponseSink,
        cancel_token: &CancellationToken,
    ) -> Result<(), RuntimeError> {
        // Phase 3: Load conversation history.
        let mut history = deps
            .transcript_store
            .load(session_key)
            .await
            .map_err(RuntimeError::TranscriptError)?;

        debug!(
            session_key,
            history_len = history.len(),
            "loaded conversation history"
        );

        // Append the new user message.
        history.push(ChatMessage {
            role: Role::User,
            content: message.to_owned(),
            name: None,
            tool_calls: None,
            tool_call_id: None,
        });

        // Phase 4: Check context window budget.
        // Rough token estimation: ~4 chars per token. This is a conservative
        // heuristic; real tokenization would use the provider's tokenizer.
        let estimated_tokens = history.iter().map(|m| m.content.len() / 4).sum::<usize>() as u64;
        let budget = (deps.provider.max_context_window() as f64
            * ContextConfig::default().budget_fraction) as u64;

        if estimated_tokens > budget {
            debug!(
                estimated_tokens,
                budget, "context budget exceeded, compaction would trigger"
            );
            // Compaction engine (#106) not yet implemented — for now, truncate
            // older messages to fit within budget. Keep system/last-N messages.
            let keep = history.len().min(20);
            let drain_count = history.len().saturating_sub(keep);
            if drain_count > 0 {
                history.drain(..drain_count);
                info!(
                    session_key,
                    removed = drain_count,
                    remaining = history.len(),
                    "truncated history to fit context budget"
                );
            }
        }

        // Phase 5: Construct system prompt.
        let tool_defs = Self::collect_tool_definitions(deps);
        let prompt_context = PromptContext {
            platform: context.channel.clone(),
            channel_name: None,
            session_id: session_key.to_owned(),
            user_identity: context.user_id.clone(),
            timestamp: chrono::Utc::now().to_rfc3339(),
            formatting_rules: None,
        };
        let system_prompt =
            PromptBuilder::build(&deps.workspace, &tool_defs, &prompt_context).await;

        // Phase 6: Select model via catalog.
        let selection_ctx = SelectionContext {
            session_override: context.model_override.clone(),
            agent_model: None,
            channel_default: None,
            global_default: None,
        };
        let model_entry = deps.catalog.resolve(&selection_ctx).ok_or_else(|| {
            RuntimeError::NoModelResolved {
                session_key: session_key.to_owned(),
            }
        })?;
        let model_id = model_entry.id.clone();
        let max_tokens = model_entry.max_tokens;

        info!(
            session_key,
            model = %model_id,
            "model selected"
        );

        // Phases 7–9: Streaming call + tool loop.
        let tools = if model_entry.supports_tools {
            Some(tool_defs)
        } else {
            None
        };

        let mut iteration = 0usize;
        loop {
            // Check cancellation before each model call.
            if cancel_token.is_cancelled() {
                return Err(RuntimeError::Cancelled {
                    session_key: session_key.to_owned(),
                });
            }

            // Guard against runaway tool loops.
            if iteration >= deps.max_tool_iterations {
                return Err(RuntimeError::ToolLoopExceeded {
                    max: deps.max_tool_iterations,
                });
            }
            iteration += 1;

            let request = ChatRequest {
                model: model_id.clone(),
                messages: history.clone(),
                system: Some(system_prompt.clone()),
                tools: tools.clone(),
                max_tokens,
                temperature: None,
                stream: true,
            };

            // Phase 7: Make streaming API call.
            debug!(
                session_key,
                iteration, "calling model (streaming)"
            );
            self.set_streaming(session_key, true).await;

            let stream = deps.provider.chat_completion_stream(&request).await?;

            // Phase 9: Deliver response chunks via streaming subscriber.
            let subscriber = StreamSubscriber::new(deps.subscriber_config.clone());
            let sub_result = subscriber
                .subscribe(stream, sink)
                .await
                .map_err(RuntimeError::ModelError)?;

            self.set_streaming(session_key, false).await;

            debug!(
                session_key,
                chunks = sub_result.chunks_delivered,
                tool_calls = sub_result.tool_calls.len(),
                "stream completed"
            );

            // Append assistant response to history.
            let assistant_tool_calls = if sub_result.tool_calls.is_empty() {
                None
            } else {
                Some(sub_result.tool_calls.clone())
            };

            history.push(ChatMessage {
                role: Role::Assistant,
                content: sub_result.full_text.clone(),
                name: None,
                tool_calls: assistant_tool_calls,
                tool_call_id: None,
            });

            // Phase 8: Process tool calls (if any).
            if sub_result.tool_calls.is_empty() {
                // No tool calls — response is final.
                break;
            }

            info!(
                session_key,
                tool_count = sub_result.tool_calls.len(),
                iteration,
                "processing tool calls"
            );

            let policy_context = PolicyContext {
                agent_id: context.agent_id.clone(),
                group_id: None,
                provider: Some(model_entry.provider.clone()),
                is_elevated: false,
            };

            for tool_call in &sub_result.tool_calls {
                // Check cancellation between tool calls.
                if cancel_token.is_cancelled() {
                    return Err(RuntimeError::Cancelled {
                        session_key: session_key.to_owned(),
                    });
                }

                let result = deps
                    .tool_pipeline
                    .execute_tool_call(tool_call, &policy_context)
                    .await;

                debug!(
                    session_key,
                    tool = %tool_call.name,
                    is_error = result.is_error,
                    "tool call executed"
                );

                // Append tool result to history.
                history.push(ChatMessage {
                    role: Role::Tool,
                    content: result.content,
                    name: Some(tool_call.name.clone()),
                    tool_calls: None,
                    tool_call_id: Some(result.tool_call_id),
                });
            }

            // Loop back to call the model again with tool results.
        }

        // Phase 10: Persist transcript.
        deps.transcript_store
            .save(session_key, &history)
            .await
            .map_err(RuntimeError::TranscriptError)?;

        info!(
            session_key,
            history_len = history.len(),
            iterations = iteration,
            "agent run completed"
        );

        Ok(())
    }

    /// Collect tool definitions from the pipeline's registry for the system
    /// prompt and the API request.
    fn collect_tool_definitions(deps: &RuntimeDeps) -> Vec<ToolDefinition> {
        deps.tool_pipeline.tool_definitions()
    }

    /// Helper to set the streaming flag on the active run state.
    async fn set_streaming(&self, session_key: &str, streaming: bool) {
        let runs = self.active_runs.lock().await;
        if let Some(state) = runs.get(session_key) {
            state.is_streaming.store(streaming, Ordering::Relaxed);
        }
    }

    /// Cooperatively cancel an active run for the given session.
    ///
    /// No-op if the session has no active run.
    pub fn abort_run(&self, session_key: &str) {
        let runs = self.active_runs.clone();
        let key = session_key.to_owned();
        // Spawn a task to avoid holding the lock synchronously in a
        // potentially sync call-site.
        tokio::spawn(async move {
            let runs = runs.lock().await;
            if let Some(state) = runs.get(&key) {
                info!(session_key = %key, "aborting agent run");
                state.cancel_token.cancel();
            } else {
                debug!(session_key = %key, "abort_run called but no active run");
            }
        });
    }

    /// Return `true` if the session currently has an active run.
    pub async fn is_run_active(&self, session_key: &str) -> bool {
        self.active_runs.lock().await.contains_key(session_key)
    }

    /// Wait until the session's active run finishes.
    ///
    /// Returns immediately if no run is active for the session.
    pub async fn wait_for_run_end(&self, session_key: &str) {
        loop {
            if !self.is_run_active(session_key).await {
                return;
            }
            self.run_ended.notified().await;
        }
    }

    /// Enqueue a message for the session, creating the queue if needed.
    pub async fn queue_message(
        &self,
        session_key: &str,
        message: QueuedMessage,
    ) -> Result<()> {
        let mut queues = self.message_queues.lock().await;
        let queue = queues
            .entry(session_key.to_owned())
            .or_insert_with(MessageQueue::new);
        queue.enqueue(message).await;
        debug!(session_key, "message enqueued");
        Ok(())
    }

    /// Return `true` if the session's active run is currently streaming.
    ///
    /// Returns `false` if the session has no active run.
    pub async fn is_streaming(&self, session_key: &str) -> bool {
        let runs = self.active_runs.lock().await;
        runs.get(session_key)
            .map(|s| s.is_streaming.load(Ordering::Relaxed))
            .unwrap_or(false)
    }
}

impl Default for AgentRunner {
    fn default() -> Self {
        Self::new()
    }
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use claw_agent_models::types::{
        ChatResponse, FinishReason, StreamChunk, ToolCall, Usage,
    };
    use claw_agent_models::provider::ChatStream;
    use claw_agent_models::catalog::ModelEntry;
    use claw_agent_tools::{PolicyEngine, ToolRegistry, PipelineConfig};
    use crate::subscriber::DeliveryError;
    use std::sync::atomic::AtomicUsize;

    // -----------------------------------------------------------------------
    // Mock TranscriptStore
    // -----------------------------------------------------------------------

    struct MockTranscriptStore {
        messages: Mutex<HashMap<String, Vec<ChatMessage>>>,
    }

    impl MockTranscriptStore {
        fn new() -> Self {
            Self {
                messages: Mutex::new(HashMap::new()),
            }
        }

        async fn saved(&self, key: &str) -> Option<Vec<ChatMessage>> {
            self.messages.lock().await.get(key).cloned()
        }
    }

    #[async_trait]
    impl TranscriptStore for MockTranscriptStore {
        async fn load(&self, session_key: &str) -> Result<Vec<ChatMessage>, String> {
            let store = self.messages.lock().await;
            Ok(store.get(session_key).cloned().unwrap_or_default())
        }

        async fn save(&self, session_key: &str, messages: &[ChatMessage]) -> Result<(), String> {
            self.messages
                .lock()
                .await
                .insert(session_key.to_owned(), messages.to_vec());
            Ok(())
        }
    }

    // -----------------------------------------------------------------------
    // Mock ModelProvider
    // -----------------------------------------------------------------------

    struct MockModelProvider {
        name: &'static str,
        responses: Mutex<Vec<MockResponse>>,
        call_count: AtomicUsize,
    }

    enum MockResponse {
        Text(String),
        WithToolCalls(String, Vec<ToolCall>),
    }

    impl MockModelProvider {
        fn new(name: &'static str, responses: Vec<MockResponse>) -> Arc<Self> {
            Arc::new(Self {
                name,
                responses: Mutex::new(responses),
                call_count: AtomicUsize::new(0),
            })
        }

        fn calls(&self) -> usize {
            self.call_count.load(Ordering::Relaxed)
        }
    }

    #[async_trait]
    impl ModelProvider for MockModelProvider {
        fn provider_name(&self) -> &str {
            self.name
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
            self.call_count.fetch_add(1, Ordering::Relaxed);
            let mut responses = self.responses.lock().await;
            let resp = if responses.is_empty() {
                MockResponse::Text("default response".into())
            } else {
                responses.remove(0)
            };
            match resp {
                MockResponse::Text(text) => Ok(ChatResponse {
                    id: "resp-1".into(),
                    model: self.name.into(),
                    content: text,
                    tool_calls: vec![],
                    usage: Usage {
                        input_tokens: 10,
                        output_tokens: 5,
                    },
                    finish_reason: FinishReason::Stop,
                }),
                MockResponse::WithToolCalls(text, calls) => Ok(ChatResponse {
                    id: "resp-1".into(),
                    model: self.name.into(),
                    content: text,
                    tool_calls: calls,
                    usage: Usage {
                        input_tokens: 10,
                        output_tokens: 5,
                    },
                    finish_reason: FinishReason::ToolUse,
                }),
            }
        }

        async fn chat_completion_stream(
            &self,
            _request: &ChatRequest,
        ) -> Result<ChatStream, claw_agent_models::ModelError> {
            self.call_count.fetch_add(1, Ordering::Relaxed);
            let mut responses = self.responses.lock().await;
            let resp = if responses.is_empty() {
                MockResponse::Text("default response".into())
            } else {
                responses.remove(0)
            };
            match resp {
                MockResponse::Text(text) => {
                    Ok(Box::pin(futures_util::stream::iter(vec![
                        Ok(StreamChunk::ContentDelta(text)),
                        Ok(StreamChunk::Done(Usage {
                            input_tokens: 10,
                            output_tokens: 5,
                        })),
                    ])))
                }
                MockResponse::WithToolCalls(text, calls) => {
                    let mut chunks: Vec<Result<StreamChunk, claw_agent_models::ModelError>> =
                        Vec::new();
                    if !text.is_empty() {
                        chunks.push(Ok(StreamChunk::ContentDelta(text)));
                    }
                    for tc in &calls {
                        chunks.push(Ok(StreamChunk::ToolCallStart {
                            id: tc.id.clone(),
                            name: tc.name.clone(),
                        }));
                        chunks.push(Ok(StreamChunk::ToolCallDelta {
                            id: tc.id.clone(),
                            arguments_delta: serde_json::to_string(&tc.arguments).unwrap(),
                        }));
                    }
                    chunks.push(Ok(StreamChunk::Done(Usage {
                        input_tokens: 10,
                        output_tokens: 5,
                    })));
                    Ok(Box::pin(futures_util::stream::iter(chunks)))
                }
            }
        }
    }

    // -----------------------------------------------------------------------
    // Mock ResponseSink
    // -----------------------------------------------------------------------

    struct MockSink {
        chunks: Mutex<Vec<String>>,
    }

    impl MockSink {
        fn new() -> Self {
            Self {
                chunks: Mutex::new(Vec::new()),
            }
        }

        async fn collected(&self) -> Vec<String> {
            self.chunks.lock().await.clone()
        }
    }

    #[async_trait]
    impl ResponseSink for MockSink {
        async fn send_text(&self, text: &str) -> Result<(), DeliveryError> {
            self.chunks.lock().await.push(text.to_owned());
            Ok(())
        }

        async fn finish(&self) -> Result<(), DeliveryError> {
            Ok(())
        }

        fn max_message_size(&self) -> usize {
            4096
        }
    }

    // -----------------------------------------------------------------------
    // Helpers
    // -----------------------------------------------------------------------

    fn test_context() -> RunContext {
        RunContext {
            agent_id: "test-agent".to_owned(),
            channel: "test-channel".to_owned(),
            user_id: None,
            model_override: None,
        }
    }

    fn test_message() -> QueuedMessage {
        QueuedMessage {
            content: "hello".to_owned(),
            channel_context: serde_json::json!({}),
            queued_at: Instant::now(),
        }
    }

    fn test_catalog() -> ModelCatalog {
        let mut catalog = ModelCatalog::new();
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
        catalog
    }

    async fn test_deps(
        provider: Arc<dyn ModelProvider>,
        store: Arc<dyn TranscriptStore>,
    ) -> RuntimeDeps {
        let tmp = tempfile::tempdir().unwrap();
        let workspace = AgentWorkspace::new(tmp.path(), "test");
        workspace.dir().ensure_dirs().await.unwrap();

        RuntimeDeps {
            catalog: test_catalog(),
            provider,
            tool_pipeline: Arc::new(ToolPipeline::new(
                ToolRegistry::new(),
                PolicyEngine::new(),
                PipelineConfig::default(),
            )),
            workspace,
            transcript_store: store,
            subscriber_config: SubscriberConfig::default(),
            max_tool_iterations: 10,
        }
    }

    // -----------------------------------------------------------------------
    // Tests
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn runner_creation() {
        let runner = AgentRunner::new();
        assert!(!runner.is_run_active("any-session").await);
    }

    #[tokio::test]
    async fn is_run_active_returns_false_initially() {
        let runner = AgentRunner::new();
        assert!(!runner.is_run_active("session-1").await);
        assert!(!runner.is_run_active("session-2").await);
    }

    #[tokio::test]
    async fn queue_message_enqueues_correctly() {
        let runner = AgentRunner::new();
        let msg = test_message();

        runner.queue_message("s1", msg.clone()).await.unwrap();
        runner.queue_message("s1", msg).await.unwrap();

        let queues = runner.message_queues.lock().await;
        let q = queues.get("s1").unwrap();
        assert_eq!(q.len().await, 2);
    }

    #[tokio::test]
    async fn abort_run_on_nonexistent_session_does_not_panic() {
        let runner = AgentRunner::new();
        // Should be a no-op, no panic.
        runner.abort_run("nonexistent");
        // Give the spawned task a moment to complete.
        tokio::task::yield_now().await;
    }

    #[tokio::test]
    async fn is_streaming_returns_false_when_no_run() {
        let runner = AgentRunner::new();
        assert!(!runner.is_streaming("no-session").await);
    }

    #[tokio::test]
    async fn run_context_creation() {
        let ctx = RunContext {
            agent_id: "a1".to_owned(),
            channel: "telegram".to_owned(),
            user_id: Some("u1".to_owned()),
            model_override: Some("gpt-4".to_owned()),
        };
        assert_eq!(ctx.agent_id, "a1");
        assert_eq!(ctx.channel, "telegram");
        assert_eq!(ctx.user_id.as_deref(), Some("u1"));
        assert_eq!(ctx.model_override.as_deref(), Some("gpt-4"));
    }

    #[tokio::test]
    async fn queued_message_creation() {
        let msg = test_message();
        assert_eq!(msg.content, "hello");
        assert_eq!(msg.channel_context, serde_json::json!({}));
    }

    #[tokio::test]
    async fn run_agent_completes_and_cleans_up() {
        let provider = MockModelProvider::new("mock", vec![MockResponse::Text("Hello!".into())]);
        let store = Arc::new(MockTranscriptStore::new());
        let deps = test_deps(provider.clone(), store.clone()).await;
        let sink = MockSink::new();
        let runner = AgentRunner::new();

        runner
            .run_agent("s1", "hi", test_context(), &deps, &sink)
            .await
            .unwrap();

        // Run should no longer be active.
        assert!(!runner.is_run_active("s1").await);
        // Provider was called exactly once.
        assert_eq!(provider.calls(), 1);
        // Sink received the response text.
        let chunks = sink.collected().await;
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0], "Hello!");
    }

    #[tokio::test]
    async fn run_agent_persists_transcript() {
        let provider = MockModelProvider::new("mock", vec![MockResponse::Text("Hey!".into())]);
        let store = Arc::new(MockTranscriptStore::new());
        let deps = test_deps(provider, store.clone()).await;
        let sink = MockSink::new();
        let runner = AgentRunner::new();

        runner
            .run_agent("s1", "greet me", test_context(), &deps, &sink)
            .await
            .unwrap();

        // Transcript should have 2 messages: user + assistant.
        let saved = store.saved("s1").await.unwrap();
        assert_eq!(saved.len(), 2);
        assert_eq!(saved[0].role, Role::User);
        assert_eq!(saved[0].content, "greet me");
        assert_eq!(saved[1].role, Role::Assistant);
        assert_eq!(saved[1].content, "Hey!");
    }

    #[tokio::test]
    async fn run_agent_loads_history() {
        let store = Arc::new(MockTranscriptStore::new());
        // Pre-populate history.
        store
            .save(
                "s1",
                &[ChatMessage {
                    role: Role::User,
                    content: "earlier message".into(),
                    name: None,
                    tool_calls: None,
                    tool_call_id: None,
                }],
            )
            .await
            .unwrap();

        let provider =
            MockModelProvider::new("mock", vec![MockResponse::Text("I remember!".into())]);
        let deps = test_deps(provider, store.clone()).await;
        let sink = MockSink::new();
        let runner = AgentRunner::new();

        runner
            .run_agent("s1", "new message", test_context(), &deps, &sink)
            .await
            .unwrap();

        // Transcript should have 3 messages: old user + new user + assistant.
        let saved = store.saved("s1").await.unwrap();
        assert_eq!(saved.len(), 3);
        assert_eq!(saved[0].content, "earlier message");
        assert_eq!(saved[1].content, "new message");
        assert_eq!(saved[2].content, "I remember!");
    }

    #[tokio::test]
    async fn run_agent_with_tool_calls() {
        let tool_call = ToolCall {
            id: "tc1".into(),
            name: "unknown_tool".into(),
            arguments: serde_json::json!({"query": "test"}),
        };
        let provider = MockModelProvider::new(
            "mock",
            vec![
                // First call: model requests a tool call.
                MockResponse::WithToolCalls("".into(), vec![tool_call]),
                // Second call: model returns final text.
                MockResponse::Text("Done with tools!".into()),
            ],
        );
        let store = Arc::new(MockTranscriptStore::new());
        let deps = test_deps(provider.clone(), store.clone()).await;
        let sink = MockSink::new();
        let runner = AgentRunner::new();

        runner
            .run_agent("s1", "use tools", test_context(), &deps, &sink)
            .await
            .unwrap();

        // Provider called twice (initial + after tool result).
        assert_eq!(provider.calls(), 2);
        // Transcript: user, assistant(tool_call), tool(result), assistant(final).
        let saved = store.saved("s1").await.unwrap();
        assert_eq!(saved.len(), 4);
        assert_eq!(saved[0].role, Role::User);
        assert_eq!(saved[1].role, Role::Assistant);
        assert_eq!(saved[2].role, Role::Tool);
        assert_eq!(saved[3].role, Role::Assistant);
        assert_eq!(saved[3].content, "Done with tools!");
    }

    #[tokio::test]
    async fn run_agent_no_model_resolved() {
        let provider = MockModelProvider::new("mock", vec![]);
        let store = Arc::new(MockTranscriptStore::new());
        let mut deps = test_deps(provider, store).await;
        // Clear catalog so no model can be resolved.
        deps.catalog = ModelCatalog::new();

        let sink = MockSink::new();
        let runner = AgentRunner::new();

        let result = runner
            .run_agent("s1", "hi", test_context(), &deps, &sink)
            .await;

        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            RuntimeError::NoModelResolved { .. }
        ));
        // Run should be cleaned up.
        assert!(!runner.is_run_active("s1").await);
    }

    #[tokio::test]
    async fn wait_for_run_end_returns_immediately_when_no_run() {
        let runner = AgentRunner::new();
        // Should return immediately — no run is active.
        runner.wait_for_run_end("s1").await;
    }
}
