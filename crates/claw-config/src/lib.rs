//! Configuration loading, validation, and types for claw-rust.
//!
//! Handles JSON5 config file parsing, `$include` directive resolution,
//! `${ENV}` variable substitution, and schema validation.

pub mod loader;
pub mod types;
pub mod validation;

pub use loader::{
    ConfigError, ConfigFileSnapshot, default_config_path, load_config, read_config_file_snapshot,
    write_config_file,
};
pub use types::*;
pub use validation::{
    ConfigValidationIssue, IssueSeverity, ValidationResult, migrate_legacy_config, validate_config,
};
