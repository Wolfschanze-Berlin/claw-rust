//! Message dispatch pipeline for claw-rust.
//!
//! Implements the inbound message dispatch pipeline, outbound delivery,
//! and the lane-based command queue for concurrency control.

pub mod command_queue;
pub mod dispatch;

pub use command_queue::{CommandQueue, MAIN_LANE};
pub use dispatch::{
    BufferedReplyDispatcher, DetectedCommand, DispatchInboundResult, GetReplyOptions,
    ReplyDispatcher, detect_command, dispatch_inbound_message,
};
