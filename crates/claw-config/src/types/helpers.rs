//! Shared helper types used across config sections.

use serde::{Deserialize, Serialize};

/// Simple toggle config used in several places (enabled: bool).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ToggleConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
}
