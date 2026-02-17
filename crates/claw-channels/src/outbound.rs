//! Outbound message delivery system.
//!
//! Ports OpenClaw's `src/infra/outbound/deliver.ts`. Handles:
//! - Payload normalization (ReplyPayload → adapter calls)
//! - Text chunking with configurable limits
//! - Routing to the correct channel outbound adapter
//! - Delivery retry with exponential backoff

use tokio_util::sync::CancellationToken;
use tracing::{debug, warn};

use claw_core::backoff::{BackoffPolicy, compute_backoff, sleep_with_abort};

use crate::msg_context::ReplyPayload;
use crate::plugin::ChannelOutboundAdapter;
use crate::registry::ChannelRegistry;
use crate::types::{ChannelError, ChannelOutboundContext, OutboundDeliveryResult};

// ---------------------------------------------------------------------------
// Text chunking
// ---------------------------------------------------------------------------

/// Default character limit per message chunk (Telegram's limit).
pub const DEFAULT_CHUNK_LIMIT: usize = 4096;

/// Split text into chunks that respect the given character limit.
///
/// Splitting strategy (in priority order):
/// 1. Split at paragraph boundaries (`\n\n`)
/// 2. Split at line boundaries (`\n`)
/// 3. Split at sentence boundaries (`. `)
/// 4. Split at word boundaries (` `)
/// 5. Hard split at the limit
///
/// Preserves open code fences across chunk boundaries by prepending
/// the fence to continuation chunks.
pub fn chunk_text(text: &str, limit: usize) -> Vec<String> {
    if text.len() <= limit {
        return vec![text.to_owned()];
    }

    let mut chunks = Vec::new();
    let mut remaining = text;

    while !remaining.is_empty() {
        if remaining.len() <= limit {
            chunks.push(remaining.to_owned());
            break;
        }

        let split_at = find_split_point(remaining, limit);
        let (chunk, rest) = remaining.split_at(split_at);
        chunks.push(chunk.trim_end().to_owned());
        remaining = rest.trim_start();
    }

    // Fix open code fences across chunks
    fix_code_fences(&mut chunks);

    chunks
}

/// Find the best split point within `text[..limit]`.
fn find_split_point(text: &str, limit: usize) -> usize {
    let search_region = &text[..limit];

    // Try paragraph boundary
    if let Some(pos) = search_region.rfind("\n\n") {
        if pos > 0 {
            return pos + 1; // include one newline
        }
    }

    // Try line boundary
    if let Some(pos) = search_region.rfind('\n') {
        if pos > 0 {
            return pos + 1;
        }
    }

    // Try sentence boundary
    if let Some(pos) = search_region.rfind(". ") {
        if pos > 0 {
            return pos + 2; // include the period and space
        }
    }

    // Try word boundary
    if let Some(pos) = search_region.rfind(' ') {
        if pos > 0 {
            return pos + 1;
        }
    }

    // Hard split at limit
    limit
}

/// Ensure code fences opened in one chunk are closed and reopened
/// in continuation chunks.
fn fix_code_fences(chunks: &mut [String]) {
    let mut open_fence: Option<String> = None;

    for chunk in chunks.iter_mut() {
        // If we have an open fence from the previous chunk, prepend it
        if let Some(fence) = open_fence.take() {
            *chunk = format!("{fence}\n{chunk}");
        }

        // Count code fences in this chunk
        let fence_count = chunk.matches("```").count();
        if fence_count % 2 != 0 {
            // Odd number of fences means one is unclosed
            // Find the opening fence line to know the language
            let fence_line = chunk
                .lines()
                .rev()
                .find(|line| line.starts_with("```"))
                .unwrap_or("```")
                .to_owned();

            // Close this chunk's fence
            chunk.push_str("\n```");

            // Remember to open the next chunk with the same fence
            open_fence = Some(fence_line);
        }
    }
}

// ---------------------------------------------------------------------------
// Delivery options
// ---------------------------------------------------------------------------

/// Configuration for outbound delivery behavior.
#[derive(Debug, Clone)]
pub struct DeliveryOptions {
    /// Maximum characters per message chunk.
    pub chunk_limit: usize,

    /// Maximum retry attempts on transient failure.
    pub max_retries: u32,

    /// Backoff policy for retries.
    pub backoff: BackoffPolicy,

    /// Cancellation token for the delivery attempt.
    pub cancel: CancellationToken,
}

impl Default for DeliveryOptions {
    fn default() -> Self {
        Self {
            chunk_limit: DEFAULT_CHUNK_LIMIT,
            max_retries: 3,
            backoff: BackoffPolicy {
                base_ms: 500,
                max_ms: 10_000,
                factor: 2.0,
                jitter: 0.1,
            },
            cancel: CancellationToken::new(),
        }
    }
}

// ---------------------------------------------------------------------------
// deliver_reply
// ---------------------------------------------------------------------------

/// Deliver a reply payload through the appropriate channel adapter.
///
/// Handles text chunking, media delivery, and retry logic.
/// Returns results for each chunk/media delivery attempt.
pub async fn deliver_reply(
    registry: &ChannelRegistry,
    channel_id: &str,
    account_id: &str,
    chat_id: &str,
    payload: &ReplyPayload,
    options: &DeliveryOptions,
) -> Result<Vec<OutboundDeliveryResult>, ChannelError> {
    let dock = registry
        .get(channel_id)
        .ok_or_else(|| ChannelError::NotFound(channel_id.to_owned()))?;

    let plugin = dock
        .plugin()
        .ok_or_else(|| ChannelError::NotEnabled(channel_id.to_owned()))?;

    let adapter = plugin
        .outbound_adapter()
        .ok_or_else(|| ChannelError::AdapterNotSupported {
            channel: channel_id.to_owned(),
            adapter: "outbound".into(),
        })?;

    let mut results = Vec::new();

    // Deliver text content (chunked)
    if let Some(text) = &payload.text {
        let chunks = chunk_text(text, options.chunk_limit);
        for (i, chunk) in chunks.iter().enumerate() {
            let ctx = ChannelOutboundContext {
                account_id: account_id.to_owned(),
                chat_id: chat_id.to_owned(),
                thread_id: None,
                // Only reply to the original message on the first chunk
                reply_to_message_id: if i == 0 {
                    payload.reply_to_id.clone()
                } else {
                    None
                },
                cancel: options.cancel.clone(),
            };

            let result =
                deliver_with_retry(adapter, &ctx, chunk, &options.backoff, options.max_retries)
                    .await;
            results.push(result);
        }
    }

    // Deliver media attachments
    let media_urls = payload
        .media_urls
        .as_deref()
        .or(payload.media_url.as_ref().map(|u| std::slice::from_ref(u)));

    if let Some(urls) = media_urls {
        for url in urls {
            let ctx = ChannelOutboundContext {
                account_id: account_id.to_owned(),
                chat_id: chat_id.to_owned(),
                thread_id: None,
                reply_to_message_id: None,
                cancel: options.cancel.clone(),
            };
            let result = adapter.send_media(&ctx, url, None).await;
            match result {
                Ok(r) => results.push(r),
                Err(e) => {
                    warn!(channel = channel_id, url = url, "media delivery failed: {e}");
                    results.push(OutboundDeliveryResult {
                        success: Some(false),
                        error: Some(e.to_string()),
                        ..Default::default()
                    });
                }
            }
        }
    }

    Ok(results)
}

/// Send text with exponential backoff retry.
async fn deliver_with_retry(
    adapter: &dyn ChannelOutboundAdapter,
    ctx: &ChannelOutboundContext,
    text: &str,
    backoff: &BackoffPolicy,
    max_retries: u32,
) -> OutboundDeliveryResult {
    for attempt in 0..=max_retries {
        match adapter.send_text(ctx, text).await {
            Ok(result) => return result,
            Err(e) if attempt < max_retries => {
                let delay = compute_backoff(backoff, attempt);
                debug!(
                    attempt,
                    delay_ms = delay.as_millis() as u64,
                    "delivery failed, retrying: {e}"
                );
                if sleep_with_abort(delay, ctx.cancel.clone()).await.is_err() {
                    return OutboundDeliveryResult {
                        success: Some(false),
                        error: Some("delivery cancelled".into()),
                        ..Default::default()
                    };
                }
            }
            Err(e) => {
                return OutboundDeliveryResult {
                    success: Some(false),
                    error: Some(e.to_string()),
                    ..Default::default()
                };
            }
        }
    }

    OutboundDeliveryResult {
        success: Some(false),
        error: Some("max retries exceeded".into()),
        ..Default::default()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -- chunk_text ---------------------------------------------------------

    #[test]
    fn short_text_not_chunked() {
        let chunks = chunk_text("hello world", 100);
        assert_eq!(chunks, vec!["hello world"]);
    }

    #[test]
    fn empty_text() {
        let chunks = chunk_text("", 100);
        assert_eq!(chunks, vec![""]);
    }

    #[test]
    fn split_at_paragraph_boundary() {
        let text = "First paragraph.\n\nSecond paragraph.";
        let chunks = chunk_text(text, 20);
        assert_eq!(chunks.len(), 2);
        assert_eq!(chunks[0], "First paragraph.");
        assert_eq!(chunks[1], "Second paragraph.");
    }

    #[test]
    fn split_at_line_boundary() {
        let text = "Line one.\nLine two.\nLine three.";
        let chunks = chunk_text(text, 15);
        assert!(chunks.len() >= 2);
        // Each chunk should be within limit
        for chunk in &chunks {
            assert!(chunk.len() <= 15, "chunk too long: {chunk}");
        }
    }

    #[test]
    fn split_at_word_boundary() {
        let text = "one two three four five six seven eight";
        let chunks = chunk_text(text, 15);
        assert!(chunks.len() >= 2);
        for chunk in &chunks {
            assert!(chunk.len() <= 15, "chunk too long: '{chunk}'");
        }
    }

    #[test]
    fn hard_split_when_no_boundary() {
        let text = "a".repeat(30);
        let chunks = chunk_text(&text, 10);
        assert_eq!(chunks.len(), 3);
        for chunk in &chunks {
            assert!(chunk.len() <= 10);
        }
    }

    #[test]
    fn code_fence_closure() {
        let text = "Before\n```python\ndef hello():\n    print('world')\n```\nAfter";
        let chunks = chunk_text(text, 30);
        // Verify no chunk has an odd number of code fences
        for chunk in &chunks {
            let fences = chunk.matches("```").count();
            assert_eq!(
                fences % 2,
                0,
                "odd fence count in chunk: {chunk}"
            );
        }
    }

    #[test]
    fn respects_default_limit() {
        let text = "x".repeat(8192);
        let chunks = chunk_text(&text, DEFAULT_CHUNK_LIMIT);
        assert_eq!(chunks.len(), 2);
        assert_eq!(chunks[0].len(), DEFAULT_CHUNK_LIMIT);
        assert_eq!(chunks[1].len(), 8192 - DEFAULT_CHUNK_LIMIT);
    }

    #[test]
    fn sentence_boundary_split() {
        let text = "First sentence. Second sentence. Third sentence.";
        let chunks = chunk_text(text, 35);
        assert!(chunks.len() >= 2);
        // First chunk should end at a sentence boundary
        assert!(
            chunks[0].ends_with(". ") || chunks[0].ends_with('.'),
            "expected sentence boundary: '{}'",
            chunks[0]
        );
    }

    // -- DeliveryOptions ----------------------------------------------------

    #[test]
    fn default_options() {
        let opts = DeliveryOptions::default();
        assert_eq!(opts.chunk_limit, DEFAULT_CHUNK_LIMIT);
        assert_eq!(opts.max_retries, 3);
    }
}
