//! Gateway server for claw-rust.
//!
//! HTTP + WebSocket server using actix-web, implementing the OpenClaw
//! JSON-RPC protocol with handshake, RPC dispatch, and event broadcasting.

pub mod channel_manager;
pub mod dispatch;
pub mod events;
pub mod handshake;
pub mod protocol;
pub mod server;
