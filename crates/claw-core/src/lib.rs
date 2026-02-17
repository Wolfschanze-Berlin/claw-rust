//! Core types, error handling, and runtime abstractions for claw-rust.
//!
//! This crate provides the foundation layer shared by all other crates:
//! - Error types and utilities
//! - RuntimeEnv abstraction
//! - Common type definitions

pub mod backoff;
pub mod error;
pub mod runtime;

pub use backoff::{BackoffPolicy, compute_backoff, sleep_with_abort};
pub use error::{CommandLaneClearedError, ErrorCode, ErrorShape, error_shape, format_error_message};
pub use runtime::{RuntimeEnv, init_tracing, init_tracing_with_default};
