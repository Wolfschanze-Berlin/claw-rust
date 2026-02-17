//! Database error types.

use thiserror::Error;

/// Errors that can occur during database operations.
#[derive(Error, Debug)]
pub enum DbError {
    #[error("SQLite error: {0}")]
    Sqlite(#[from] rusqlite::Error),

    #[error("migration failed: {0}")]
    Migration(String),

    #[error("serialization error: {0}")]
    Json(#[from] serde_json::Error),
}

/// Convenience alias for database results.
pub type DbResult<T> = Result<T, DbError>;
