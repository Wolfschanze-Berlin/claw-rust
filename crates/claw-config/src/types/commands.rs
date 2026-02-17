//! Command configuration types.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Native command toggles.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct CommandsConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub native: Option<bool>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub native_skills: Option<bool>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub bash: Option<bool>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub config: Option<bool>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub debug: Option<bool>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub restart: Option<bool>,

    // --- New fields from OpenClaw config reference ---

    /// Enable text-based command parsing.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<bool>,

    /// Foreground timeout for bash commands (ms).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bash_foreground_ms: Option<u32>,

    /// Restrict commands to specific user roles/groups.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allow_from: Option<Vec<String>>,

    /// Use access group system for command permissions.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub use_access_groups: Option<bool>,

    /// Catch-all for extension fields.
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}
