//! Plugin system for claw-rust.
//!
//! Provides plugin registration, lifecycle management, hook pipeline,
//! and the PluginApi interface for extending the gateway.

pub mod hooks;
pub mod registry;
pub mod runtime;
pub mod types;

pub use hooks::{HookAbortError, HookEvent, HookPipeline, HookPipelineResult};
pub use registry::{PluginRegistry, PluginState};
pub use runtime::{PluginLogger, PluginRuntime};
pub use types::{
    CommandHandler, HookHandler, HookResult, HttpRouteHandler, MethodHandler, PluginApi,
    PluginDefinition, PluginKind, Service, ToolHandler,
};
