//! RPC method dispatcher for the gateway.
//!
//! Routes incoming [`RequestFrame`]s to registered [`MethodHandler`]
//! implementations by method name. Supports built-in method stubs,
//! plugin-registered methods, and request timeout handling.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use serde_json::Value;
use tracing::warn;

use claw_core::{ErrorCode, ErrorShape, error_shape};

use crate::protocol::frames::{GatewayFrame, RequestFrame, ResponseFrame};

// ---------------------------------------------------------------------------
// MethodHandler trait
// ---------------------------------------------------------------------------

/// Context provided to method handlers on each invocation.
#[derive(Debug, Clone)]
pub struct MethodContext {
    /// Connection ID of the requesting client.
    pub conn_id: String,
}

/// Trait for RPC method handlers.
///
/// Implement this for each gateway method (e.g. `chat.send`, `config.get`).
/// Handlers receive the request params and return either a success payload
/// or an error.
#[async_trait]
pub trait MethodHandler: Send + Sync {
    /// Handle an RPC request. Returns `Ok(payload)` on success or
    /// `Err(ErrorShape)` on failure.
    async fn handle(
        &self,
        params: Value,
        ctx: &MethodContext,
    ) -> Result<Value, ErrorShape>;
}

// ---------------------------------------------------------------------------
// MethodRegistry
// ---------------------------------------------------------------------------

/// Registry of RPC method handlers.
///
/// Methods are registered at startup. Plugins can add custom methods via
/// [`register`]. The registry is wrapped in `Arc` for sharing across
/// connections.
pub struct MethodRegistry {
    methods: HashMap<String, Box<dyn MethodHandler>>,
    default_timeout: Duration,
}

impl MethodRegistry {
    /// Create a new empty registry with a default request timeout.
    pub fn new(default_timeout: Duration) -> Self {
        Self {
            methods: HashMap::new(),
            default_timeout,
        }
    }

    /// Register a method handler. Replaces any existing handler for the
    /// same method name.
    pub fn register(
        &mut self,
        method: impl Into<String>,
        handler: Box<dyn MethodHandler>,
    ) {
        self.methods.insert(method.into(), handler);
    }

    /// Check if a method is registered.
    pub fn has_method(&self, method: &str) -> bool {
        self.methods.contains_key(method)
    }

    /// List all registered method names.
    pub fn method_names(&self) -> Vec<&str> {
        self.methods.keys().map(|s| s.as_str()).collect()
    }

    /// Number of registered methods.
    pub fn len(&self) -> usize {
        self.methods.len()
    }

    /// Whether the registry is empty.
    pub fn is_empty(&self) -> bool {
        self.methods.is_empty()
    }
}

impl std::fmt::Debug for MethodRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MethodRegistry")
            .field("methods", &self.methods.keys().collect::<Vec<_>>())
            .field("default_timeout", &self.default_timeout)
            .finish()
    }
}

// ---------------------------------------------------------------------------
// Dispatcher
// ---------------------------------------------------------------------------

/// Dispatches incoming RPC requests to registered method handlers.
///
/// Wraps a [`MethodRegistry`] in an `Arc` for concurrent access across
/// WebSocket connections. Handles timeout, unknown methods, and error
/// formatting.
#[derive(Clone)]
pub struct Dispatcher {
    registry: Arc<MethodRegistry>,
}

impl Dispatcher {
    /// Create a dispatcher from a method registry.
    pub fn new(registry: MethodRegistry) -> Self {
        Self {
            registry: Arc::new(registry),
        }
    }

    /// Dispatch a request frame and return a response frame.
    ///
    /// This is the main entry point called from the WebSocket message loop.
    /// It handles:
    /// - Unknown method → INVALID_REQUEST error
    /// - Handler timeout → AGENT_TIMEOUT error
    /// - Handler error → forwarded as-is
    /// - Handler success → wrapped in ResponseFrame::ok
    pub async fn dispatch(
        &self,
        request: &RequestFrame,
        ctx: &MethodContext,
    ) -> GatewayFrame {
        let handler = match self.registry.methods.get(&request.method) {
            Some(h) => h,
            None => {
                return GatewayFrame::Response(ResponseFrame::err(
                    &request.id,
                    error_shape(
                        ErrorCode::InvalidRequest,
                        format!("unknown method: {}", request.method),
                    ),
                ));
            }
        };

        let timeout = self.registry.default_timeout;

        match tokio::time::timeout(
            timeout,
            handler.handle(request.params.clone(), ctx),
        )
        .await
        {
            Ok(Ok(payload)) => {
                GatewayFrame::Response(ResponseFrame::ok(&request.id, payload))
            }
            Ok(Err(err)) => {
                GatewayFrame::Response(ResponseFrame::err(&request.id, err))
            }
            Err(_elapsed) => {
                warn!(
                    method = %request.method,
                    id = %request.id,
                    timeout_ms = timeout.as_millis() as u64,
                    "method handler timed out"
                );
                GatewayFrame::Response(ResponseFrame::err(
                    &request.id,
                    error_shape(
                        ErrorCode::AgentTimeout,
                        format!(
                            "method '{}' timed out after {}ms",
                            request.method,
                            timeout.as_millis()
                        ),
                    )
                    .with_retry(None),
                ))
            }
        }
    }
}

impl std::fmt::Debug for Dispatcher {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Dispatcher")
            .field("registry", &self.registry)
            .finish()
    }
}

// ---------------------------------------------------------------------------
// Built-in method stubs
// ---------------------------------------------------------------------------

/// Create a [`MethodRegistry`] populated with OpenClaw's built-in method stubs.
///
/// These stubs return placeholder responses. Real implementations will be
/// wired in as the corresponding subsystems are built (chat, config, session,
/// channels).
pub fn builtin_registry(timeout: Duration) -> MethodRegistry {
    let mut registry = MethodRegistry::new(timeout);

    registry.register("ping", Box::new(PingHandler));
    registry.register("chat.send", Box::new(StubHandler("chat.send")));
    registry.register("chat.history", Box::new(StubHandler("chat.history")));
    registry.register("chat.cancel", Box::new(StubHandler("chat.cancel")));
    registry.register("config.get", Box::new(StubHandler("config.get")));
    registry.register("config.set", Box::new(StubHandler("config.set")));
    registry.register("channel.start", Box::new(StubHandler("channel.start")));
    registry.register("channel.stop", Box::new(StubHandler("channel.stop")));
    registry.register("channel.status", Box::new(StubHandler("channel.status")));
    registry.register("session.list", Box::new(StubHandler("session.list")));
    registry.register("session.get", Box::new(StubHandler("session.get")));
    registry.register("session.delete", Box::new(StubHandler("session.delete")));
    registry.register("agent.list", Box::new(StubHandler("agent.list")));
    registry.register("health.get", Box::new(StubHandler("health.get")));

    registry
}

/// Handler for the `ping` method — returns `{"pong": true}`.
struct PingHandler;

#[async_trait]
impl MethodHandler for PingHandler {
    async fn handle(
        &self,
        _params: Value,
        _ctx: &MethodContext,
    ) -> Result<Value, ErrorShape> {
        Ok(serde_json::json!({"pong": true}))
    }
}

/// Stub handler that returns a "not implemented" error.
///
/// Used as a placeholder for methods whose real implementation depends on
/// subsystems that haven't been built yet.
struct StubHandler(&'static str);

#[async_trait]
impl MethodHandler for StubHandler {
    async fn handle(
        &self,
        _params: Value,
        _ctx: &MethodContext,
    ) -> Result<Value, ErrorShape> {
        Err(error_shape(
            ErrorCode::Unavailable,
            format!("method '{}' is not yet implemented", self.0),
        ))
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn test_ctx() -> MethodContext {
        MethodContext {
            conn_id: "test-conn".into(),
        }
    }

    // -- PingHandler --------------------------------------------------------

    #[tokio::test]
    async fn ping_handler_returns_pong() {
        let handler = PingHandler;
        let result = handler.handle(json!({}), &test_ctx()).await;
        assert_eq!(result.unwrap(), json!({"pong": true}));
    }

    // -- StubHandler --------------------------------------------------------

    #[tokio::test]
    async fn stub_handler_returns_unavailable() {
        let handler = StubHandler("chat.send");
        let result = handler.handle(json!({}), &test_ctx()).await;
        let err = result.unwrap_err();
        assert_eq!(err.code, "UNAVAILABLE");
        assert!(err.message.contains("chat.send"));
    }

    // -- MethodRegistry -----------------------------------------------------

    #[test]
    fn registry_register_and_lookup() {
        let mut registry = MethodRegistry::new(Duration::from_secs(30));
        assert!(registry.is_empty());

        registry.register("ping", Box::new(PingHandler));
        assert_eq!(registry.len(), 1);
        assert!(registry.has_method("ping"));
        assert!(!registry.has_method("unknown"));
    }

    #[test]
    fn registry_method_names() {
        let mut registry = MethodRegistry::new(Duration::from_secs(30));
        registry.register("ping", Box::new(PingHandler));
        registry.register("chat.send", Box::new(StubHandler("chat.send")));

        let mut names = registry.method_names();
        names.sort();
        assert_eq!(names, vec!["chat.send", "ping"]);
    }

    #[test]
    fn registry_replace_handler() {
        let mut registry = MethodRegistry::new(Duration::from_secs(30));
        registry.register("ping", Box::new(StubHandler("ping")));
        registry.register("ping", Box::new(PingHandler));
        // Should have replaced, not duplicated
        assert_eq!(registry.len(), 1);
    }

    #[test]
    fn builtin_registry_has_expected_methods() {
        let registry = builtin_registry(Duration::from_secs(30));
        assert!(registry.has_method("ping"));
        assert!(registry.has_method("chat.send"));
        assert!(registry.has_method("chat.history"));
        assert!(registry.has_method("config.get"));
        assert!(registry.has_method("config.set"));
        assert!(registry.has_method("channel.start"));
        assert!(registry.has_method("channel.stop"));
        assert!(registry.has_method("channel.status"));
        assert!(registry.has_method("session.list"));
        assert!(registry.has_method("agent.list"));
        assert!(registry.has_method("health.get"));
        assert_eq!(registry.len(), 14);
    }

    // -- Dispatcher ---------------------------------------------------------

    #[tokio::test]
    async fn dispatch_ping() {
        let registry = builtin_registry(Duration::from_secs(30));
        let dispatcher = Dispatcher::new(registry);
        let ctx = test_ctx();

        let req = RequestFrame {
            id: "r1".into(),
            method: "ping".into(),
            params: json!({}),
        };

        let response = dispatcher.dispatch(&req, &ctx).await;
        match response {
            GatewayFrame::Response(res) => {
                assert!(res.ok);
                assert_eq!(res.id, "r1");
                assert_eq!(res.payload, Some(json!({"pong": true})));
            }
            _ => panic!("expected Response frame"),
        }
    }

    #[tokio::test]
    async fn dispatch_unknown_method() {
        let registry = builtin_registry(Duration::from_secs(30));
        let dispatcher = Dispatcher::new(registry);
        let ctx = test_ctx();

        let req = RequestFrame {
            id: "r2".into(),
            method: "nonexistent.method".into(),
            params: json!({}),
        };

        let response = dispatcher.dispatch(&req, &ctx).await;
        match response {
            GatewayFrame::Response(res) => {
                assert!(!res.ok);
                assert_eq!(res.id, "r2");
                let err = res.error.unwrap();
                assert_eq!(err.code, "INVALID_REQUEST");
                assert!(err.message.contains("unknown method"));
                assert!(err.message.contains("nonexistent.method"));
            }
            _ => panic!("expected Response frame"),
        }
    }

    #[tokio::test]
    async fn dispatch_stub_method_returns_unavailable() {
        let registry = builtin_registry(Duration::from_secs(30));
        let dispatcher = Dispatcher::new(registry);
        let ctx = test_ctx();

        let req = RequestFrame {
            id: "r3".into(),
            method: "chat.send".into(),
            params: json!({"text": "hello"}),
        };

        let response = dispatcher.dispatch(&req, &ctx).await;
        match response {
            GatewayFrame::Response(res) => {
                assert!(!res.ok);
                assert_eq!(res.id, "r3");
                let err = res.error.unwrap();
                assert_eq!(err.code, "UNAVAILABLE");
            }
            _ => panic!("expected Response frame"),
        }
    }

    #[tokio::test]
    async fn dispatch_timeout() {
        // Create a handler that sleeps longer than the timeout
        struct SlowHandler;

        #[async_trait]
        impl MethodHandler for SlowHandler {
            async fn handle(
                &self,
                _params: Value,
                _ctx: &MethodContext,
            ) -> Result<Value, ErrorShape> {
                tokio::time::sleep(Duration::from_secs(5)).await;
                Ok(json!({}))
            }
        }

        let mut registry = MethodRegistry::new(Duration::from_millis(50));
        registry.register("slow", Box::new(SlowHandler));
        let dispatcher = Dispatcher::new(registry);
        let ctx = test_ctx();

        let req = RequestFrame {
            id: "r4".into(),
            method: "slow".into(),
            params: json!({}),
        };

        let response = dispatcher.dispatch(&req, &ctx).await;
        match response {
            GatewayFrame::Response(res) => {
                assert!(!res.ok);
                assert_eq!(res.id, "r4");
                let err = res.error.unwrap();
                assert_eq!(err.code, "AGENT_TIMEOUT");
                assert_eq!(err.retryable, Some(true));
                assert!(err.message.contains("timed out"));
            }
            _ => panic!("expected Response frame"),
        }
    }

    #[tokio::test]
    async fn dispatch_handler_error_forwarded() {
        struct FailHandler;

        #[async_trait]
        impl MethodHandler for FailHandler {
            async fn handle(
                &self,
                _params: Value,
                _ctx: &MethodContext,
            ) -> Result<Value, ErrorShape> {
                Err(ErrorShape {
                    code: "CUSTOM_ERROR".into(),
                    message: "something broke".into(),
                    details: Some(json!({"field": "name"})),
                    retryable: None,
                    retry_after_ms: None,
                })
            }
        }

        let mut registry = MethodRegistry::new(Duration::from_secs(30));
        registry.register("fail", Box::new(FailHandler));
        let dispatcher = Dispatcher::new(registry);
        let ctx = test_ctx();

        let req = RequestFrame {
            id: "r5".into(),
            method: "fail".into(),
            params: json!({}),
        };

        let response = dispatcher.dispatch(&req, &ctx).await;
        match response {
            GatewayFrame::Response(res) => {
                assert!(!res.ok);
                let err = res.error.unwrap();
                assert_eq!(err.code, "CUSTOM_ERROR");
                assert_eq!(err.message, "something broke");
                assert_eq!(err.details, Some(json!({"field": "name"})));
            }
            _ => panic!("expected Response frame"),
        }
    }

    // -- Response serialization check ---------------------------------------

    #[tokio::test]
    async fn dispatch_response_serializes_correctly() {
        let registry = builtin_registry(Duration::from_secs(30));
        let dispatcher = Dispatcher::new(registry);
        let ctx = test_ctx();

        let req = RequestFrame {
            id: "s1".into(),
            method: "ping".into(),
            params: json!({}),
        };

        let response = dispatcher.dispatch(&req, &ctx).await;
        let json_str = serde_json::to_string(&response).unwrap();
        assert!(json_str.contains(r#""type":"res""#));
        assert!(json_str.contains(r#""ok":true"#));
        assert!(json_str.contains(r#""pong":true"#));
    }

    // -- Custom handler registration ----------------------------------------

    #[tokio::test]
    async fn register_custom_method() {
        struct CustomHandler;

        #[async_trait]
        impl MethodHandler for CustomHandler {
            async fn handle(
                &self,
                _params: Value,
                _ctx: &MethodContext,
            ) -> Result<Value, ErrorShape> {
                Ok(json!({"custom": true}))
            }
        }

        let mut registry = builtin_registry(Duration::from_secs(30));
        registry.register("plugin.custom", Box::new(CustomHandler));

        let dispatcher = Dispatcher::new(registry);
        let ctx = test_ctx();

        let req = RequestFrame {
            id: "c1".into(),
            method: "plugin.custom".into(),
            params: json!({}),
        };

        let response = dispatcher.dispatch(&req, &ctx).await;
        match response {
            GatewayFrame::Response(res) => {
                assert!(res.ok);
                assert_eq!(res.payload, Some(json!({"custom": true})));
            }
            _ => panic!("expected Response frame"),
        }
    }
}
