//! Handshake handler logic for gateway WebSocket connections.
//!
//! Processes incoming [`ConnectParams`], validates authentication and protocol
//! version, and produces either a [`HelloOk`] response or an [`ErrorShape`]
//! rejection. Also manages a thread-safe registry of active connections.

use std::collections::HashMap;
use std::sync::Arc;

use chrono::Utc;
use serde_json::json;
use tokio::sync::RwLock;
use uuid::Uuid;

use claw_core::{ErrorCode, ErrorShape, error_shape};

use crate::protocol::handshake::{
    ConnectParams, HelloOk, PresenceEntry, Snapshot, StateVersion,
};

// ---------------------------------------------------------------------------
// ConnId
// ---------------------------------------------------------------------------

/// Unique connection identifier, generated as a UUID v4 string.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ConnId(pub String);

impl ConnId {
    /// Generate a new random connection ID.
    pub fn new() -> Self {
        Self(Uuid::new_v4().to_string())
    }
}

impl std::fmt::Display for ConnId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

// ---------------------------------------------------------------------------
// AuthMode
// ---------------------------------------------------------------------------

/// Server-side authentication mode configuration.
#[derive(Debug, Clone)]
pub enum AuthMode {
    /// No authentication required.
    None,
    /// Token-based authentication.
    Token { token: String },
    /// Password-based authentication (plaintext comparison for now).
    Password { password: String },
    /// Trusted proxy — accept if proxy header is present.
    TrustedProxy,
}

// ---------------------------------------------------------------------------
// HandshakeConfig
// ---------------------------------------------------------------------------

/// Configuration for the handshake handler.
#[derive(Debug, Clone)]
pub struct HandshakeConfig {
    pub server_name: String,
    pub server_version: String,
    /// Inclusive range of protocol versions this server supports.
    pub min_protocol: u32,
    pub max_protocol: u32,
    pub auth_mode: AuthMode,
    pub features: Option<Vec<String>>,
}

// ---------------------------------------------------------------------------
// HandshakeResult
// ---------------------------------------------------------------------------

/// Outcome of processing a client handshake.
#[derive(Debug)]
pub enum HandshakeResult {
    /// Handshake succeeded — send HelloOk and track the connection.
    Success {
        hello: HelloOk,
        conn_id: ConnId,
        state: ConnectionState,
    },
    /// Handshake rejected — send error and close connection.
    Rejected(ErrorShape),
}

// ---------------------------------------------------------------------------
// ConnectionState
// ---------------------------------------------------------------------------

/// Tracked state for an active WebSocket connection.
#[derive(Debug, Clone)]
pub struct ConnectionState {
    pub conn_id: ConnId,
    pub client_name: Option<String>,
    pub client_type: Option<String>,
    pub connected_at: String,
    pub device: Option<String>,
    pub caps: Option<Vec<String>>,
}

impl ConnectionState {
    /// Convert to a [`PresenceEntry`] for inclusion in snapshots.
    pub fn to_presence_entry(&self) -> PresenceEntry {
        PresenceEntry {
            id: self.conn_id.0.clone(),
            name: self.client_name.clone(),
            client_type: self.client_type.clone(),
            connected_at: self.connected_at.clone(),
            device: self.device.clone(),
        }
    }
}

// ---------------------------------------------------------------------------
// handle_connect
// ---------------------------------------------------------------------------

/// Process an incoming handshake request.
///
/// Validates protocol version overlap and authentication, then produces
/// a `HandshakeResult::Success` with a `HelloOk` payload or a
/// `HandshakeResult::Rejected` with an `ErrorShape`.
pub fn handle_connect(
    params: &ConnectParams,
    config: &HandshakeConfig,
    registry: &ConnectionRegistry,
) -> HandshakeResult {
    // 1. Protocol version negotiation
    let negotiated = negotiate_protocol(
        params.min_protocol,
        params.max_protocol,
        config.min_protocol,
        config.max_protocol,
    );

    let protocol = match negotiated {
        Some(v) => v,
        None => {
            return HandshakeResult::Rejected(
                error_shape(
                    ErrorCode::InvalidRequest,
                    format!(
                        "protocol version mismatch: client [{}-{}], server [{}-{}]",
                        params.min_protocol,
                        params.max_protocol,
                        config.min_protocol,
                        config.max_protocol,
                    ),
                ),
            );
        }
    };

    // 2. Authentication
    if let Err(err) = validate_auth(&config.auth_mode, &params.auth) {
        return HandshakeResult::Rejected(err);
    }

    // 3. Build connection state
    let conn_id = ConnId::new();
    let now = Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);

    let state = ConnectionState {
        conn_id: conn_id.clone(),
        client_name: params.client_name.clone(),
        client_type: params.client_type.clone(),
        connected_at: now,
        device: params.device.clone(),
        caps: params.caps.clone(),
    };

    // 4. Build snapshot with current presence (blocking read is fine here
    //    since we hold no async context — callers should wrap in spawn_blocking
    //    if needed, but in practice this is called during handshake setup)
    let presence = registry.get_presence_entries_sync();

    let snapshot = Snapshot {
        presence,
        health: json!({}),
        state_version: StateVersion {
            presence: 0,
            health: 0,
        },
        uptime_ms: 0,
        config_path: None,
        state_dir: None,
    };

    let hello = HelloOk {
        protocol,
        server_name: config.server_name.clone(),
        server_version: config.server_version.clone(),
        features: config.features.clone(),
        snapshot,
        policy: None,
        auth: None,
    };

    HandshakeResult::Success {
        hello,
        conn_id,
        state,
    }
}

// ---------------------------------------------------------------------------
// Protocol negotiation
// ---------------------------------------------------------------------------

/// Find the highest protocol version both sides support, or `None` if
/// ranges don't overlap.
fn negotiate_protocol(
    client_min: u32,
    client_max: u32,
    server_min: u32,
    server_max: u32,
) -> Option<u32> {
    let lo = client_min.max(server_min);
    let hi = client_max.min(server_max);
    if lo <= hi { Some(hi) } else { None }
}

// ---------------------------------------------------------------------------
// Auth validation
// ---------------------------------------------------------------------------

/// Validate client authentication against the configured auth mode.
fn validate_auth(
    mode: &AuthMode,
    client_auth: &Option<serde_json::Value>,
) -> Result<(), ErrorShape> {
    match mode {
        AuthMode::None => Ok(()),

        AuthMode::Token { token: expected } => {
            let provided = client_auth
                .as_ref()
                .and_then(|v| v.get("token"))
                .and_then(|v| v.as_str());

            match provided {
                Some(t) if t == expected => Ok(()),
                Some(_) => Err(error_shape(
                    ErrorCode::InvalidRequest,
                    "invalid authentication token",
                )),
                None => Err(error_shape(
                    ErrorCode::InvalidRequest,
                    "authentication token required",
                )),
            }
        }

        AuthMode::Password { password: expected } => {
            let provided = client_auth
                .as_ref()
                .and_then(|v| v.get("password"))
                .and_then(|v| v.as_str());

            match provided {
                Some(p) if p == expected => Ok(()),
                Some(_) => Err(error_shape(
                    ErrorCode::InvalidRequest,
                    "invalid password",
                )),
                None => Err(error_shape(
                    ErrorCode::InvalidRequest,
                    "password required",
                )),
            }
        }

        AuthMode::TrustedProxy => {
            // In trusted proxy mode, the proxy itself is trusted.
            // Header validation happens at the HTTP layer before reaching
            // the handshake handler; here we accept unconditionally.
            Ok(())
        }
    }
}

// ---------------------------------------------------------------------------
// ConnectionRegistry
// ---------------------------------------------------------------------------

/// Thread-safe registry of active WebSocket connections.
///
/// Uses `Arc<RwLock<HashMap>>` so reads (presence queries) can proceed
/// concurrently while writes (connect/disconnect) are exclusive.
#[derive(Debug, Clone)]
pub struct ConnectionRegistry {
    connections: Arc<RwLock<HashMap<String, ConnectionState>>>,
}

impl ConnectionRegistry {
    /// Create an empty registry.
    pub fn new() -> Self {
        Self {
            connections: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Register a new connection. Returns the connection ID.
    pub async fn register(&self, state: ConnectionState) -> ConnId {
        let id = state.conn_id.clone();
        self.connections
            .write()
            .await
            .insert(id.0.clone(), state);
        id
    }

    /// Remove a connection by ID. Returns the state if it existed.
    pub async fn unregister(&self, conn_id: &ConnId) -> Option<ConnectionState> {
        self.connections.write().await.remove(&conn_id.0)
    }

    /// Get presence entries for all active connections.
    pub async fn get_presence_entries(&self) -> Vec<PresenceEntry> {
        self.connections
            .read()
            .await
            .values()
            .map(|s| s.to_presence_entry())
            .collect()
    }

    /// Synchronous read for use in non-async contexts (e.g. during handshake
    /// construction). Uses `try_read` and falls back to an empty list if the
    /// lock is contended.
    pub fn get_presence_entries_sync(&self) -> Vec<PresenceEntry> {
        match self.connections.try_read() {
            Ok(guard) => guard.values().map(|s| s.to_presence_entry()).collect(),
            Err(_) => Vec::new(),
        }
    }

    /// Number of active connections.
    pub async fn len(&self) -> usize {
        self.connections.read().await.len()
    }

    /// Whether the registry is empty.
    pub async fn is_empty(&self) -> bool {
        self.connections.read().await.is_empty()
    }
}

impl Default for ConnectionRegistry {
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
    use serde_json::json;

    fn test_config(auth_mode: AuthMode) -> HandshakeConfig {
        HandshakeConfig {
            server_name: "claw-test".into(),
            server_version: "0.1.0".into(),
            min_protocol: 1,
            max_protocol: 1,
            auth_mode,
            features: Some(vec!["streaming".into()]),
        }
    }

    fn test_params() -> ConnectParams {
        ConnectParams {
            min_protocol: 1,
            max_protocol: 1,
            client_name: Some("Test Client".into()),
            client_version: Some("0.1.0".into()),
            client_type: Some("test".into()),
            caps: Some(vec!["streaming".into()]),
            auth: None,
            device: Some("test-device".into()),
        }
    }

    // -- Protocol negotiation -----------------------------------------------

    #[test]
    fn negotiate_exact_match() {
        assert_eq!(negotiate_protocol(1, 1, 1, 1), Some(1));
    }

    #[test]
    fn negotiate_overlapping_ranges() {
        // Client [1-3], Server [2-5] → highest overlap = 3
        assert_eq!(negotiate_protocol(1, 3, 2, 5), Some(3));
    }

    #[test]
    fn negotiate_no_overlap() {
        // Client [1-2], Server [3-4] → no overlap
        assert_eq!(negotiate_protocol(1, 2, 3, 4), None);
    }

    #[test]
    fn negotiate_server_higher() {
        // Client [1-1], Server [2-3] → no overlap
        assert_eq!(negotiate_protocol(1, 1, 2, 3), None);
    }

    #[test]
    fn negotiate_picks_highest_common() {
        // Client [1-5], Server [1-3] → 3
        assert_eq!(negotiate_protocol(1, 5, 1, 3), Some(3));
    }

    // -- Handshake with no auth ---------------------------------------------

    #[test]
    fn handshake_no_auth_success() {
        let config = test_config(AuthMode::None);
        let params = test_params();
        let registry = ConnectionRegistry::new();

        match handle_connect(&params, &config, &registry) {
            HandshakeResult::Success { hello, conn_id, state } => {
                assert_eq!(hello.protocol, 1);
                assert_eq!(hello.server_name, "claw-test");
                assert_eq!(hello.server_version, "0.1.0");
                assert_eq!(hello.features, Some(vec!["streaming".into()]));
                assert!(!conn_id.0.is_empty());
                assert_eq!(state.client_name, Some("Test Client".into()));
                assert_eq!(state.device, Some("test-device".into()));
            }
            HandshakeResult::Rejected(err) => {
                panic!("expected success, got rejection: {}", err.message);
            }
        }
    }

    // -- Handshake with token auth ------------------------------------------

    #[test]
    fn handshake_token_auth_success() {
        let config = test_config(AuthMode::Token {
            token: "secret-123".into(),
        });
        let mut params = test_params();
        params.auth = Some(json!({"token": "secret-123"}));
        let registry = ConnectionRegistry::new();

        match handle_connect(&params, &config, &registry) {
            HandshakeResult::Success { hello, .. } => {
                assert_eq!(hello.protocol, 1);
            }
            HandshakeResult::Rejected(err) => {
                panic!("expected success, got rejection: {}", err.message);
            }
        }
    }

    #[test]
    fn handshake_token_auth_wrong_token() {
        let config = test_config(AuthMode::Token {
            token: "secret-123".into(),
        });
        let mut params = test_params();
        params.auth = Some(json!({"token": "wrong-token"}));
        let registry = ConnectionRegistry::new();

        match handle_connect(&params, &config, &registry) {
            HandshakeResult::Success { .. } => {
                panic!("expected rejection for wrong token");
            }
            HandshakeResult::Rejected(err) => {
                assert_eq!(err.code, "INVALID_REQUEST");
                assert!(err.message.contains("invalid"));
            }
        }
    }

    #[test]
    fn handshake_token_auth_missing() {
        let config = test_config(AuthMode::Token {
            token: "secret-123".into(),
        });
        let params = test_params(); // no auth field
        let registry = ConnectionRegistry::new();

        match handle_connect(&params, &config, &registry) {
            HandshakeResult::Success { .. } => {
                panic!("expected rejection for missing token");
            }
            HandshakeResult::Rejected(err) => {
                assert_eq!(err.code, "INVALID_REQUEST");
                assert!(err.message.contains("required"));
            }
        }
    }

    // -- Handshake with password auth ---------------------------------------

    #[test]
    fn handshake_password_auth_success() {
        let config = test_config(AuthMode::Password {
            password: "hunter2".into(),
        });
        let mut params = test_params();
        params.auth = Some(json!({"password": "hunter2"}));
        let registry = ConnectionRegistry::new();

        match handle_connect(&params, &config, &registry) {
            HandshakeResult::Success { .. } => {}
            HandshakeResult::Rejected(err) => {
                panic!("expected success, got rejection: {}", err.message);
            }
        }
    }

    #[test]
    fn handshake_password_auth_wrong() {
        let config = test_config(AuthMode::Password {
            password: "hunter2".into(),
        });
        let mut params = test_params();
        params.auth = Some(json!({"password": "wrong"}));
        let registry = ConnectionRegistry::new();

        match handle_connect(&params, &config, &registry) {
            HandshakeResult::Rejected(err) => {
                assert!(err.message.contains("invalid password"));
            }
            HandshakeResult::Success { .. } => {
                panic!("expected rejection");
            }
        }
    }

    // -- Protocol version mismatch ------------------------------------------

    #[test]
    fn handshake_protocol_mismatch() {
        let config = test_config(AuthMode::None);
        let mut params = test_params();
        params.min_protocol = 5;
        params.max_protocol = 6;
        let registry = ConnectionRegistry::new();

        match handle_connect(&params, &config, &registry) {
            HandshakeResult::Rejected(err) => {
                assert_eq!(err.code, "INVALID_REQUEST");
                assert!(err.message.contains("protocol version mismatch"));
            }
            HandshakeResult::Success { .. } => {
                panic!("expected rejection for protocol mismatch");
            }
        }
    }

    // -- HelloOk response correctness --------------------------------------

    #[test]
    fn hello_ok_contains_correct_server_info() {
        let config = HandshakeConfig {
            server_name: "my-server".into(),
            server_version: "2.0.0".into(),
            min_protocol: 1,
            max_protocol: 3,
            auth_mode: AuthMode::None,
            features: Some(vec!["streaming".into(), "presence".into()]),
        };
        let mut params = test_params();
        params.min_protocol = 2;
        params.max_protocol = 4;
        let registry = ConnectionRegistry::new();

        match handle_connect(&params, &config, &registry) {
            HandshakeResult::Success { hello, .. } => {
                assert_eq!(hello.protocol, 3); // min(4,3) = 3
                assert_eq!(hello.server_name, "my-server");
                assert_eq!(hello.server_version, "2.0.0");
                assert_eq!(
                    hello.features,
                    Some(vec!["streaming".into(), "presence".into()])
                );
                assert!(hello.snapshot.presence.is_empty());
            }
            HandshakeResult::Rejected(err) => {
                panic!("expected success: {}", err.message);
            }
        }
    }

    // -- ConnectionRegistry -------------------------------------------------

    #[tokio::test]
    async fn registry_register_and_list() {
        let registry = ConnectionRegistry::new();
        assert!(registry.is_empty().await);

        let state = ConnectionState {
            conn_id: ConnId("test-1".into()),
            client_name: Some("Client A".into()),
            client_type: Some("web".into()),
            connected_at: "2026-01-01T00:00:00Z".into(),
            device: None,
            caps: None,
        };

        registry.register(state).await;
        assert_eq!(registry.len().await, 1);

        let entries = registry.get_presence_entries().await;
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].id, "test-1");
        assert_eq!(entries[0].name, Some("Client A".into()));
    }

    #[tokio::test]
    async fn registry_unregister() {
        let registry = ConnectionRegistry::new();

        let state = ConnectionState {
            conn_id: ConnId("test-2".into()),
            client_name: None,
            client_type: None,
            connected_at: "2026-01-01T00:00:00Z".into(),
            device: None,
            caps: None,
        };

        registry.register(state).await;
        assert_eq!(registry.len().await, 1);

        let removed = registry.unregister(&ConnId("test-2".into())).await;
        assert!(removed.is_some());
        assert!(registry.is_empty().await);
    }

    #[tokio::test]
    async fn registry_unregister_nonexistent() {
        let registry = ConnectionRegistry::new();
        let removed = registry.unregister(&ConnId("nope".into())).await;
        assert!(removed.is_none());
    }

    #[tokio::test]
    async fn registry_multiple_connections() {
        let registry = ConnectionRegistry::new();

        for i in 0..3 {
            let state = ConnectionState {
                conn_id: ConnId(format!("conn-{i}")),
                client_name: Some(format!("Client {i}")),
                client_type: None,
                connected_at: "2026-01-01T00:00:00Z".into(),
                device: None,
                caps: None,
            };
            registry.register(state).await;
        }

        assert_eq!(registry.len().await, 3);

        let entries = registry.get_presence_entries().await;
        assert_eq!(entries.len(), 3);

        // Remove middle one
        registry.unregister(&ConnId("conn-1".into())).await;
        assert_eq!(registry.len().await, 2);
    }

    // -- ConnId generation --------------------------------------------------

    #[test]
    fn conn_id_generates_unique_values() {
        let a = ConnId::new();
        let b = ConnId::new();
        assert_ne!(a, b);
        // UUID v4 format: 8-4-4-4-12 hex digits
        assert_eq!(a.0.len(), 36);
    }

    #[test]
    fn conn_id_display() {
        let id = ConnId("abc-123".into());
        assert_eq!(format!("{id}"), "abc-123");
    }

    // -- ConnectionState to PresenceEntry -----------------------------------

    #[test]
    fn connection_state_to_presence_entry() {
        let state = ConnectionState {
            conn_id: ConnId("c1".into()),
            client_name: Some("Web".into()),
            client_type: Some("web".into()),
            connected_at: "2026-01-01T00:00:00Z".into(),
            device: Some("laptop".into()),
            caps: Some(vec!["streaming".into()]),
        };

        let entry = state.to_presence_entry();
        assert_eq!(entry.id, "c1");
        assert_eq!(entry.name, Some("Web".into()));
        assert_eq!(entry.client_type, Some("web".into()));
        assert_eq!(entry.device, Some("laptop".into()));
    }

    // -- Trusted proxy auth -------------------------------------------------

    #[test]
    fn handshake_trusted_proxy_accepts() {
        let config = test_config(AuthMode::TrustedProxy);
        let params = test_params();
        let registry = ConnectionRegistry::new();

        match handle_connect(&params, &config, &registry) {
            HandshakeResult::Success { .. } => {}
            HandshakeResult::Rejected(err) => {
                panic!("expected success: {}", err.message);
            }
        }
    }
}
