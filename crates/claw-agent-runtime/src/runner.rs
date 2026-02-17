//! Core agent execution engine.
//!
//! [`AgentRunner`] manages the lifecycle of agent runs — one per session key
//! at a time. While a run is active, incoming messages are buffered in a
//! per-session [`MessageQueue`] and drained when the run completes.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use anyhow::Result;
use tokio::sync::{Mutex, Notify};
use tokio::time::Instant;
use tokio_util::sync::CancellationToken;
use tracing::{debug, info};

use crate::error::RuntimeError;
use crate::queue::MessageQueue;

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
    /// Returns [`RuntimeError::SessionBusy`] if the session already has an
    /// active run. The actual agent lifecycle (tool loop, streaming, etc.)
    /// will be implemented in a later issue — for now this is a stub that
    /// registers the run, logs, and returns.
    pub async fn run_agent(
        &self,
        session_key: &str,
        message: &str,
        context: RunContext,
    ) -> Result<()> {
        // Check for an existing active run.
        {
            let runs = self.active_runs.lock().await;
            if runs.contains_key(session_key) {
                return Err(RuntimeError::SessionBusy {
                    session_key: session_key.to_owned(),
                }
                .into());
            }
        }

        let cancel_token = CancellationToken::new();
        let state = RunState {
            session_key: session_key.to_owned(),
            cancel_token,
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

        // --- stub: actual execution loop will go here ---

        // Clean up and notify waiters.
        self.active_runs.lock().await.remove(session_key);
        self.run_ended.notify_waiters();

        Ok(())
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
    async fn run_agent_stub_completes_and_cleans_up() {
        let runner = AgentRunner::new();
        runner
            .run_agent("s1", "hi", test_context())
            .await
            .unwrap();
        // After the stub returns, the run should no longer be active.
        assert!(!runner.is_run_active("s1").await);
    }

    #[tokio::test]
    async fn wait_for_run_end_returns_immediately_when_no_run() {
        let runner = AgentRunner::new();
        // Should return immediately — no run is active.
        runner.wait_for_run_end("s1").await;
    }
}
