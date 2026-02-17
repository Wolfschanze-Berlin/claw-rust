//! Error types for the Claude Code runtime.

use std::time::Duration;

/// Errors that can occur during Claude Code subprocess operations.
#[derive(thiserror::Error, Debug)]
pub enum ClaudeCodeError {
    /// The `claude` CLI binary was not found in PATH.
    #[error("Claude Code CLI not found at '{path}'")]
    CliNotFound { path: String },

    /// Failed to spawn the subprocess.
    #[error("failed to spawn Claude Code process: {reason}")]
    SpawnFailed { reason: String },

    /// Process exited with a non-zero status code.
    #[error("Claude Code process exited with code {code}: {stderr}")]
    NonZeroExit { code: i32, stderr: String },

    /// Malformed NDJSON line from stdout.
    #[error("NDJSON parse error on line {line}: {message}")]
    NdjsonParse { line: usize, message: String },

    /// A single JSON object exceeded the buffer limit.
    #[error("NDJSON buffer overflow: line exceeds {size} bytes")]
    BufferOverflow { size: usize },

    /// No session mapping found for the given claw session key.
    #[error("no Claude Code session mapped for '{session_key}'")]
    SessionNotMapped { session_key: String },

    /// Process was killed via cancellation or timeout.
    #[error("Claude Code process killed (timeout: {timeout:?})")]
    ProcessKilled { timeout: Option<Duration> },

    /// Process I/O error (stdout/stderr read failure).
    #[error("process I/O error: {0}")]
    ProcessIo(#[from] std::io::Error),

    /// SQLite session store error.
    #[error("session store error: {0}")]
    StoreError(String),

    /// Underlying SQLite error.
    #[error("database error: {0}")]
    DbError(#[from] rusqlite::Error),

    /// JSON serialization/deserialization error.
    #[error("JSON error: {0}")]
    JsonError(#[from] serde_json::Error),

    /// An unexpected internal error.
    #[error("internal error: {0}")]
    Internal(String),
}

/// Convenience alias for Claude Code results.
pub type ClaudeCodeResult<T> = Result<T, ClaudeCodeError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_display_messages() {
        let cli = ClaudeCodeError::CliNotFound {
            path: "claude".into(),
        };
        assert!(cli.to_string().contains("not found"));

        let spawn = ClaudeCodeError::SpawnFailed {
            reason: "permission denied".into(),
        };
        assert!(spawn.to_string().contains("permission denied"));

        let exit = ClaudeCodeError::NonZeroExit {
            code: 1,
            stderr: "fatal error".into(),
        };
        assert!(exit.to_string().contains("code 1"));

        let ndjson = ClaudeCodeError::NdjsonParse {
            line: 42,
            message: "unexpected EOF".into(),
        };
        assert!(ndjson.to_string().contains("line 42"));

        let overflow = ClaudeCodeError::BufferOverflow { size: 1_048_576 };
        assert!(overflow.to_string().contains("1048576"));

        let session = ClaudeCodeError::SessionNotMapped {
            session_key: "sk1".into(),
        };
        assert!(session.to_string().contains("sk1"));

        let killed = ClaudeCodeError::ProcessKilled {
            timeout: Some(Duration::from_secs(5)),
        };
        assert!(killed.to_string().contains("killed"));

        let store = ClaudeCodeError::StoreError("lock timeout".into());
        assert!(store.to_string().contains("lock timeout"));

        let internal = ClaudeCodeError::Internal("boom".into());
        assert!(internal.to_string().contains("boom"));
    }

    #[test]
    fn error_is_send_and_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<ClaudeCodeError>();
    }
}
