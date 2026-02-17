//! Agent runtime crate for claw-rust.
//!
//! Provides the core execution engine that manages agent run lifecycles —
//! one active run per session key at a time, with message queueing for
//! requests that arrive while a run is in progress.

pub mod error;
pub mod queue;
pub mod runner;

pub use error::RuntimeError;
pub use queue::MessageQueue;
pub use runner::{AgentRunner, QueuedMessage, RunContext};
