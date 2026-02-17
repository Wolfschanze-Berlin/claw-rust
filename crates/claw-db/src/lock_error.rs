//! Error types for session write locks.

use std::path::PathBuf;

use thiserror::Error;

/// Errors that can occur during lock operations.
#[derive(Error, Debug)]
pub enum LockError {
    /// Timed out waiting to acquire the lock.
    #[error("lock timeout for session `{session_key}` after {elapsed_ms}ms")]
    Timeout { session_key: String, elapsed_ms: u64 },

    /// Filesystem I/O error.
    #[error("lock I/O error at `{path}`: {source}")]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },

    /// A stale lock was detected and broken. This is informational —
    /// the lock was successfully acquired after removing the stale file.
    #[error("broke stale lock for session `{session_key}` (age: {age_secs}s)")]
    StaleLockBroken { session_key: String, age_secs: u64 },
}

impl LockError {
    pub(crate) fn io(path: impl Into<PathBuf>, source: std::io::Error) -> Self {
        Self::Io {
            path: path.into(),
            source,
        }
    }
}
