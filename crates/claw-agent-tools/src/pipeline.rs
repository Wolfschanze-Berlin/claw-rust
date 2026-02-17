//! Tool execution pipeline with hooks.
//!
//! Orchestrates the full lifecycle of a tool call: resolve from registry,
//! check policy, run before-hooks, execute, truncate oversized results,
//! run after-hooks, and return.

use std::collections::HashMap;

use async_trait::async_trait;
use tracing::{debug, warn};

use claw_agent_models::types::{ToolCall, ToolResult};

use crate::policy::{PolicyContext, PolicyDecision, PolicyEngine};

// ---------------------------------------------------------------------------
// ToolExecutionError
// ---------------------------------------------------------------------------

/// Errors that can occur during tool execution.
#[derive(Debug, thiserror::Error)]
pub enum ToolExecutionError {
    #[error("tool execution failed: {0}")]
    Failed(String),

    #[error("tool timed out after {0}ms")]
    Timeout(u64),

    #[error("invalid arguments: {0}")]
    InvalidArguments(String),
}

// ---------------------------------------------------------------------------
// Tool trait
// ---------------------------------------------------------------------------

/// A tool that can be executed by the pipeline.
#[async_trait]
pub trait Tool: Send + Sync {
    /// Unique name identifying this tool.
    fn name(&self) -> &str;

    /// Human-readable description of what this tool does.
    fn description(&self) -> &str;

    /// JSON Schema describing the tool's parameters.
    fn parameters_schema(&self) -> serde_json::Value;

    /// Execute the tool with the given arguments.
    async fn execute(&self, arguments: serde_json::Value) -> Result<String, ToolExecutionError>;
}

// ---------------------------------------------------------------------------
// ToolRegistry
// ---------------------------------------------------------------------------

/// Registry of available tools, keyed by name.
pub struct ToolRegistry {
    tools: HashMap<String, Box<dyn Tool>>,
}

impl ToolRegistry {
    /// Create an empty registry.
    pub fn new() -> Self {
        Self {
            tools: HashMap::new(),
        }
    }

    /// Register a tool. Overwrites any existing tool with the same name.
    pub fn register(&mut self, tool: Box<dyn Tool>) {
        let name = tool.name().to_owned();
        debug!(tool = %name, "registered tool");
        self.tools.insert(name, tool);
    }

    /// Look up a tool by name.
    pub fn get(&self, name: &str) -> Option<&dyn Tool> {
        self.tools.get(name).map(|t| t.as_ref())
    }

    /// Number of registered tools.
    pub fn len(&self) -> usize {
        self.tools.len()
    }

    /// Whether the registry is empty.
    pub fn is_empty(&self) -> bool {
        self.tools.is_empty()
    }

    /// Return definitions for all registered tools.
    pub fn definitions(&self) -> Vec<claw_agent_models::ToolDefinition> {
        self.tools
            .values()
            .map(|t| claw_agent_models::ToolDefinition {
                name: t.name().to_owned(),
                description: t.description().to_owned(),
                parameters: t.parameters_schema(),
            })
            .collect()
    }
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// ToolHook trait
// ---------------------------------------------------------------------------

/// Decision returned by a before-call hook.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HookDecision {
    /// Allow the tool call to proceed.
    Allow,
    /// Block the tool call with a reason.
    Deny(String),
}

/// Hook called before and/or after tool execution.
///
/// Hooks run in registration order. A single `Deny` from any before-hook
/// short-circuits the pipeline.
#[async_trait]
pub trait ToolHook: Send + Sync {
    /// Called before the tool executes. Return `Deny` to block execution.
    async fn before_call(
        &self,
        tool_name: &str,
        arguments: &serde_json::Value,
    ) -> HookDecision;

    /// Called after the tool executes. Return `Some(modified)` to replace
    /// the result, or `None` to leave it unchanged.
    async fn after_call(&self, tool_name: &str, result: &str) -> Option<String>;
}

// ---------------------------------------------------------------------------
// PipelineConfig
// ---------------------------------------------------------------------------

/// Configuration for the tool execution pipeline.
#[derive(Debug, Clone)]
pub struct PipelineConfig {
    /// Maximum result size in characters before truncation kicks in.
    pub max_result_size: usize,
    /// Marker inserted at the truncation point.
    pub truncation_marker: String,
    /// Number of characters to preserve from the end of the result.
    pub preserve_end_chars: usize,
}

impl Default for PipelineConfig {
    fn default() -> Self {
        Self {
            max_result_size: 100_000,
            truncation_marker: "\n\n... [truncated] ...\n\n".to_owned(),
            preserve_end_chars: 1_000,
        }
    }
}

// ---------------------------------------------------------------------------
// ToolPipeline
// ---------------------------------------------------------------------------

/// Orchestrates tool execution: registry lookup, policy check, hooks,
/// execution, truncation.
pub struct ToolPipeline {
    registry: ToolRegistry,
    policy_engine: PolicyEngine,
    hooks: Vec<Box<dyn ToolHook>>,
    config: PipelineConfig,
}

impl ToolPipeline {
    /// Create a new pipeline with the given components.
    pub fn new(
        registry: ToolRegistry,
        policy_engine: PolicyEngine,
        config: PipelineConfig,
    ) -> Self {
        Self {
            registry,
            policy_engine,
            hooks: Vec::new(),
            config,
        }
    }

    /// Add a hook to the pipeline. Hooks run in registration order.
    pub fn add_hook(&mut self, hook: Box<dyn ToolHook>) {
        self.hooks.push(hook);
    }

    /// Return tool definitions for all registered tools.
    ///
    /// These are used in the system prompt and the model API request's
    /// `tools` parameter so the model knows what tools are available.
    pub fn tool_definitions(&self) -> Vec<claw_agent_models::ToolDefinition> {
        self.registry.definitions()
    }

    /// Execute a tool call through the full pipeline.
    ///
    /// Steps:
    /// 1. Resolve tool from registry
    /// 2. Evaluate policy
    /// 3. Run before-call hooks
    /// 4. Execute tool
    /// 5. Truncate result if oversized
    /// 6. Run after-call hooks
    /// 7. Return result
    pub async fn execute_tool_call(
        &self,
        call: &ToolCall,
        context: &PolicyContext,
    ) -> ToolResult {
        // 1. Resolve tool
        let tool = match self.registry.get(&call.name) {
            Some(t) => t,
            None => {
                warn!(tool = %call.name, "unknown tool");
                return ToolResult {
                    tool_call_id: call.id.clone(),
                    content: format!("unknown tool: `{}`", call.name),
                    is_error: true,
                };
            }
        };

        // 2. Evaluate policy
        let decision = self.policy_engine.evaluate(&call.name, context);
        if let PolicyDecision::Deny(reason) = decision {
            debug!(tool = %call.name, %reason, "tool denied by policy");
            return ToolResult {
                tool_call_id: call.id.clone(),
                content: format!("tool denied: {reason}"),
                is_error: true,
            };
        }

        // 3. Run before-call hooks
        for hook in &self.hooks {
            let decision = hook.before_call(&call.name, &call.arguments).await;
            if let HookDecision::Deny(reason) = decision {
                debug!(tool = %call.name, %reason, "tool denied by hook");
                return ToolResult {
                    tool_call_id: call.id.clone(),
                    content: format!("tool blocked by hook: {reason}"),
                    is_error: true,
                };
            }
        }

        // 4. Execute
        let content = match tool.execute(call.arguments.clone()).await {
            Ok(output) => output,
            Err(err) => {
                warn!(tool = %call.name, %err, "tool execution failed");
                return ToolResult {
                    tool_call_id: call.id.clone(),
                    content: err.to_string(),
                    is_error: true,
                };
            }
        };

        // 5. Truncate if oversized
        let mut content = self.maybe_truncate(&content);

        // 6. Run after-call hooks
        for hook in &self.hooks {
            if let Some(modified) = hook.after_call(&call.name, &content).await {
                content = modified;
            }
        }

        // 7. Return
        ToolResult {
            tool_call_id: call.id.clone(),
            content,
            is_error: false,
        }
    }

    /// Truncate content that exceeds `max_result_size`, preserving the
    /// beginning and end with a truncation marker in between.
    fn maybe_truncate(&self, content: &str) -> String {
        let cfg = &self.config;
        if content.len() <= cfg.max_result_size {
            return content.to_owned();
        }

        let marker_len = cfg.truncation_marker.len();
        let end_len = cfg.preserve_end_chars;

        // Guard: if marker + end exceed budget, just hard-truncate
        if marker_len + end_len >= cfg.max_result_size {
            return content[..cfg.max_result_size].to_owned();
        }

        let keep_start = cfg.max_result_size - end_len - marker_len;
        let end_start = content.len() - end_len;

        format!(
            "{}{}{}",
            &content[..keep_start],
            cfg.truncation_marker,
            &content[end_start..]
        )
    }

    /// Access the tool registry.
    pub fn registry(&self) -> &ToolRegistry {
        &self.registry
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layers::AllowListLayer;

    // -- Mock tool --

    struct EchoTool;

    #[async_trait]
    impl Tool for EchoTool {
        fn name(&self) -> &str {
            "echo"
        }
        fn description(&self) -> &str {
            "Echoes the input"
        }
        fn parameters_schema(&self) -> serde_json::Value {
            serde_json::json!({"type": "object", "properties": {"text": {"type": "string"}}})
        }
        async fn execute(
            &self,
            arguments: serde_json::Value,
        ) -> Result<String, ToolExecutionError> {
            Ok(arguments
                .get("text")
                .and_then(|v| v.as_str())
                .unwrap_or("no text")
                .to_owned())
        }
    }

    struct FailingTool;

    #[async_trait]
    impl Tool for FailingTool {
        fn name(&self) -> &str {
            "fail"
        }
        fn description(&self) -> &str {
            "Always fails"
        }
        fn parameters_schema(&self) -> serde_json::Value {
            serde_json::json!({})
        }
        async fn execute(
            &self,
            _arguments: serde_json::Value,
        ) -> Result<String, ToolExecutionError> {
            Err(ToolExecutionError::Failed("intentional failure".into()))
        }
    }

    // -- Mock hook --

    struct DenyHook {
        blocked_tool: String,
    }

    #[async_trait]
    impl ToolHook for DenyHook {
        async fn before_call(
            &self,
            tool_name: &str,
            _arguments: &serde_json::Value,
        ) -> HookDecision {
            if tool_name == self.blocked_tool {
                HookDecision::Deny("hook says no".into())
            } else {
                HookDecision::Allow
            }
        }
        async fn after_call(&self, _tool_name: &str, _result: &str) -> Option<String> {
            None
        }
    }

    struct SuffixHook {
        suffix: String,
    }

    #[async_trait]
    impl ToolHook for SuffixHook {
        async fn before_call(
            &self,
            _tool_name: &str,
            _arguments: &serde_json::Value,
        ) -> HookDecision {
            HookDecision::Allow
        }
        async fn after_call(&self, _tool_name: &str, result: &str) -> Option<String> {
            Some(format!("{result}{}", self.suffix))
        }
    }

    // -- Helpers --

    fn make_pipeline(tools: Vec<Box<dyn Tool>>, allowed: Vec<&str>) -> ToolPipeline {
        let mut registry = ToolRegistry::new();
        for tool in tools {
            registry.register(tool);
        }
        let mut engine = PolicyEngine::new();
        engine.add_layer(Box::new(AllowListLayer::new(
            allowed.into_iter().map(String::from).collect(),
        )));
        ToolPipeline::new(registry, engine, PipelineConfig::default())
    }

    fn call(name: &str, args: serde_json::Value) -> ToolCall {
        ToolCall {
            id: format!("call-{name}"),
            name: name.to_owned(),
            arguments: args,
        }
    }

    fn ctx() -> PolicyContext {
        PolicyContext::default()
    }

    // -- Tests --

    #[tokio::test]
    async fn execute_mock_tool_successfully() {
        let pipeline = make_pipeline(vec![Box::new(EchoTool)], vec!["echo"]);
        let tc = call("echo", serde_json::json!({"text": "hello"}));
        let result = pipeline.execute_tool_call(&tc, &ctx()).await;
        assert!(!result.is_error);
        assert_eq!(result.content, "hello");
        assert_eq!(result.tool_call_id, "call-echo");
    }

    #[tokio::test]
    async fn unknown_tool_returns_error() {
        let pipeline = make_pipeline(vec![], vec![]);
        let tc = call("nonexistent", serde_json::json!({}));
        let result = pipeline.execute_tool_call(&tc, &ctx()).await;
        assert!(result.is_error);
        assert!(result.content.contains("unknown tool"));
    }

    #[tokio::test]
    async fn policy_deny_blocks_execution() {
        // echo is registered but NOT in allow list
        let pipeline = make_pipeline(vec![Box::new(EchoTool)], vec![]);
        let tc = call("echo", serde_json::json!({"text": "hello"}));
        let result = pipeline.execute_tool_call(&tc, &ctx()).await;
        assert!(result.is_error);
        assert!(result.content.contains("tool denied"));
    }

    #[tokio::test]
    async fn hook_deny_blocks_execution() {
        let mut pipeline = make_pipeline(vec![Box::new(EchoTool)], vec!["echo"]);
        pipeline.add_hook(Box::new(DenyHook {
            blocked_tool: "echo".into(),
        }));
        let tc = call("echo", serde_json::json!({"text": "hello"}));
        let result = pipeline.execute_tool_call(&tc, &ctx()).await;
        assert!(result.is_error);
        assert!(result.content.contains("hook says no"));
    }

    #[tokio::test]
    async fn tool_execution_failure_returns_error() {
        let pipeline = make_pipeline(vec![Box::new(FailingTool)], vec!["fail"]);
        let tc = call("fail", serde_json::json!({}));
        let result = pipeline.execute_tool_call(&tc, &ctx()).await;
        assert!(result.is_error);
        assert!(result.content.contains("intentional failure"));
    }

    #[tokio::test]
    async fn result_within_limit_not_truncated() {
        let pipeline = make_pipeline(vec![Box::new(EchoTool)], vec!["echo"]);
        let tc = call("echo", serde_json::json!({"text": "short"}));
        let result = pipeline.execute_tool_call(&tc, &ctx()).await;
        assert_eq!(result.content, "short");
    }

    #[tokio::test]
    async fn result_exceeding_limit_is_truncated() {
        let mut registry = ToolRegistry::new();
        registry.register(Box::new(EchoTool));
        let mut engine = PolicyEngine::new();
        engine.add_layer(Box::new(AllowListLayer::new(vec!["echo".into()])));

        let config = PipelineConfig {
            max_result_size: 50,
            truncation_marker: "[CUT]".to_owned(),
            preserve_end_chars: 10,
        };
        let pipeline = ToolPipeline::new(registry, engine, config);

        // Create a string longer than 50 chars
        let long_text = "A".repeat(30) + &"B".repeat(30);
        let tc = call("echo", serde_json::json!({"text": long_text}));
        let result = pipeline.execute_tool_call(&tc, &ctx()).await;

        assert!(!result.is_error);
        assert!(result.content.len() <= 50);
        assert!(result.content.contains("[CUT]"));
        // Should preserve last 10 chars (all B's)
        assert!(result.content.ends_with("BBBBBBBBBB"));
        // Should preserve beginning (all A's)
        assert!(result.content.starts_with("AAAA"));
    }

    #[tokio::test]
    async fn after_hook_can_modify_result() {
        let mut pipeline = make_pipeline(vec![Box::new(EchoTool)], vec!["echo"]);
        pipeline.add_hook(Box::new(SuffixHook {
            suffix: " [audited]".into(),
        }));
        let tc = call("echo", serde_json::json!({"text": "data"}));
        let result = pipeline.execute_tool_call(&tc, &ctx()).await;
        assert!(!result.is_error);
        assert_eq!(result.content, "data [audited]");
    }

    // -- Registry tests --

    #[test]
    fn registry_len_and_is_empty() {
        let mut reg = ToolRegistry::new();
        assert!(reg.is_empty());
        assert_eq!(reg.len(), 0);
        reg.register(Box::new(EchoTool));
        assert!(!reg.is_empty());
        assert_eq!(reg.len(), 1);
    }

    #[test]
    fn registry_get_returns_none_for_missing() {
        let reg = ToolRegistry::new();
        assert!(reg.get("nope").is_none());
    }

    // -- Truncation edge cases --

    #[test]
    fn truncation_preserves_beginning_marker_end() {
        let config = PipelineConfig {
            max_result_size: 30,
            truncation_marker: "...".to_owned(),
            preserve_end_chars: 5,
        };
        let pipeline = ToolPipeline::new(
            ToolRegistry::new(),
            PolicyEngine::new(),
            config,
        );

        // 50 chars total, budget = 30, marker = 3, end = 5, start = 22
        let input = "X".repeat(50);
        let result = pipeline.maybe_truncate(&input);
        assert_eq!(result.len(), 30);
        assert_eq!(&result[..22], &"X".repeat(22));
        assert_eq!(&result[22..25], "...");
        assert_eq!(&result[25..], "XXXXX");
    }

    #[test]
    fn no_truncation_when_within_limit() {
        let pipeline = ToolPipeline::new(
            ToolRegistry::new(),
            PolicyEngine::new(),
            PipelineConfig::default(),
        );
        let input = "short string";
        assert_eq!(pipeline.maybe_truncate(input), input);
    }
}
