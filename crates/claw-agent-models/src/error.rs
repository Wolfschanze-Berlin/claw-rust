//! Error types for model provider operations.

use std::time::Duration;

use thiserror::Error;

/// Errors that can occur during model provider interactions.
#[derive(Error, Debug)]
pub enum ModelError {
    /// The provider returned an error.
    #[error("provider `{provider}` error: {message}")]
    ProviderError {
        provider: String,
        message: String,
        /// HTTP status code, if applicable.
        status_code: Option<u16>,
    },

    /// The provider rate-limited the request.
    #[error("rate limited by `{provider}`")]
    RateLimited {
        provider: String,
        retry_after: Option<Duration>,
    },

    /// Authentication with the provider failed.
    #[error("auth error for `{provider}`: {message}")]
    AuthError { provider: String, message: String },

    /// The request exceeded the model's context window.
    #[error("context length exceeded: {actual} tokens > {limit} limit")]
    ContextLengthExceeded { limit: u64, actual: u64 },

    /// The request timed out.
    #[error("request timed out after {elapsed:?}")]
    Timeout { elapsed: Duration },

    /// The request was cancelled via a cancellation token.
    #[error("request cancelled")]
    Cancelled,
}

impl ModelError {
    /// Whether this error is retriable (failover should try the next provider).
    ///
    /// Rate limits and server errors (5xx) are retriable.
    /// Auth errors, context length, and cancellation are not.
    pub fn is_retriable(&self) -> bool {
        match self {
            Self::RateLimited { .. } => true,
            Self::Timeout { .. } => true,
            Self::ProviderError { status_code, .. } => {
                status_code.map_or(false, |code| code >= 500)
            }
            Self::AuthError { .. }
            | Self::ContextLengthExceeded { .. }
            | Self::Cancelled => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rate_limited_is_retriable() {
        let err = ModelError::RateLimited {
            provider: "anthropic".into(),
            retry_after: Some(Duration::from_secs(5)),
        };
        assert!(err.is_retriable());
    }

    #[test]
    fn timeout_is_retriable() {
        let err = ModelError::Timeout {
            elapsed: Duration::from_secs(30),
        };
        assert!(err.is_retriable());
    }

    #[test]
    fn server_error_5xx_is_retriable() {
        let err = ModelError::ProviderError {
            provider: "anthropic".into(),
            message: "internal".into(),
            status_code: Some(500),
        };
        assert!(err.is_retriable());

        let err_503 = ModelError::ProviderError {
            provider: "anthropic".into(),
            message: "overloaded".into(),
            status_code: Some(503),
        };
        assert!(err_503.is_retriable());
    }

    #[test]
    fn client_error_4xx_not_retriable() {
        let err = ModelError::ProviderError {
            provider: "anthropic".into(),
            message: "bad request".into(),
            status_code: Some(400),
        };
        assert!(!err.is_retriable());
    }

    #[test]
    fn auth_error_not_retriable() {
        let err = ModelError::AuthError {
            provider: "openai".into(),
            message: "invalid key".into(),
        };
        assert!(!err.is_retriable());
    }

    #[test]
    fn context_exceeded_not_retriable() {
        let err = ModelError::ContextLengthExceeded {
            limit: 100_000,
            actual: 120_000,
        };
        assert!(!err.is_retriable());
    }

    #[test]
    fn cancelled_not_retriable() {
        assert!(!ModelError::Cancelled.is_retriable());
    }

    #[test]
    fn provider_error_no_status_not_retriable() {
        let err = ModelError::ProviderError {
            provider: "custom".into(),
            message: "unknown".into(),
            status_code: None,
        };
        assert!(!err.is_retriable());
    }

    #[test]
    fn error_display_messages() {
        let err = ModelError::RateLimited {
            provider: "anthropic".into(),
            retry_after: None,
        };
        assert!(err.to_string().contains("rate limited"));

        let err = ModelError::ContextLengthExceeded {
            limit: 100_000,
            actual: 120_000,
        };
        assert!(err.to_string().contains("120000"));
    }
}
