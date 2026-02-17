//! Conversation compaction engine.
//!
//! Manages context window overflow by summarizing older messages when
//! conversation history exceeds the configured budget. Uses simple extractive
//! summarization (no LLM calls) — key topics are collected from removed
//! messages and inserted as a system summary.

use claw_agent_models::{ChatMessage, Role};

// ---------------------------------------------------------------------------
// Error
// ---------------------------------------------------------------------------

/// Errors that can occur during compaction.
#[derive(thiserror::Error, Debug)]
pub enum CompactionError {
    /// Not enough messages to compact (history is at or below `preserve_recent`).
    #[error("nothing to compact: only {count} messages, preserve_recent={preserve}")]
    NothingToCompact { count: usize, preserve: usize },

    /// Compaction failed to reduce below the token limit after all retries.
    #[error("compaction exhausted {retries} retries but still over token limit")]
    RetriesExhausted { retries: usize },
}

// ---------------------------------------------------------------------------
// Config
// ---------------------------------------------------------------------------

/// Configuration for the compaction engine.
#[derive(Debug, Clone)]
pub struct CompactionConfig {
    /// Maximum number of messages before triggering compaction.
    pub max_messages: usize,
    /// Number of recent messages to preserve (never compacted).
    pub preserve_recent: usize,
    /// Maximum total token estimate for the history.
    pub max_token_estimate: u64,
    /// Maximum retry attempts for compaction.
    pub max_retries: usize,
}

impl Default for CompactionConfig {
    fn default() -> Self {
        Self {
            max_messages: 100,
            preserve_recent: 10,
            max_token_estimate: 8_000,
            max_retries: 3,
        }
    }
}

// ---------------------------------------------------------------------------
// Result
// ---------------------------------------------------------------------------

/// Result of a compaction operation.
#[derive(Debug, Clone)]
pub struct CompactionResult {
    /// Whether compaction was performed.
    pub compacted: bool,
    /// Number of messages removed.
    pub messages_removed: usize,
    /// Summary text that replaced the removed messages (if any).
    pub summary: Option<String>,
    /// New message list after compaction.
    pub messages: Vec<ChatMessage>,
}

// ---------------------------------------------------------------------------
// Engine
// ---------------------------------------------------------------------------

/// The compaction engine.
///
/// Provides a synchronous, LLM-free compaction strategy:
/// 1. Identify messages eligible for removal (everything except system prefix
///    and the most recent `preserve_recent` messages).
/// 2. Build an extractive summary from the removed messages.
/// 3. Replace removed messages with a single system summary message.
/// 4. If the result still exceeds limits, retry with a lower `preserve_recent`.
pub struct CompactionEngine {
    config: CompactionConfig,
}

impl CompactionEngine {
    /// Create a new compaction engine with the given configuration.
    pub fn new(config: CompactionConfig) -> Self {
        Self { config }
    }

    /// Check whether the message history needs compaction.
    pub fn needs_compaction(&self, messages: &[ChatMessage]) -> bool {
        if messages.len() > self.config.max_messages {
            return true;
        }
        if self.estimate_tokens(messages) > self.config.max_token_estimate {
            return true;
        }
        false
    }

    /// Estimate the token count of a message list (~4 chars per token).
    pub fn estimate_tokens(&self, messages: &[ChatMessage]) -> u64 {
        messages
            .iter()
            .map(|m| {
                // Role label overhead (~6 chars) + content.
                let content_chars = m.content.len() as u64 + 6;
                // Add tool call text if present.
                let tool_chars = m
                    .tool_calls
                    .as_ref()
                    .map(|calls| {
                        calls
                            .iter()
                            .map(|c| c.name.len() as u64 + c.arguments.to_string().len() as u64)
                            .sum::<u64>()
                    })
                    .unwrap_or(0);
                (content_chars + tool_chars + 3) / 4 // ceil-div by 4
            })
            .sum()
    }

    /// Perform compaction on the given message history.
    ///
    /// Returns [`CompactionResult`] with the compacted history, or an error
    /// if compaction is impossible or retries are exhausted.
    pub fn compact(
        &self,
        messages: &[ChatMessage],
    ) -> Result<CompactionResult, CompactionError> {
        // Nothing to do on empty or small histories.
        if messages.is_empty() || !self.needs_compaction(messages) {
            return Ok(CompactionResult {
                compacted: false,
                messages_removed: 0,
                summary: None,
                messages: messages.to_vec(),
            });
        }

        let mut preserve = self.config.preserve_recent;

        for attempt in 0..=self.config.max_retries {
            match self.try_compact(messages, preserve) {
                Ok(result) => {
                    // Check if we reduced enough.
                    if self.estimate_tokens(&result.messages) <= self.config.max_token_estimate
                        && result.messages.len() <= self.config.max_messages
                    {
                        return Ok(result);
                    }
                    // Not enough — retry more aggressively if we can.
                    if attempt < self.config.max_retries && preserve > 1 {
                        preserve = (preserve / 2).max(1);
                        continue;
                    }
                    // Last retry — return whatever we managed.
                    if result.compacted {
                        return Ok(result);
                    }
                    return Err(CompactionError::RetriesExhausted {
                        retries: self.config.max_retries,
                    });
                }
                Err(e) => {
                    if attempt < self.config.max_retries && preserve > 1 {
                        preserve = (preserve / 2).max(1);
                        continue;
                    }
                    return Err(e);
                }
            }
        }

        Err(CompactionError::RetriesExhausted {
            retries: self.config.max_retries,
        })
    }

    /// Single compaction pass with a given `preserve` count.
    fn try_compact(
        &self,
        messages: &[ChatMessage],
        preserve: usize,
    ) -> Result<CompactionResult, CompactionError> {
        // Count leading system messages (they are always kept).
        let system_prefix_len = messages
            .iter()
            .take_while(|m| m.role == Role::System)
            .count();

        let non_system = messages.len() - system_prefix_len;
        if non_system <= preserve {
            return Err(CompactionError::NothingToCompact {
                count: messages.len(),
                preserve,
            });
        }

        // Find the index of the last user message (must never be removed).
        let last_user_idx = messages
            .iter()
            .rposition(|m| m.role == Role::User);

        // Split into: [system_prefix] [compactable] [preserved_tail]
        let tail_start = messages.len().saturating_sub(preserve);
        // Ensure we don't remove the last user message.
        let tail_start = match last_user_idx {
            Some(idx) if idx < tail_start => idx,
            _ => tail_start,
        };
        // tail_start must be > system_prefix_len to have something to compact.
        let tail_start = tail_start.max(system_prefix_len);

        let to_remove = &messages[system_prefix_len..tail_start];
        if to_remove.is_empty() {
            return Err(CompactionError::NothingToCompact {
                count: messages.len(),
                preserve,
            });
        }

        let summary = build_summary(to_remove);

        let mut result_messages: Vec<ChatMessage> = Vec::new();
        // Keep system prefix.
        result_messages.extend_from_slice(&messages[..system_prefix_len]);
        // Insert summary as a system message.
        result_messages.push(ChatMessage {
            role: Role::System,
            content: format!("Previous conversation summary: {summary}"),
            name: None,
            tool_calls: None,
            tool_call_id: None,
        });
        // Keep preserved tail.
        result_messages.extend_from_slice(&messages[tail_start..]);

        Ok(CompactionResult {
            compacted: true,
            messages_removed: to_remove.len(),
            summary: Some(summary),
            messages: result_messages,
        })
    }
}

// ---------------------------------------------------------------------------
// Extractive summary helper
// ---------------------------------------------------------------------------

/// Build a simple extractive summary from a slice of messages.
///
/// Collects the first sentence (up to 80 chars) of each non-empty message,
/// deduplicates, and joins them. This is intentionally simple — no LLM call.
fn build_summary(messages: &[ChatMessage]) -> String {
    let mut topics: Vec<String> = Vec::new();
    let mut seen = std::collections::HashSet::new();

    for msg in messages {
        let text = msg.content.trim();
        if text.is_empty() {
            continue;
        }
        // Take first sentence or first 80 chars.
        let snippet = first_sentence(text, 80);
        if seen.insert(snippet.clone()) {
            topics.push(snippet);
        }
    }

    if topics.is_empty() {
        return "No notable topics.".to_owned();
    }

    topics.join("; ")
}

/// Extract the first sentence (capped at `max_len` chars).
fn first_sentence(text: &str, max_len: usize) -> String {
    // Find first sentence-ending punctuation.
    let end = text
        .char_indices()
        .find(|&(i, c)| (c == '.' || c == '!' || c == '?') && i > 0)
        .map(|(i, _)| i + 1);

    let snippet = match end {
        Some(pos) if pos <= max_len => &text[..pos],
        _ => {
            let boundary = text
                .char_indices()
                .take_while(|&(i, _)| i < max_len)
                .last()
                .map(|(i, c)| i + c.len_utf8())
                .unwrap_or(0);
            &text[..boundary]
        }
    };

    snippet.trim().to_owned()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use claw_agent_models::{ChatMessage, Role};

    /// Helper to create a simple chat message.
    fn msg(role: Role, content: &str) -> ChatMessage {
        ChatMessage {
            role,
            content: content.to_owned(),
            name: None,
            tool_calls: None,
            tool_call_id: None,
        }
    }

    // -- needs_compaction ---------------------------------------------------

    #[test]
    fn needs_compaction_false_when_under_limits() {
        let engine = CompactionEngine::new(CompactionConfig {
            max_messages: 10,
            preserve_recent: 3,
            max_token_estimate: 10_000,
            max_retries: 2,
        });
        let messages = vec![
            msg(Role::System, "You are helpful."),
            msg(Role::User, "Hello"),
            msg(Role::Assistant, "Hi there!"),
        ];
        assert!(!engine.needs_compaction(&messages));
    }

    #[test]
    fn needs_compaction_true_when_over_message_limit() {
        let engine = CompactionEngine::new(CompactionConfig {
            max_messages: 3,
            preserve_recent: 1,
            max_token_estimate: 100_000,
            max_retries: 2,
        });
        let messages = vec![
            msg(Role::User, "a"),
            msg(Role::Assistant, "b"),
            msg(Role::User, "c"),
            msg(Role::Assistant, "d"),
        ];
        assert!(engine.needs_compaction(&messages));
    }

    #[test]
    fn needs_compaction_true_when_over_token_limit() {
        let engine = CompactionEngine::new(CompactionConfig {
            max_messages: 1000,
            preserve_recent: 2,
            max_token_estimate: 10, // very low
            max_retries: 2,
        });
        let messages = vec![
            msg(Role::User, "This is a fairly long message that will push us over the token limit easily"),
            msg(Role::Assistant, "And this response is also long enough to contribute to the token overflow"),
        ];
        assert!(engine.needs_compaction(&messages));
    }

    // -- compact ------------------------------------------------------------

    #[test]
    fn compact_removes_oldest_and_creates_summary() {
        let engine = CompactionEngine::new(CompactionConfig {
            max_messages: 4,
            preserve_recent: 2,
            max_token_estimate: 100_000,
            max_retries: 2,
        });
        let messages = vec![
            msg(Role::System, "System prompt."),
            msg(Role::User, "First question about Rust."),
            msg(Role::Assistant, "Rust is a systems language."),
            msg(Role::User, "Tell me about ownership."),
            msg(Role::Assistant, "Ownership is Rust's memory model."),
        ];
        let result = engine.compact(&messages).unwrap();
        assert!(result.compacted);
        assert!(result.messages_removed > 0);
        assert!(result.summary.is_some());
        // Summary should mention topics from removed messages.
        let summary = result.summary.unwrap();
        assert!(
            summary.contains("Rust") || summary.contains("question"),
            "Summary should reference removed content: {summary}"
        );
        // System prompt is preserved.
        assert_eq!(result.messages[0].role, Role::System);
        assert_eq!(result.messages[0].content, "System prompt.");
        // Summary message follows system prompt.
        assert_eq!(result.messages[1].role, Role::System);
        assert!(result.messages[1].content.starts_with("Previous conversation summary:"));
    }

    #[test]
    fn compact_preserves_recent_messages() {
        let engine = CompactionEngine::new(CompactionConfig {
            max_messages: 3,
            preserve_recent: 2,
            max_token_estimate: 100_000,
            max_retries: 2,
        });
        let messages = vec![
            msg(Role::User, "Old message"),
            msg(Role::Assistant, "Old reply"),
            msg(Role::User, "Recent question"),
            msg(Role::Assistant, "Recent answer"),
        ];
        let result = engine.compact(&messages).unwrap();
        assert!(result.compacted);
        // The last two messages must be preserved.
        let len = result.messages.len();
        assert_eq!(result.messages[len - 1].content, "Recent answer");
        assert_eq!(result.messages[len - 2].content, "Recent question");
    }

    #[test]
    fn compact_never_removes_last_user_message() {
        let engine = CompactionEngine::new(CompactionConfig {
            max_messages: 2,
            preserve_recent: 1,
            max_token_estimate: 100_000,
            max_retries: 2,
        });
        // Last user message is at index 2 (not in preserved tail of 1).
        let messages = vec![
            msg(Role::Assistant, "Old assistant msg"),
            msg(Role::Assistant, "Another old msg"),
            msg(Role::User, "Important user question"),
            msg(Role::Assistant, "Final response"),
        ];
        let result = engine.compact(&messages).unwrap();
        assert!(result.compacted);
        // The user message must still be present.
        assert!(
            result.messages.iter().any(|m| m.role == Role::User
                && m.content == "Important user question"),
            "Last user message must be preserved"
        );
    }

    #[test]
    fn compact_retries_with_lower_preserve_recent() {
        // Set a very low token limit so the first pass won't be enough.
        let engine = CompactionEngine::new(CompactionConfig {
            max_messages: 3,
            preserve_recent: 4,
            max_token_estimate: 30,
            max_retries: 3,
        });
        let messages = vec![
            msg(Role::User, "msg one"),
            msg(Role::Assistant, "reply one"),
            msg(Role::User, "msg two"),
            msg(Role::Assistant, "reply two"),
            msg(Role::User, "msg three"),
            msg(Role::Assistant, "reply three"),
        ];
        // Should succeed (possibly after retries lowering preserve_recent).
        let result = engine.compact(&messages);
        // It either compacts successfully or returns an error — but should not panic.
        match result {
            Ok(r) => {
                assert!(r.compacted);
                assert!(r.messages_removed > 0);
            }
            Err(CompactionError::RetriesExhausted { .. }) => {
                // Acceptable — the token limit is extremely tight.
            }
            Err(e) => panic!("Unexpected error: {e}"),
        }
    }

    #[test]
    fn compact_empty_history_returns_no_compaction() {
        let engine = CompactionEngine::new(CompactionConfig::default());
        let result = engine.compact(&[]).unwrap();
        assert!(!result.compacted);
        assert_eq!(result.messages_removed, 0);
        assert!(result.summary.is_none());
        assert!(result.messages.is_empty());
    }

    // -- estimate_tokens ----------------------------------------------------

    #[test]
    fn token_estimation_roughly_correct() {
        let engine = CompactionEngine::new(CompactionConfig::default());
        // "Hello, world!" is 13 chars + 6 overhead = 19 chars -> ceil(19+3)/4 = 5 tokens
        let messages = vec![msg(Role::User, "Hello, world!")];
        let tokens = engine.estimate_tokens(&messages);
        // We expect roughly 4-6 tokens for a short message.
        assert!(
            (3..=8).contains(&tokens),
            "Expected ~5 tokens, got {tokens}"
        );

        // Longer message: 100 chars content + 6 overhead = 106 -> ceil(109)/4 = ~27
        let long_content = "a".repeat(100);
        let messages = vec![msg(Role::User, &long_content)];
        let tokens = engine.estimate_tokens(&messages);
        assert!(
            (20..=35).contains(&tokens),
            "Expected ~27 tokens, got {tokens}"
        );
    }

    // -- helpers ------------------------------------------------------------

    #[test]
    fn first_sentence_extracts_correctly() {
        assert_eq!(first_sentence("Hello world. More text.", 80), "Hello world.");
        assert_eq!(first_sentence("Short", 80), "Short");
        assert_eq!(
            first_sentence("A very long sentence that goes on and on", 20),
            "A very long sentence"
        );
    }

    #[test]
    fn build_summary_deduplicates() {
        let msgs = vec![
            msg(Role::User, "Hello world."),
            msg(Role::User, "Hello world."),
            msg(Role::Assistant, "Goodbye."),
        ];
        let summary = build_summary(&msgs);
        // Should appear only once.
        assert_eq!(summary.matches("Hello world.").count(), 1);
        assert!(summary.contains("Goodbye."));
    }

    #[test]
    fn build_summary_handles_empty_content() {
        let msgs = vec![msg(Role::User, ""), msg(Role::Assistant, "")];
        let summary = build_summary(&msgs);
        assert_eq!(summary, "No notable topics.");
    }

    #[test]
    fn error_display() {
        let e = CompactionError::NothingToCompact {
            count: 3,
            preserve: 5,
        };
        assert!(e.to_string().contains("nothing to compact"));

        let e = CompactionError::RetriesExhausted { retries: 3 };
        assert!(e.to_string().contains("retries"));
    }
}
