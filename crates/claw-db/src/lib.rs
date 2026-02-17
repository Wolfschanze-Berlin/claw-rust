//! SQLite database layer for claw-rust.
//!
//! Manages session state, transcript events, message history,
//! and channel state using SQLite with WAL mode for concurrent access.

pub mod channel_state;
pub mod database;
pub mod error;
pub mod lock_error;
pub mod messages;
pub mod schema;
pub mod sessions;
pub mod transcripts;
pub mod write_lock;

pub use channel_state::ChannelStateRow;
pub use database::Database;
pub use error::{DbError, DbResult};
pub use lock_error::LockError;
pub use messages::MessageRow;
pub use sessions::SessionRow;
pub use transcripts::TranscriptEventRow;
pub use write_lock::{LockGuard, SessionWriteLock};
