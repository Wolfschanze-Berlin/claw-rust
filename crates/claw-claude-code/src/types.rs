//! Typed NDJSON message structs for the Claude Code SDK protocol.
//!
//! The Claude Code CLI (`claude --print --output-format stream-json`) emits
//! newline-delimited JSON messages. Each line is a JSON object with a `type`
//! field that discriminates the message variant.

use serde::{Deserialize, Serialize};
use serde_json::Value;

// ---------------------------------------------------------------------------
// Top-level message envelope
// ---------------------------------------------------------------------------

/// A single NDJSON message from Claude Code stdout.
///
/// Uses `#[serde(tag = "type")]` to match the SDK protocol's discriminated
/// union pattern. Unknown message types deserialize to [`ClaudeMessage::Unknown`]
/// for forward compatibility.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ClaudeMessage {
    /// Initialization message with session metadata.
    #[serde(rename = "system")]
    System(SystemMessage),

    /// Echoed user input (not always present in --print mode).
    #[serde(rename = "user")]
    User(UserMessage),

    /// Assistant response with content blocks.
    #[serde(rename = "assistant")]
    Assistant(AssistantMessage),

    /// Final result with session ID, usage stats, and cost.
    #[serde(rename = "result")]
    Result(ResultMessage),

    /// Unknown/future message type — logged and skipped.
    #[serde(other)]
    Unknown,
}

// ---------------------------------------------------------------------------
// Message variants
// ---------------------------------------------------------------------------

/// System initialization message.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemMessage {
    /// The Claude Code session identifier (for --resume).
    #[serde(default)]
    pub session_id: String,

    /// Additional fields for forward compatibility.
    #[serde(flatten)]
    pub extra: Value,
}

/// Echoed user input.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserMessage {
    /// The user's message text.
    #[serde(default)]
    pub message: Value,

    /// Additional fields for forward compatibility.
    #[serde(flatten)]
    pub extra: Value,
}

/// Assistant response containing content blocks.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssistantMessage {
    /// Ordered list of content blocks (text, thinking, tool_use, tool_result).
    #[serde(default)]
    pub content: Vec<ContentBlock>,

    /// Additional fields for forward compatibility.
    #[serde(flatten)]
    pub extra: Value,
}

/// Final result message emitted when Claude Code finishes processing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResultMessage {
    /// The Claude Code session identifier.
    #[serde(default)]
    pub session_id: String,

    /// Number of conversation turns in this run.
    #[serde(default)]
    pub num_turns: u32,

    /// Wall-clock duration in milliseconds.
    #[serde(default)]
    pub duration_ms: u64,

    /// Total cost in USD (may be absent for cached/free runs).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub total_cost_usd: Option<f64>,

    /// The final result text.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<String>,

    /// Additional fields for forward compatibility.
    #[serde(flatten)]
    pub extra: Value,
}

// ---------------------------------------------------------------------------
// Content blocks
// ---------------------------------------------------------------------------

/// A single content block within an [`AssistantMessage`].
///
/// Mirrors the Anthropic API content block types used by Claude Code.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ContentBlock {
    /// Plain text output.
    #[serde(rename = "text")]
    Text {
        /// The text content.
        text: String,
    },

    /// Extended thinking / chain-of-thought (when enabled).
    #[serde(rename = "thinking")]
    Thinking {
        /// The thinking content.
        thinking: String,
    },

    /// Tool invocation request.
    #[serde(rename = "tool_use")]
    ToolUse {
        /// Unique tool use ID for correlating with results.
        id: String,
        /// Tool name (e.g. "Read", "Edit", "Bash").
        name: String,
        /// Tool input arguments as a JSON object.
        input: Value,
    },

    /// Result from a tool invocation.
    #[serde(rename = "tool_result")]
    ToolResult {
        /// The tool_use ID this result corresponds to.
        tool_use_id: String,
        /// The tool's output content.
        content: Value,
    },

    /// Unknown/future content block type.
    #[serde(other)]
    Unknown,
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

impl AssistantMessage {
    /// Extract all text content from this message's content blocks.
    pub fn text_content(&self) -> String {
        self.content
            .iter()
            .filter_map(|block| match block {
                ContentBlock::Text { text } => Some(text.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("")
    }
}

impl ClaudeMessage {
    /// Extract the session ID if this is a System or Result message.
    pub fn session_id(&self) -> Option<&str> {
        match self {
            ClaudeMessage::System(sys) => Some(&sys.session_id),
            ClaudeMessage::Result(res) => Some(&res.session_id),
            _ => None,
        }
    }
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deserialize_system_message() {
        let json = r#"{"type":"system","session_id":"cc-sess-abc123","subtype":"init"}"#;
        let msg: ClaudeMessage = serde_json::from_str(json).unwrap();
        match msg {
            ClaudeMessage::System(sys) => {
                assert_eq!(sys.session_id, "cc-sess-abc123");
            }
            other => panic!("expected System, got {other:?}"),
        }
    }

    #[test]
    fn deserialize_assistant_text_block() {
        let json = r#"{"type":"assistant","content":[{"type":"text","text":"Hello world!"}]}"#;
        let msg: ClaudeMessage = serde_json::from_str(json).unwrap();
        match msg {
            ClaudeMessage::Assistant(ass) => {
                assert_eq!(ass.content.len(), 1);
                assert_eq!(ass.text_content(), "Hello world!");
            }
            other => panic!("expected Assistant, got {other:?}"),
        }
    }

    #[test]
    fn deserialize_assistant_tool_use() {
        let json = r#"{
            "type": "assistant",
            "content": [
                {"type": "tool_use", "id": "tu_1", "name": "Read", "input": {"file_path": "/tmp/test.rs"}},
                {"type": "text", "text": "I read the file."}
            ]
        }"#;
        let msg: ClaudeMessage = serde_json::from_str(json).unwrap();
        match msg {
            ClaudeMessage::Assistant(ass) => {
                assert_eq!(ass.content.len(), 2);
                match &ass.content[0] {
                    ContentBlock::ToolUse { id, name, .. } => {
                        assert_eq!(id, "tu_1");
                        assert_eq!(name, "Read");
                    }
                    other => panic!("expected ToolUse, got {other:?}"),
                }
                assert_eq!(ass.text_content(), "I read the file.");
            }
            other => panic!("expected Assistant, got {other:?}"),
        }
    }

    #[test]
    fn deserialize_result_message() {
        let json = r#"{
            "type": "result",
            "session_id": "cc-sess-abc123",
            "num_turns": 3,
            "duration_ms": 4500,
            "total_cost_usd": 0.0042,
            "result": "Done."
        }"#;
        let msg: ClaudeMessage = serde_json::from_str(json).unwrap();
        match msg {
            ClaudeMessage::Result(res) => {
                assert_eq!(res.session_id, "cc-sess-abc123");
                assert_eq!(res.num_turns, 3);
                assert_eq!(res.duration_ms, 4500);
                assert_eq!(res.total_cost_usd, Some(0.0042));
                assert_eq!(res.result.as_deref(), Some("Done."));
            }
            other => panic!("expected Result, got {other:?}"),
        }
    }

    #[test]
    fn deserialize_unknown_message_type() {
        let json = r#"{"type":"future_feature","data":"something"}"#;
        let msg: ClaudeMessage = serde_json::from_str(json).unwrap();
        assert!(matches!(msg, ClaudeMessage::Unknown));
    }

    #[test]
    fn deserialize_unknown_content_block() {
        let json = r#"{"type":"assistant","content":[{"type":"server_tool_use","data":"x"},{"type":"text","text":"ok"}]}"#;
        let msg: ClaudeMessage = serde_json::from_str(json).unwrap();
        match msg {
            ClaudeMessage::Assistant(ass) => {
                assert_eq!(ass.content.len(), 2);
                assert!(matches!(ass.content[0], ContentBlock::Unknown));
                assert_eq!(ass.text_content(), "ok");
            }
            other => panic!("expected Assistant, got {other:?}"),
        }
    }

    #[test]
    fn session_id_extraction() {
        let sys_json = r#"{"type":"system","session_id":"s1"}"#;
        let msg: ClaudeMessage = serde_json::from_str(sys_json).unwrap();
        assert_eq!(msg.session_id(), Some("s1"));

        let res_json = r#"{"type":"result","session_id":"s2","num_turns":1,"duration_ms":100}"#;
        let msg: ClaudeMessage = serde_json::from_str(res_json).unwrap();
        assert_eq!(msg.session_id(), Some("s2"));

        let asst_json = r#"{"type":"assistant","content":[]}"#;
        let msg: ClaudeMessage = serde_json::from_str(asst_json).unwrap();
        assert_eq!(msg.session_id(), None);
    }

    #[test]
    fn roundtrip_serialize() {
        let original = ClaudeMessage::Assistant(AssistantMessage {
            content: vec![ContentBlock::Text {
                text: "hello".into(),
            }],
            extra: Value::Object(Default::default()),
        });
        let json = serde_json::to_string(&original).unwrap();
        let back: ClaudeMessage = serde_json::from_str(&json).unwrap();
        match back {
            ClaudeMessage::Assistant(ass) => assert_eq!(ass.text_content(), "hello"),
            other => panic!("expected Assistant, got {other:?}"),
        }
    }
}
