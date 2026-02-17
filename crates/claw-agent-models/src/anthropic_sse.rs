//! Server-Sent Events (SSE) stream parser for the Anthropic Messages API.
//!
//! Parses the raw byte stream from an Anthropic streaming response into
//! [`StreamChunk`]s that the agent runtime can consume.

use futures_util::{Stream, StreamExt};
use tracing::warn;

use crate::error::ModelError;
use crate::types::{StreamChunk, Usage};

/// Parse a raw byte stream from reqwest into a stream of [`StreamChunk`]s.
///
/// Anthropic SSE events follow the format:
/// ```text
/// event: <event_type>
/// data: <json_payload>
/// ```
pub(crate) fn parse_sse_stream(
    byte_stream: impl Stream<Item = Result<bytes::Bytes, reqwest::Error>> + Send + 'static,
) -> impl Stream<Item = Result<StreamChunk, ModelError>> + Send {
    let state = SseParserState::default();

    futures_util::stream::unfold(
        (Box::pin(byte_stream), state),
        |(mut stream, mut state)| async move {
            loop {
                // First, drain any buffered chunks from the last parse.
                if let Some(chunk) = state.pending_chunks.pop() {
                    return Some((chunk, (stream, state)));
                }

                // Read next bytes from the wire.
                match stream.next().await {
                    None => return None,
                    Some(Err(e)) => {
                        let err = Err(ModelError::ProviderError {
                            provider: "anthropic".into(),
                            message: format!("stream error: {e}"),
                            status_code: None,
                        });
                        return Some((err, (stream, state)));
                    }
                    Some(Ok(bytes)) => {
                        state
                            .buffer
                            .push_str(&String::from_utf8_lossy(&bytes));
                        parse_sse_buffer(&mut state);
                        // Loop back to drain pending_chunks.
                    }
                }
            }
        },
    )
}

// ---------------------------------------------------------------------------
// Parser state
// ---------------------------------------------------------------------------

#[derive(Default)]
pub(crate) struct SseParserState {
    buffer: String,
    current_event_type: Option<String>,
    pending_chunks: Vec<Result<StreamChunk, ModelError>>,
    /// Track tool call IDs by content block index.
    tool_call_ids: Vec<Option<String>>,
}

// ---------------------------------------------------------------------------
// Buffer parsing
// ---------------------------------------------------------------------------

/// Parse complete SSE lines from the buffer, converting them to stream chunks.
pub(crate) fn parse_sse_buffer(state: &mut SseParserState) {
    let mut new_chunks = Vec::new();

    while let Some(newline_pos) = state.buffer.find('\n') {
        let line = state.buffer[..newline_pos]
            .trim_end_matches('\r')
            .to_string();
        state.buffer = state.buffer[newline_pos + 1..].to_string();

        if line.is_empty() {
            state.current_event_type = None;
            continue;
        }

        if let Some(event_type) = line.strip_prefix("event: ") {
            state.current_event_type = Some(event_type.to_string());
            continue;
        }

        if let Some(data) = line.strip_prefix("data: ") {
            if let Some(chunk) = parse_sse_data(data, state) {
                new_chunks.push(chunk);
            }
        }
    }

    // Prepend new chunks (they should come before any existing pending).
    new_chunks.extend(state.pending_chunks.drain(..));
    state.pending_chunks = new_chunks;
    // Reverse so pop() yields in order.
    state.pending_chunks.reverse();
}

// ---------------------------------------------------------------------------
// Event data parsing
// ---------------------------------------------------------------------------

/// Parse a single SSE `data:` payload into an optional [`StreamChunk`].
pub(crate) fn parse_sse_data(
    data: &str,
    state: &mut SseParserState,
) -> Option<Result<StreamChunk, ModelError>> {
    let value: serde_json::Value = match serde_json::from_str(data) {
        Ok(v) => v,
        Err(e) => {
            warn!(data, error = %e, "failed to parse SSE data as JSON");
            return None;
        }
    };

    let event_type = value
        .get("type")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    match event_type {
        "content_block_start" => parse_content_block_start(&value, state),
        "content_block_delta" => parse_content_block_delta(&value, state),
        "message_delta" => parse_message_delta(&value),
        "message_start" => {
            // Usage comes at message_delta; nothing to emit here.
            None
        }
        "error" => {
            let msg = value
                .get("error")
                .and_then(|e| e.get("message"))
                .and_then(|m| m.as_str())
                .unwrap_or("unknown streaming error");
            Some(Ok(StreamChunk::Error(msg.to_string())))
        }
        // content_block_stop, message_stop, ping -- no action needed.
        _ => None,
    }
}

fn parse_content_block_start(
    value: &serde_json::Value,
    state: &mut SseParserState,
) -> Option<Result<StreamChunk, ModelError>> {
    let index = value.get("index").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
    let block = value.get("content_block")?;
    let block_type = block.get("type").and_then(|v| v.as_str()).unwrap_or("");

    if block_type == "tool_use" {
        let id = block.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string();
        let name = block.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string();

        while state.tool_call_ids.len() <= index {
            state.tool_call_ids.push(None);
        }
        state.tool_call_ids[index] = Some(id.clone());

        return Some(Ok(StreamChunk::ToolCallStart { id, name }));
    }
    None
}

fn parse_content_block_delta(
    value: &serde_json::Value,
    state: &mut SseParserState,
) -> Option<Result<StreamChunk, ModelError>> {
    let index = value.get("index").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
    let delta = value.get("delta")?;
    let delta_type = delta.get("type").and_then(|v| v.as_str()).unwrap_or("");

    match delta_type {
        "text_delta" => {
            let text = delta.get("text").and_then(|v| v.as_str()).unwrap_or("");
            Some(Ok(StreamChunk::ContentDelta(text.to_string())))
        }
        "input_json_delta" => {
            let partial = delta
                .get("partial_json")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let id = state
                .tool_call_ids
                .get(index)
                .and_then(|opt| opt.clone())
                .unwrap_or_default();
            Some(Ok(StreamChunk::ToolCallDelta {
                id,
                arguments_delta: partial.to_string(),
            }))
        }
        _ => None,
    }
}

fn parse_message_delta(
    value: &serde_json::Value,
) -> Option<Result<StreamChunk, ModelError>> {
    let usage_val = value.get("usage");
    let usage = usage_val
        .map(|u| Usage {
            input_tokens: u.get("input_tokens").and_then(|v| v.as_u64()).unwrap_or(0),
            output_tokens: u.get("output_tokens").and_then(|v| v.as_u64()).unwrap_or(0),
        })
        .unwrap_or_default();
    Some(Ok(StreamChunk::Done(usage)))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sse_content_delta_parsing() {
        let mut state = SseParserState::default();
        let data = r#"{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Hello"}}"#;
        let result = parse_sse_data(data, &mut state);
        assert!(result.is_some());
        let chunk = result.unwrap().expect("ok chunk");
        assert!(matches!(chunk, StreamChunk::ContentDelta(text) if text == "Hello"));
    }

    #[test]
    fn sse_tool_call_start_parsing() {
        let mut state = SseParserState::default();
        let data = r#"{"type":"content_block_start","index":1,"content_block":{"type":"tool_use","id":"tc_1","name":"search"}}"#;
        let result = parse_sse_data(data, &mut state);
        assert!(result.is_some());
        let chunk = result.unwrap().expect("ok chunk");
        assert!(matches!(chunk, StreamChunk::ToolCallStart { ref id, ref name }
            if id == "tc_1" && name == "search"));

        // Verify the tool call ID was tracked.
        assert_eq!(
            state.tool_call_ids.get(1).and_then(|o| o.as_deref()),
            Some("tc_1")
        );
    }

    #[test]
    fn sse_tool_call_delta_parsing() {
        let mut state = SseParserState::default();
        state.tool_call_ids.push(Some("tc_1".into()));

        let data = r#"{"type":"content_block_delta","index":0,"delta":{"type":"input_json_delta","partial_json":"{\"q\":"}}"#;
        let result = parse_sse_data(data, &mut state);
        assert!(result.is_some());
        let chunk = result.unwrap().expect("ok chunk");
        assert!(
            matches!(chunk, StreamChunk::ToolCallDelta { ref id, ref arguments_delta }
                if id == "tc_1" && arguments_delta == "{\"q\":")
        );
    }

    #[test]
    fn sse_message_delta_parsing() {
        let mut state = SseParserState::default();
        let data = r#"{"type":"message_delta","delta":{"stop_reason":"end_turn"},"usage":{"output_tokens":42}}"#;
        let result = parse_sse_data(data, &mut state);
        assert!(result.is_some());
        let chunk = result.unwrap().expect("ok chunk");
        assert!(matches!(chunk, StreamChunk::Done(usage) if usage.output_tokens == 42));
    }

    #[test]
    fn sse_error_event_parsing() {
        let mut state = SseParserState::default();
        let data = r#"{"type":"error","error":{"type":"overloaded_error","message":"Overloaded"}}"#;
        let result = parse_sse_data(data, &mut state);
        assert!(result.is_some());
        let chunk = result.unwrap().expect("ok chunk");
        assert!(matches!(chunk, StreamChunk::Error(msg) if msg == "Overloaded"));
    }

    #[test]
    fn sse_unknown_event_ignored() {
        let mut state = SseParserState::default();
        let data = r#"{"type":"ping"}"#;
        assert!(parse_sse_data(data, &mut state).is_none());
    }

    #[test]
    fn sse_buffer_parsing() {
        let mut state = SseParserState::default();
        state.buffer = "event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"Hi\"}}\n\n".into();
        parse_sse_buffer(&mut state);

        assert_eq!(state.pending_chunks.len(), 1);
        let chunk = state.pending_chunks.pop().unwrap().expect("ok");
        assert!(matches!(chunk, StreamChunk::ContentDelta(text) if text == "Hi"));
    }

    #[test]
    fn sse_partial_buffer_waits_for_newline() {
        let mut state = SseParserState::default();
        state.buffer = "data: {\"type\":\"content_block_delta\"".into();
        parse_sse_buffer(&mut state);

        assert!(state.pending_chunks.is_empty());
        assert!(state.buffer.contains("content_block_delta"));
    }
}
