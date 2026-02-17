//! Message dispatch pipeline for claw-rust.
//!
//! Implements the inbound message dispatch pipeline, outbound delivery,
//! and the lane-based command queue for concurrency control.

pub mod bridge;
pub mod command_queue;
pub mod dispatch;

pub use bridge::{ChannelReplyBridge, run_dispatch_loop};
// Re-export ClaudeCodeDispatchContext so callers can pass it to run_dispatch_loop.
pub use claw_claude_code::ClaudeCodeDispatchContext;
pub use command_queue::{CommandQueue, MAIN_LANE};
pub use dispatch::{
    AgentDispatchContext, BufferedReplyDispatcher, DetectedCommand, DispatchInboundResult,
    GetReplyOptions, ReplyDispatcher, detect_command, dispatch_inbound_message,
    dispatch_with_agent,
};
