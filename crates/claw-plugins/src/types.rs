//! Core plugin types: definitions, handler traits, and the PluginApi registration interface.
//!
//! This module defines the data structures and traits that form the plugin
//! contract. Plugins receive a [`PluginApi`] during registration and use it
//! to register tools, hooks, channels, gateway methods, and other extensions.

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use claw_channels::plugin::ChannelPlugin;
pub use claw_gateway::dispatch::MethodHandler;

use crate::hooks::HookEvent;

// ---------------------------------------------------------------------------
// PluginDefinition
// ---------------------------------------------------------------------------

/// Metadata describing a plugin — its identity, kind, and optional config schema.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginDefinition {
    pub id: String,
    pub name: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,

    pub kind: PluginKind,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub config_schema: Option<Value>,
}

/// The kind/origin of a plugin.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PluginKind {
    Bundled,
    Managed,
    Workspace,
    Extension,
}

// ---------------------------------------------------------------------------
// Handler traits (stubs — fleshed out in #22)
// ---------------------------------------------------------------------------

/// Handler for a plugin-provided tool.
#[async_trait]
pub trait ToolHandler: Send + Sync {
    async fn execute(&self, params: Value) -> anyhow::Result<Value>;
}

/// Handler for lifecycle/event hooks.
#[async_trait]
pub trait HookHandler: Send + Sync {
    async fn execute(&self, event: &str, data: Value) -> anyhow::Result<HookResult>;
}

/// Result of a hook execution — controls the pipeline flow.
#[derive(Debug, Clone)]
pub enum HookResult {
    /// Continue to the next hook in the chain.
    Continue,
    /// Replace the event data with a modified version.
    Modified(Value),
    /// Abort the pipeline with a reason.
    Abort(String),
}

/// Handler for plugin-registered HTTP routes.
#[async_trait]
pub trait HttpRouteHandler: Send + Sync {
    async fn handle(&self, request: Value) -> anyhow::Result<Value>;
}

/// A named service exposed by a plugin.
pub trait Service: Send + Sync {
    fn name(&self) -> &str;
}

/// Handler for plugin-registered slash commands.
#[async_trait]
pub trait CommandHandler: Send + Sync {
    async fn execute(&self, args: &str) -> anyhow::Result<String>;
}

// ---------------------------------------------------------------------------
// Registration records
// ---------------------------------------------------------------------------

/// A tool registered by a plugin.
pub struct ToolRegistration {
    pub name: String,
    pub handler: Arc<dyn ToolHandler>,
}

/// A hook registered by a plugin.
pub struct HookRegistration {
    pub events: Vec<HookEvent>,
    pub handler: Arc<dyn HookHandler>,
    pub priority: i32,
}

/// An HTTP route registered by a plugin.
pub struct HttpRouteRegistration {
    pub method: String,
    pub path: String,
    pub handler: Arc<dyn HttpRouteHandler>,
}

/// A channel registered by a plugin.
pub struct ChannelRegistration {
    pub plugin: Arc<dyn ChannelPlugin>,
}

/// A gateway RPC method registered by a plugin.
pub struct GatewayMethodRegistration {
    pub method: String,
    pub handler: Arc<dyn MethodHandler>,
}

/// A named service registered by a plugin.
pub struct ServiceRegistration {
    pub name: String,
    pub service: Arc<dyn Service>,
}

/// A slash command registered by a plugin.
pub struct CommandRegistration {
    pub name: String,
    pub handler: Arc<dyn CommandHandler>,
}

// ---------------------------------------------------------------------------
// PluginRegistrations (internal collection)
// ---------------------------------------------------------------------------

/// Collects all registrations made by a single plugin during setup.
pub(crate) struct PluginRegistrations {
    pub tools: Vec<ToolRegistration>,
    pub hooks: Vec<HookRegistration>,
    pub http_routes: Vec<HttpRouteRegistration>,
    pub channels: Vec<ChannelRegistration>,
    pub gateway_methods: Vec<GatewayMethodRegistration>,
    pub services: Vec<ServiceRegistration>,
    pub commands: Vec<CommandRegistration>,
}

impl PluginRegistrations {
    pub(crate) fn new() -> Self {
        Self {
            tools: Vec::new(),
            hooks: Vec::new(),
            http_routes: Vec::new(),
            channels: Vec::new(),
            gateway_methods: Vec::new(),
            services: Vec::new(),
            commands: Vec::new(),
        }
    }
}

// ---------------------------------------------------------------------------
// PluginApi
// ---------------------------------------------------------------------------

/// The API surface given to each plugin during registration.
///
/// Plugins call methods on this struct to register their tools, hooks,
/// channels, gateway methods, services, and commands.
pub struct PluginApi {
    plugin_id: String,
    registrations: Arc<Mutex<PluginRegistrations>>,
}

impl PluginApi {
    /// Create a new `PluginApi` backed by a shared registration store.
    pub(crate) fn new(plugin_id: String, registrations: Arc<Mutex<PluginRegistrations>>) -> Self {
        Self {
            plugin_id,
            registrations,
        }
    }

    /// The ID of the plugin this API belongs to.
    pub fn plugin_id(&self) -> &str {
        &self.plugin_id
    }

    /// Register a tool handler.
    pub fn register_tool(&self, name: &str, handler: Arc<dyn ToolHandler>) {
        let mut regs = self.registrations.lock().expect("poisoned lock");
        regs.tools.push(ToolRegistration {
            name: name.to_owned(),
            handler,
        });
    }

    /// Register a hook handler for the given events.
    pub fn register_hook(
        &self,
        events: &[HookEvent],
        handler: Arc<dyn HookHandler>,
        priority: i32,
    ) {
        let mut regs = self.registrations.lock().expect("poisoned lock");
        regs.hooks.push(HookRegistration {
            events: events.to_vec(),
            handler,
            priority,
        });
    }

    /// Register an HTTP route handler.
    pub fn register_http_route(
        &self,
        method: &str,
        path: &str,
        handler: Arc<dyn HttpRouteHandler>,
    ) {
        let mut regs = self.registrations.lock().expect("poisoned lock");
        regs.http_routes.push(HttpRouteRegistration {
            method: method.to_owned(),
            path: path.to_owned(),
            handler,
        });
    }

    /// Register a channel plugin.
    pub fn register_channel(&self, plugin: Arc<dyn ChannelPlugin>) {
        let mut regs = self.registrations.lock().expect("poisoned lock");
        regs.channels.push(ChannelRegistration { plugin });
    }

    /// Register a gateway RPC method handler.
    pub fn register_gateway_method(&self, method: &str, handler: Arc<dyn MethodHandler>) {
        let mut regs = self.registrations.lock().expect("poisoned lock");
        regs.gateway_methods.push(GatewayMethodRegistration {
            method: method.to_owned(),
            handler,
        });
    }

    /// Register a named service.
    pub fn register_service(&self, name: &str, service: Arc<dyn Service>) {
        let mut regs = self.registrations.lock().expect("poisoned lock");
        regs.services.push(ServiceRegistration {
            name: name.to_owned(),
            service,
        });
    }

    /// Register a slash command handler.
    pub fn register_command(&self, name: &str, handler: Arc<dyn CommandHandler>) {
        let mut regs = self.registrations.lock().expect("poisoned lock");
        regs.commands.push(CommandRegistration {
            name: name.to_owned(),
            handler,
        });
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plugin_kind_serde_roundtrip() {
        for (kind, expected) in [
            (PluginKind::Bundled, r#""bundled""#),
            (PluginKind::Managed, r#""managed""#),
            (PluginKind::Workspace, r#""workspace""#),
            (PluginKind::Extension, r#""extension""#),
        ] {
            let json = serde_json::to_string(&kind).unwrap();
            assert_eq!(json, expected);
            let parsed: PluginKind = serde_json::from_str(&json).unwrap();
            assert_eq!(parsed, kind);
        }
    }

    #[test]
    fn plugin_definition_serde_camel_case() {
        let def = PluginDefinition {
            id: "test-plugin".into(),
            name: "Test Plugin".into(),
            description: Some("A test".into()),
            version: Some("1.0.0".into()),
            kind: PluginKind::Bundled,
            config_schema: Some(serde_json::json!({"type": "object"})),
        };
        let json = serde_json::to_value(&def).unwrap();
        let obj = json.as_object().unwrap();
        // Field should use camelCase, not snake_case
        assert!(obj.contains_key("configSchema"));
        assert!(!obj.contains_key("config_schema"));
    }

    #[test]
    fn plugin_definition_omits_none_fields() {
        let def = PluginDefinition {
            id: "p".into(),
            name: "P".into(),
            description: None,
            version: None,
            kind: PluginKind::Workspace,
            config_schema: None,
        };
        let json = serde_json::to_value(&def).unwrap();
        let obj = json.as_object().unwrap();
        assert!(!obj.contains_key("description"));
        assert!(!obj.contains_key("version"));
        assert!(!obj.contains_key("configSchema"));
    }

    struct DummyTool;

    #[async_trait]
    impl ToolHandler for DummyTool {
        async fn execute(&self, _params: Value) -> anyhow::Result<Value> {
            Ok(Value::Null)
        }
    }

    struct DummyHook;

    #[async_trait]
    impl HookHandler for DummyHook {
        async fn execute(&self, _event: &str, _data: Value) -> anyhow::Result<HookResult> {
            Ok(HookResult::Continue)
        }
    }

    struct DummyCommand;

    #[async_trait]
    impl CommandHandler for DummyCommand {
        async fn execute(&self, _args: &str) -> anyhow::Result<String> {
            Ok("done".into())
        }
    }

    #[test]
    fn plugin_api_registers_tools() {
        let regs = Arc::new(Mutex::new(PluginRegistrations::new()));
        let api = PluginApi::new("test".into(), Arc::clone(&regs));

        api.register_tool("my_tool", Arc::new(DummyTool));

        let locked = regs.lock().unwrap();
        assert_eq!(locked.tools.len(), 1);
        assert_eq!(locked.tools[0].name, "my_tool");
    }

    #[test]
    fn plugin_api_registers_hooks() {
        let regs = Arc::new(Mutex::new(PluginRegistrations::new()));
        let api = PluginApi::new("test".into(), Arc::clone(&regs));

        api.register_hook(
            &[HookEvent::MessageSending, HookEvent::MessageSent],
            Arc::new(DummyHook),
            10,
        );

        let locked = regs.lock().unwrap();
        assert_eq!(locked.hooks.len(), 1);
        assert_eq!(
            locked.hooks[0].events,
            vec![HookEvent::MessageSending, HookEvent::MessageSent]
        );
        assert_eq!(locked.hooks[0].priority, 10);
    }

    #[test]
    fn plugin_api_registers_commands() {
        let regs = Arc::new(Mutex::new(PluginRegistrations::new()));
        let api = PluginApi::new("test".into(), Arc::clone(&regs));

        api.register_command("greet", Arc::new(DummyCommand));

        let locked = regs.lock().unwrap();
        assert_eq!(locked.commands.len(), 1);
        assert_eq!(locked.commands[0].name, "greet");
    }
}
