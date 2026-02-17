//! Model provider abstraction layer for the claw agent runtime.
//!
//! Defines the [`ModelProvider`] trait that all LLM providers implement,
//! along with the message types ([`ChatMessage`], [`ChatRequest`], etc.)
//! used across the entire agent execution pipeline.

pub mod error;
pub mod provider;
pub mod types;

pub use error::ModelError;
pub use provider::ModelProvider;
pub use types::*;
