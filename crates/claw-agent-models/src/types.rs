//! Core message types for LLM conversations.
//!
//! These types model the request/response protocol between the agent runtime
//! and any LLM provider. They are provider-agnostic — each concrete provider
//! maps these to its own wire format.

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Role
// ---------------------------------------------------------------------------

/// The role of a message participant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    System,
    User,
    Assistant,
    Tool,
}

// ---------------------------------------------------------------------------
// Tool types
// ---------------------------------------------------------------------------

/// A tool call requested by the model.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: serde_json::Value,
}

/// The result of executing a tool call.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResult {
    pub tool_call_id: String,
    pub content: String,
    #[serde(default)]
    pub is_error: bool,
}

/// Schema definition for a tool available to the model.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}

// ---------------------------------------------------------------------------
// Attachment (multimodal content)
// ---------------------------------------------------------------------------

/// A file attachment associated with a user message (image, document, etc.).
///
/// Attachments are carried alongside the text content and converted to
/// provider-specific formats (e.g. Anthropic base64 image blocks) at the
/// provider layer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Attachment {
    /// Absolute path to the downloaded file on the local filesystem.
    pub file_path: String,
    /// MIME type of the file (e.g. "image/jpeg", "application/pdf").
    pub mime_type: String,
    /// Optional human-readable filename.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_name: Option<String>,
}

// ---------------------------------------------------------------------------
// ChatMessage
// ---------------------------------------------------------------------------

/// A single message in a conversation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: Role,
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCall>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    /// File attachments (images, documents) for multimodal messages.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attachments: Option<Vec<Attachment>>,
}

// ---------------------------------------------------------------------------
// ChatRequest / ChatResponse
// ---------------------------------------------------------------------------

/// Request sent to a model provider.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatRequest {
    pub model: String,
    pub messages: Vec<ChatMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<ToolDefinition>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f64>,
    #[serde(default)]
    pub stream: bool,
}

/// Token usage statistics from a model response.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct Usage {
    pub input_tokens: u64,
    pub output_tokens: u64,
}

/// Why the model stopped generating.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FinishReason {
    Stop,
    ToolUse,
    MaxTokens,
    Error,
}

/// Complete response from a model provider (non-streaming).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatResponse {
    pub id: String,
    pub model: String,
    pub content: String,
    #[serde(default)]
    pub tool_calls: Vec<ToolCall>,
    pub usage: Usage,
    pub finish_reason: FinishReason,
}

/// A chunk from a streaming model response.
#[derive(Debug, Clone)]
pub enum StreamChunk {
    /// Incremental text content.
    ContentDelta(String),
    /// Start of a tool call.
    ToolCallStart { id: String, name: String },
    /// Incremental tool call arguments.
    ToolCallDelta { id: String, arguments_delta: String },
    /// Stream complete with usage stats.
    Done(Usage),
    /// Stream error.
    Error(String),
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn role_serde_lowercase() {
        assert_eq!(serde_json::to_string(&Role::System).unwrap(), r#""system""#);
        assert_eq!(serde_json::to_string(&Role::User).unwrap(), r#""user""#);
        assert_eq!(serde_json::to_string(&Role::Assistant).unwrap(), r#""assistant""#);
        assert_eq!(serde_json::to_string(&Role::Tool).unwrap(), r#""tool""#);
    }

    #[test]
    fn role_roundtrip() {
        for role in [Role::System, Role::User, Role::Assistant, Role::Tool] {
            let json = serde_json::to_string(&role).unwrap();
            let parsed: Role = serde_json::from_str(&json).unwrap();
            assert_eq!(parsed, role);
        }
    }

    #[test]
    fn finish_reason_serde() {
        assert_eq!(serde_json::to_string(&FinishReason::Stop).unwrap(), r#""stop""#);
        assert_eq!(serde_json::to_string(&FinishReason::ToolUse).unwrap(), r#""tool_use""#);
        assert_eq!(serde_json::to_string(&FinishReason::MaxTokens).unwrap(), r#""max_tokens""#);
        assert_eq!(serde_json::to_string(&FinishReason::Error).unwrap(), r#""error""#);
    }

    #[test]
    fn finish_reason_roundtrip() {
        for reason in [FinishReason::Stop, FinishReason::ToolUse, FinishReason::MaxTokens, FinishReason::Error] {
            let json = serde_json::to_string(&reason).unwrap();
            let parsed: FinishReason = serde_json::from_str(&json).unwrap();
            assert_eq!(parsed, reason);
        }
    }

    #[test]
    fn attachment_serde_roundtrip() {
        let a = Attachment {
            file_path: "/tmp/photo.jpg".into(),
            mime_type: "image/jpeg".into(),
            file_name: Some("photo.jpg".into()),
        };
        let json = serde_json::to_string(&a).unwrap();
        let parsed: Attachment = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.file_path, "/tmp/photo.jpg");
        assert_eq!(parsed.mime_type, "image/jpeg");
        assert_eq!(parsed.file_name.as_deref(), Some("photo.jpg"));
    }

    #[test]
    fn attachment_file_name_omitted_when_none() {
        let a = Attachment {
            file_path: "/tmp/file.bin".into(),
            mime_type: "application/octet-stream".into(),
            file_name: None,
        };
        let json = serde_json::to_value(&a).unwrap();
        assert!(json.get("file_name").is_none());
    }

    #[test]
    fn chat_message_with_attachments_serde() {
        let msg = ChatMessage {
            role: Role::User,
            content: "check this image".into(),
            name: None,
            tool_calls: None,
            tool_call_id: None,
            attachments: Some(vec![Attachment {
                file_path: "/tmp/img.png".into(),
                mime_type: "image/png".into(),
                file_name: Some("img.png".into()),
            }]),
        };
        let json = serde_json::to_value(&msg).unwrap();
        let atts = json["attachments"].as_array().unwrap();
        assert_eq!(atts.len(), 1);
        assert_eq!(atts[0]["mime_type"], "image/png");

        // Round-trip
        let parsed: ChatMessage = serde_json::from_value(json).unwrap();
        let atts = parsed.attachments.unwrap();
        assert_eq!(atts[0].file_path, "/tmp/img.png");
    }

    #[test]
    fn chat_message_attachments_omitted_when_none() {
        let msg = ChatMessage {
            role: Role::User,
            content: "plain text".into(),
            name: None,
            tool_calls: None,
            tool_call_id: None,
            attachments: None,
        };
        let json = serde_json::to_value(&msg).unwrap();
        assert!(json.get("attachments").is_none());
    }

    #[test]
    fn chat_message_serde_omits_none_fields() {
        let msg = ChatMessage {
            role: Role::User,
            content: "hello".into(),
            name: None,
            tool_calls: None,
            tool_call_id: None,
            attachments: None,
        };
        let json = serde_json::to_value(&msg).unwrap();
        let obj = json.as_object().unwrap();
        assert!(!obj.contains_key("name"));
        assert!(!obj.contains_key("tool_calls"));
        assert!(!obj.contains_key("tool_call_id"));
    }

    #[test]
    fn chat_request_serde_omits_none_fields() {
        let req = ChatRequest {
            model: "claude-3".into(),
            messages: vec![],
            system: None,
            tools: None,
            max_tokens: None,
            temperature: None,
            stream: false,
        };
        let json = serde_json::to_value(&req).unwrap();
        let obj = json.as_object().unwrap();
        assert!(!obj.contains_key("system"));
        assert!(!obj.contains_key("tools"));
        assert!(!obj.contains_key("max_tokens"));
        assert!(!obj.contains_key("temperature"));
    }

    #[test]
    fn chat_response_roundtrip() {
        let resp = ChatResponse {
            id: "resp-1".into(),
            model: "claude-3".into(),
            content: "Hello!".into(),
            tool_calls: vec![],
            usage: Usage { input_tokens: 10, output_tokens: 5 },
            finish_reason: FinishReason::Stop,
        };
        let json = serde_json::to_string(&resp).unwrap();
        let parsed: ChatResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.id, "resp-1");
        assert_eq!(parsed.usage.input_tokens, 10);
        assert_eq!(parsed.finish_reason, FinishReason::Stop);
    }

    #[test]
    fn tool_call_serde() {
        let tc = ToolCall {
            id: "tc-1".into(),
            name: "search".into(),
            arguments: serde_json::json!({"query": "rust"}),
        };
        let json = serde_json::to_string(&tc).unwrap();
        let parsed: ToolCall = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.name, "search");
        assert_eq!(parsed.arguments["query"], "rust");
    }

    #[test]
    fn tool_result_serde() {
        let tr = ToolResult {
            tool_call_id: "tc-1".into(),
            content: "found 42 results".into(),
            is_error: false,
        };
        let json = serde_json::to_string(&tr).unwrap();
        let parsed: ToolResult = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.tool_call_id, "tc-1");
        assert!(!parsed.is_error);
    }

    #[test]
    fn usage_default() {
        let u = Usage::default();
        assert_eq!(u.input_tokens, 0);
        assert_eq!(u.output_tokens, 0);
    }

    #[test]
    fn stream_chunk_variants() {
        let delta = StreamChunk::ContentDelta("hello".into());
        assert!(matches!(delta, StreamChunk::ContentDelta(s) if s == "hello"));

        let start = StreamChunk::ToolCallStart { id: "1".into(), name: "search".into() };
        assert!(matches!(start, StreamChunk::ToolCallStart { ref name, .. } if name == "search"));

        let done = StreamChunk::Done(Usage { input_tokens: 5, output_tokens: 10 });
        assert!(matches!(done, StreamChunk::Done(u) if u.output_tokens == 10));

        let err = StreamChunk::Error("boom".into());
        assert!(matches!(err, StreamChunk::Error(s) if s == "boom"));
    }
}
