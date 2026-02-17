//! Plugin and skills configuration types.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Plugin configuration with entries.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct PluginsConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entries: Option<HashMap<String, PluginEntry>>,
}

/// A single plugin entry.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct PluginEntry {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,

    /// Catch-all for plugin-specific fields.
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

/// Skills loading, installation, and entries configuration.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct SkillsConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub load: Option<SkillsLoadConfig>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub install: Option<SkillsInstallConfig>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub entries: Option<HashMap<String, SkillEntry>>,
}

/// Skills file-watching configuration.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct SkillsLoadConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub watch: Option<bool>,
}

/// Skills installation preferences.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct SkillsInstallConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prefer_brew: Option<bool>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub node_manager: Option<String>,
}

/// A single skill entry.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct SkillEntry {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,

    /// Catch-all for skill-specific fields.
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}
