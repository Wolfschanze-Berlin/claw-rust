//! Anthropic Claude API provider implementation.
//!
//! Implements the [`ModelProvider`] trait for the Anthropic Messages API,
//! supporting both blocking and streaming chat completions, tool use,
//! and system prompt injection.
//!
//! SSE stream parsing is delegated to the [`anthropic_sse`](crate::anthropic_sse) module.

use std::time::Duration;

use async_trait::async_trait;
use reqwest::header::{HeaderMap, HeaderValue, CONTENT_TYPE};
use serde::{Deserialize, Serialize};
use tracing::debug;

use crate::anthropic_sse::parse_sse_stream;
use crate::error::ModelError;
use crate::provider::{ChatStream, ModelProvider};
use crate::types::{
    Attachment, ChatMessage, ChatRequest, ChatResponse, FinishReason, Role, ToolCall,
    ToolDefinition, Usage,
};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

const DEFAULT_BASE_URL: &str = "https://api.anthropic.com";
const API_VERSION: &str = "2023-06-01";
const DEFAULT_MODEL: &str = "claude-sonnet-4-20250514";
const DEFAULT_MAX_TOKENS: u64 = 4096;

// ---------------------------------------------------------------------------
// Provider
// ---------------------------------------------------------------------------

/// Anthropic Claude model provider.
///
/// Communicates with the [Anthropic Messages API](https://docs.anthropic.com/en/api/messages)
/// using `reqwest` for HTTP and server-sent events for streaming.
///
/// # Example
///
/// ```no_run
/// use claw_agent_models::anthropic::AnthropicProvider;
///
/// let provider = AnthropicProvider::new("sk-ant-...".into());
/// ```
pub struct AnthropicProvider {
    client: reqwest::Client,
    api_key: String,
    base_url: String,
    default_model: String,
    max_context_window: u64,
}

impl AnthropicProvider {
    /// Create a new Anthropic provider with the given API key.
    pub fn new(api_key: String) -> Self {
        Self {
            client: reqwest::Client::new(),
            api_key,
            base_url: DEFAULT_BASE_URL.into(),
            default_model: DEFAULT_MODEL.into(),
            max_context_window: 200_000,
        }
    }

    /// Override the base URL (useful for testing or proxies).
    pub fn with_base_url(mut self, base_url: String) -> Self {
        self.base_url = base_url;
        self
    }

    /// Override the default model.
    pub fn with_model(mut self, model: String) -> Self {
        self.default_model = model;
        self
    }

    /// Override the maximum context window size.
    pub fn with_max_context_window(mut self, max_tokens: u64) -> Self {
        self.max_context_window = max_tokens;
        self
    }

    /// Build the common headers for Anthropic API requests.
    fn build_headers(&self) -> Result<HeaderMap, ModelError> {
        let mut headers = HeaderMap::new();
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        headers.insert(
            "x-api-key",
            HeaderValue::from_str(&self.api_key).map_err(|e| ModelError::AuthError {
                provider: "anthropic".into(),
                message: format!("invalid API key header value: {e}"),
            })?,
        );
        headers.insert(
            "anthropic-version",
            HeaderValue::from_static(API_VERSION),
        );
        Ok(headers)
    }

    /// Convert our generic [`ChatRequest`] into the Anthropic wire format.
    fn build_request_body(&self, request: &ChatRequest, stream: bool) -> AnthropicRequest {
        let model = if request.model.is_empty() {
            self.default_model.clone()
        } else {
            request.model.clone()
        };

        let max_tokens = request.max_tokens.unwrap_or(DEFAULT_MAX_TOKENS);

        let messages: Vec<AnthropicMessage> = request
            .messages
            .iter()
            .filter(|m| m.role != Role::System)
            .map(|m| convert_message(m))
            .collect();

        let tools: Option<Vec<AnthropicTool>> = request
            .tools
            .as_ref()
            .map(|defs| defs.iter().map(convert_tool_definition).collect());

        AnthropicRequest {
            model,
            max_tokens,
            messages,
            system: request.system.clone(),
            tools,
            temperature: request.temperature,
            stream,
        }
    }

    /// Classify an HTTP error response into the appropriate [`ModelError`].
    async fn classify_error(&self, response: reqwest::Response) -> ModelError {
        let status = response.status().as_u16();
        let retry_after = response
            .headers()
            .get("retry-after")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.parse::<u64>().ok())
            .map(Duration::from_secs);

        let body = response.text().await.unwrap_or_default();

        match status {
            401 => ModelError::AuthError {
                provider: "anthropic".into(),
                message: extract_error_message(&body),
            },
            429 => ModelError::RateLimited {
                provider: "anthropic".into(),
                retry_after,
            },
            400 if body.contains("context") || body.contains("token") => {
                ModelError::ContextLengthExceeded {
                    limit: self.max_context_window,
                    actual: 0, // Anthropic doesn't always report the exact count
                }
            }
            _ => ModelError::ProviderError {
                provider: "anthropic".into(),
                message: extract_error_message(&body),
                status_code: Some(status),
            },
        }
    }

    /// Map a reqwest transport error into the appropriate [`ModelError`].
    fn map_transport_error(e: reqwest::Error) -> ModelError {
        if e.is_timeout() {
            ModelError::Timeout {
                elapsed: Duration::from_secs(0),
            }
        } else {
            ModelError::ProviderError {
                provider: "anthropic".into(),
                message: e.to_string(),
                status_code: e.status().map(|s| s.as_u16()),
            }
        }
    }
}

// ---------------------------------------------------------------------------
// ModelProvider implementation
// ---------------------------------------------------------------------------

#[async_trait]
impl ModelProvider for AnthropicProvider {
    fn provider_name(&self) -> &str {
        "anthropic"
    }

    fn supports_tools(&self) -> bool {
        true
    }

    fn max_context_window(&self) -> u64 {
        self.max_context_window
    }

    async fn chat_completion(
        &self,
        request: &ChatRequest,
    ) -> Result<ChatResponse, ModelError> {
        let headers = self.build_headers()?;
        let body = self.build_request_body(request, false);

        debug!(model = %body.model, "sending non-streaming request to Anthropic");

        let response = self
            .client
            .post(format!("{}/v1/messages", self.base_url))
            .headers(headers)
            .json(&body)
            .send()
            .await
            .map_err(Self::map_transport_error)?;

        if !response.status().is_success() {
            return Err(self.classify_error(response).await);
        }

        let api_resp: AnthropicResponse = response.json().await.map_err(|e| {
            ModelError::ProviderError {
                provider: "anthropic".into(),
                message: format!("failed to parse response: {e}"),
                status_code: None,
            }
        })?;

        Ok(convert_response(api_resp))
    }

    async fn chat_completion_stream(
        &self,
        request: &ChatRequest,
    ) -> Result<ChatStream, ModelError> {
        let headers = self.build_headers()?;
        let body = self.build_request_body(request, true);

        debug!(model = %body.model, "sending streaming request to Anthropic");

        let response = self
            .client
            .post(format!("{}/v1/messages", self.base_url))
            .headers(headers)
            .json(&body)
            .send()
            .await
            .map_err(Self::map_transport_error)?;

        if !response.status().is_success() {
            return Err(self.classify_error(response).await);
        }

        let byte_stream = response.bytes_stream();
        let sse_stream = parse_sse_stream(byte_stream);
        Ok(Box::pin(sse_stream))
    }
}

// ---------------------------------------------------------------------------
// Anthropic API wire types (private)
// ---------------------------------------------------------------------------

#[derive(Serialize)]
struct AnthropicRequest {
    model: String,
    max_tokens: u64,
    messages: Vec<AnthropicMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    system: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<AnthropicTool>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f64>,
    stream: bool,
}

#[derive(Serialize)]
struct AnthropicMessage {
    role: String,
    content: AnthropicContent,
}

/// Anthropic content can be a plain string or an array of content blocks.
#[derive(Serialize)]
#[serde(untagged)]
enum AnthropicContent {
    Text(String),
    Blocks(Vec<AnthropicContentBlock>),
}

#[derive(Serialize)]
#[serde(tag = "type")]
enum AnthropicContentBlock {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "image")]
    Image { source: ImageSource },
    #[serde(rename = "tool_use")]
    ToolUse {
        id: String,
        name: String,
        input: serde_json::Value,
    },
    #[serde(rename = "tool_result")]
    ToolResult {
        tool_use_id: String,
        content: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        is_error: Option<bool>,
    },
}

/// Base64-encoded image source for the Anthropic content block.
#[derive(Serialize)]
struct ImageSource {
    #[serde(rename = "type")]
    source_type: &'static str,
    media_type: String,
    data: String,
}

#[derive(Serialize)]
struct AnthropicTool {
    name: String,
    description: String,
    input_schema: serde_json::Value,
}

// ---------------------------------------------------------------------------
// Anthropic API response types (private)
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct AnthropicResponse {
    id: String,
    model: String,
    content: Vec<AnthropicResponseBlock>,
    stop_reason: Option<String>,
    usage: AnthropicUsage,
}

#[derive(Deserialize)]
#[serde(tag = "type")]
enum AnthropicResponseBlock {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "tool_use")]
    ToolUse {
        id: String,
        name: String,
        input: serde_json::Value,
    },
}

#[derive(Deserialize, Default)]
struct AnthropicUsage {
    #[serde(default)]
    input_tokens: u64,
    #[serde(default)]
    output_tokens: u64,
}

// ---------------------------------------------------------------------------
// Conversion helpers
// ---------------------------------------------------------------------------

/// Convert a generic [`ChatMessage`] into the Anthropic wire format.
fn convert_message(msg: &ChatMessage) -> AnthropicMessage {
    let role = match msg.role {
        Role::User | Role::System => "user",
        Role::Assistant => "assistant",
        Role::Tool => "user",
    };

    // Tool result messages use structured content blocks.
    if msg.role == Role::Tool {
        if let Some(ref tool_call_id) = msg.tool_call_id {
            return AnthropicMessage {
                role: role.into(),
                content: AnthropicContent::Blocks(vec![AnthropicContentBlock::ToolResult {
                    tool_use_id: tool_call_id.clone(),
                    content: msg.content.clone(),
                    is_error: None,
                }]),
            };
        }
    }

    // Assistant messages with tool calls use structured content blocks.
    if msg.role == Role::Assistant {
        if let Some(ref calls) = msg.tool_calls {
            if !calls.is_empty() {
                let mut blocks = Vec::new();
                if !msg.content.is_empty() {
                    blocks.push(AnthropicContentBlock::Text {
                        text: msg.content.clone(),
                    });
                }
                for call in calls {
                    blocks.push(AnthropicContentBlock::ToolUse {
                        id: call.id.clone(),
                        name: call.name.clone(),
                        input: call.arguments.clone(),
                    });
                }
                return AnthropicMessage {
                    role: role.into(),
                    content: AnthropicContent::Blocks(blocks),
                };
            }
        }
    }

    // User messages with attachments use structured content blocks
    // (image blocks before text block).
    if msg.role == Role::User {
        if let Some(ref attachments) = msg.attachments {
            let image_blocks = build_image_blocks(attachments);
            if !image_blocks.is_empty() {
                let mut blocks = image_blocks;
                if !msg.content.is_empty() {
                    blocks.push(AnthropicContentBlock::Text {
                        text: msg.content.clone(),
                    });
                }
                return AnthropicMessage {
                    role: role.into(),
                    content: AnthropicContent::Blocks(blocks),
                };
            }
        }
    }

    AnthropicMessage {
        role: role.into(),
        content: AnthropicContent::Text(msg.content.clone()),
    }
}

/// Read attachments from disk, base64-encode them, and produce Image content blocks.
///
/// Non-image attachments and files that fail to read are silently skipped.
fn build_image_blocks(attachments: &[Attachment]) -> Vec<AnthropicContentBlock> {
    use base64::Engine;

    attachments
        .iter()
        .filter(|a| a.mime_type.starts_with("image/"))
        .filter_map(|a| {
            let bytes = std::fs::read(&a.file_path)
                .map_err(|e| {
                    tracing::warn!(
                        file_path = %a.file_path,
                        error = %e,
                        "failed to read attachment file for base64 encoding"
                    );
                    e
                })
                .ok()?;
            let encoded = base64::engine::general_purpose::STANDARD.encode(&bytes);
            Some(AnthropicContentBlock::Image {
                source: ImageSource {
                    source_type: "base64",
                    media_type: a.mime_type.clone(),
                    data: encoded,
                },
            })
        })
        .collect()
}

/// Convert a [`ToolDefinition`] into the Anthropic tool format.
fn convert_tool_definition(def: &ToolDefinition) -> AnthropicTool {
    AnthropicTool {
        name: def.name.clone(),
        description: def.description.clone(),
        input_schema: def.parameters.clone(),
    }
}

/// Convert an Anthropic API response into our generic [`ChatResponse`].
fn convert_response(resp: AnthropicResponse) -> ChatResponse {
    let mut content = String::new();
    let mut tool_calls = Vec::new();

    for block in &resp.content {
        match block {
            AnthropicResponseBlock::Text { text } => {
                content.push_str(text);
            }
            AnthropicResponseBlock::ToolUse { id, name, input } => {
                tool_calls.push(ToolCall {
                    id: id.clone(),
                    name: name.clone(),
                    arguments: input.clone(),
                });
            }
        }
    }

    let finish_reason = match resp.stop_reason.as_deref() {
        Some("end_turn") | Some("stop") => FinishReason::Stop,
        Some("tool_use") => FinishReason::ToolUse,
        Some("max_tokens") => FinishReason::MaxTokens,
        _ => FinishReason::Stop,
    };

    ChatResponse {
        id: resp.id,
        model: resp.model,
        content,
        tool_calls,
        usage: Usage {
            input_tokens: resp.usage.input_tokens,
            output_tokens: resp.usage.output_tokens,
        },
        finish_reason,
    }
}

/// Extract a human-readable error message from an Anthropic error response body.
fn extract_error_message(body: &str) -> String {
    serde_json::from_str::<serde_json::Value>(body)
        .ok()
        .and_then(|v| {
            v.get("error")
                .and_then(|e| e.get("message"))
                .and_then(|m| m.as_str())
                .map(String::from)
        })
        .unwrap_or_else(|| body.to_string())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use base64::Engine;

    #[test]
    fn provider_info() {
        let provider = AnthropicProvider::new("test-key".into());
        assert_eq!(provider.provider_name(), "anthropic");
        assert!(provider.supports_tools());
        assert_eq!(provider.max_context_window(), 200_000);
    }

    #[test]
    fn builder_methods() {
        let provider = AnthropicProvider::new("key".into())
            .with_base_url("http://localhost:8080".into())
            .with_model("claude-3-haiku".into())
            .with_max_context_window(100_000);

        assert_eq!(provider.base_url, "http://localhost:8080");
        assert_eq!(provider.default_model, "claude-3-haiku");
        assert_eq!(provider.max_context_window, 100_000);
    }

    #[test]
    fn request_body_serialization() {
        let provider = AnthropicProvider::new("key".into());
        let request = ChatRequest {
            model: "claude-sonnet-4-20250514".into(),
            messages: vec![ChatMessage {
                role: Role::User,
                content: "Hello".into(),
                name: None,
                tool_calls: None,
                tool_call_id: None,
                attachments: None,
            }],
            system: Some("You are helpful.".into()),
            tools: None,
            max_tokens: Some(1024),
            temperature: Some(0.7),
            stream: false,
        };

        let body = provider.build_request_body(&request, false);
        let json = serde_json::to_value(&body).expect("serialize");

        assert_eq!(json["model"], "claude-sonnet-4-20250514");
        assert_eq!(json["max_tokens"], 1024);
        assert_eq!(json["system"], "You are helpful.");
        assert_eq!(json["temperature"], 0.7);
        assert!(!json["stream"].as_bool().unwrap());
        assert_eq!(json["messages"][0]["role"], "user");
        assert_eq!(json["messages"][0]["content"], "Hello");
        // system should NOT appear as a message
        assert_eq!(json["messages"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn request_body_uses_default_model_when_empty() {
        let provider = AnthropicProvider::new("key".into());
        let request = ChatRequest {
            model: String::new(),
            messages: vec![],
            system: None,
            tools: None,
            max_tokens: None,
            temperature: None,
            stream: false,
        };

        let body = provider.build_request_body(&request, false);
        assert_eq!(body.model, DEFAULT_MODEL);
        assert_eq!(body.max_tokens, DEFAULT_MAX_TOKENS);
    }

    #[test]
    fn tool_definition_conversion() {
        let def = ToolDefinition {
            name: "search".into(),
            description: "Search the web".into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string" }
                }
            }),
        };

        let converted = convert_tool_definition(&def);
        assert_eq!(converted.name, "search");
        assert_eq!(converted.description, "Search the web");
        assert_eq!(converted.input_schema["type"], "object");
    }

    #[test]
    fn request_body_with_tools() {
        let provider = AnthropicProvider::new("key".into());
        let request = ChatRequest {
            model: "claude-sonnet-4-20250514".into(),
            messages: vec![],
            system: None,
            tools: Some(vec![ToolDefinition {
                name: "get_weather".into(),
                description: "Get weather".into(),
                parameters: serde_json::json!({"type": "object"}),
            }]),
            max_tokens: None,
            temperature: None,
            stream: false,
        };

        let body = provider.build_request_body(&request, false);
        let json = serde_json::to_value(&body).expect("serialize");

        let tools = json["tools"].as_array().expect("tools array");
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0]["name"], "get_weather");
        assert_eq!(tools[0]["input_schema"]["type"], "object");
    }

    #[test]
    fn tool_result_message_conversion() {
        let msg = ChatMessage {
            role: Role::Tool,
            content: "The weather is sunny.".into(),
            name: None,
            tool_calls: None,
            tool_call_id: Some("tc_123".into()),
            attachments: None,
        };

        let converted = convert_message(&msg);
        assert_eq!(converted.role, "user");

        let json = serde_json::to_value(&converted).expect("serialize");
        let blocks = json["content"].as_array().expect("content blocks");
        assert_eq!(blocks[0]["type"], "tool_result");
        assert_eq!(blocks[0]["tool_use_id"], "tc_123");
        assert_eq!(blocks[0]["content"], "The weather is sunny.");
    }

    #[test]
    fn assistant_tool_call_message_conversion() {
        let msg = ChatMessage {
            role: Role::Assistant,
            content: "Let me check.".into(),
            name: None,
            tool_calls: Some(vec![ToolCall {
                id: "tc_456".into(),
                name: "search".into(),
                arguments: serde_json::json!({"q": "rust"}),
            }]),
            tool_call_id: None,
            attachments: None,
        };

        let converted = convert_message(&msg);
        assert_eq!(converted.role, "assistant");

        let json = serde_json::to_value(&converted).expect("serialize");
        let blocks = json["content"].as_array().expect("content blocks");
        assert_eq!(blocks.len(), 2);
        assert_eq!(blocks[0]["type"], "text");
        assert_eq!(blocks[0]["text"], "Let me check.");
        assert_eq!(blocks[1]["type"], "tool_use");
        assert_eq!(blocks[1]["id"], "tc_456");
    }

    #[test]
    fn response_conversion() {
        let api_resp = AnthropicResponse {
            id: "msg_123".into(),
            model: "claude-sonnet-4-20250514".into(),
            content: vec![AnthropicResponseBlock::Text {
                text: "Hello!".into(),
            }],
            stop_reason: Some("end_turn".into()),
            usage: AnthropicUsage {
                input_tokens: 10,
                output_tokens: 5,
            },
        };

        let resp = convert_response(api_resp);
        assert_eq!(resp.id, "msg_123");
        assert_eq!(resp.content, "Hello!");
        assert_eq!(resp.finish_reason, FinishReason::Stop);
        assert_eq!(resp.usage.input_tokens, 10);
        assert_eq!(resp.usage.output_tokens, 5);
        assert!(resp.tool_calls.is_empty());
    }

    #[test]
    fn response_conversion_with_tool_use() {
        let api_resp = AnthropicResponse {
            id: "msg_456".into(),
            model: "claude-sonnet-4-20250514".into(),
            content: vec![
                AnthropicResponseBlock::Text {
                    text: "I'll search.".into(),
                },
                AnthropicResponseBlock::ToolUse {
                    id: "tc_789".into(),
                    name: "search".into(),
                    input: serde_json::json!({"query": "rust"}),
                },
            ],
            stop_reason: Some("tool_use".into()),
            usage: AnthropicUsage {
                input_tokens: 20,
                output_tokens: 15,
            },
        };

        let resp = convert_response(api_resp);
        assert_eq!(resp.content, "I'll search.");
        assert_eq!(resp.finish_reason, FinishReason::ToolUse);
        assert_eq!(resp.tool_calls.len(), 1);
        assert_eq!(resp.tool_calls[0].name, "search");
    }

    #[test]
    fn response_conversion_max_tokens() {
        let api_resp = AnthropicResponse {
            id: "msg_max".into(),
            model: "claude-sonnet-4-20250514".into(),
            content: vec![AnthropicResponseBlock::Text {
                text: "truncated...".into(),
            }],
            stop_reason: Some("max_tokens".into()),
            usage: AnthropicUsage::default(),
        };

        let resp = convert_response(api_resp);
        assert_eq!(resp.finish_reason, FinishReason::MaxTokens);
    }

    #[test]
    fn extract_error_message_from_json() {
        let body = r#"{"type":"error","error":{"type":"authentication_error","message":"invalid x-api-key"}}"#;
        assert_eq!(extract_error_message(body), "invalid x-api-key");
    }

    #[test]
    fn extract_error_message_from_plain_text() {
        let body = "Internal Server Error";
        assert_eq!(extract_error_message(body), "Internal Server Error");
    }

    #[test]
    fn convert_message_user_with_image_attachment() {
        // Create a temp file with known bytes to test base64 encoding.
        let dir = tempfile::tempdir().unwrap();
        let img_path = dir.path().join("test.png");
        let pixel_bytes: Vec<u8> = vec![0x89, 0x50, 0x4E, 0x47]; // PNG magic bytes
        std::fs::write(&img_path, &pixel_bytes).unwrap();

        let msg = ChatMessage {
            role: Role::User,
            content: "What is in this image?".into(),
            name: None,
            tool_calls: None,
            tool_call_id: None,
            attachments: Some(vec![Attachment {
                file_path: img_path.to_str().unwrap().into(),
                mime_type: "image/png".into(),
                file_name: Some("test.png".into()),
            }]),
        };

        let converted = convert_message(&msg);
        assert_eq!(converted.role, "user");

        let json = serde_json::to_value(&converted).expect("serialize");
        let blocks = json["content"].as_array().expect("content blocks");
        // Should have 2 blocks: image first, then text.
        assert_eq!(blocks.len(), 2);
        assert_eq!(blocks[0]["type"], "image");
        assert_eq!(blocks[0]["source"]["type"], "base64");
        assert_eq!(blocks[0]["source"]["media_type"], "image/png");
        // Verify the base64 data matches our pixel bytes.
        let expected_b64 = base64::engine::general_purpose::STANDARD.encode(&pixel_bytes);
        assert_eq!(blocks[0]["source"]["data"], expected_b64);
        assert_eq!(blocks[1]["type"], "text");
        assert_eq!(blocks[1]["text"], "What is in this image?");
    }

    #[test]
    fn convert_message_user_non_image_attachment_skipped() {
        // Non-image MIME types should be silently skipped.
        let dir = tempfile::tempdir().unwrap();
        let pdf_path = dir.path().join("doc.pdf");
        std::fs::write(&pdf_path, b"fake pdf content").unwrap();

        let msg = ChatMessage {
            role: Role::User,
            content: "Check this PDF".into(),
            name: None,
            tool_calls: None,
            tool_call_id: None,
            attachments: Some(vec![Attachment {
                file_path: pdf_path.to_str().unwrap().into(),
                mime_type: "application/pdf".into(),
                file_name: Some("doc.pdf".into()),
            }]),
        };

        let converted = convert_message(&msg);
        let json = serde_json::to_value(&converted).expect("serialize");
        // No image blocks, so should fall back to plain text content.
        assert_eq!(json["content"], "Check this PDF");
    }

    #[test]
    fn convert_message_user_missing_file_skipped() {
        let msg = ChatMessage {
            role: Role::User,
            content: "Look at this".into(),
            name: None,
            tool_calls: None,
            tool_call_id: None,
            attachments: Some(vec![Attachment {
                file_path: "/nonexistent/path/image.png".into(),
                mime_type: "image/png".into(),
                file_name: None,
            }]),
        };

        let converted = convert_message(&msg);
        let json = serde_json::to_value(&converted).expect("serialize");
        // File doesn't exist → image block skipped → plain text fallback.
        assert_eq!(json["content"], "Look at this");
    }

    #[test]
    fn build_image_blocks_multiple_attachments() {
        let dir = tempfile::tempdir().unwrap();
        let img1 = dir.path().join("a.jpg");
        let img2 = dir.path().join("b.png");
        std::fs::write(&img1, b"jpeg-data").unwrap();
        std::fs::write(&img2, b"png-data").unwrap();

        let attachments = vec![
            Attachment {
                file_path: img1.to_str().unwrap().into(),
                mime_type: "image/jpeg".into(),
                file_name: None,
            },
            Attachment {
                file_path: img2.to_str().unwrap().into(),
                mime_type: "image/png".into(),
                file_name: None,
            },
        ];

        let blocks = build_image_blocks(&attachments);
        assert_eq!(blocks.len(), 2);

        let json: Vec<serde_json::Value> = blocks
            .iter()
            .map(|b| serde_json::to_value(b).unwrap())
            .collect();
        assert_eq!(json[0]["source"]["media_type"], "image/jpeg");
        assert_eq!(json[1]["source"]["media_type"], "image/png");
    }

    #[test]
    fn system_messages_filtered_from_messages() {
        let provider = AnthropicProvider::new("key".into());
        let request = ChatRequest {
            model: "claude-sonnet-4-20250514".into(),
            messages: vec![
                ChatMessage {
                    role: Role::System,
                    content: "Be helpful".into(),
                    name: None,
                    tool_calls: None,
                    tool_call_id: None,
                    attachments: None,
                },
                ChatMessage {
                    role: Role::User,
                    content: "Hi".into(),
                    name: None,
                    tool_calls: None,
                    tool_call_id: None,
                    attachments: None,
                },
            ],
            system: Some("System prompt".into()),
            tools: None,
            max_tokens: None,
            temperature: None,
            stream: false,
        };

        let body = provider.build_request_body(&request, false);
        assert_eq!(body.messages.len(), 1);
        assert_eq!(body.messages[0].role, "user");
    }
}
