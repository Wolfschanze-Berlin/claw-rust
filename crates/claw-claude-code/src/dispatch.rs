//! Claude Code dispatch pipeline bridge.
//!
//! Provides [`ClaudeCodeDispatchContext`] and [`dispatch_with_claude_code()`],
//! which parallel the existing `AgentDispatchContext` / `dispatch_with_agent()`
//! in `claw-dispatch`. This module bridges inbound channel messages to the
//! Claude Code subprocess, streaming responses back via the reply dispatcher.

use std::sync::Arc;

use anyhow::Result;
use tokio_util::sync::CancellationToken;
use tracing::{debug, info, warn};

use claw_channels::ReplyPayload;

use crate::config::ClaudeCodeConfig;
use crate::process::ClaudeCodeProcess;
use crate::session::SessionManager;
use crate::types::{ClaudeMessage, ContentBlock};

// ---------------------------------------------------------------------------
// ClaudeCodeDispatchContext
// ---------------------------------------------------------------------------

/// Dependencies for dispatching messages through Claude Code.
///
/// Wraps the session manager and config in `Arc`s so they can be shared
/// across async tasks and moved into `'static` closures.
#[derive(Clone)]
pub struct ClaudeCodeDispatchContext {
    /// Claude Code CLI configuration.
    pub config: ClaudeCodeConfig,

    /// Session manager for mapping claw sessions to Claude Code sessions.
    pub session_mgr: Arc<SessionManager>,
}

// ---------------------------------------------------------------------------
// RunResult
// ---------------------------------------------------------------------------

/// Result of a single Claude Code dispatch run.
#[derive(Debug)]
pub struct ClaudeCodeRunResult {
    /// Accumulated response text from AssistantMessage content blocks.
    pub response_text: String,

    /// Claude Code session ID (from SystemMessage or ResultMessage).
    pub session_id: Option<String>,

    /// Number of turns reported by the ResultMessage.
    pub num_turns: Option<u32>,

    /// Duration in ms reported by the ResultMessage.
    pub duration_ms: Option<u64>,

    /// Cost in USD reported by the ResultMessage.
    pub cost_usd: Option<f64>,
}

// ---------------------------------------------------------------------------
// dispatch_with_claude_code (core dispatch function)
// ---------------------------------------------------------------------------

/// Run a prompt through Claude Code and return the result.
///
/// This is the core dispatch function that:
/// 1. Looks up the existing Claude Code session for this claw session key
/// 2. Spawns or resumes a Claude Code process
/// 3. Streams NDJSON messages, accumulating text content
/// 4. Updates the session mapping with the new session ID
///
/// Returns a [`ClaudeCodeRunResult`] with the response text and metadata.
pub async fn run_claude_code(
    ctx: &ClaudeCodeDispatchContext,
    session_key: &str,
    prompt: &str,
    cancel: CancellationToken,
) -> Result<ClaudeCodeRunResult> {
    // 1. Get or create session mapping.
    let mapping = ctx.session_mgr.get_or_create(session_key).await?;

    // 2. Spawn or resume process.
    let mut process = if mapping.has_session() {
        debug!(
            session_key,
            cc_session = %mapping.claude_session_id,
            "resuming Claude Code session"
        );
        ClaudeCodeProcess::spawn_resume(
            &ctx.config,
            &mapping.claude_session_id,
            prompt,
            cancel.clone(),
        )
        .await?
    } else {
        debug!(session_key, "starting new Claude Code session");
        ClaudeCodeProcess::spawn_new(&ctx.config, prompt, cancel.clone()).await?
    };

    // 3. Stream and parse NDJSON messages.
    let mut rx = process.stream_messages()?;
    process.spawn_stderr_logger();

    let mut response_text = String::new();
    let mut session_id: Option<String> = None;
    let mut num_turns: Option<u32> = None;
    let mut duration_ms: Option<u64> = None;
    let mut cost_usd: Option<f64> = None;

    while let Some(msg) = rx.recv().await {
        match msg {
            ClaudeMessage::System(sys) => {
                if !sys.session_id.is_empty() {
                    session_id = Some(sys.session_id.clone());
                    debug!(session_id = %sys.session_id, "received system message");
                }
            }
            ClaudeMessage::Assistant(ass) => {
                for block in &ass.content {
                    if let ContentBlock::Text { text } = block {
                        response_text.push_str(text);
                    }
                }
            }
            ClaudeMessage::Result(res) => {
                if !res.session_id.is_empty() {
                    session_id = Some(res.session_id.clone());
                }
                num_turns = Some(res.num_turns);
                duration_ms = Some(res.duration_ms);
                cost_usd = res.total_cost_usd;

                info!(
                    session_id = %res.session_id,
                    turns = res.num_turns,
                    duration_ms = res.duration_ms,
                    cost = ?res.total_cost_usd,
                    "Claude Code run complete"
                );
            }
            ClaudeMessage::User(_) | ClaudeMessage::Unknown => {}
        }
    }

    // 4. Wait for process to exit.
    if let Err(e) = process.wait().await {
        warn!(error = %e, "error waiting for Claude Code process");
    }

    // 5. Update session mapping.
    if let Some(ref sid) = session_id {
        let summary = if response_text.len() > 200 {
            Some(&response_text[..200])
        } else if !response_text.is_empty() {
            Some(response_text.as_str())
        } else {
            None
        };

        if let Err(e) = ctx
            .session_mgr
            .update_after_run(
                session_key,
                sid,
                mapping.message_count + 1,
                summary,
                ctx.config.model.as_deref(),
            )
            .await
        {
            warn!(error = %e, "failed to update session mapping");
        }
    }

    Ok(ClaudeCodeRunResult {
        response_text,
        session_id,
        num_turns,
        duration_ms,
        cost_usd,
    })
}

// ---------------------------------------------------------------------------
// Command handlers
// ---------------------------------------------------------------------------

/// Handle the `/reset` command for a Claude Code session.
///
/// Deletes the session mapping so the next message starts a fresh
/// Claude Code conversation.
pub async fn handle_reset(
    ctx: &ClaudeCodeDispatchContext,
    session_key: &str,
) -> Result<ReplyPayload> {
    ctx.session_mgr.reset(session_key).await?;
    info!(session_key, "Claude Code session reset");

    Ok(ReplyPayload {
        text: Some("Session reset. Your next message will start a fresh conversation.".into()),
        ..Default::default()
    })
}

/// Handle the `/status` command — show session metadata.
pub async fn handle_status(
    ctx: &ClaudeCodeDispatchContext,
    session_key: &str,
) -> Result<ReplyPayload> {
    let mapping = ctx.session_mgr.get(session_key).await?;

    let text = match mapping {
        Some(m) if m.has_session() => {
            format!(
                "Claude Code session active\n\
                 Session ID: {}\n\
                 Messages: {}\n\
                 Model: {}\n\
                 Last used: {}",
                m.claude_session_id,
                m.message_count,
                m.model.as_deref().unwrap_or("default"),
                m.updated_at.format("%Y-%m-%d %H:%M UTC"),
            )
        }
        Some(_) => "No active Claude Code session (mapping exists but no session ID yet).".into(),
        None => "No Claude Code session found for this conversation.".into(),
    };

    Ok(ReplyPayload {
        text: Some(text),
        ..Default::default()
    })
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::InMemorySessionStore;

    fn test_ctx() -> ClaudeCodeDispatchContext {
        let store = InMemorySessionStore::new();
        ClaudeCodeDispatchContext {
            config: ClaudeCodeConfig::default(),
            session_mgr: Arc::new(SessionManager::new(store)),
        }
    }

    #[tokio::test]
    async fn handle_reset_clears_session() {
        let ctx = test_ctx();

        // Create a session first.
        ctx.session_mgr.get_or_create("sk1").await.unwrap();
        ctx.session_mgr
            .update_after_run("sk1", "cc-123", 1, None, None)
            .await
            .unwrap();

        // Reset it.
        let reply = handle_reset(&ctx, "sk1").await.unwrap();
        assert!(reply.text.unwrap().contains("reset"));

        // Session should be gone.
        assert!(ctx.session_mgr.get("sk1").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn handle_reset_nonexistent_is_ok() {
        let ctx = test_ctx();
        let reply = handle_reset(&ctx, "nonexistent").await.unwrap();
        assert!(reply.text.unwrap().contains("reset"));
    }

    #[tokio::test]
    async fn handle_status_no_session() {
        let ctx = test_ctx();
        let reply = handle_status(&ctx, "sk1").await.unwrap();
        assert!(reply.text.unwrap().contains("No Claude Code session"));
    }

    #[tokio::test]
    async fn handle_status_active_session() {
        let ctx = test_ctx();
        ctx.session_mgr.get_or_create("sk1").await.unwrap();
        ctx.session_mgr
            .update_after_run("sk1", "cc-abc", 5, Some("test summary"), Some("opus"))
            .await
            .unwrap();

        let reply = handle_status(&ctx, "sk1").await.unwrap();
        let text = reply.text.unwrap();
        assert!(text.contains("cc-abc"));
        assert!(text.contains("5"));
        assert!(text.contains("opus"));
    }

    #[tokio::test]
    async fn dispatch_context_is_clone() {
        let ctx = test_ctx();
        let _cloned = ctx.clone();
    }
}
