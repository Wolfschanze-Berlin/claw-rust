//! Hook execution pipeline — runs registered hooks in priority order.

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tracing::{debug, warn};

use crate::registry::PluginRegistry;
use crate::types::HookResult;

// ---------------------------------------------------------------------------
// HookEvent
// ---------------------------------------------------------------------------

/// All lifecycle hook points in the system.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HookEvent {
    BeforeModelResolve,
    BeforePromptBuild,
    BeforeAgentStart,
    LlmInput,
    BeforeToolCall,
    AfterToolCall,
    LlmOutput,
    AgentEnd,
    MessageReceived,
    MessageSending,
    MessageSent,
    SessionStart,
    SessionEnd,
    GatewayStart,
    GatewayStop,
    BeforeCompaction,
    AfterCompaction,
    BeforeReset,
    ToolResultPersist,
    BeforeMessageWrite,
}

impl HookEvent {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::BeforeModelResolve => "before_model_resolve",
            Self::BeforePromptBuild => "before_prompt_build",
            Self::BeforeAgentStart => "before_agent_start",
            Self::LlmInput => "llm_input",
            Self::BeforeToolCall => "before_tool_call",
            Self::AfterToolCall => "after_tool_call",
            Self::LlmOutput => "llm_output",
            Self::AgentEnd => "agent_end",
            Self::MessageReceived => "message_received",
            Self::MessageSending => "message_sending",
            Self::MessageSent => "message_sent",
            Self::SessionStart => "session_start",
            Self::SessionEnd => "session_end",
            Self::GatewayStart => "gateway_start",
            Self::GatewayStop => "gateway_stop",
            Self::BeforeCompaction => "before_compaction",
            Self::AfterCompaction => "after_compaction",
            Self::BeforeReset => "before_reset",
            Self::ToolResultPersist => "tool_result_persist",
            Self::BeforeMessageWrite => "before_message_write",
        }
    }
}

impl std::fmt::Display for HookEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// The outcome of a successful hook pipeline execution.
#[derive(Debug, Clone)]
pub struct HookPipelineResult {
    pub data: Value,
    pub hooks_executed: usize,
    pub was_modified: bool,
}

/// Error returned when a hook aborts the pipeline.
#[derive(Debug, Clone, thiserror::Error)]
#[error("hook pipeline aborted by plugin '{plugin_id}': {reason}")]
pub struct HookAbortError {
    pub plugin_id: String,
    pub reason: String,
    pub hooks_executed_before_abort: usize,
}

/// Executes hook handlers registered in the [`PluginRegistry`] for a given event.
pub struct HookPipeline {
    registry: Arc<PluginRegistry>,
}

impl HookPipeline {
    pub fn new(registry: Arc<PluginRegistry>) -> Self {
        Self { registry }
    }

    /// Execute all hooks registered for `event`, threading `data` through the chain.
    pub async fn execute(
        &self,
        event: HookEvent,
        data: Value,
    ) -> Result<HookPipelineResult, HookAbortError> {
        let event_str = event.as_str();
        let hooks = self.registry.get_all_hooks(event);

        if hooks.is_empty() {
            debug!(event = event_str, "no hooks registered");
            return Ok(HookPipelineResult { data, hooks_executed: 0, was_modified: false });
        }

        debug!(event = event_str, count = hooks.len(), "executing hook pipeline");

        let mut current_data = data;
        let mut was_modified = false;
        let mut hooks_executed = 0;

        for (plugin_id, handler, priority) in &hooks {
            debug!(event = event_str, plugin_id, priority, "executing hook");

            let result = handler
                .execute(event_str, current_data.clone())
                .await
                .map_err(|err| {
                    warn!(event = event_str, plugin_id, %err, "hook handler error");
                    HookAbortError {
                        plugin_id: plugin_id.clone(),
                        reason: err.to_string(),
                        hooks_executed_before_abort: hooks_executed,
                    }
                })?;

            hooks_executed += 1;

            match result {
                HookResult::Continue => {}
                HookResult::Modified(new_data) => {
                    current_data = new_data;
                    was_modified = true;
                }
                HookResult::Abort(reason) => {
                    return Err(HookAbortError {
                        plugin_id: plugin_id.clone(),
                        reason,
                        hooks_executed_before_abort: hooks_executed - 1,
                    });
                }
            }
        }

        Ok(HookPipelineResult { data: current_data, hooks_executed, was_modified })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{HookHandler, HookResult, PluginDefinition, PluginKind};
    use async_trait::async_trait;

    fn test_def(id: &str) -> PluginDefinition {
        PluginDefinition {
            id: id.into(), name: format!("Plugin {id}"),
            description: None, version: None,
            kind: PluginKind::Bundled, config_schema: None,
        }
    }

    fn pipeline_from(reg: PluginRegistry) -> HookPipeline {
        HookPipeline::new(Arc::new(reg))
    }

    struct ContinueHook;
    #[async_trait]
    impl HookHandler for ContinueHook {
        async fn execute(&self, _e: &str, _d: Value) -> anyhow::Result<HookResult> {
            Ok(HookResult::Continue)
        }
    }

    struct ModifyHook { key: String, value: Value }
    #[async_trait]
    impl HookHandler for ModifyHook {
        async fn execute(&self, _e: &str, mut d: Value) -> anyhow::Result<HookResult> {
            if let Some(o) = d.as_object_mut() { o.insert(self.key.clone(), self.value.clone()); }
            Ok(HookResult::Modified(d))
        }
    }

    struct AbortHook { reason: String }
    #[async_trait]
    impl HookHandler for AbortHook {
        async fn execute(&self, _e: &str, _d: Value) -> anyhow::Result<HookResult> {
            Ok(HookResult::Abort(self.reason.clone()))
        }
    }

    struct ErrorHook;
    #[async_trait]
    impl HookHandler for ErrorHook {
        async fn execute(&self, _e: &str, _d: Value) -> anyhow::Result<HookResult> {
            Err(anyhow::anyhow!("handler crashed"))
        }
    }

    #[test]
    fn hook_event_serde_roundtrip() {
        for (ev, exp) in [
            (HookEvent::BeforeModelResolve, "\"before_model_resolve\""),
            (HookEvent::LlmInput, "\"llm_input\""),
            (HookEvent::GatewayStart, "\"gateway_start\""),
        ] {
            let j = serde_json::to_string(&ev).unwrap();
            assert_eq!(j, exp);
            assert_eq!(serde_json::from_str::<HookEvent>(&j).unwrap(), ev);
        }
    }

    #[test]
    fn hook_event_display() {
        assert_eq!(HookEvent::BeforeAgentStart.to_string(), "before_agent_start");
        assert_eq!(HookEvent::SessionEnd.to_string(), "session_end");
    }

    #[test]
    fn hook_event_count_is_20() {
        let all = [
            HookEvent::BeforeModelResolve, HookEvent::BeforePromptBuild,
            HookEvent::BeforeAgentStart, HookEvent::LlmInput,
            HookEvent::BeforeToolCall, HookEvent::AfterToolCall,
            HookEvent::LlmOutput, HookEvent::AgentEnd,
            HookEvent::MessageReceived, HookEvent::MessageSending,
            HookEvent::MessageSent, HookEvent::SessionStart,
            HookEvent::SessionEnd, HookEvent::GatewayStart,
            HookEvent::GatewayStop, HookEvent::BeforeCompaction,
            HookEvent::AfterCompaction, HookEvent::BeforeReset,
            HookEvent::ToolResultPersist, HookEvent::BeforeMessageWrite,
        ];
        assert_eq!(all.len(), 20);
        for ev in &all {
            let s = serde_json::to_string(ev).unwrap();
            assert_eq!(s, format!("\"{}\"", ev.as_str()));
        }
    }

    #[tokio::test]
    async fn empty_pipeline() {
        let r = pipeline_from(PluginRegistry::new())
            .execute(HookEvent::MessageSending, serde_json::json!({}))
            .await.unwrap();
        assert_eq!(r.hooks_executed, 0);
        assert!(!r.was_modified);
    }

    #[tokio::test]
    async fn continue_passes_through() {
        let mut reg = PluginRegistry::new();
        let api = reg.register(test_def("a"));
        api.register_hook(&[HookEvent::MessageSending], Arc::new(ContinueHook), 0);
        reg.activate("a").unwrap();
        let r = pipeline_from(reg).execute(HookEvent::MessageSending, serde_json::json!({})).await.unwrap();
        assert_eq!(r.hooks_executed, 1);
        assert!(!r.was_modified);
    }

    #[tokio::test]
    async fn modified_changes_data() {
        let mut reg = PluginRegistry::new();
        let api = reg.register(test_def("a"));
        api.register_hook(&[HookEvent::MessageSending], Arc::new(ModifyHook { key: "k".into(), value: Value::Bool(true) }), 0);
        reg.activate("a").unwrap();
        let r = pipeline_from(reg).execute(HookEvent::MessageSending, serde_json::json!({})).await.unwrap();
        assert_eq!(r.data["k"], true);
        assert!(r.was_modified);
    }

    #[tokio::test]
    async fn abort_stops_pipeline() {
        let mut reg = PluginRegistry::new();
        let api = reg.register(test_def("b"));
        api.register_hook(&[HookEvent::MessageSending], Arc::new(AbortHook { reason: "no".into() }), 0);
        reg.activate("b").unwrap();
        let e = pipeline_from(reg).execute(HookEvent::MessageSending, serde_json::json!({})).await.unwrap_err();
        assert_eq!(e.plugin_id, "b");
        assert_eq!(e.reason, "no");
    }

    #[tokio::test]
    async fn priority_ordering() {
        let mut reg = PluginRegistry::new();
        let hi = reg.register(test_def("hi"));
        hi.register_hook(&[HookEvent::BeforeToolCall], Arc::new(ModifyHook { key: "v".into(), value: serde_json::json!("hi") }), 100);
        reg.activate("hi").unwrap();
        let lo = reg.register(test_def("lo"));
        lo.register_hook(&[HookEvent::BeforeToolCall], Arc::new(ModifyHook { key: "v".into(), value: serde_json::json!("lo") }), 10);
        reg.activate("lo").unwrap();
        let r = pipeline_from(reg).execute(HookEvent::BeforeToolCall, serde_json::json!({})).await.unwrap();
        assert_eq!(r.data["v"], "hi"); // last writer wins
    }

    #[tokio::test]
    async fn chained_modifications() {
        let mut reg = PluginRegistry::new();
        let a = reg.register(test_def("a"));
        a.register_hook(&[HookEvent::LlmInput], Arc::new(ModifyHook { key: "a".into(), value: Value::Bool(true) }), 10);
        reg.activate("a").unwrap();
        let b = reg.register(test_def("b"));
        b.register_hook(&[HookEvent::LlmInput], Arc::new(ModifyHook { key: "b".into(), value: Value::Bool(true) }), 20);
        reg.activate("b").unwrap();
        let r = pipeline_from(reg).execute(HookEvent::LlmInput, serde_json::json!({})).await.unwrap();
        assert_eq!(r.data["a"], true);
        assert_eq!(r.data["b"], true);
    }

    #[tokio::test]
    async fn hooks_executed_before_abort() {
        let mut reg = PluginRegistry::new();
        let a = reg.register(test_def("a"));
        a.register_hook(&[HookEvent::AgentEnd], Arc::new(ContinueHook), 10);
        reg.activate("a").unwrap();
        let b = reg.register(test_def("b"));
        b.register_hook(&[HookEvent::AgentEnd], Arc::new(ContinueHook), 20);
        reg.activate("b").unwrap();
        let c = reg.register(test_def("c"));
        c.register_hook(&[HookEvent::AgentEnd], Arc::new(AbortHook { reason: "stop".into() }), 30);
        reg.activate("c").unwrap();
        let e = pipeline_from(reg).execute(HookEvent::AgentEnd, serde_json::json!({})).await.unwrap_err();
        assert_eq!(e.hooks_executed_before_abort, 2);
    }

    #[tokio::test]
    async fn handler_error_as_abort() {
        let mut reg = PluginRegistry::new();
        let api = reg.register(test_def("x"));
        api.register_hook(&[HookEvent::GatewayStart], Arc::new(ErrorHook), 0);
        reg.activate("x").unwrap();
        let e = pipeline_from(reg).execute(HookEvent::GatewayStart, serde_json::json!({})).await.unwrap_err();
        assert_eq!(e.reason, "handler crashed");
    }
}
