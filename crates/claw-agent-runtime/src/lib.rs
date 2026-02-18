//! Agent runtime crate for claw-rust.
//!
//! Provides the core execution engine that manages agent run lifecycles —
//! one active run per session key at a time, with message queueing for
//! requests that arrive while a run is in progress.

pub mod compaction;
pub mod error;
pub mod prompt;
pub mod pruning;
pub mod queue;
pub mod repair;
pub mod runner;
pub mod subagent;
pub mod subscriber;

pub use compaction::{CompactionConfig, CompactionEngine, CompactionError, CompactionResult};
pub use pruning::{has_pending_tool_call, PruningConfig, PruningEngine, PruningResult};
pub use error::RuntimeError;
pub use prompt::{PromptBuilder, PromptContext};
pub use queue::MessageQueue;
pub use repair::{repair_transcript, RepairResult};
pub use runner::{AgentRunner, ContextConfig, QueuedMessage, RunContext, RuntimeDeps, TranscriptStore, UserMessage};
pub use subagent::{
    AnnounceMessage, AnnounceReceiver, SpawnedSubagent, SpawnerConfig, SubagentEntry,
    SubagentRegistry, SubagentSpawner, SubagentState,
};
pub use subscriber::{
    DeliveryError, ResponseSink, StreamSubscriber, SubscribeResult, SubscriberConfig,
};
