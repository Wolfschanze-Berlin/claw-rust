//! Model catalog — maps model IDs to provider configurations.
//!
//! The catalog holds all known models and resolves which model to use
//! through a 4-level selection hierarchy:
//!
//! 1. **Session override** — a model pinned to the current session.
//! 2. **Agent configuration** — the model from the agent's workspace settings.
//! 3. **Channel default** — a default model assigned to the channel.
//! 4. **Global default** — the system-wide fallback model.
//!
//! The first level that specifies a model ID present in the catalog wins.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use tracing::warn;

// ---------------------------------------------------------------------------
// Input modalities
// ---------------------------------------------------------------------------

/// Input modalities a model supports beyond plain text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum InputModality {
    Text,
    Vision,
    Audio,
}

// ---------------------------------------------------------------------------
// ModelEntry
// ---------------------------------------------------------------------------

/// A single model entry in the catalog.
///
/// Describes the model's identity, capabilities, and which provider serves it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelEntry {
    /// Unique model identifier (e.g. "claude-sonnet-4-20250514").
    pub id: String,

    /// Human-readable display name (e.g. "Claude Sonnet 4").
    pub name: String,

    /// Provider that serves this model (e.g. "anthropic", "openai").
    pub provider: String,

    /// Maximum context window size in tokens.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context_window: Option<u64>,

    /// Maximum output tokens the model can generate.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u64>,

    /// Input modalities supported by this model.
    #[serde(default = "default_modalities")]
    pub input_modalities: Vec<InputModality>,

    /// Whether the model supports extended reasoning/thinking.
    #[serde(default)]
    pub supports_reasoning: bool,

    /// Whether the model supports tool/function calling.
    #[serde(default)]
    pub supports_tools: bool,
}

fn default_modalities() -> Vec<InputModality> {
    vec![InputModality::Text]
}

impl ModelEntry {
    /// Whether this model supports vision input.
    pub fn supports_vision(&self) -> bool {
        self.input_modalities.contains(&InputModality::Vision)
    }
}

// ---------------------------------------------------------------------------
// SelectionContext
// ---------------------------------------------------------------------------

/// Context for the 4-level model selection hierarchy.
///
/// Each field is an optional model ID. The resolver checks them in priority
/// order and returns the first one that exists in the catalog.
#[derive(Debug, Clone, Default)]
pub struct SelectionContext {
    /// Level 1: model pinned to the current session.
    pub session_override: Option<String>,

    /// Level 2: model configured in the agent's workspace.
    pub agent_model: Option<String>,

    /// Level 3: default model for the channel.
    pub channel_default: Option<String>,

    /// Level 4: system-wide global default (overrides catalog's own default).
    pub global_default: Option<String>,
}

// ---------------------------------------------------------------------------
// ModelCatalog
// ---------------------------------------------------------------------------

/// Catalog of all known models, with a 4-level selection hierarchy.
///
/// # Example
///
/// ```
/// use claw_agent_models::catalog::{ModelCatalog, ModelEntry, SelectionContext};
///
/// let mut catalog = ModelCatalog::new();
/// catalog.register(ModelEntry {
///     id: "claude-sonnet-4-20250514".into(),
///     name: "Claude Sonnet 4".into(),
///     provider: "anthropic".into(),
///     context_window: Some(200_000),
///     max_tokens: Some(8_192),
///     input_modalities: vec![],
///     supports_reasoning: false,
///     supports_tools: true,
/// });
/// catalog.set_global_default("claude-sonnet-4-20250514");
///
/// let ctx = SelectionContext::default();
/// let model = catalog.resolve(&ctx).unwrap();
/// assert_eq!(model.id, "claude-sonnet-4-20250514");
/// ```
#[derive(Debug, Clone)]
pub struct ModelCatalog {
    entries: HashMap<String, ModelEntry>,
    global_default: Option<String>,
}

impl Default for ModelCatalog {
    fn default() -> Self {
        Self::new()
    }
}

impl ModelCatalog {
    /// Create an empty catalog.
    pub fn new() -> Self {
        Self {
            entries: HashMap::new(),
            global_default: None,
        }
    }

    /// Register a model entry. Overwrites any existing entry with the same ID.
    pub fn register(&mut self, entry: ModelEntry) {
        self.entries.insert(entry.id.clone(), entry);
    }

    /// Look up a model by its exact ID.
    pub fn get(&self, model_id: &str) -> Option<&ModelEntry> {
        self.entries.get(model_id)
    }

    /// Set the catalog-level global default model.
    ///
    /// The model must already be registered. Logs a warning and does nothing
    /// if the ID is unknown.
    pub fn set_global_default(&mut self, model_id: &str) {
        if self.entries.contains_key(model_id) {
            self.global_default = Some(model_id.to_owned());
        } else {
            warn!(
                model_id,
                "attempted to set global default to unknown model"
            );
        }
    }

    /// Return the catalog-level global default model ID, if set.
    pub fn global_default(&self) -> Option<&str> {
        self.global_default.as_deref()
    }

    /// Resolve a model through the 4-level selection hierarchy.
    ///
    /// Checks in order:
    /// 1. `ctx.session_override`
    /// 2. `ctx.agent_model`
    /// 3. `ctx.channel_default`
    /// 4. `ctx.global_default`
    /// 5. Catalog-level global default (set via [`set_global_default`])
    ///
    /// Returns `None` only if no level specifies a model that exists
    /// in the catalog.
    pub fn resolve(&self, ctx: &SelectionContext) -> Option<&ModelEntry> {
        let levels = [
            ctx.session_override.as_deref(),
            ctx.agent_model.as_deref(),
            ctx.channel_default.as_deref(),
            ctx.global_default.as_deref(),
            self.global_default.as_deref(),
        ];

        for candidate in levels.into_iter().flatten() {
            if let Some(entry) = self.entries.get(candidate) {
                return Some(entry);
            }
        }

        None
    }

    /// List all registered model entries.
    pub fn list(&self) -> Vec<&ModelEntry> {
        self.entries.values().collect()
    }

    /// Number of models in the catalog.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the catalog is empty.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Remove a model from the catalog. Returns the removed entry if found.
    pub fn remove(&mut self, model_id: &str) -> Option<ModelEntry> {
        let removed = self.entries.remove(model_id);
        // Clear global default if it pointed to the removed model.
        if self.global_default.as_deref() == Some(model_id) {
            self.global_default = None;
        }
        removed
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_entry(id: &str, provider: &str) -> ModelEntry {
        ModelEntry {
            id: id.into(),
            name: format!("Test Model {id}"),
            provider: provider.into(),
            context_window: Some(128_000),
            max_tokens: Some(4_096),
            input_modalities: vec![InputModality::Text],
            supports_reasoning: false,
            supports_tools: true,
        }
    }

    // -- Registration & lookup ------------------------------------------------

    #[test]
    fn register_and_get() {
        let mut catalog = ModelCatalog::new();
        catalog.register(sample_entry("model-a", "anthropic"));

        let entry = catalog.get("model-a").unwrap();
        assert_eq!(entry.id, "model-a");
        assert_eq!(entry.provider, "anthropic");
    }

    #[test]
    fn get_unknown_returns_none() {
        let catalog = ModelCatalog::new();
        assert!(catalog.get("nonexistent").is_none());
    }

    #[test]
    fn register_overwrites_existing() {
        let mut catalog = ModelCatalog::new();
        catalog.register(sample_entry("model-a", "anthropic"));
        catalog.register(sample_entry("model-a", "openai"));

        assert_eq!(catalog.get("model-a").unwrap().provider, "openai");
        assert_eq!(catalog.len(), 1);
    }

    #[test]
    fn list_returns_all_entries() {
        let mut catalog = ModelCatalog::new();
        catalog.register(sample_entry("model-a", "anthropic"));
        catalog.register(sample_entry("model-b", "openai"));

        let models = catalog.list();
        assert_eq!(models.len(), 2);
    }

    #[test]
    fn len_and_is_empty() {
        let mut catalog = ModelCatalog::new();
        assert!(catalog.is_empty());
        assert_eq!(catalog.len(), 0);

        catalog.register(sample_entry("model-a", "anthropic"));
        assert!(!catalog.is_empty());
        assert_eq!(catalog.len(), 1);
    }

    #[test]
    fn remove_existing_model() {
        let mut catalog = ModelCatalog::new();
        catalog.register(sample_entry("model-a", "anthropic"));
        catalog.set_global_default("model-a");

        let removed = catalog.remove("model-a");
        assert!(removed.is_some());
        assert!(catalog.get("model-a").is_none());
        assert!(catalog.global_default().is_none());
    }

    #[test]
    fn remove_nonexistent_returns_none() {
        let mut catalog = ModelCatalog::new();
        assert!(catalog.remove("ghost").is_none());
    }

    // -- Global default -------------------------------------------------------

    #[test]
    fn set_global_default_valid() {
        let mut catalog = ModelCatalog::new();
        catalog.register(sample_entry("model-a", "anthropic"));
        catalog.set_global_default("model-a");

        assert_eq!(catalog.global_default(), Some("model-a"));
    }

    #[test]
    fn set_global_default_unknown_is_noop() {
        let mut catalog = ModelCatalog::new();
        catalog.set_global_default("nonexistent");

        assert!(catalog.global_default().is_none());
    }

    // -- 4-level selection hierarchy ------------------------------------------

    #[test]
    fn resolve_empty_catalog_returns_none() {
        let catalog = ModelCatalog::new();
        let ctx = SelectionContext::default();
        assert!(catalog.resolve(&ctx).is_none());
    }

    #[test]
    fn resolve_empty_context_uses_catalog_global_default() {
        let mut catalog = ModelCatalog::new();
        catalog.register(sample_entry("fallback", "anthropic"));
        catalog.set_global_default("fallback");

        let ctx = SelectionContext::default();
        let model = catalog.resolve(&ctx).unwrap();
        assert_eq!(model.id, "fallback");
    }

    #[test]
    fn resolve_session_override_wins() {
        let mut catalog = ModelCatalog::new();
        catalog.register(sample_entry("session-model", "anthropic"));
        catalog.register(sample_entry("agent-model", "openai"));
        catalog.register(sample_entry("channel-model", "anthropic"));
        catalog.register(sample_entry("global-model", "openai"));
        catalog.set_global_default("global-model");

        let ctx = SelectionContext {
            session_override: Some("session-model".into()),
            agent_model: Some("agent-model".into()),
            channel_default: Some("channel-model".into()),
            global_default: Some("global-model".into()),
        };
        assert_eq!(catalog.resolve(&ctx).unwrap().id, "session-model");
    }

    #[test]
    fn resolve_agent_model_when_no_session() {
        let mut catalog = ModelCatalog::new();
        catalog.register(sample_entry("agent-model", "openai"));
        catalog.register(sample_entry("channel-model", "anthropic"));

        let ctx = SelectionContext {
            session_override: None,
            agent_model: Some("agent-model".into()),
            channel_default: Some("channel-model".into()),
            global_default: None,
        };
        assert_eq!(catalog.resolve(&ctx).unwrap().id, "agent-model");
    }

    #[test]
    fn resolve_channel_default_when_no_session_or_agent() {
        let mut catalog = ModelCatalog::new();
        catalog.register(sample_entry("channel-model", "anthropic"));

        let ctx = SelectionContext {
            session_override: None,
            agent_model: None,
            channel_default: Some("channel-model".into()),
            global_default: None,
        };
        assert_eq!(catalog.resolve(&ctx).unwrap().id, "channel-model");
    }

    #[test]
    fn resolve_context_global_default_before_catalog_default() {
        let mut catalog = ModelCatalog::new();
        catalog.register(sample_entry("ctx-global", "openai"));
        catalog.register(sample_entry("catalog-global", "anthropic"));
        catalog.set_global_default("catalog-global");

        let ctx = SelectionContext {
            session_override: None,
            agent_model: None,
            channel_default: None,
            global_default: Some("ctx-global".into()),
        };
        assert_eq!(catalog.resolve(&ctx).unwrap().id, "ctx-global");
    }

    #[test]
    fn resolve_skips_unknown_model_ids() {
        let mut catalog = ModelCatalog::new();
        catalog.register(sample_entry("channel-model", "anthropic"));

        let ctx = SelectionContext {
            session_override: Some("ghost-session".into()),
            agent_model: Some("ghost-agent".into()),
            channel_default: Some("channel-model".into()),
            global_default: None,
        };
        // Session and agent models don't exist, falls through to channel.
        assert_eq!(catalog.resolve(&ctx).unwrap().id, "channel-model");
    }

    #[test]
    fn resolve_all_unknown_returns_none() {
        let mut catalog = ModelCatalog::new();
        catalog.register(sample_entry("real-model", "anthropic"));

        let ctx = SelectionContext {
            session_override: Some("ghost-1".into()),
            agent_model: Some("ghost-2".into()),
            channel_default: Some("ghost-3".into()),
            global_default: Some("ghost-4".into()),
        };
        assert!(catalog.resolve(&ctx).is_none());
    }

    // -- ModelEntry helpers ---------------------------------------------------

    #[test]
    fn supports_vision_flag() {
        let mut entry = sample_entry("vision-model", "openai");
        assert!(!entry.supports_vision());

        entry.input_modalities.push(InputModality::Vision);
        assert!(entry.supports_vision());
    }

    // -- Serde roundtrip ------------------------------------------------------

    #[test]
    fn model_entry_serde_roundtrip() {
        let entry = ModelEntry {
            id: "claude-sonnet-4-20250514".into(),
            name: "Claude Sonnet 4".into(),
            provider: "anthropic".into(),
            context_window: Some(200_000),
            max_tokens: Some(8_192),
            input_modalities: vec![InputModality::Text, InputModality::Vision],
            supports_reasoning: true,
            supports_tools: true,
        };
        let json = serde_json::to_string(&entry).unwrap();
        let parsed: ModelEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.id, entry.id);
        assert_eq!(parsed.provider, entry.provider);
        assert_eq!(parsed.context_window, entry.context_window);
        assert!(parsed.supports_reasoning);
        assert!(parsed.supports_vision());
    }

    #[test]
    fn model_entry_serde_defaults() {
        let json = r#"{
            "id": "test",
            "name": "Test",
            "provider": "test-provider"
        }"#;
        let entry: ModelEntry = serde_json::from_str(json).unwrap();
        assert!(!entry.supports_reasoning);
        assert!(!entry.supports_tools);
        assert_eq!(entry.input_modalities, vec![InputModality::Text]);
        assert!(entry.context_window.is_none());
        assert!(entry.max_tokens.is_none());
    }

    #[test]
    fn model_entry_omits_none_fields() {
        let entry = ModelEntry {
            id: "test".into(),
            name: "Test".into(),
            provider: "test".into(),
            context_window: None,
            max_tokens: None,
            input_modalities: vec![InputModality::Text],
            supports_reasoning: false,
            supports_tools: false,
        };
        let json = serde_json::to_value(&entry).unwrap();
        let obj = json.as_object().unwrap();
        assert!(!obj.contains_key("context_window"));
        assert!(!obj.contains_key("max_tokens"));
    }

    #[test]
    fn input_modality_serde() {
        assert_eq!(
            serde_json::to_string(&InputModality::Vision).unwrap(),
            r#""vision""#
        );
        let parsed: InputModality = serde_json::from_str(r#""audio""#).unwrap();
        assert_eq!(parsed, InputModality::Audio);
    }

    // -- Default trait --------------------------------------------------------

    #[test]
    fn catalog_default_is_empty() {
        let catalog = ModelCatalog::default();
        assert!(catalog.is_empty());
        assert!(catalog.global_default().is_none());
    }
}
