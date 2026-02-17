//! Built-in policy layers implementing the 6-layer precedence system.

use std::collections::{HashMap, HashSet};

use crate::policy::{PolicyContext, PolicyDecision, PolicyLayer};

// ---------------------------------------------------------------------------
// Layer 1: Sandbox (priority 100)
// ---------------------------------------------------------------------------

/// Hard sandbox restrictions — non-overridable.
///
/// Blocks tools based on a set of forbidden tool names. These represent
/// system-level safety constraints (filesystem, network, process spawning).
pub struct SandboxLayer {
    blocked: HashSet<String>,
}

impl SandboxLayer {
    pub fn new(blocked_tools: Vec<String>) -> Self {
        Self {
            blocked: blocked_tools.into_iter().collect(),
        }
    }
}

impl PolicyLayer for SandboxLayer {
    fn evaluate(&self, tool_name: &str, _context: &PolicyContext) -> Option<PolicyDecision> {
        if self.blocked.contains(tool_name) {
            Some(PolicyDecision::Deny(format!(
                "sandbox restriction: `{tool_name}` is blocked"
            )))
        } else {
            None
        }
    }

    fn priority(&self) -> u32 {
        100
    }
}

// ---------------------------------------------------------------------------
// Layer 2: Global Deny List (priority 90)
// ---------------------------------------------------------------------------

/// Tools blocked system-wide regardless of agent or group.
pub struct DenyListLayer {
    denied: HashSet<String>,
}

impl DenyListLayer {
    pub fn new(denied_tools: Vec<String>) -> Self {
        Self {
            denied: denied_tools.into_iter().collect(),
        }
    }
}

impl PolicyLayer for DenyListLayer {
    fn evaluate(&self, tool_name: &str, _context: &PolicyContext) -> Option<PolicyDecision> {
        if self.denied.contains(tool_name) {
            Some(PolicyDecision::Deny(format!(
                "global deny list: `{tool_name}` is blocked"
            )))
        } else {
            None
        }
    }

    fn priority(&self) -> u32 {
        90
    }
}

// ---------------------------------------------------------------------------
// Layer 3: Global Allow List (priority 80)
// ---------------------------------------------------------------------------

/// Tools permitted system-wide as a baseline.
pub struct AllowListLayer {
    allowed: HashSet<String>,
}

impl AllowListLayer {
    pub fn new(allowed_tools: Vec<String>) -> Self {
        Self {
            allowed: allowed_tools.into_iter().collect(),
        }
    }
}

impl PolicyLayer for AllowListLayer {
    fn evaluate(&self, tool_name: &str, _context: &PolicyContext) -> Option<PolicyDecision> {
        if self.allowed.contains(tool_name) {
            Some(PolicyDecision::Allow)
        } else {
            None
        }
    }

    fn priority(&self) -> u32 {
        80
    }
}

// ---------------------------------------------------------------------------
// Layer 4: Group Policies (priority 70)
// ---------------------------------------------------------------------------

/// Per-group tool policies (e.g. per Discord server).
///
/// Each group can have its own allow/deny sets.
pub struct GroupPolicyLayer {
    /// Group ID → allowed tools.
    allow: HashMap<String, HashSet<String>>,
    /// Group ID → denied tools.
    deny: HashMap<String, HashSet<String>>,
}

impl GroupPolicyLayer {
    pub fn new() -> Self {
        Self {
            allow: HashMap::new(),
            deny: HashMap::new(),
        }
    }

    pub fn allow_for_group(&mut self, group_id: &str, tools: Vec<String>) {
        self.allow
            .entry(group_id.to_owned())
            .or_default()
            .extend(tools);
    }

    pub fn deny_for_group(&mut self, group_id: &str, tools: Vec<String>) {
        self.deny
            .entry(group_id.to_owned())
            .or_default()
            .extend(tools);
    }
}

impl Default for GroupPolicyLayer {
    fn default() -> Self {
        Self::new()
    }
}

impl PolicyLayer for GroupPolicyLayer {
    fn evaluate(&self, tool_name: &str, context: &PolicyContext) -> Option<PolicyDecision> {
        let group_id = context.group_id.as_deref()?;

        // Deny takes precedence within the group layer.
        if let Some(denied) = self.deny.get(group_id) {
            if denied.contains(tool_name) {
                return Some(PolicyDecision::Deny(format!(
                    "group `{group_id}` denies `{tool_name}`"
                )));
            }
        }

        if let Some(allowed) = self.allow.get(group_id) {
            if allowed.contains(tool_name) {
                return Some(PolicyDecision::Allow);
            }
        }

        None
    }

    fn priority(&self) -> u32 {
        70
    }
}

// ---------------------------------------------------------------------------
// Layer 5: Agent Overrides (priority 60)
// ---------------------------------------------------------------------------

/// Per-agent tool policy overrides.
pub struct AgentOverrideLayer {
    /// Agent ID → allowed tools.
    allow: HashMap<String, HashSet<String>>,
    /// Agent ID → denied tools.
    deny: HashMap<String, HashSet<String>>,
}

impl AgentOverrideLayer {
    pub fn new() -> Self {
        Self {
            allow: HashMap::new(),
            deny: HashMap::new(),
        }
    }

    pub fn allow_for_agent(&mut self, agent_id: &str, tools: Vec<String>) {
        self.allow
            .entry(agent_id.to_owned())
            .or_default()
            .extend(tools);
    }

    pub fn deny_for_agent(&mut self, agent_id: &str, tools: Vec<String>) {
        self.deny
            .entry(agent_id.to_owned())
            .or_default()
            .extend(tools);
    }
}

impl Default for AgentOverrideLayer {
    fn default() -> Self {
        Self::new()
    }
}

impl PolicyLayer for AgentOverrideLayer {
    fn evaluate(&self, tool_name: &str, context: &PolicyContext) -> Option<PolicyDecision> {
        if let Some(denied) = self.deny.get(&context.agent_id) {
            if denied.contains(tool_name) {
                return Some(PolicyDecision::Deny(format!(
                    "agent `{}` override denies `{tool_name}`",
                    context.agent_id
                )));
            }
        }

        if let Some(allowed) = self.allow.get(&context.agent_id) {
            if allowed.contains(tool_name) {
                return Some(PolicyDecision::Allow);
            }
        }

        None
    }

    fn priority(&self) -> u32 {
        60
    }
}

// ---------------------------------------------------------------------------
// Layer 6: Provider Policies (priority 50)
// ---------------------------------------------------------------------------

/// Provider-specific tool restrictions.
///
/// Some model providers don't support certain tools.
pub struct ProviderPolicyLayer {
    /// Provider name → unsupported tools.
    unsupported: HashMap<String, HashSet<String>>,
}

impl ProviderPolicyLayer {
    pub fn new() -> Self {
        Self {
            unsupported: HashMap::new(),
        }
    }

    pub fn add_unsupported(&mut self, provider: &str, tools: Vec<String>) {
        self.unsupported
            .entry(provider.to_owned())
            .or_default()
            .extend(tools);
    }
}

impl Default for ProviderPolicyLayer {
    fn default() -> Self {
        Self::new()
    }
}

impl PolicyLayer for ProviderPolicyLayer {
    fn evaluate(&self, tool_name: &str, context: &PolicyContext) -> Option<PolicyDecision> {
        let provider = context.provider.as_deref()?;

        if let Some(unsupported) = self.unsupported.get(provider) {
            if unsupported.contains(tool_name) {
                return Some(PolicyDecision::Deny(format!(
                    "provider `{provider}` does not support `{tool_name}`"
                )));
            }
        }

        None
    }

    fn priority(&self) -> u32 {
        50
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx() -> PolicyContext {
        PolicyContext {
            agent_id: "agent-1".into(),
            group_id: Some("guild-1".into()),
            provider: Some("anthropic".into()),
            is_elevated: false,
        }
    }

    // -- SandboxLayer --

    #[test]
    fn sandbox_blocks_forbidden_tools() {
        let layer = SandboxLayer::new(vec!["exec_process".into(), "write_file".into()]);
        assert!(matches!(
            layer.evaluate("exec_process", &ctx()),
            Some(PolicyDecision::Deny(_))
        ));
    }

    #[test]
    fn sandbox_passes_through_allowed_tools() {
        let layer = SandboxLayer::new(vec!["exec_process".into()]);
        assert!(layer.evaluate("read_file", &ctx()).is_none());
    }

    // -- DenyListLayer --

    #[test]
    fn deny_list_blocks() {
        let layer = DenyListLayer::new(vec!["dangerous_tool".into()]);
        assert!(matches!(
            layer.evaluate("dangerous_tool", &ctx()),
            Some(PolicyDecision::Deny(_))
        ));
    }

    #[test]
    fn deny_list_passes_through() {
        let layer = DenyListLayer::new(vec!["dangerous_tool".into()]);
        assert!(layer.evaluate("safe_tool", &ctx()).is_none());
    }

    // -- AllowListLayer --

    #[test]
    fn allow_list_allows() {
        let layer = AllowListLayer::new(vec!["search".into()]);
        assert_eq!(layer.evaluate("search", &ctx()), Some(PolicyDecision::Allow));
    }

    #[test]
    fn allow_list_passes_through_unlisted() {
        let layer = AllowListLayer::new(vec!["search".into()]);
        assert!(layer.evaluate("shell", &ctx()).is_none());
    }

    // -- GroupPolicyLayer --

    #[test]
    fn group_deny_blocks_in_group() {
        let mut layer = GroupPolicyLayer::new();
        layer.deny_for_group("guild-1", vec!["spam_tool".into()]);
        assert!(matches!(
            layer.evaluate("spam_tool", &ctx()),
            Some(PolicyDecision::Deny(_))
        ));
    }

    #[test]
    fn group_allow_allows_in_group() {
        let mut layer = GroupPolicyLayer::new();
        layer.allow_for_group("guild-1", vec!["mod_tool".into()]);
        assert_eq!(layer.evaluate("mod_tool", &ctx()), Some(PolicyDecision::Allow));
    }

    #[test]
    fn group_no_opinion_without_group_id() {
        let mut layer = GroupPolicyLayer::new();
        layer.deny_for_group("guild-1", vec!["tool".into()]);
        let ctx_no_group = PolicyContext {
            group_id: None,
            ..ctx()
        };
        assert!(layer.evaluate("tool", &ctx_no_group).is_none());
    }

    // -- AgentOverrideLayer --

    #[test]
    fn agent_override_deny() {
        let mut layer = AgentOverrideLayer::new();
        layer.deny_for_agent("agent-1", vec!["shell".into()]);
        assert!(matches!(
            layer.evaluate("shell", &ctx()),
            Some(PolicyDecision::Deny(_))
        ));
    }

    #[test]
    fn agent_override_allow() {
        let mut layer = AgentOverrideLayer::new();
        layer.allow_for_agent("agent-1", vec!["special_tool".into()]);
        assert_eq!(
            layer.evaluate("special_tool", &ctx()),
            Some(PolicyDecision::Allow)
        );
    }

    #[test]
    fn agent_override_no_opinion_for_other_agent() {
        let mut layer = AgentOverrideLayer::new();
        layer.deny_for_agent("agent-2", vec!["shell".into()]);
        // agent-1 context should not match agent-2's rules
        assert!(layer.evaluate("shell", &ctx()).is_none());
    }

    // -- ProviderPolicyLayer --

    #[test]
    fn provider_blocks_unsupported() {
        let mut layer = ProviderPolicyLayer::new();
        layer.add_unsupported("anthropic", vec!["image_gen".into()]);
        assert!(matches!(
            layer.evaluate("image_gen", &ctx()),
            Some(PolicyDecision::Deny(_))
        ));
    }

    #[test]
    fn provider_no_opinion_for_supported() {
        let mut layer = ProviderPolicyLayer::new();
        layer.add_unsupported("anthropic", vec!["image_gen".into()]);
        assert!(layer.evaluate("search", &ctx()).is_none());
    }

    #[test]
    fn provider_no_opinion_without_provider() {
        let mut layer = ProviderPolicyLayer::new();
        layer.add_unsupported("anthropic", vec!["image_gen".into()]);
        let ctx_no_provider = PolicyContext {
            provider: None,
            ..ctx()
        };
        assert!(layer.evaluate("image_gen", &ctx_no_provider).is_none());
    }

    // -- Priority ordering --

    #[test]
    fn layer_priorities_are_correct() {
        assert_eq!(SandboxLayer::new(vec![]).priority(), 100);
        assert_eq!(DenyListLayer::new(vec![]).priority(), 90);
        assert_eq!(AllowListLayer::new(vec![]).priority(), 80);
        assert_eq!(GroupPolicyLayer::new().priority(), 70);
        assert_eq!(AgentOverrideLayer::new().priority(), 60);
        assert_eq!(ProviderPolicyLayer::new().priority(), 50);
    }
}
