//! Workspace error types.

use std::path::PathBuf;

use thiserror::Error;

/// Errors that can occur during workspace operations.
#[derive(Error, Debug)]
pub enum WorkspaceError {
    /// Filesystem I/O error.
    #[error("workspace I/O error at `{path}`: {source}")]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },

    /// The workspace directory does not exist.
    #[error("workspace not found: `{0}`")]
    NotFound(PathBuf),
}

impl WorkspaceError {
    pub(crate) fn io(path: impl Into<PathBuf>, source: std::io::Error) -> Self {
        Self::Io {
            path: path.into(),
            source,
        }
    }
}
