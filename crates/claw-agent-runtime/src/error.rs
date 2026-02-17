//! Error types for the agent runtime.

/// Errors that can occur during agent runtime operations.
#[derive(thiserror::Error, Debug)]
pub enum RuntimeError {
    /// The session already has an active run in progress.
    #[error("session '{session_key}' is busy with an active run")]
    SessionBusy { session_key: String },

    /// No active run was found for the given session key.
    #[error("session '{session_key}' not found")]
    SessionNotFound { session_key: String },

    /// The run was cancelled via its cancellation token.
    #[error("run cancelled for session '{session_key}'")]
    Cancelled { session_key: String },

    /// Timed out waiting to acquire a lock.
    #[error("lock timeout: {0}")]
    LockTimeout(String),

    /// An unexpected internal error.
    #[error("internal error: {0}")]
    InternalError(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_display_messages() {
        let busy = RuntimeError::SessionBusy {
            session_key: "sk1".to_owned(),
        };
        assert_eq!(busy.to_string(), "session 'sk1' is busy with an active run");

        let not_found = RuntimeError::SessionNotFound {
            session_key: "sk2".to_owned(),
        };
        assert!(not_found.to_string().contains("not found"));

        let cancelled = RuntimeError::Cancelled {
            session_key: "sk3".to_owned(),
        };
        assert!(cancelled.to_string().contains("cancelled"));

        let timeout = RuntimeError::LockTimeout("mutex".to_owned());
        assert!(timeout.to_string().contains("lock timeout"));

        let internal = RuntimeError::InternalError("boom".to_owned());
        assert!(internal.to_string().contains("boom"));
    }

    #[test]
    fn error_is_send_and_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<RuntimeError>();
    }
}
