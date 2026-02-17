//! Error types and formatting utilities for the claw protocol.
//!
//! Ports OpenClaw's `src/infra/errors.ts` to idiomatic Rust, providing:
//! - [`ErrorShape`] for protocol-level error responses
//! - [`ErrorCode`] enum for well-known error codes
//! - [`error_shape`] constructor for building error responses
//! - [`format_error_message`] with automatic token/secret redaction
//! - [`CommandLaneClearedError`] for command queue interruption

use regex::Regex;
use serde::{Deserialize, Serialize};
use std::sync::LazyLock;
use thiserror::Error;

// ---------------------------------------------------------------------------
// ErrorCode
// ---------------------------------------------------------------------------

/// Well-known error codes matching the OpenClaw protocol.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ErrorCode {
    NotLinked,
    NotPaired,
    AgentTimeout,
    InvalidRequest,
    Unavailable,
}

impl ErrorCode {
    /// Returns the string representation used in wire format.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::NotLinked => "NOT_LINKED",
            Self::NotPaired => "NOT_PAIRED",
            Self::AgentTimeout => "AGENT_TIMEOUT",
            Self::InvalidRequest => "INVALID_REQUEST",
            Self::Unavailable => "UNAVAILABLE",
        }
    }
}

impl std::fmt::Display for ErrorCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

// ---------------------------------------------------------------------------
// ErrorShape
// ---------------------------------------------------------------------------

/// Protocol-level error shape matching OpenClaw's `ErrorShape` interface.
///
/// Serializes to camelCase JSON to stay wire-compatible with the TypeScript
/// implementation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ErrorShape {
    pub code: String,
    pub message: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<serde_json::Value>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub retryable: Option<bool>,

    #[serde(rename = "retryAfterMs", skip_serializing_if = "Option::is_none")]
    pub retry_after_ms: Option<u64>,
}

/// Construct an [`ErrorShape`] from an [`ErrorCode`] and human-readable message.
///
/// Optional fields (`details`, `retryable`, `retry_after_ms`) default to `None`.
///
/// ```
/// use claw_core::error::{error_shape, ErrorCode};
///
/// let shape = error_shape(ErrorCode::Unavailable, "service down");
/// assert_eq!(shape.code, "UNAVAILABLE");
/// assert!(shape.retryable.is_none());
/// ```
pub fn error_shape(code: ErrorCode, message: impl Into<String>) -> ErrorShape {
    ErrorShape {
        code: code.to_string(),
        message: message.into(),
        details: None,
        retryable: None,
        retry_after_ms: None,
    }
}

impl ErrorShape {
    /// Builder helper -- mark this error as retryable with an optional delay.
    pub fn with_retry(mut self, retry_after_ms: Option<u64>) -> Self {
        self.retryable = Some(true);
        self.retry_after_ms = retry_after_ms;
        self
    }

    /// Builder helper -- attach arbitrary JSON details.
    pub fn with_details(mut self, details: serde_json::Value) -> Self {
        self.details = Some(details);
        self
    }
}

// ---------------------------------------------------------------------------
// CommandLaneClearedError
// ---------------------------------------------------------------------------

/// Raised when a command lane is cleared while a pending operation is waiting.
///
/// This mirrors OpenClaw's `CommandLaneClearedError` and is used to signal
/// that an in-flight command should be discarded because the lane was reset.
#[derive(Error, Debug, Clone)]
#[error("command lane cleared: {reason}")]
pub struct CommandLaneClearedError {
    pub reason: String,
}

impl CommandLaneClearedError {
    pub fn new(reason: impl Into<String>) -> Self {
        Self {
            reason: reason.into(),
        }
    }
}

// ---------------------------------------------------------------------------
// Error formatting with token redaction
// ---------------------------------------------------------------------------

/// Regex that matches common secret/token patterns so they can be redacted
/// before being written to logs or returned in error messages.
///
/// Patterns matched:
/// - Bearer tokens (`Bearer <token>`)
/// - API keys (`sk-...`, `pk-...`)
/// - Hex strings >= 32 chars (SHA hashes, etc.)
/// - Base64-ish strings >= 40 chars
static TOKEN_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?i)(?:bearer\s+)(\S+)|(?:(?:sk|pk|api[_-]?key)[_-]?\S{8,})|([a-f0-9]{32,})|([A-Za-z0-9+/=]{40,})",
    )
    .expect("token redaction regex must compile")
});

/// Format an error message, redacting anything that looks like a secret token.
///
/// ```
/// use claw_core::error::format_error_message;
///
/// let msg = format_error_message("auth failed with Bearer eyJhbGciOiJIUzI1NiJ9.test.signature");
/// assert!(!msg.contains("eyJhbGciOiJIUzI1NiJ9"));
/// assert!(msg.contains("[REDACTED]"));
/// ```
pub fn format_error_message(message: &str) -> String {
    TOKEN_RE.replace_all(message, "[REDACTED]").into_owned()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -- ErrorCode ---------------------------------------------------------

    #[test]
    fn error_code_display() {
        assert_eq!(ErrorCode::NotLinked.to_string(), "NOT_LINKED");
        assert_eq!(ErrorCode::NotPaired.to_string(), "NOT_PAIRED");
        assert_eq!(ErrorCode::AgentTimeout.to_string(), "AGENT_TIMEOUT");
        assert_eq!(ErrorCode::InvalidRequest.to_string(), "INVALID_REQUEST");
        assert_eq!(ErrorCode::Unavailable.to_string(), "UNAVAILABLE");
    }

    #[test]
    fn error_code_serde_roundtrip() {
        let code = ErrorCode::AgentTimeout;
        let json = serde_json::to_string(&code).unwrap();
        assert_eq!(json, r#""AGENT_TIMEOUT""#);

        let parsed: ErrorCode = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, code);
    }

    // -- ErrorShape --------------------------------------------------------

    #[test]
    fn error_shape_basic() {
        let shape = error_shape(ErrorCode::Unavailable, "service down");
        assert_eq!(shape.code, "UNAVAILABLE");
        assert_eq!(shape.message, "service down");
        assert!(shape.details.is_none());
        assert!(shape.retryable.is_none());
        assert!(shape.retry_after_ms.is_none());
    }

    #[test]
    fn error_shape_with_retry() {
        let shape = error_shape(ErrorCode::AgentTimeout, "timed out")
            .with_retry(Some(5000));
        assert_eq!(shape.retryable, Some(true));
        assert_eq!(shape.retry_after_ms, Some(5000));
    }

    #[test]
    fn error_shape_with_details() {
        let details = serde_json::json!({"field": "name"});
        let shape = error_shape(ErrorCode::InvalidRequest, "bad input")
            .with_details(details.clone());
        assert_eq!(shape.details, Some(details));
    }

    #[test]
    fn error_shape_json_omits_none_fields() {
        let shape = error_shape(ErrorCode::NotLinked, "not linked");
        let json = serde_json::to_value(&shape).unwrap();
        let obj = json.as_object().unwrap();
        assert!(!obj.contains_key("details"));
        assert!(!obj.contains_key("retryable"));
        assert!(!obj.contains_key("retryAfterMs"));
    }

    #[test]
    fn error_shape_json_includes_retry_after_ms_camel_case() {
        let shape = error_shape(ErrorCode::Unavailable, "retry later")
            .with_retry(Some(1000));
        let json = serde_json::to_value(&shape).unwrap();
        let obj = json.as_object().unwrap();
        assert!(obj.contains_key("retryAfterMs"));
        assert!(!obj.contains_key("retry_after_ms"));
    }

    #[test]
    fn error_shape_deserialize_camel_case() {
        let json = r#"{
            "code": "UNAVAILABLE",
            "message": "down",
            "retryAfterMs": 3000,
            "retryable": true
        }"#;
        let shape: ErrorShape = serde_json::from_str(json).unwrap();
        assert_eq!(shape.retry_after_ms, Some(3000));
        assert_eq!(shape.retryable, Some(true));
    }

    // -- CommandLaneClearedError -------------------------------------------

    #[test]
    fn command_lane_cleared_error_display() {
        let err = CommandLaneClearedError::new("user disconnected");
        assert_eq!(
            err.to_string(),
            "command lane cleared: user disconnected"
        );
    }

    #[test]
    fn command_lane_cleared_is_std_error() {
        let err = CommandLaneClearedError::new("test");
        let _: &dyn std::error::Error = &err;
    }

    // -- format_error_message ---------------------------------------------

    #[test]
    fn redacts_bearer_token() {
        let msg = format_error_message("failed with Bearer abc123secret456");
        assert!(msg.contains("[REDACTED]"));
        assert!(!msg.contains("abc123secret456"));
    }

    #[test]
    fn redacts_api_key_prefix() {
        let msg = format_error_message("key was sk-proj-abcdefghij");
        assert!(msg.contains("[REDACTED]"));
        assert!(!msg.contains("sk-proj-abcdefghij"));
    }

    #[test]
    fn redacts_long_hex_string() {
        let hex = "a".repeat(40);
        let input = format!("hash is {hex}");
        let msg = format_error_message(&input);
        assert!(msg.contains("[REDACTED]"));
        assert!(!msg.contains(&hex));
    }

    #[test]
    fn preserves_normal_text() {
        let input = "something went wrong in module foo";
        let msg = format_error_message(input);
        assert_eq!(msg, input);
    }

    #[test]
    fn redacts_long_base64_string() {
        let b64 = "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9abcdefgh";
        let input = format!("token: {b64}");
        let msg = format_error_message(&input);
        assert!(msg.contains("[REDACTED]"));
        assert!(!msg.contains(b64));
    }
}
