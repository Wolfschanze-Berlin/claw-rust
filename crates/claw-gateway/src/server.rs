//! Gateway HTTP + WebSocket server.
//!
//! Provides `GatewayServer` and `start_gateway_server()` to run the combined
//! HTTP + WS gateway on a single port using actix-web and actix-ws.

use std::sync::Arc;
use std::time::{Duration, Instant};

use actix_web::web::{self, Data, Payload};
use actix_web::{App, HttpRequest, HttpResponse, HttpServer, middleware};
use futures_util::StreamExt;
use serde::Serialize;
use tokio::sync::Notify;
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};

use crate::dispatch::{Dispatcher, MethodContext, MethodRegistry, builtin_registry};
use crate::handshake::{
    AuthMode, ConnectionRegistry, ConnId, HandshakeConfig, HandshakeResult, handle_connect,
};
use crate::protocol::handshake::ConnectParams;

// ---------------------------------------------------------------------------
// GatewayBindMode
// ---------------------------------------------------------------------------

/// Determines which network interface the gateway listens on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GatewayBindMode {
    /// Auto-detect: use loopback in dev, LAN in production.
    Auto,
    /// Bind to LAN interfaces (0.0.0.0).
    Lan,
    /// Bind to loopback only (127.0.0.1).
    Loopback,
    /// Bind to a custom host address.
    Custom(String),
    /// Bind for Tailscale network access.
    Tailnet,
}

impl GatewayBindMode {
    /// Resolve the bind address string for actix-web.
    pub fn bind_addr(&self) -> String {
        match self {
            Self::Auto | Self::Loopback => "127.0.0.1".into(),
            Self::Lan | Self::Tailnet => "0.0.0.0".into(),
            Self::Custom(addr) => addr.clone(),
        }
    }
}

// ---------------------------------------------------------------------------
// GatewayServerOptions
// ---------------------------------------------------------------------------

/// Options for configuring the gateway server.
#[derive(Debug)]
pub struct GatewayServerOptions {
    pub port: u16,
    pub bind_mode: GatewayBindMode,
    pub server_name: String,
    pub server_version: String,
    pub auth_mode: AuthMode,
    pub features: Option<Vec<String>>,
    pub control_ui_enabled: bool,
    /// Custom method registry. If `None`, uses [`builtin_registry`] with a
    /// 30-second default timeout.
    pub method_registry: Option<MethodRegistry>,
}

impl Default for GatewayServerOptions {
    fn default() -> Self {
        Self {
            port: 18789,
            bind_mode: GatewayBindMode::Loopback,
            server_name: "claw-rust".into(),
            server_version: env!("CARGO_PKG_VERSION").into(),
            auth_mode: AuthMode::None,
            features: None,
            control_ui_enabled: false,
            method_registry: None,
        }
    }
}

// ---------------------------------------------------------------------------
// Shared server state
// ---------------------------------------------------------------------------

/// Internal state shared across all request handlers via actix-web `Data`.
pub struct GatewayState {
    pub registry: ConnectionRegistry,
    pub handshake_config: HandshakeConfig,
    pub dispatcher: Dispatcher,
    pub start_time: Instant,
    pub cancel: CancellationToken,
    pub server_name: String,
    pub server_version: String,
}

// ---------------------------------------------------------------------------
// GatewayServer handle
// ---------------------------------------------------------------------------

/// Handle to a running gateway server. Call [`close`] to shut down.
pub struct GatewayServer {
    cancel: CancellationToken,
    shutdown_complete: Arc<Notify>,
    pub port: u16,
    pub bind_addr: String,
}

impl GatewayServer {
    /// Gracefully shut down the server.
    pub async fn close(&self, reason: &str) {
        info!(reason, "gateway server shutting down");
        self.cancel.cancel();
        self.shutdown_complete.notified().await;
    }
}

// ---------------------------------------------------------------------------
// start_gateway_server
// ---------------------------------------------------------------------------

/// Start the gateway HTTP + WebSocket server.
///
/// Returns a [`GatewayServer`] handle that can be used to shut down the
/// server gracefully.
pub async fn start_gateway_server(
    opts: GatewayServerOptions,
) -> std::io::Result<GatewayServer> {
    let cancel = CancellationToken::new();
    let shutdown_complete = Arc::new(Notify::new());

    let bind_addr = opts.bind_mode.bind_addr();
    let full_addr = format!("{bind_addr}:{}", opts.port);

    let handshake_config = HandshakeConfig {
        server_name: opts.server_name.clone(),
        server_version: opts.server_version.clone(),
        min_protocol: 1,
        max_protocol: 1,
        auth_mode: opts.auth_mode.clone(),
        features: opts.features.clone(),
    };

    let registry = opts
        .method_registry
        .unwrap_or_else(|| builtin_registry(Duration::from_secs(30)));
    let dispatcher = Dispatcher::new(registry);

    let state = Data::new(GatewayState {
        registry: ConnectionRegistry::new(),
        handshake_config,
        dispatcher,
        start_time: Instant::now(),
        cancel: cancel.clone(),
        server_name: opts.server_name.clone(),
        server_version: opts.server_version.clone(),
    });

    let server = HttpServer::new(move || {
        App::new()
            .app_data(state.clone())
            .wrap(middleware::Logger::default())
            .route("/health", web::get().to(health_handler))
            .route("/status", web::get().to(status_handler))
            .route("/ws", web::get().to(ws_handler))
    })
    .bind(&full_addr)?
    .disable_signals()
    .run();

    let server_handle = server.handle();
    let sc = shutdown_complete.clone();
    let cancel_clone = cancel.clone();

    // Spawn the server task (Server is Send, so tokio::spawn works)
    tokio::spawn(async move {
        tokio::select! {
            result = server => {
                if let Err(e) = result {
                    warn!(error = %e, "gateway server error");
                }
            }
            _ = cancel_clone.cancelled() => {
                info!("shutdown signal received, stopping server");
                server_handle.stop(true).await;
            }
        }
        sc.notify_one();
    });

    info!(addr = %full_addr, "gateway server started");

    Ok(GatewayServer {
        cancel,
        shutdown_complete,
        port: opts.port,
        bind_addr,
    })
}

// ---------------------------------------------------------------------------
// HTTP handlers
// ---------------------------------------------------------------------------

/// Health check endpoint — returns 200 with `{"status": "ok"}`.
async fn health_handler() -> HttpResponse {
    HttpResponse::Ok().json(HealthResponse { status: "ok" })
}

#[derive(Serialize)]
struct HealthResponse {
    status: &'static str,
}

/// Status endpoint — returns server info and uptime.
async fn status_handler(state: Data<GatewayState>) -> HttpResponse {
    let uptime_ms = state.start_time.elapsed().as_millis() as u64;
    let connections = state.registry.get_presence_entries_sync().len();

    HttpResponse::Ok().json(StatusResponse {
        server_name: &state.server_name,
        server_version: &state.server_version,
        uptime_ms,
        connections,
    })
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct StatusResponse<'a> {
    server_name: &'a str,
    server_version: &'a str,
    uptime_ms: u64,
    connections: usize,
}

// ---------------------------------------------------------------------------
// WebSocket handler
// ---------------------------------------------------------------------------

/// WebSocket upgrade handler at `/ws`.
///
/// Performs the HTTP → WS upgrade, then spawns a task to handle the
/// connection lifecycle: handshake → message loop → cleanup.
async fn ws_handler(
    req: HttpRequest,
    body: Payload,
    state: Data<GatewayState>,
) -> actix_web::Result<HttpResponse> {
    let (response, mut session, msg_stream) = actix_ws::handle(&req, body)?;

    let cancel = state.cancel.clone();
    let registry = state.registry.clone();
    let handshake_config = state.handshake_config.clone();
    let dispatcher = state.dispatcher.clone();

    actix_web::rt::spawn(async move {
        use actix_ws::AggregatedMessage;

        // Aggregate fragmented messages for easier handling
        let mut stream = msg_stream.aggregate_continuations().max_continuation_size(64 * 1024);

        // Wait for the first message (should be ConnectParams)
        let conn_id: Option<ConnId> = loop {
            tokio::select! {
                _ = cancel.cancelled() => {
                    let _ = session.close(Some(actix_ws::CloseReason {
                        code: actix_ws::CloseCode::Away,
                        description: Some("server shutting down".into()),
                    })).await;
                    return;
                }
                msg = stream.next() => {
                    match msg {
                        Some(Ok(AggregatedMessage::Text(text))) => {
                            match serde_json::from_str::<ConnectParams>(&text) {
                                Ok(params) => {
                                    match handle_connect(&params, &handshake_config, &registry) {
                                        HandshakeResult::Success { hello, conn_id, state: conn_state } => {
                                            registry.register(conn_state).await;

                                            let frame = crate::protocol::frames::ResponseFrame::ok(
                                                "handshake",
                                                serde_json::to_value(&hello).unwrap_or_default(),
                                            );
                                            let envelope = crate::protocol::frames::GatewayFrame::Response(frame);
                                            let json = serde_json::to_string(&envelope).unwrap_or_default();
                                            let _ = session.text(json).await;

                                            break Some(conn_id);
                                        }
                                        HandshakeResult::Rejected(err) => {
                                            let frame = crate::protocol::frames::ResponseFrame::err(
                                                "handshake",
                                                err,
                                            );
                                            let envelope = crate::protocol::frames::GatewayFrame::Response(frame);
                                            let json = serde_json::to_string(&envelope).unwrap_or_default();
                                            let _ = session.text(json).await;
                                            let _ = session.close(None).await;
                                            return;
                                        }
                                    }
                                }
                                Err(e) => {
                                    let err = claw_core::error_shape(
                                        claw_core::ErrorCode::InvalidRequest,
                                        format!("invalid handshake: {e}"),
                                    );
                                    let frame = crate::protocol::frames::ResponseFrame::err("handshake", err);
                                    let envelope = crate::protocol::frames::GatewayFrame::Response(frame);
                                    let json = serde_json::to_string(&envelope).unwrap_or_default();
                                    let _ = session.text(json).await;
                                    let _ = session.close(None).await;
                                    return;
                                }
                            }
                        }
                        Some(Ok(AggregatedMessage::Close(_))) | None => return,
                        Some(Ok(AggregatedMessage::Ping(data))) => {
                            let _ = session.pong(&data).await;
                            continue;
                        }
                        _ => continue,
                    }
                }
            }
        };

        let conn_id = match conn_id {
            Some(id) => id,
            None => return,
        };

        // Post-handshake message loop with RPC dispatch
        let method_ctx = MethodContext {
            conn_id: conn_id.to_string(),
        };

        loop {
            tokio::select! {
                _ = cancel.cancelled() => {
                    let _ = session.close(Some(actix_ws::CloseReason {
                        code: actix_ws::CloseCode::Away,
                        description: Some("server shutting down".into()),
                    })).await;
                    break;
                }
                msg = stream.next() => {
                    match msg {
                        Some(Ok(AggregatedMessage::Text(text))) => {
                            // Parse as RequestFrame and dispatch
                            match serde_json::from_str::<crate::protocol::frames::RequestFrame>(&text) {
                                Ok(req_frame) => {
                                    let response = dispatcher.dispatch(&req_frame, &method_ctx).await;
                                    let json = serde_json::to_string(&response).unwrap_or_default();
                                    let _ = session.text(json).await;
                                }
                                Err(e) => {
                                    let err = claw_core::error_shape(
                                        claw_core::ErrorCode::InvalidRequest,
                                        format!("invalid request frame: {e}"),
                                    );
                                    let frame = crate::protocol::frames::ResponseFrame::err("unknown", err);
                                    let envelope = crate::protocol::frames::GatewayFrame::Response(frame);
                                    let json = serde_json::to_string(&envelope).unwrap_or_default();
                                    let _ = session.text(json).await;
                                }
                            }
                        }
                        Some(Ok(AggregatedMessage::Ping(data))) => {
                            let _ = session.pong(&data).await;
                        }
                        Some(Ok(AggregatedMessage::Close(_))) | None => break,
                        _ => continue,
                    }
                }
            }
        }

        // Cleanup: remove from registry
        registry.unregister(&conn_id).await;
    });

    Ok(response)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bind_mode_loopback_addr() {
        assert_eq!(GatewayBindMode::Loopback.bind_addr(), "127.0.0.1");
    }

    #[test]
    fn bind_mode_lan_addr() {
        assert_eq!(GatewayBindMode::Lan.bind_addr(), "0.0.0.0");
    }

    #[test]
    fn bind_mode_auto_defaults_to_loopback() {
        assert_eq!(GatewayBindMode::Auto.bind_addr(), "127.0.0.1");
    }

    #[test]
    fn bind_mode_custom_addr() {
        let mode = GatewayBindMode::Custom("192.168.1.100".into());
        assert_eq!(mode.bind_addr(), "192.168.1.100");
    }

    #[test]
    fn bind_mode_tailnet_addr() {
        assert_eq!(GatewayBindMode::Tailnet.bind_addr(), "0.0.0.0");
    }

    #[test]
    fn default_options() {
        let opts = GatewayServerOptions::default();
        assert_eq!(opts.port, 18789);
        assert_eq!(opts.bind_mode, GatewayBindMode::Loopback);
        assert_eq!(opts.server_name, "claw-rust");
        assert!(!opts.control_ui_enabled);
    }

    #[test]
    fn health_response_serialization() {
        let resp = HealthResponse { status: "ok" };
        let json = serde_json::to_string(&resp).unwrap();
        assert_eq!(json, r#"{"status":"ok"}"#);
    }

    #[test]
    fn status_response_serialization() {
        let resp = StatusResponse {
            server_name: "claw-rust",
            server_version: "0.1.0",
            uptime_ms: 12345,
            connections: 3,
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains(r#""serverName":"claw-rust""#));
        assert!(json.contains(r#""uptimeMs":12345"#));
        assert!(json.contains(r#""connections":3"#));
    }

    #[actix_web::test]
    async fn server_starts_and_stops() {
        let opts = GatewayServerOptions {
            port: 19876, // use a non-default port to avoid conflicts
            ..Default::default()
        };

        let server = start_gateway_server(opts).await.expect("server should start");
        assert_eq!(server.port, 19876);
        assert_eq!(server.bind_addr, "127.0.0.1");

        // Give server a moment to be ready
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        // Verify health endpoint
        let client = reqwest::Client::new();
        let resp = client
            .get("http://127.0.0.1:19876/health")
            .send()
            .await
            .expect("health request should succeed");
        assert_eq!(resp.status(), 200);
        let body: serde_json::Value = resp.json().await.unwrap();
        assert_eq!(body["status"], "ok");

        // Verify status endpoint
        let resp = client
            .get("http://127.0.0.1:19876/status")
            .send()
            .await
            .expect("status request should succeed");
        assert_eq!(resp.status(), 200);
        let body: serde_json::Value = resp.json().await.unwrap();
        assert_eq!(body["serverName"], "claw-rust");
        assert!(body["uptimeMs"].as_u64().unwrap() > 0);

        // Graceful shutdown
        server.close("test complete").await;
    }

    #[actix_web::test]
    async fn ws_handshake_integration() {
        let opts = GatewayServerOptions {
            port: 19877,
            ..Default::default()
        };

        let server = start_gateway_server(opts).await.expect("server should start");
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        // Connect via WebSocket
        let (mut ws, _resp) = tokio_tungstenite::connect_async("ws://127.0.0.1:19877/ws")
            .await
            .expect("ws connect should succeed");

        use futures_util::{SinkExt, StreamExt};
        use tokio_tungstenite::tungstenite::Message;

        // Send handshake
        let connect_params = serde_json::json!({
            "minProtocol": 1,
            "maxProtocol": 1,
            "clientName": "Test",
            "clientType": "test"
        });
        ws.send(Message::Text(connect_params.to_string().into()))
            .await
            .expect("send should succeed");

        // Read response
        let msg = ws.next().await.expect("should get response").expect("should be ok");
        let text = msg.into_text().expect("should be text");
        let frame: serde_json::Value = serde_json::from_str(&text).unwrap();

        assert_eq!(frame["type"], "res");
        assert_eq!(frame["ok"], true);
        assert_eq!(frame["payload"]["serverName"], "claw-rust");
        assert_eq!(frame["payload"]["protocol"], 1);

        // Clean close
        ws.close(None).await.ok();
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        server.close("test complete").await;
    }

    #[actix_web::test]
    async fn ws_rpc_dispatch_integration() {
        let opts = GatewayServerOptions {
            port: 19878,
            ..Default::default()
        };

        let server = start_gateway_server(opts).await.expect("server should start");
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        let (mut ws, _resp) = tokio_tungstenite::connect_async("ws://127.0.0.1:19878/ws")
            .await
            .expect("ws connect should succeed");

        use futures_util::{SinkExt, StreamExt};
        use tokio_tungstenite::tungstenite::Message;

        // 1. Handshake first
        let connect_params = serde_json::json!({
            "minProtocol": 1,
            "maxProtocol": 1,
            "clientName": "RPC Test"
        });
        ws.send(Message::Text(connect_params.to_string().into()))
            .await
            .unwrap();
        let _hello = ws.next().await.unwrap().unwrap(); // consume HelloOk

        // 2. Send ping RPC
        let ping_req = serde_json::json!({
            "id": "ping-1",
            "method": "ping",
            "params": {}
        });
        ws.send(Message::Text(ping_req.to_string().into()))
            .await
            .unwrap();

        let msg = ws.next().await.unwrap().unwrap();
        let text = msg.into_text().unwrap();
        let frame: serde_json::Value = serde_json::from_str(&text).unwrap();

        assert_eq!(frame["type"], "res");
        assert_eq!(frame["ok"], true);
        assert_eq!(frame["id"], "ping-1");
        assert_eq!(frame["payload"]["pong"], true);

        // 3. Send unknown method
        let unknown_req = serde_json::json!({
            "id": "unk-1",
            "method": "does.not.exist",
            "params": {}
        });
        ws.send(Message::Text(unknown_req.to_string().into()))
            .await
            .unwrap();

        let msg = ws.next().await.unwrap().unwrap();
        let text = msg.into_text().unwrap();
        let frame: serde_json::Value = serde_json::from_str(&text).unwrap();

        assert_eq!(frame["type"], "res");
        assert_eq!(frame["ok"], false);
        assert_eq!(frame["id"], "unk-1");
        assert_eq!(frame["error"]["code"], "INVALID_REQUEST");

        // 4. Send stub method (chat.send)
        let stub_req = serde_json::json!({
            "id": "stub-1",
            "method": "chat.send",
            "params": {"text": "hello"}
        });
        ws.send(Message::Text(stub_req.to_string().into()))
            .await
            .unwrap();

        let msg = ws.next().await.unwrap().unwrap();
        let text = msg.into_text().unwrap();
        let frame: serde_json::Value = serde_json::from_str(&text).unwrap();

        assert_eq!(frame["type"], "res");
        assert_eq!(frame["ok"], false);
        assert_eq!(frame["error"]["code"], "UNAVAILABLE");

        ws.close(None).await.ok();
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        server.close("test complete").await;
    }
}
