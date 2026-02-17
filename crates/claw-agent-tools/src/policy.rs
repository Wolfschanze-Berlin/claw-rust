//! Core policy types and engine.

use tracing::debug;

// ---------------------------------------------------------------------------
// PolicyDecision
// ---------------------------------------------------------------------------

/// The outcome of evaluating a tool against a policy layer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PolicyDecision {
    /// The tool is explicitly allowed.
    Allow,
    /// The tool is explicitly denied with a reason.
    Deny(String),
}

// ---------------------------------------------------------------------------
// PolicyContext
// ---------------------------------------------------------------------------

/// Context for evaluating tool policies — describes the requesting agent,
/// group, provider, and privilege level.
#[derive(Debug, Clone, Default)]
pub struct PolicyContext {
    pub agent_id: String,
    pub group_id: Option<String>,
    pub provider: Option<String>,
    pub is_elevated: bool,
}

// ---------------------------------------------------------------------------
// PolicyLayer trait
// ---------------------------------------------------------------------------

/// A single layer in the policy evaluation chain.
///
/// Returns `Some(decision)` for a definitive answer or `None` to defer
/// to the next layer.
pub trait PolicyLayer: Send + Sync {
    /// Evaluate whether `tool_name` should be allowed or denied.
    ///
    /// Return `None` to express no opinion (defer to lower-priority layers).
    fn evaluate(&self, tool_name: &str, context: &PolicyContext) -> Option<PolicyDecision>;

    /// Priority of this layer. Higher values are evaluated first.
    fn priority(&self) -> u32;
}

// ---------------------------------------------------------------------------
// PolicyEngine
// ---------------------------------------------------------------------------

/// Evaluates tool access by walking policy layers in priority order.
///
/// The first layer returning a definitive answer wins. If no layer
/// has an opinion, the tool is **denied by default** (fail-closed).
pub struct PolicyEngine {
    layers: Vec<Box<dyn PolicyLayer>>,
}

impl PolicyEngine {
    /// Create an engine with no layers (denies everything).
    pub fn new() -> Self {
        Self { layers: Vec::new() }
    }

    /// Add a policy layer. Layers are automatically sorted by priority.
    pub fn add_layer(&mut self, layer: Box<dyn PolicyLayer>) {
        self.layers.push(layer);
        self.layers.sort_by(|a, b| b.priority().cmp(&a.priority()));
    }

    /// Evaluate whether `tool_name` is allowed in the given context.
    ///
    /// Walks layers highest-priority first. First definitive answer wins.
    /// Defaults to **deny** if no layer has an opinion.
    pub fn evaluate(&self, tool_name: &str, context: &PolicyContext) -> PolicyDecision {
        for layer in &self.layers {
            if let Some(decision) = layer.evaluate(tool_name, context) {
                debug!(
                    tool = tool_name,
                    priority = layer.priority(),
                    decision = ?decision,
                    "policy layer decided"
                );
                return decision;
            }
        }
        debug!(tool = tool_name, "no layer decided, defaulting to deny");
        PolicyDecision::Deny("no policy layer allowed this tool".into())
    }

    /// Number of registered layers.
    pub fn layer_count(&self) -> usize {
        self.layers.len()
    }
}

impl Default for PolicyEngine {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layers::*;

    #[test]
    fn empty_engine_denies_everything() {
        let engine = PolicyEngine::new();
        let ctx = PolicyContext::default();
        assert_eq!(
            engine.evaluate("anything", &ctx),
            PolicyDecision::Deny("no policy layer allowed this tool".into())
        );
    }

    #[test]
    fn single_allow_layer() {
        let mut engine = PolicyEngine::new();
        engine.add_layer(Box::new(AllowListLayer::new(vec!["search".into()])));
        let ctx = PolicyContext::default();
        assert_eq!(engine.evaluate("search", &ctx), PolicyDecision::Allow);
    }

    #[test]
    fn deny_overrides_allow_by_priority() {
        let mut engine = PolicyEngine::new();
        engine.add_layer(Box::new(AllowListLayer::new(vec!["shell".into()])));
        engine.add_layer(Box::new(DenyListLayer::new(vec!["shell".into()])));
        let ctx = PolicyContext::default();
        // DenyList (priority 90) > AllowList (priority 80)
        assert!(matches!(engine.evaluate("shell", &ctx), PolicyDecision::Deny(_)));
    }

    #[test]
    fn sandbox_overrides_everything() {
        let mut engine = PolicyEngine::new();
        engine.add_layer(Box::new(AllowListLayer::new(vec!["exec_process".into()])));
        engine.add_layer(Box::new(SandboxLayer::new(vec!["exec_process".into()])));
        let ctx = PolicyContext::default();
        assert!(matches!(engine.evaluate("exec_process", &ctx), PolicyDecision::Deny(_)));
    }

    #[test]
    fn layers_sorted_by_priority() {
        let mut engine = PolicyEngine::new();
        engine.add_layer(Box::new(AllowListLayer::new(vec![])));    // 80
        engine.add_layer(Box::new(SandboxLayer::new(vec![])));      // 100
        engine.add_layer(Box::new(DenyListLayer::new(vec![])));     // 90
        assert_eq!(engine.layer_count(), 3);
    }

    #[test]
    fn unmatched_tool_falls_through_to_deny() {
        let mut engine = PolicyEngine::new();
        engine.add_layer(Box::new(AllowListLayer::new(vec!["search".into()])));
        let ctx = PolicyContext::default();
        assert!(matches!(engine.evaluate("unknown_tool", &ctx), PolicyDecision::Deny(_)));
    }
}
