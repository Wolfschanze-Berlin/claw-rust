//! Context pruning engine.
//!
//! Provides a lighter-weight alternative to full compaction for managing
//! context window size. Runs more frequently than compaction by applying
//! three strategies in order:
//!
//! 1. **Strip verbose tool results** — truncate tool results that have
//!    already been summarized by a following assistant message.
//! 2. **Remove redundant system messages** — deduplicate consecutive
//!    system messages with identical content.
//! 3. **Trim overly long messages** — cap any individual message body
//!    at a configurable character limit.

use claw_agent_models::{ChatMessage, Role};

// ---------------------------------------------------------------------------
// Config
// ---------------------------------------------------------------------------

/// Configuration thresholds for the pruning engine.
#[derive(Debug, Clone)]
pub struct PruningConfig {
    /// Maximum character length for tool result messages before truncation.
    pub max_tool_result_chars: usize,
    /// Maximum character length for any individual message before truncation.
    pub max_message_chars: usize,
    /// Whether to remove consecutive duplicate system messages.
    pub remove_redundant_system: bool,
    /// Number of most-recent messages that are never pruned.
    pub preserve_last_n: usize,
}

impl Default for PruningConfig {
    fn default() -> Self {
        Self {
            max_tool_result_chars: 2000,
            max_message_chars: 4000,
            remove_redundant_system: true,
            preserve_last_n: 5,
        }
    }
}

// ---------------------------------------------------------------------------
// Result
// ---------------------------------------------------------------------------

/// Outcome of a pruning operation.
#[derive(Debug, Clone)]
pub struct PruningResult {
    /// Whether any pruning was performed.
    pub pruned: bool,
    /// Number of messages that were trimmed or removed.
    pub messages_trimmed: usize,
    /// Total characters removed across all messages.
    pub chars_removed: usize,
    /// The message list after pruning.
    pub messages: Vec<ChatMessage>,
}

// ---------------------------------------------------------------------------
// Engine
// ---------------------------------------------------------------------------

/// The pruning engine.
///
/// Applies lightweight, non-destructive transformations to a conversation
/// history to reduce context size without full compaction.
pub struct PruningEngine {
    config: PruningConfig,
}

impl PruningEngine {
    /// Create a new pruning engine with the given configuration.
    pub fn new(config: PruningConfig) -> Self {
        Self { config }
    }

    /// Check whether the message history would benefit from pruning.
    ///
    /// Returns `true` if any message in the prunable region exceeds the
    /// configured character limits, or if redundant system messages exist.
    pub fn needs_pruning(&self, messages: &[ChatMessage]) -> bool {
        let prunable = prunable_range(messages, self.config.preserve_last_n);

        for (i, msg) in prunable.iter().enumerate() {
            if msg.role == Role::Tool && msg.content.len() > self.config.max_tool_result_chars {
                return true;
            }
            if msg.content.len() > self.config.max_message_chars {
                return true;
            }
            if self.config.remove_redundant_system
                && msg.role == Role::System
                && i + 1 < prunable.len()
                && prunable[i + 1].role == Role::System
                && prunable[i + 1].content == msg.content
            {
                return true;
            }
        }

        false
    }

    /// Prune the given message history.
    ///
    /// Strategies are applied in order:
    /// 1. Strip verbose tool results already summarized by the model.
    /// 2. Remove redundant consecutive system messages.
    /// 3. Trim overly long individual messages.
    pub fn prune(&self, messages: Vec<ChatMessage>) -> PruningResult {
        if messages.is_empty() {
            return PruningResult {
                pruned: false,
                messages_trimmed: 0,
                chars_removed: 0,
                messages,
            };
        }

        let original_chars: usize = messages.iter().map(|m| m.content.len()).sum();
        let split = messages.len().saturating_sub(self.config.preserve_last_n);

        let (prunable_slice, tail) = messages.split_at(split);
        let mut prunable = prunable_slice.to_vec();
        let mut trimmed_count: usize = 0;

        // Strategy 1: Strip verbose tool results (uses indexed look-ahead).
        strip_verbose_tool_results(&mut prunable, self.config.max_tool_result_chars, &mut trimmed_count);

        // Strategy 2: Remove redundant system messages.
        if self.config.remove_redundant_system {
            prunable = remove_redundant_system_messages(prunable, &mut trimmed_count);
        }

        // Strategy 3: Trim overly long messages.
        prunable = trim_long_messages(prunable, self.config.max_message_chars, &mut trimmed_count);

        // Reassemble with protected tail.
        let mut result = prunable;
        result.extend_from_slice(tail);

        let new_chars: usize = result.iter().map(|m| m.content.len()).sum();
        let chars_removed = original_chars.saturating_sub(new_chars);

        PruningResult {
            pruned: trimmed_count > 0,
            messages_trimmed: trimmed_count,
            chars_removed,
            messages: result,
        }
    }
}

// ---------------------------------------------------------------------------
// Compaction safeguard
// ---------------------------------------------------------------------------

/// Check whether the last assistant message has a tool call without a
/// matching tool result. When this returns `true`, compaction should be
/// deferred to avoid corrupting in-flight tool interactions.
pub fn has_pending_tool_call(messages: &[ChatMessage]) -> bool {
    let last_assistant_with_tools = messages.iter().rposition(|m| {
        m.role == Role::Assistant
            && m.tool_calls
                .as_ref()
                .is_some_and(|calls| !calls.is_empty())
    });

    let Some(assistant_idx) = last_assistant_with_tools else {
        return false;
    };

    let tool_calls = messages[assistant_idx].tool_calls.as_ref().unwrap();

    for call in tool_calls {
        let has_result = messages[assistant_idx + 1..].iter().any(|m| {
            m.role == Role::Tool && m.tool_call_id.as_deref() == Some(&call.id)
        });
        if !has_result {
            return true;
        }
    }

    false
}

// ---------------------------------------------------------------------------
// Strategy helpers
// ---------------------------------------------------------------------------

/// Return the slice of messages eligible for pruning (everything except
/// the last `preserve_last_n`).
fn prunable_range(messages: &[ChatMessage], preserve_last_n: usize) -> &[ChatMessage] {
    let end = messages.len().saturating_sub(preserve_last_n);
    &messages[..end]
}

/// Strategy 1: Truncate tool results only when the *next* message is an
/// assistant message (indicating the model already summarized the output).
fn strip_verbose_tool_results(
    messages: &mut [ChatMessage],
    max_chars: usize,
    trimmed: &mut usize,
) {
    for i in 0..messages.len().saturating_sub(1) {
        if messages[i].role == Role::Tool
            && messages[i].content.len() > max_chars
            && messages[i + 1].role == Role::Assistant
        {
            let truncated = truncate_at_boundary(&messages[i].content, max_chars).to_owned();
            if truncated.len() < messages[i].content.len() {
                messages[i].content = format!("{truncated}... [pruned]");
                *trimmed += 1;
            }
        }
    }
}

/// Strategy 2: Remove consecutive system messages with identical content,
/// keeping only the last one in each run.
fn remove_redundant_system_messages(
    messages: Vec<ChatMessage>,
    trimmed: &mut usize,
) -> Vec<ChatMessage> {
    let mut result: Vec<ChatMessage> = Vec::with_capacity(messages.len());

    for msg in messages {
        if msg.role == Role::System {
            if let Some(prev) = result.last() {
                if prev.role == Role::System && prev.content == msg.content {
                    result.pop();
                    *trimmed += 1;
                }
            }
        }
        result.push(msg);
    }

    result
}

/// Strategy 3: Truncate any message whose content exceeds `max_chars`.
fn trim_long_messages(
    messages: Vec<ChatMessage>,
    max_chars: usize,
    trimmed: &mut usize,
) -> Vec<ChatMessage> {
    messages
        .into_iter()
        .map(|mut msg| {
            if msg.content.len() > max_chars {
                let truncated = truncate_at_boundary(&msg.content, max_chars);
                msg.content = format!("{truncated}... [truncated]");
                *trimmed += 1;
            }
            msg
        })
        .collect()
}

/// Truncate a string at a UTF-8 safe boundary, not exceeding `max_len` bytes.
fn truncate_at_boundary(text: &str, max_len: usize) -> &str {
    if text.len() <= max_len {
        return text;
    }
    let mut end = max_len;
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    &text[..end]
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use claw_agent_models::{ChatMessage, Role, ToolCall};

    fn msg(role: Role, content: &str) -> ChatMessage {
        ChatMessage {
            role,
            content: content.to_owned(),
            name: None,
            tool_calls: None,
            tool_call_id: None,
        }
    }

    fn tool_msg(content: &str, tool_call_id: &str) -> ChatMessage {
        ChatMessage {
            role: Role::Tool,
            content: content.to_owned(),
            name: None,
            tool_calls: None,
            tool_call_id: Some(tool_call_id.to_owned()),
        }
    }

    fn assistant_with_tool_call(content: &str, call_id: &str, call_name: &str) -> ChatMessage {
        ChatMessage {
            role: Role::Assistant,
            content: content.to_owned(),
            name: None,
            tool_calls: Some(vec![ToolCall {
                id: call_id.to_owned(),
                name: call_name.to_owned(),
                arguments: serde_json::json!({}),
            }]),
            tool_call_id: None,
        }
    }

    // -- PruningConfig defaults -----------------------------------------------

    #[test]
    fn default_config_has_expected_values() {
        let cfg = PruningConfig::default();
        assert_eq!(cfg.max_tool_result_chars, 2000);
        assert_eq!(cfg.max_message_chars, 4000);
        assert!(cfg.remove_redundant_system);
        assert_eq!(cfg.preserve_last_n, 5);
    }

    // -- needs_pruning --------------------------------------------------------

    #[test]
    fn needs_pruning_false_when_all_short() {
        let engine = PruningEngine::new(PruningConfig::default());
        let messages = vec![
            msg(Role::User, "Hello"),
            msg(Role::Assistant, "Hi there"),
        ];
        assert!(!engine.needs_pruning(&messages));
    }

    #[test]
    fn needs_pruning_true_for_long_tool_result() {
        let engine = PruningEngine::new(PruningConfig {
            max_tool_result_chars: 50,
            preserve_last_n: 0,
            ..PruningConfig::default()
        });
        let messages = vec![tool_msg(&"x".repeat(100), "tc-1")];
        assert!(engine.needs_pruning(&messages));
    }

    #[test]
    fn needs_pruning_true_for_long_message() {
        let engine = PruningEngine::new(PruningConfig {
            max_message_chars: 50,
            preserve_last_n: 0,
            ..PruningConfig::default()
        });
        let messages = vec![msg(Role::User, &"y".repeat(100))];
        assert!(engine.needs_pruning(&messages));
    }

    #[test]
    fn needs_pruning_true_for_redundant_system() {
        let engine = PruningEngine::new(PruningConfig {
            preserve_last_n: 0,
            ..PruningConfig::default()
        });
        let messages = vec![
            msg(Role::System, "You are helpful."),
            msg(Role::System, "You are helpful."),
        ];
        assert!(engine.needs_pruning(&messages));
    }

    #[test]
    fn needs_pruning_ignores_protected_tail() {
        let engine = PruningEngine::new(PruningConfig {
            max_message_chars: 50,
            preserve_last_n: 2,
            ..PruningConfig::default()
        });
        let messages = vec![
            msg(Role::User, "short"),
            msg(Role::User, &"z".repeat(100)),
            msg(Role::Assistant, &"z".repeat(100)),
        ];
        assert!(!engine.needs_pruning(&messages));
    }

    // -- prune (strategy 1: tool results) ------------------------------------

    #[test]
    fn prune_truncates_verbose_tool_result() {
        let engine = PruningEngine::new(PruningConfig {
            max_tool_result_chars: 20,
            max_message_chars: 10_000,
            remove_redundant_system: false,
            preserve_last_n: 0,
        });
        let messages = vec![
            tool_msg(&"a".repeat(100), "tc-1"),
            msg(Role::Assistant, "The tool returned some data."),
        ];
        let result = engine.prune(messages);
        assert!(result.pruned);
        assert!(result.messages[0].content.ends_with("... [pruned]"));
        assert!(result.messages[0].content.len() < 100);
        assert!(result.chars_removed > 0);
    }

    #[test]
    fn prune_does_not_truncate_tool_result_without_following_assistant() {
        let engine = PruningEngine::new(PruningConfig {
            max_tool_result_chars: 20,
            max_message_chars: 10_000,
            remove_redundant_system: false,
            preserve_last_n: 0,
        });
        let long_tool = "a".repeat(100);
        let messages = vec![
            tool_msg(&long_tool, "tc-1"),
            msg(Role::User, "What happened?"),
        ];
        let result = engine.prune(messages);
        assert_eq!(result.messages[0].content, long_tool);
    }

    // -- prune (strategy 2: redundant system) --------------------------------

    #[test]
    fn prune_removes_redundant_system_messages() {
        let engine = PruningEngine::new(PruningConfig {
            max_tool_result_chars: 10_000,
            max_message_chars: 10_000,
            remove_redundant_system: true,
            preserve_last_n: 0,
        });
        let messages = vec![
            msg(Role::System, "Be helpful."),
            msg(Role::System, "Be helpful."),
            msg(Role::System, "Be helpful."),
            msg(Role::User, "Hi"),
        ];
        let result = engine.prune(messages);
        assert!(result.pruned);
        assert_eq!(result.messages_trimmed, 2);
        let system_count = result.messages.iter().filter(|m| m.role == Role::System).count();
        assert_eq!(system_count, 1);
        assert_eq!(result.messages.len(), 2);
    }

    #[test]
    fn prune_keeps_different_system_messages() {
        let engine = PruningEngine::new(PruningConfig {
            max_tool_result_chars: 10_000,
            max_message_chars: 10_000,
            remove_redundant_system: true,
            preserve_last_n: 0,
        });
        let messages = vec![
            msg(Role::System, "Rule A."),
            msg(Role::System, "Rule B."),
        ];
        let result = engine.prune(messages);
        assert!(!result.pruned);
        assert_eq!(result.messages.len(), 2);
    }

    // -- prune (strategy 3: long messages) -----------------------------------

    #[test]
    fn prune_truncates_long_messages() {
        let engine = PruningEngine::new(PruningConfig {
            max_tool_result_chars: 10_000,
            max_message_chars: 30,
            remove_redundant_system: false,
            preserve_last_n: 0,
        });
        let messages = vec![msg(Role::User, &"b".repeat(100))];
        let result = engine.prune(messages);
        assert!(result.pruned);
        assert!(result.messages[0].content.ends_with("... [truncated]"));
        assert!(result.chars_removed > 0);
    }

    // -- prune (preserve_last_n) ---------------------------------------------

    #[test]
    fn prune_preserves_last_n_messages() {
        let engine = PruningEngine::new(PruningConfig {
            max_tool_result_chars: 10_000,
            max_message_chars: 30,
            remove_redundant_system: false,
            preserve_last_n: 2,
        });
        let long = "c".repeat(100);
        let messages = vec![
            msg(Role::User, &long),
            msg(Role::User, &long),
            msg(Role::Assistant, &long),
        ];
        let result = engine.prune(messages);
        assert!(result.pruned);
        assert!(result.messages[0].content.ends_with("... [truncated]"));
        assert_eq!(result.messages[1].content, long);
        assert_eq!(result.messages[2].content, long);
    }

    // -- prune (empty input) -------------------------------------------------

    #[test]
    fn prune_empty_returns_empty() {
        let engine = PruningEngine::new(PruningConfig::default());
        let result = engine.prune(vec![]);
        assert!(!result.pruned);
        assert_eq!(result.messages_trimmed, 0);
        assert_eq!(result.chars_removed, 0);
        assert!(result.messages.is_empty());
    }

    // -- has_pending_tool_call ------------------------------------------------

    #[test]
    fn pending_tool_call_detected() {
        let messages = vec![
            msg(Role::User, "Search for X"),
            assistant_with_tool_call("Let me search.", "tc-1", "search"),
        ];
        assert!(has_pending_tool_call(&messages));
    }

    #[test]
    fn no_pending_tool_call_when_result_present() {
        let messages = vec![
            msg(Role::User, "Search for X"),
            assistant_with_tool_call("Let me search.", "tc-1", "search"),
            tool_msg("Found 42 results.", "tc-1"),
            msg(Role::Assistant, "I found 42 results."),
        ];
        assert!(!has_pending_tool_call(&messages));
    }

    #[test]
    fn no_pending_tool_call_when_no_tool_calls() {
        let messages = vec![
            msg(Role::User, "Hello"),
            msg(Role::Assistant, "Hi!"),
        ];
        assert!(!has_pending_tool_call(&messages));
    }

    #[test]
    fn pending_tool_call_partial_results() {
        let messages = vec![
            msg(Role::User, "Do two things"),
            ChatMessage {
                role: Role::Assistant,
                content: "I'll do both.".to_owned(),
                name: None,
                tool_calls: Some(vec![
                    ToolCall {
                        id: "tc-1".to_owned(),
                        name: "search".to_owned(),
                        arguments: serde_json::json!({}),
                    },
                    ToolCall {
                        id: "tc-2".to_owned(),
                        name: "fetch".to_owned(),
                        arguments: serde_json::json!({}),
                    },
                ]),
                tool_call_id: None,
            },
            tool_msg("Result for search.", "tc-1"),
        ];
        assert!(has_pending_tool_call(&messages));
    }

    // -- truncate_at_boundary -------------------------------------------------

    #[test]
    fn truncate_at_boundary_respects_utf8() {
        let text = "cafébar";
        let result = truncate_at_boundary(text, 4);
        assert_eq!(result, "caf");
        let result = truncate_at_boundary(text, 5);
        assert_eq!(result, "café");
    }
}
