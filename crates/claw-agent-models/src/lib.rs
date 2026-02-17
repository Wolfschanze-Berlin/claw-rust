//! Model provider abstraction layer for the claw agent runtime.
//!
//! Defines the [`ModelProvider`] trait that all LLM providers implement,
//! along with the message types ([`ChatMessage`], [`ChatRequest`], etc.)
//! used across the entire agent execution pipeline.

pub mod anthropic;
pub(crate) mod anthropic_sse;
pub mod catalog;
pub mod error;
pub mod failover;
pub mod provider;
pub mod types;

pub use catalog::{ModelCatalog, ModelEntry, SelectionContext};
pub use error::ModelError;
pub use failover::{FailoverChain, FailoverConfig};
pub use provider::ModelProvider;
pub use types::*;
