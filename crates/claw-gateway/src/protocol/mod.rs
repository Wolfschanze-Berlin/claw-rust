//! Gateway protocol types for WebSocket JSON-RPC communication.
//!
//! This module contains all wire-format types used by the gateway:
//! - [`frames`] — Request, Response, and Event frame envelopes
//! - [`handshake`] — Connection negotiation types (ConnectParams, HelloOk, etc.)

pub mod frames;
pub mod handshake;

// Re-export primary types for convenience.
pub use frames::{EventFrame, GatewayFrame, RequestFrame, ResponseFrame};
pub use handshake::{ConnectParams, HelloOk, PresenceEntry, Snapshot, StateVersion};
