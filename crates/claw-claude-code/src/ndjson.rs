//! NDJSON line parser with buffer management.
//!
//! Parses newline-delimited JSON from Claude Code's stdout stream into
//! typed [`ClaudeMessage`] values. Handles partial lines across chunk
//! boundaries, empty lines, and enforces a configurable buffer size limit.

use tracing::warn;

use crate::error::{ClaudeCodeError, ClaudeCodeResult};
use crate::types::ClaudeMessage;

/// Default maximum size for a single NDJSON line (1 MB).
pub const DEFAULT_MAX_LINE_SIZE: usize = 1024 * 1024;

// ---------------------------------------------------------------------------
// NdjsonParser
// ---------------------------------------------------------------------------

/// Streaming NDJSON parser that buffers partial lines across chunk boundaries.
///
/// # Usage
///
/// ```ignore
/// let mut parser = NdjsonParser::new();
///
/// // Feed chunks as they arrive from stdout.
/// for chunk in chunks {
///     let messages = parser.feed(&chunk)?;
///     for msg in messages {
///         // process msg
///     }
/// }
///
/// // At EOF, flush any remaining buffered data.
/// let remaining = parser.finish()?;
/// ```
pub struct NdjsonParser {
    /// Partial line buffer accumulating bytes until a newline arrives.
    buffer: String,

    /// Number of complete lines processed so far (for error reporting).
    line_count: usize,

    /// Maximum allowed line size before returning `BufferOverflow`.
    max_line_size: usize,
}

impl NdjsonParser {
    /// Create a new parser with the default 1 MB buffer limit.
    pub fn new() -> Self {
        Self {
            buffer: String::new(),
            line_count: 0,
            max_line_size: DEFAULT_MAX_LINE_SIZE,
        }
    }

    /// Create a parser with a custom buffer size limit.
    pub fn with_max_line_size(max_line_size: usize) -> Self {
        Self {
            buffer: String::new(),
            line_count: 0,
            max_line_size,
        }
    }

    /// Feed a chunk of text and return all complete messages parsed from it.
    ///
    /// Partial lines (no trailing newline) are buffered for the next `feed()`
    /// call. Empty lines are silently skipped. Malformed JSON lines are logged
    /// at warn level and skipped (forward compatibility).
    ///
    /// Returns `Err(BufferOverflow)` if the accumulated buffer exceeds the
    /// configured maximum line size.
    pub fn feed(&mut self, chunk: &str) -> ClaudeCodeResult<Vec<ClaudeMessage>> {
        self.buffer.push_str(chunk);

        // Check buffer overflow before parsing.
        if self.buffer.len() > self.max_line_size {
            let size = self.buffer.len();
            self.buffer.clear();
            return Err(ClaudeCodeError::BufferOverflow { size });
        }

        let mut messages = Vec::new();

        // Extract and parse all complete lines (terminated by \n).
        while let Some(newline_pos) = self.buffer.find('\n') {
            let line: String = self.buffer.drain(..=newline_pos).collect();
            self.line_count += 1;

            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }

            match serde_json::from_str::<ClaudeMessage>(trimmed) {
                Ok(msg) => messages.push(msg),
                Err(e) => {
                    warn!(
                        line = self.line_count,
                        error = %e,
                        content = %truncate(trimmed, 200),
                        "skipping unparseable NDJSON line"
                    );
                }
            }
        }

        Ok(messages)
    }

    /// Flush any remaining buffered data at EOF.
    ///
    /// Call this when the Claude Code process stdout closes. If a non-empty
    /// partial line remains, it will be parsed as a complete message.
    pub fn finish(&mut self) -> ClaudeCodeResult<Vec<ClaudeMessage>> {
        let remaining = std::mem::take(&mut self.buffer);
        let trimmed = remaining.trim();

        if trimmed.is_empty() {
            return Ok(Vec::new());
        }

        self.line_count += 1;

        match serde_json::from_str::<ClaudeMessage>(trimmed) {
            Ok(msg) => Ok(vec![msg]),
            Err(e) => {
                warn!(
                    line = self.line_count,
                    error = %e,
                    "failed to parse final NDJSON line"
                );
                Ok(Vec::new())
            }
        }
    }

    /// Number of complete lines processed so far.
    pub fn lines_processed(&self) -> usize {
        self.line_count
    }
}

impl Default for NdjsonParser {
    fn default() -> Self {
        Self::new()
    }
}

/// Truncate a string for log output.
fn truncate(s: &str, max: usize) -> &str {
    if s.len() <= max {
        s
    } else {
        &s[..max]
    }
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::ClaudeMessage;

    #[test]
    fn parse_complete_lines() {
        let mut parser = NdjsonParser::new();
        let chunk = concat!(
            r#"{"type":"system","session_id":"s1"}"#,
            "\n",
            r#"{"type":"assistant","content":[{"type":"text","text":"hi"}]}"#,
            "\n",
        );
        let msgs = parser.feed(chunk).unwrap();
        assert_eq!(msgs.len(), 2);
        assert!(matches!(msgs[0], ClaudeMessage::System(_)));
        assert!(matches!(msgs[1], ClaudeMessage::Assistant(_)));
        assert_eq!(parser.lines_processed(), 2);
    }

    #[test]
    fn buffer_partial_line_across_chunks() {
        let mut parser = NdjsonParser::new();

        // First chunk: partial line (no newline).
        let msgs1 = parser.feed(r#"{"type":"system","sess"#).unwrap();
        assert_eq!(msgs1.len(), 0);

        // Second chunk: completes the line.
        let msgs2 = parser.feed("ion_id\":\"s1\"}\n").unwrap();
        assert_eq!(msgs2.len(), 1);
        assert!(matches!(msgs2[0], ClaudeMessage::System(_)));
    }

    #[test]
    fn skip_empty_lines() {
        let mut parser = NdjsonParser::new();
        let chunk = "\n\n{\"type\":\"system\",\"session_id\":\"s1\"}\n\n\n";
        let msgs = parser.feed(chunk).unwrap();
        assert_eq!(msgs.len(), 1);
    }

    #[test]
    fn skip_malformed_json() {
        let mut parser = NdjsonParser::new();
        let chunk = "not json at all\n{\"type\":\"system\",\"session_id\":\"s1\"}\n";
        let msgs = parser.feed(chunk).unwrap();
        // Malformed line skipped, valid line parsed.
        assert_eq!(msgs.len(), 1);
        assert!(matches!(msgs[0], ClaudeMessage::System(_)));
    }

    #[test]
    fn finish_parses_remaining_buffer() {
        let mut parser = NdjsonParser::new();
        // No newline at end — simulate EOF.
        parser.feed(r#"{"type":"system","session_id":"s1"}"#).unwrap();
        let msgs = parser.finish().unwrap();
        assert_eq!(msgs.len(), 1);
    }

    #[test]
    fn finish_empty_buffer() {
        let mut parser = NdjsonParser::new();
        let msgs = parser.finish().unwrap();
        assert_eq!(msgs.len(), 0);
    }

    #[test]
    fn buffer_overflow_returns_error() {
        let mut parser = NdjsonParser::with_max_line_size(100);
        let huge_line = "x".repeat(101);
        let err = parser.feed(&huge_line).unwrap_err();
        assert!(matches!(err, ClaudeCodeError::BufferOverflow { size: 101 }));
    }

    #[test]
    fn multiple_messages_in_one_chunk() {
        let mut parser = NdjsonParser::new();
        let chunk = concat!(
            r#"{"type":"system","session_id":"s1"}"#,
            "\n",
            r#"{"type":"assistant","content":[{"type":"text","text":"a"}]}"#,
            "\n",
            r#"{"type":"assistant","content":[{"type":"text","text":"b"}]}"#,
            "\n",
            r#"{"type":"result","session_id":"s1","num_turns":1,"duration_ms":100}"#,
            "\n",
        );
        let msgs = parser.feed(chunk).unwrap();
        assert_eq!(msgs.len(), 4);
    }

    #[test]
    fn unknown_message_type_is_parsed() {
        let mut parser = NdjsonParser::new();
        let chunk = "{\"type\":\"future_feature\",\"data\":\"value\"}\n";
        let msgs = parser.feed(chunk).unwrap();
        assert_eq!(msgs.len(), 1);
        assert!(matches!(msgs[0], ClaudeMessage::Unknown));
    }

    #[test]
    fn lines_processed_counter() {
        let mut parser = NdjsonParser::new();
        parser
            .feed("{\"type\":\"system\",\"session_id\":\"s1\"}\n")
            .unwrap();
        assert_eq!(parser.lines_processed(), 1);

        parser
            .feed("{\"type\":\"system\",\"session_id\":\"s2\"}\n")
            .unwrap();
        assert_eq!(parser.lines_processed(), 2);
    }
}
