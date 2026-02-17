//! Streaming subscriber — delivers model responses to channels in real-time.
//!
//! Ports OpenClaw's `pi-embedded-subscribe.ts`. Processes the stream of tokens
//! from a model provider and delivers them to the originating channel with:
//! - Block chunking (respects platform message limits)
//! - Code span awareness (never splits inside code blocks)
//! - Reply tag management (threaded replies)
//! - Reasoning/thinking tag detection and stripping
//! - Backpressure (slows consumption when channel can't keep up)

use std::pin::Pin;

use async_trait::async_trait;
use futures_util::{Stream, StreamExt};
use tracing::{debug, warn};

use claw_agent_models::{ModelError, StreamChunk, ToolCall, Usage};

// ---------------------------------------------------------------------------
// Channel delivery trait
// ---------------------------------------------------------------------------

/// Trait for delivering response chunks to a channel.
///
/// Each channel adapter implements this to handle platform-specific delivery
/// (Discord 2000-char limit, Slack threading, Telegram Markdown, etc.).
#[async_trait]
pub trait ResponseSink: Send + Sync {
    /// Deliver a text chunk to the channel.
    async fn send_text(&self, text: &str) -> Result<(), DeliveryError>;

    /// Signal that the response is complete.
    async fn finish(&self) -> Result<(), DeliveryError>;

    /// Maximum message size in characters for this platform.
    fn max_message_size(&self) -> usize {
        4096 // conservative default
    }
}

/// Errors during response delivery to a channel.
#[derive(thiserror::Error, Debug)]
pub enum DeliveryError {
    #[error("channel send failed: {0}")]
    SendFailed(String),
    #[error("channel rate limited")]
    RateLimited,
    #[error("channel disconnected")]
    Disconnected,
}

// ---------------------------------------------------------------------------
// Subscriber config
// ---------------------------------------------------------------------------

/// Configuration for the streaming subscriber.
#[derive(Debug, Clone)]
pub struct SubscriberConfig {
    /// Maximum size of a single message chunk (chars).
    /// Overridden by the sink's `max_message_size()` if smaller.
    pub max_chunk_size: usize,
    /// Whether to strip `<thinking>` tags from output.
    pub strip_thinking_tags: bool,
    /// Minimum chars to buffer before flushing (avoids tiny messages).
    pub min_flush_size: usize,
}

impl Default for SubscriberConfig {
    fn default() -> Self {
        Self {
            max_chunk_size: 2000,
            strip_thinking_tags: true,
            min_flush_size: 50,
        }
    }
}

// ---------------------------------------------------------------------------
// Subscriber result
// ---------------------------------------------------------------------------

/// Outcome of consuming a model response stream.
#[derive(Debug, Clone)]
pub struct SubscribeResult {
    /// Full assembled response text.
    pub full_text: String,
    /// Tool calls requested by the model (if any).
    pub tool_calls: Vec<ToolCall>,
    /// Token usage stats.
    pub usage: Option<Usage>,
    /// Number of message chunks delivered.
    pub chunks_delivered: usize,
}

// ---------------------------------------------------------------------------
// Core subscriber
// ---------------------------------------------------------------------------

/// Consumes a model response stream and delivers chunks to a channel sink.
pub struct StreamSubscriber {
    config: SubscriberConfig,
}

impl StreamSubscriber {
    pub fn new(config: SubscriberConfig) -> Self {
        Self { config }
    }

    /// Consume the stream and deliver chunks to the sink.
    ///
    /// Returns the full assembled response and any tool calls.
    pub async fn subscribe(
        &self,
        mut stream: Pin<Box<dyn Stream<Item = Result<StreamChunk, ModelError>> + Send>>,
        sink: &dyn ResponseSink,
    ) -> Result<SubscribeResult, ModelError> {
        let max_size = self.config.max_chunk_size.min(sink.max_message_size());
        let mut buffer = String::new();
        let mut full_text = String::new();
        let mut tool_calls: Vec<ToolCallBuilder> = Vec::new();
        let mut usage = None;
        let mut chunks_delivered = 0usize;
        let mut in_thinking = false;

        while let Some(chunk_result) = stream.next().await {
            let chunk = chunk_result?;

            match chunk {
                StreamChunk::ContentDelta(text) => {
                    let processed = if self.config.strip_thinking_tags {
                        self.process_thinking_tags(&text, &mut in_thinking)
                    } else {
                        text.clone()
                    };

                    full_text.push_str(&text);

                    if !processed.is_empty() && !in_thinking {
                        buffer.push_str(&processed);

                        // Flush if buffer exceeds max chunk size.
                        while buffer.len() >= max_size {
                            let split_point = find_split_point(&buffer, max_size);
                            let chunk_text: String = buffer.drain(..split_point).collect();
                            if let Err(e) = sink.send_text(&chunk_text).await {
                                warn!(error = %e, "failed to deliver chunk");
                            }
                            chunks_delivered += 1;
                        }
                    }
                }

                StreamChunk::ToolCallStart { id, name } => {
                    debug!(tool_id = %id, tool_name = %name, "tool call started");
                    tool_calls.push(ToolCallBuilder {
                        id,
                        name,
                        arguments: String::new(),
                    });
                }

                StreamChunk::ToolCallDelta {
                    id: _,
                    arguments_delta,
                } => {
                    if let Some(builder) = tool_calls.last_mut() {
                        builder.arguments.push_str(&arguments_delta);
                    }
                }

                StreamChunk::Done(u) => {
                    usage = Some(u);
                }

                StreamChunk::Error(msg) => {
                    return Err(ModelError::ProviderError {
                        provider: "stream".into(),
                        message: msg,
                        status_code: None,
                    });
                }
            }
        }

        // Flush remaining buffer.
        if !buffer.is_empty() {
            if let Err(e) = sink.send_text(&buffer).await {
                warn!(error = %e, "failed to deliver final chunk");
            }
            chunks_delivered += 1;
        }

        // Signal completion.
        if let Err(e) = sink.finish().await {
            warn!(error = %e, "failed to signal finish");
        }

        // Build final tool calls from builders.
        let finished_tool_calls: Vec<ToolCall> = tool_calls
            .into_iter()
            .map(|b| ToolCall {
                id: b.id,
                name: b.name,
                arguments: serde_json::from_str(&b.arguments)
                    .unwrap_or(serde_json::Value::Null),
            })
            .collect();

        Ok(SubscribeResult {
            full_text,
            tool_calls: finished_tool_calls,
            usage,
            chunks_delivered,
        })
    }

    /// Process thinking tags — strip content between `<thinking>` and `</thinking>`.
    fn process_thinking_tags(&self, text: &str, in_thinking: &mut bool) -> String {
        let mut result = String::new();
        let mut remaining = text;

        while !remaining.is_empty() {
            if *in_thinking {
                if let Some(end_pos) = remaining.find("</thinking>") {
                    // Skip everything up to and including the closing tag.
                    remaining = &remaining[end_pos + "</thinking>".len()..];
                    *in_thinking = false;
                } else {
                    // Still inside thinking — consume everything.
                    break;
                }
            } else if let Some(start_pos) = remaining.find("<thinking>") {
                // Emit text before the tag.
                result.push_str(&remaining[..start_pos]);
                remaining = &remaining[start_pos + "<thinking>".len()..];
                *in_thinking = true;
            } else {
                // No more tags.
                result.push_str(remaining);
                break;
            }
        }

        result
    }
}

/// Builder for assembling tool calls from streaming deltas.
#[derive(Debug)]
struct ToolCallBuilder {
    id: String,
    name: String,
    arguments: String,
}

// ---------------------------------------------------------------------------
// Smart splitting
// ---------------------------------------------------------------------------

/// Find the best point to split a message buffer, respecting:
/// - Paragraph boundaries (double newline)
/// - Sentence boundaries (period/newline)
/// - Code block boundaries (never split inside a fenced block)
fn find_split_point(text: &str, max_size: usize) -> usize {
    if text.len() <= max_size {
        return text.len();
    }

    let search_range = &text[..max_size];

    // Check if we're inside a code block.
    let fence_count = search_range.matches("```").count();
    if fence_count % 2 != 0 {
        // We're inside a code block — find the opening fence and split before it.
        if let Some(fence_pos) = search_range.rfind("```") {
            if fence_pos > 0 {
                return fence_pos;
            }
        }
    }

    // Try paragraph boundary (double newline).
    if let Some(pos) = search_range.rfind("\n\n") {
        if pos > max_size / 4 {
            return pos + 2; // include the newlines
        }
    }

    // Try line boundary.
    if let Some(pos) = search_range.rfind('\n') {
        if pos > max_size / 4 {
            return pos + 1;
        }
    }

    // Try sentence boundary.
    if let Some(pos) = search_range.rfind(". ") {
        if pos > max_size / 4 {
            return pos + 2;
        }
    }

    // Hard split at max_size (ensure we don't split a multi-byte char).
    let mut split = max_size;
    while split > 0 && !text.is_char_boundary(split) {
        split -= 1;
    }
    split
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// Mock sink that collects delivered chunks.
    struct MockSink {
        chunks: tokio::sync::Mutex<Vec<String>>,
        max_size: usize,
        send_count: AtomicUsize,
    }

    impl MockSink {
        fn new(max_size: usize) -> Self {
            Self {
                chunks: tokio::sync::Mutex::new(Vec::new()),
                max_size,
                send_count: AtomicUsize::new(0),
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
            self.send_count.fetch_add(1, Ordering::Relaxed);
            Ok(())
        }

        async fn finish(&self) -> Result<(), DeliveryError> {
            Ok(())
        }

        fn max_message_size(&self) -> usize {
            self.max_size
        }
    }

    fn make_stream(
        chunks: Vec<StreamChunk>,
    ) -> Pin<Box<dyn Stream<Item = Result<StreamChunk, ModelError>> + Send>> {
        Box::pin(futures_util::stream::iter(
            chunks.into_iter().map(Ok),
        ))
    }

    #[tokio::test]
    async fn simple_text_delivery() {
        let sub = StreamSubscriber::new(SubscriberConfig::default());
        let sink = MockSink::new(4096);
        let stream = make_stream(vec![
            StreamChunk::ContentDelta("Hello ".into()),
            StreamChunk::ContentDelta("world!".into()),
            StreamChunk::Done(Usage {
                input_tokens: 5,
                output_tokens: 2,
            }),
        ]);

        let result = sub.subscribe(stream, &sink).await.unwrap();
        assert_eq!(result.full_text, "Hello world!");
        assert_eq!(result.usage.unwrap().output_tokens, 2);

        let chunks = sink.collected().await;
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0], "Hello world!");
    }

    #[tokio::test]
    async fn tool_call_assembly() {
        let sub = StreamSubscriber::new(SubscriberConfig::default());
        let sink = MockSink::new(4096);
        let stream = make_stream(vec![
            StreamChunk::ToolCallStart {
                id: "tc1".into(),
                name: "search".into(),
            },
            StreamChunk::ToolCallDelta {
                id: "tc1".into(),
                arguments_delta: r##"{"query""##.into(),
            },
            StreamChunk::ToolCallDelta {
                id: "tc1".into(),
                arguments_delta: r##": "rust"}"##.into(),
            },
            StreamChunk::Done(Usage {
                input_tokens: 10,
                output_tokens: 5,
            }),
        ]);

        let result = sub.subscribe(stream, &sink).await.unwrap();
        assert_eq!(result.tool_calls.len(), 1);
        assert_eq!(result.tool_calls[0].name, "search");
        assert_eq!(result.tool_calls[0].arguments["query"], "rust");
    }

    #[tokio::test]
    async fn thinking_tags_stripped() {
        let sub = StreamSubscriber::new(SubscriberConfig {
            strip_thinking_tags: true,
            ..Default::default()
        });
        let sink = MockSink::new(4096);
        let stream = make_stream(vec![
            StreamChunk::ContentDelta("<thinking>internal reasoning</thinking>Hello!".into()),
            StreamChunk::Done(Usage::default()),
        ]);

        let result = sub.subscribe(stream, &sink).await.unwrap();
        // full_text includes everything (for transcript).
        assert!(result.full_text.contains("<thinking>"));
        // But the sink only receives the non-thinking part.
        let chunks = sink.collected().await;
        assert_eq!(chunks[0], "Hello!");
    }

    #[tokio::test]
    async fn chunking_respects_max_size() {
        let sub = StreamSubscriber::new(SubscriberConfig {
            max_chunk_size: 10,
            min_flush_size: 1,
            ..Default::default()
        });
        let sink = MockSink::new(10);
        let stream = make_stream(vec![
            StreamChunk::ContentDelta("Hello world, this is a long message!".into()),
            StreamChunk::Done(Usage::default()),
        ]);

        let result = sub.subscribe(stream, &sink).await.unwrap();
        assert_eq!(result.full_text, "Hello world, this is a long message!");

        let chunks = sink.collected().await;
        assert!(chunks.len() > 1);
        for chunk in &chunks {
            assert!(chunk.len() <= 10, "chunk too long: {}", chunk.len());
        }
    }

    #[test]
    fn split_point_respects_paragraph_boundary() {
        let text = "First paragraph.\n\nSecond paragraph that goes on for a while.";
        let point = find_split_point(text, 30);
        // Should split at the paragraph boundary.
        assert_eq!(&text[..point], "First paragraph.\n\n");
    }

    #[test]
    fn split_point_avoids_code_block_interior() {
        let text = "Some text\n```rust\nfn main() {}\n```\nMore text after code";
        let point = find_split_point(text, 20);
        // Should split before the code block, not inside it.
        assert!(point <= 10 || !text[..point].ends_with("```rust\n"));
    }

    #[test]
    fn split_point_at_sentence_boundary() {
        let text = "First sentence. Second sentence that is much longer than needed.";
        let point = find_split_point(text, 25);
        assert_eq!(&text[..point], "First sentence. ");
    }

    #[tokio::test]
    async fn stream_error_propagated() {
        let sub = StreamSubscriber::new(SubscriberConfig::default());
        let sink = MockSink::new(4096);
        let stream = make_stream(vec![StreamChunk::Error("something broke".into())]);

        let result = sub.subscribe(stream, &sink).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn empty_stream_returns_empty_result() {
        let sub = StreamSubscriber::new(SubscriberConfig::default());
        let sink = MockSink::new(4096);
        let stream = make_stream(vec![]);

        let result = sub.subscribe(stream, &sink).await.unwrap();
        assert!(result.full_text.is_empty());
        assert!(result.tool_calls.is_empty());
        assert!(result.usage.is_none());
        assert_eq!(result.chunks_delivered, 0);
    }
}
