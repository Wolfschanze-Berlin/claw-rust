//! Model provider and registry configuration types.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Model provider and registry configuration.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ModelsConfig {
    /// Default model identifier (e.g. "anthropic/claude-sonnet-4-20250514").
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default: Option<String>,

    /// Providers keyed by provider name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub providers: Option<HashMap<String, ModelProvider>>,

    /// Catch-all for extension fields.
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

/// A model provider (e.g. "anthropic", "openai").
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ModelProvider {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_url: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub auth: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub api: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub models: Option<Vec<ModelEntry>>,

    /// Catch-all for extension fields.
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

/// A single model definition within a provider.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ModelEntry {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub input: Option<Vec<String>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<bool>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub context_window: Option<u64>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u64>,

    /// Catch-all for extension fields.
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}
