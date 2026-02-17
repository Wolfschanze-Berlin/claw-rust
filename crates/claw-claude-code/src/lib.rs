//! Claude Code CLI subprocess runtime for claw-rust.
//!
//! Delegates agentic work to a local Claude Code CLI instance via its SDK
//! subprocess protocol (NDJSON streaming over stdout). Provides session
//! management persisted in SQLite and integration with the dispatch pipeline.

pub mod config;
pub mod dispatch;
pub mod error;
pub mod ndjson;
pub mod process;
pub mod session;
pub mod store;
pub mod types;

pub use config::ClaudeCodeConfig;
pub use dispatch::{ClaudeCodeDispatchContext, ClaudeCodeRunResult};
pub use error::{ClaudeCodeError, ClaudeCodeResult};
pub use ndjson::NdjsonParser;
pub use process::ClaudeCodeProcess;
pub use session::{InMemorySessionStore, SessionManager, SessionMapping, SessionStore};
pub use store::SqliteSessionStore;
pub use types::{AssistantMessage, ClaudeMessage, ContentBlock, ResultMessage, SystemMessage};
