//! Agent workspace management for claw-rust.
//!
//! Every agent operates within an isolated workspace directory containing
//! its identity, skills, memory, sessions, auth profiles, and tool policies.
//! This crate provides the [`WorkspaceDir`] path resolver and
//! [`AgentWorkspace`] file loader.

pub mod error;
pub mod workspace;
pub mod workspace_dir;

pub use error::WorkspaceError;
pub use workspace::AgentWorkspace;
pub use workspace_dir::WorkspaceDir;
