//! Plugin registry — stores registered plugins and queries their contributions.
//!
//! The [`PluginRegistry`] is the central store for all plugins. It manages
//! plugin lifecycle (register → activate → deactivate) and provides query
//! methods to look up tools, hooks, channels, and gateway methods across
//! all active plugins.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use claw_channels::plugin::ChannelPlugin;

use crate::hooks::HookEvent;
use crate::types::{
    HookHandler, MethodHandler, PluginApi, PluginDefinition, PluginRegistrations, ToolHandler,
};

// ---------------------------------------------------------------------------
// PluginState
// ---------------------------------------------------------------------------

/// Lifecycle state of a registered plugin.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PluginState {
    Registered,
    Active,
    Disabled,
    Error,
}

// ---------------------------------------------------------------------------
// RegisteredPlugin (internal)
// ---------------------------------------------------------------------------

struct RegisteredPlugin {
    definition: PluginDefinition,
    registrations: Arc<Mutex<PluginRegistrations>>,
    state: PluginState,
}

// ---------------------------------------------------------------------------
// PluginRegistry
// ---------------------------------------------------------------------------

/// Central registry for all plugins and their contributions.
///
/// Plugins are first registered (receiving a [`PluginApi`] to declare their
/// extensions), then activated. Only active plugins contribute tools, hooks,
/// channels, etc. to the running system.
pub struct PluginRegistry {
    plugins: HashMap<String, RegisteredPlugin>,
}

impl PluginRegistry {
    pub fn new() -> Self {
        Self {
            plugins: HashMap::new(),
        }
    }

    /// Register a plugin and return the [`PluginApi`] for it to declare extensions.
    ///
    /// The plugin starts in [`PluginState::Registered`] — call [`activate`](Self::activate)
    /// to make its contributions visible to the system.
    pub fn register(&mut self, definition: PluginDefinition) -> PluginApi {
        let regs = Arc::new(Mutex::new(PluginRegistrations::new()));
        let api = PluginApi::new(definition.id.clone(), Arc::clone(&regs));

        self.plugins.insert(
            definition.id.clone(),
            RegisteredPlugin {
                definition,
                registrations: regs,
                state: PluginState::Registered,
            },
        );

        api
    }

    /// Activate a plugin, making its registrations visible to queries.
    pub fn activate(&mut self, plugin_id: &str) -> anyhow::Result<()> {
        let plugin = self
            .plugins
            .get_mut(plugin_id)
            .ok_or_else(|| anyhow::anyhow!("plugin not found: {plugin_id}"))?;

        plugin.state = PluginState::Active;
        Ok(())
    }

    /// Deactivate a plugin. Its registrations remain but are excluded from queries.
    pub fn deactivate(&mut self, plugin_id: &str) -> anyhow::Result<()> {
        let plugin = self
            .plugins
            .get_mut(plugin_id)
            .ok_or_else(|| anyhow::anyhow!("plugin not found: {plugin_id}"))?;

        plugin.state = PluginState::Disabled;
        Ok(())
    }

    /// Get a plugin definition by ID.
    pub fn get(&self, plugin_id: &str) -> Option<&PluginDefinition> {
        self.plugins.get(plugin_id).map(|p| &p.definition)
    }

    /// List all registered plugin definitions.
    pub fn list(&self) -> Vec<&PluginDefinition> {
        self.plugins.values().map(|p| &p.definition).collect()
    }

    /// List only active plugin definitions.
    pub fn list_active(&self) -> Vec<&PluginDefinition> {
        self.plugins
            .values()
            .filter(|p| p.state == PluginState::Active)
            .map(|p| &p.definition)
            .collect()
    }

    /// Get the lifecycle state of a plugin.
    pub fn get_state(&self, plugin_id: &str) -> Option<PluginState> {
        self.plugins.get(plugin_id).map(|p| p.state)
    }

    // -- Cross-plugin queries ------------------------------------------------

    /// Get all hooks registered for a given event across active plugins,
    /// returned as `(plugin_id, handler, priority)` sorted by priority (ascending).
    pub fn get_all_hooks(&self, event: HookEvent) -> Vec<(String, Arc<dyn HookHandler>, i32)> {
        let mut results = Vec::new();

        for (id, plugin) in &self.plugins {
            if plugin.state != PluginState::Active {
                continue;
            }
            let regs = plugin.registrations.lock().expect("poisoned lock");
            for hook in &regs.hooks {
                if hook.events.contains(&event) {
                    results.push((id.clone(), Arc::clone(&hook.handler), hook.priority));
                }
            }
        }

        results.sort_by_key(|(_, _, priority)| *priority);
        results
    }

    /// Get all tools registered across active plugins as `(name, handler)`.
    pub fn get_all_tools(&self) -> Vec<(String, Arc<dyn ToolHandler>)> {
        let mut results = Vec::new();

        for plugin in self.plugins.values() {
            if plugin.state != PluginState::Active {
                continue;
            }
            let regs = plugin.registrations.lock().expect("poisoned lock");
            for tool in &regs.tools {
                results.push((tool.name.clone(), Arc::clone(&tool.handler)));
            }
        }

        results
    }

    /// Get all channel plugins registered across active plugins.
    pub fn get_all_channels(&self) -> Vec<Arc<dyn ChannelPlugin>> {
        let mut results = Vec::new();

        for plugin in self.plugins.values() {
            if plugin.state != PluginState::Active {
                continue;
            }
            let regs = plugin.registrations.lock().expect("poisoned lock");
            for ch in &regs.channels {
                results.push(Arc::clone(&ch.plugin));
            }
        }

        results
    }

    /// Look up a gateway method handler by name across active plugins.
    ///
    /// Returns the first match found — if multiple plugins register the same
    /// method name, the first one wins (registration order within the HashMap
    /// is not guaranteed; a priority system can be added later).
    pub fn get_gateway_method(&self, method: &str) -> Option<Arc<dyn MethodHandler>> {
        for plugin in self.plugins.values() {
            if plugin.state != PluginState::Active {
                continue;
            }
            let regs = plugin.registrations.lock().expect("poisoned lock");
            for gm in &regs.gateway_methods {
                if gm.method == method {
                    return Some(Arc::clone(&gm.handler));
                }
            }
        }

        None
    }
}

impl Default for PluginRegistry {
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
    use crate::hooks::HookEvent;
    use crate::types::{HookHandler, HookResult, PluginKind, ToolHandler};
    use async_trait::async_trait;
    use serde_json::Value;

    fn test_def(id: &str) -> PluginDefinition {
        PluginDefinition {
            id: id.into(),
            name: format!("Plugin {id}"),
            description: None,
            version: None,
            kind: PluginKind::Bundled,
            config_schema: None,
        }
    }

    struct EchoTool;

    #[async_trait]
    impl ToolHandler for EchoTool {
        async fn execute(&self, params: Value) -> anyhow::Result<Value> {
            Ok(params)
        }
    }

    struct TestHook;

    #[async_trait]
    impl HookHandler for TestHook {
        async fn execute(&self, _event: &str, _data: Value) -> anyhow::Result<HookResult> {
            Ok(HookResult::Continue)
        }
    }

    #[test]
    fn register_and_get() {
        let mut registry = PluginRegistry::new();
        let _api = registry.register(test_def("a"));

        assert!(registry.get("a").is_some());
        assert_eq!(registry.get("a").unwrap().name, "Plugin a");
        assert!(registry.get("nonexistent").is_none());
    }

    #[test]
    fn lifecycle_register_activate_deactivate() {
        let mut registry = PluginRegistry::new();
        let _api = registry.register(test_def("lc"));

        assert_eq!(registry.get_state("lc"), Some(PluginState::Registered));

        registry.activate("lc").unwrap();
        assert_eq!(registry.get_state("lc"), Some(PluginState::Active));

        registry.deactivate("lc").unwrap();
        assert_eq!(registry.get_state("lc"), Some(PluginState::Disabled));
    }

    #[test]
    fn activate_unknown_plugin_errors() {
        let mut registry = PluginRegistry::new();
        assert!(registry.activate("ghost").is_err());
    }

    #[test]
    fn list_all_vs_active() {
        let mut registry = PluginRegistry::new();
        let _a = registry.register(test_def("a"));
        let _b = registry.register(test_def("b"));
        let _c = registry.register(test_def("c"));

        registry.activate("a").unwrap();
        registry.activate("c").unwrap();

        assert_eq!(registry.list().len(), 3);
        assert_eq!(registry.list_active().len(), 2);

        let active_ids: Vec<&str> = registry.list_active().iter().map(|d| d.id.as_str()).collect();
        assert!(active_ids.contains(&"a"));
        assert!(active_ids.contains(&"c"));
        assert!(!active_ids.contains(&"b"));
    }

    #[test]
    fn query_tools_only_from_active() {
        let mut registry = PluginRegistry::new();

        let api_a = registry.register(test_def("a"));
        api_a.register_tool("tool_a", Arc::new(EchoTool));

        let api_b = registry.register(test_def("b"));
        api_b.register_tool("tool_b", Arc::new(EchoTool));

        // Only activate "a"
        registry.activate("a").unwrap();

        let tools = registry.get_all_tools();
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].0, "tool_a");
    }

    #[test]
    fn query_hooks_sorted_by_priority() {
        let mut registry = PluginRegistry::new();

        let api_a = registry.register(test_def("a"));
        api_a.register_hook(
            &[HookEvent::MessageSending],
            Arc::new(TestHook),
            100,
        );

        let api_b = registry.register(test_def("b"));
        api_b.register_hook(
            &[HookEvent::MessageSending],
            Arc::new(TestHook),
            10,
        );

        registry.activate("a").unwrap();
        registry.activate("b").unwrap();

        let hooks = registry.get_all_hooks(HookEvent::MessageSending);
        assert_eq!(hooks.len(), 2);
        // Priority 10 should come before 100
        assert_eq!(hooks[0].2, 10);
        assert_eq!(hooks[1].2, 100);
    }

    #[test]
    fn query_hooks_filters_by_event() {
        let mut registry = PluginRegistry::new();

        let api = registry.register(test_def("a"));
        api.register_hook(
            &[HookEvent::MessageSending],
            Arc::new(TestHook),
            0,
        );
        api.register_hook(
            &[HookEvent::MessageReceived],
            Arc::new(TestHook),
            0,
        );

        registry.activate("a").unwrap();

        assert_eq!(registry.get_all_hooks(HookEvent::MessageSending).len(), 1);
        assert_eq!(registry.get_all_hooks(HookEvent::MessageReceived).len(), 1);
        assert_eq!(registry.get_all_hooks(HookEvent::GatewayStart).len(), 0);
    }

    #[test]
    fn inactive_plugins_excluded_from_hooks() {
        let mut registry = PluginRegistry::new();

        let api = registry.register(test_def("a"));
        api.register_hook(
            &[HookEvent::SessionStart],
            Arc::new(TestHook),
            0,
        );
        // Don't activate — should not appear in queries
        assert_eq!(registry.get_all_hooks(HookEvent::SessionStart).len(), 0);
    }
}
