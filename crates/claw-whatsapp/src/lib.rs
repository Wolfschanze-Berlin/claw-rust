//! WhatsApp channel plugin for claw-rust.
//!
//! Implements the ChannelPlugin trait for WhatsApp via session-based
//! authentication (QR code / pairing code). Uses SQLite for session
//! persistence and supports DM, group, and media messaging.

pub mod adapters;
pub mod gateway;
pub mod normalize;
pub mod outbound;
pub mod plugin;

pub use plugin::WhatsAppPlugin;
