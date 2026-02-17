//! Channel system for claw-rust.
//!
//! Defines the ChannelPlugin trait, adapter traits, MsgContext types,
//! channel registry, and message dispatch infrastructure.

pub mod manager;
pub mod msg_context;
pub mod outbound;
pub mod plugin;
pub mod registry;
pub mod types;

pub use manager::*;
pub use msg_context::*;
pub use outbound::*;
pub use plugin::*;
pub use registry::*;
pub use types::*;
