//! Tool policy engine for the claw agent runtime.
//!
//! Implements a 6-layer policy evaluation system that controls which tools
//! an agent can use. Layers are evaluated in priority order (highest first),
//! and the first definitive answer (allow or deny) wins.
//!
//! ## Layer Precedence (highest → lowest)
//!
//! 1. **Sandbox** (100) — hard limits, non-overridable
//! 2. **Global deny** (90) — system-wide blocked tools
//! 3. **Global allow** (80) — system-wide permitted tools
//! 4. **Group** (70) — per-group (e.g. Discord server) policies
//! 5. **Agent override** (60) — per-agent restrictions/expansions
//! 6. **Provider** (50) — provider-specific limitations

pub mod layers;
pub mod pipeline;
pub mod policy;
pub mod profiles;

pub use layers::*;
pub use pipeline::{
    HookDecision, PipelineConfig, Tool, ToolExecutionError, ToolHook, ToolPipeline, ToolRegistry,
};
pub use policy::{PolicyContext, PolicyDecision, PolicyEngine, PolicyLayer};
pub use profiles::ToolProfile;
