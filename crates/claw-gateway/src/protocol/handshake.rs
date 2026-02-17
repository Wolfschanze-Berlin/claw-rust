//! Handshake types for the gateway protocol.
//!
//! Covers the initial connection negotiation: [`ConnectParams`] sent by
//! clients and [`HelloOk`] returned by the server, along with supporting
//! types like [`Snapshot`], [`StateVersion`], and [`PresenceEntry`].

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// StateVersion
// ---------------------------------------------------------------------------

/// Monotonic version counters for presence and health state.
///
/// Clients use these to detect stale data and request incremental updates.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct StateVersion {
    pub presence: u64,
    pub health: u64,
}

// ---------------------------------------------------------------------------
// PresenceEntry
// ---------------------------------------------------------------------------

/// Information about a connected client, included in presence snapshots.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PresenceEntry {
    /// Unique connection identifier.
    pub id: String,

    /// Human-readable client name (e.g. "Web UI", "CLI").
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,

    /// Client type identifier.
    #[serde(rename = "clientType", skip_serializing_if = "Option::is_none")]
    pub client_type: Option<String>,

    /// ISO 8601 timestamp when the client connected.
    #[serde(rename = "connectedAt")]
    pub connected_at: String,

    /// Device identifier provided during handshake.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub device: Option<String>,
}

// ---------------------------------------------------------------------------
// Snapshot
// ---------------------------------------------------------------------------

/// Server state snapshot delivered during handshake.
///
/// Gives newly connected clients a complete picture of current state.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Snapshot {
    pub presence: Vec<PresenceEntry>,

    pub health: serde_json::Value,

    #[serde(rename = "stateVersion")]
    pub state_version: StateVersion,

    #[serde(rename = "uptimeMs")]
    pub uptime_ms: u64,

    #[serde(rename = "configPath", skip_serializing_if = "Option::is_none")]
    pub config_path: Option<String>,

    #[serde(rename = "stateDir", skip_serializing_if = "Option::is_none")]
    pub state_dir: Option<String>,
}

// ---------------------------------------------------------------------------
// ConnectParams
// ---------------------------------------------------------------------------

/// Parameters sent by a client during the WebSocket handshake.
///
/// ```json
/// {
///   "minProtocol": 1,
///   "maxProtocol": 1,
///   "clientName": "Web UI",
///   "clientVersion": "0.1.0",
///   "clientType": "web",
///   "caps": ["streaming"],
///   "auth": { "token": "abc" },
///   "device": "macbook-pro"
/// }
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ConnectParams {
    #[serde(rename = "minProtocol")]
    pub min_protocol: u32,

    #[serde(rename = "maxProtocol")]
    pub max_protocol: u32,

    #[serde(rename = "clientName", skip_serializing_if = "Option::is_none")]
    pub client_name: Option<String>,

    #[serde(rename = "clientVersion", skip_serializing_if = "Option::is_none")]
    pub client_version: Option<String>,

    #[serde(rename = "clientType", skip_serializing_if = "Option::is_none")]
    pub client_type: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub caps: Option<Vec<String>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub auth: Option<serde_json::Value>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub device: Option<String>,
}

// ---------------------------------------------------------------------------
// HelloOk
// ---------------------------------------------------------------------------

/// Successful handshake response from the server.
///
/// ```json
/// {
///   "protocol": 1,
///   "serverName": "claw-rust",
///   "serverVersion": "0.1.0",
///   "features": ["streaming"],
///   "snapshot": { ... },
///   "policy": { ... },
///   "auth": { ... }
/// }
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HelloOk {
    pub protocol: u32,

    #[serde(rename = "serverName")]
    pub server_name: String,

    #[serde(rename = "serverVersion")]
    pub server_version: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub features: Option<Vec<String>>,

    pub snapshot: Snapshot,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub policy: Option<serde_json::Value>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub auth: Option<serde_json::Value>,
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // -- StateVersion -----------------------------------------------------

    #[test]
    fn state_version_serialization() {
        let sv = StateVersion {
            presence: 10,
            health: 5,
        };
        let json_str = serde_json::to_string(&sv).unwrap();
        assert_eq!(json_str, r#"{"presence":10,"health":5}"#);
    }

    #[test]
    fn state_version_roundtrip() {
        let sv = StateVersion {
            presence: 42,
            health: 99,
        };
        let json_str = serde_json::to_string(&sv).unwrap();
        let decoded: StateVersion = serde_json::from_str(&json_str).unwrap();
        assert_eq!(sv, decoded);
    }

    // -- PresenceEntry ----------------------------------------------------

    #[test]
    fn presence_entry_full_serialization() {
        let entry = PresenceEntry {
            id: "conn-1".into(),
            name: Some("Web UI".into()),
            client_type: Some("web".into()),
            connected_at: "2026-01-01T00:00:00Z".into(),
            device: Some("macbook".into()),
        };
        let json_str = serde_json::to_string(&entry).unwrap();
        assert_eq!(
            json_str,
            r#"{"id":"conn-1","name":"Web UI","clientType":"web","connectedAt":"2026-01-01T00:00:00Z","device":"macbook"}"#
        );
    }

    #[test]
    fn presence_entry_minimal_serialization() {
        let entry = PresenceEntry {
            id: "conn-2".into(),
            name: None,
            client_type: None,
            connected_at: "2026-01-01T00:00:00Z".into(),
            device: None,
        };
        let json_str = serde_json::to_string(&entry).unwrap();
        assert_eq!(
            json_str,
            r#"{"id":"conn-2","connectedAt":"2026-01-01T00:00:00Z"}"#
        );
    }

    #[test]
    fn presence_entry_roundtrip() {
        let entry = PresenceEntry {
            id: "conn-3".into(),
            name: Some("CLI".into()),
            client_type: Some("cli".into()),
            connected_at: "2026-02-17T12:00:00Z".into(),
            device: None,
        };
        let json_str = serde_json::to_string(&entry).unwrap();
        let decoded: PresenceEntry = serde_json::from_str(&json_str).unwrap();
        assert_eq!(entry, decoded);
    }

    // -- Snapshot ---------------------------------------------------------

    #[test]
    fn snapshot_serialization() {
        let snapshot = Snapshot {
            presence: vec![],
            health: json!({"status": "ok"}),
            state_version: StateVersion {
                presence: 1,
                health: 1,
            },
            uptime_ms: 60000,
            config_path: Some("/etc/claw/config.json".into()),
            state_dir: None,
        };
        let json_str = serde_json::to_string(&snapshot).unwrap();
        assert_eq!(
            json_str,
            r#"{"presence":[],"health":{"status":"ok"},"stateVersion":{"presence":1,"health":1},"uptimeMs":60000,"configPath":"/etc/claw/config.json"}"#
        );
    }

    #[test]
    fn snapshot_minimal_serialization() {
        let snapshot = Snapshot {
            presence: vec![],
            health: json!({}),
            state_version: StateVersion {
                presence: 0,
                health: 0,
            },
            uptime_ms: 0,
            config_path: None,
            state_dir: None,
        };
        let json_str = serde_json::to_string(&snapshot).unwrap();
        assert_eq!(
            json_str,
            r#"{"presence":[],"health":{},"stateVersion":{"presence":0,"health":0},"uptimeMs":0}"#
        );
    }

    #[test]
    fn snapshot_with_presence_entries() {
        let snapshot = Snapshot {
            presence: vec![PresenceEntry {
                id: "c1".into(),
                name: Some("UI".into()),
                client_type: None,
                connected_at: "2026-01-01T00:00:00Z".into(),
                device: None,
            }],
            health: json!({}),
            state_version: StateVersion {
                presence: 1,
                health: 0,
            },
            uptime_ms: 1000,
            config_path: None,
            state_dir: None,
        };
        let json_str = serde_json::to_string(&snapshot).unwrap();
        assert_eq!(
            json_str,
            r#"{"presence":[{"id":"c1","name":"UI","connectedAt":"2026-01-01T00:00:00Z"}],"health":{},"stateVersion":{"presence":1,"health":0},"uptimeMs":1000}"#
        );
    }

    #[test]
    fn snapshot_roundtrip() {
        let snapshot = Snapshot {
            presence: vec![],
            health: json!({"agents": []}),
            state_version: StateVersion {
                presence: 5,
                health: 3,
            },
            uptime_ms: 999999,
            config_path: Some("/cfg".into()),
            state_dir: Some("/state".into()),
        };
        let json_str = serde_json::to_string(&snapshot).unwrap();
        let decoded: Snapshot = serde_json::from_str(&json_str).unwrap();
        assert_eq!(snapshot, decoded);
    }

    // -- ConnectParams ----------------------------------------------------

    #[test]
    fn connect_params_full_serialization() {
        let params = ConnectParams {
            min_protocol: 1,
            max_protocol: 1,
            client_name: Some("Web UI".into()),
            client_version: Some("0.1.0".into()),
            client_type: Some("web".into()),
            caps: Some(vec!["streaming".into()]),
            auth: Some(json!({"token": "abc"})),
            device: Some("macbook-pro".into()),
        };
        let json_str = serde_json::to_string(&params).unwrap();
        assert_eq!(
            json_str,
            r#"{"minProtocol":1,"maxProtocol":1,"clientName":"Web UI","clientVersion":"0.1.0","clientType":"web","caps":["streaming"],"auth":{"token":"abc"},"device":"macbook-pro"}"#
        );
    }

    #[test]
    fn connect_params_minimal_serialization() {
        let params = ConnectParams {
            min_protocol: 1,
            max_protocol: 1,
            client_name: None,
            client_version: None,
            client_type: None,
            caps: None,
            auth: None,
            device: None,
        };
        let json_str = serde_json::to_string(&params).unwrap();
        assert_eq!(json_str, r#"{"minProtocol":1,"maxProtocol":1}"#);
    }

    #[test]
    fn connect_params_deserialization_minimal() {
        let input = r#"{"minProtocol":1,"maxProtocol":2}"#;
        let params: ConnectParams = serde_json::from_str(input).unwrap();
        assert_eq!(params.min_protocol, 1);
        assert_eq!(params.max_protocol, 2);
        assert!(params.client_name.is_none());
        assert!(params.caps.is_none());
    }

    #[test]
    fn connect_params_roundtrip() {
        let params = ConnectParams {
            min_protocol: 1,
            max_protocol: 3,
            client_name: Some("Test".into()),
            client_version: None,
            client_type: Some("test".into()),
            caps: Some(vec!["a".into(), "b".into()]),
            auth: Some(json!({"mode": "token", "value": "secret"})),
            device: None,
        };
        let json_str = serde_json::to_string(&params).unwrap();
        let decoded: ConnectParams = serde_json::from_str(&json_str).unwrap();
        assert_eq!(params, decoded);
    }

    // -- HelloOk ----------------------------------------------------------

    #[test]
    fn hello_ok_full_serialization() {
        let hello = HelloOk {
            protocol: 1,
            server_name: "claw-rust".into(),
            server_version: "0.1.0".into(),
            features: Some(vec!["streaming".into()]),
            snapshot: Snapshot {
                presence: vec![],
                health: json!({}),
                state_version: StateVersion {
                    presence: 0,
                    health: 0,
                },
                uptime_ms: 0,
                config_path: None,
                state_dir: None,
            },
            policy: Some(json!({"maxMessageLength": 4096})),
            auth: Some(json!({"mode": "none"})),
        };
        let json_str = serde_json::to_string(&hello).unwrap();
        assert_eq!(
            json_str,
            r#"{"protocol":1,"serverName":"claw-rust","serverVersion":"0.1.0","features":["streaming"],"snapshot":{"presence":[],"health":{},"stateVersion":{"presence":0,"health":0},"uptimeMs":0},"policy":{"maxMessageLength":4096},"auth":{"mode":"none"}}"#
        );
    }

    #[test]
    fn hello_ok_minimal_serialization() {
        let hello = HelloOk {
            protocol: 1,
            server_name: "claw".into(),
            server_version: "0.1.0".into(),
            features: None,
            snapshot: Snapshot {
                presence: vec![],
                health: json!({}),
                state_version: StateVersion {
                    presence: 0,
                    health: 0,
                },
                uptime_ms: 0,
                config_path: None,
                state_dir: None,
            },
            policy: None,
            auth: None,
        };
        let json_str = serde_json::to_string(&hello).unwrap();
        assert_eq!(
            json_str,
            r#"{"protocol":1,"serverName":"claw","serverVersion":"0.1.0","snapshot":{"presence":[],"health":{},"stateVersion":{"presence":0,"health":0},"uptimeMs":0}}"#
        );
    }

    #[test]
    fn hello_ok_roundtrip() {
        let hello = HelloOk {
            protocol: 1,
            server_name: "claw-rust".into(),
            server_version: "0.1.0".into(),
            features: Some(vec!["streaming".into(), "presence".into()]),
            snapshot: Snapshot {
                presence: vec![PresenceEntry {
                    id: "c1".into(),
                    name: Some("Admin".into()),
                    client_type: Some("web".into()),
                    connected_at: "2026-01-01T00:00:00Z".into(),
                    device: Some("desktop".into()),
                }],
                health: json!({"agents": [{"name": "gpt4", "status": "ok"}]}),
                state_version: StateVersion {
                    presence: 1,
                    health: 1,
                },
                uptime_ms: 5000,
                config_path: Some("/etc/claw.json".into()),
                state_dir: Some("/var/lib/claw".into()),
            },
            policy: Some(json!({})),
            auth: Some(json!({"authenticated": true})),
        };
        let json_str = serde_json::to_string(&hello).unwrap();
        let decoded: HelloOk = serde_json::from_str(&json_str).unwrap();
        assert_eq!(hello, decoded);
    }

    // -- Cross-type integration -------------------------------------------

    #[test]
    fn hello_ok_as_response_payload() {
        let hello = HelloOk {
            protocol: 1,
            server_name: "claw".into(),
            server_version: "0.1.0".into(),
            features: None,
            snapshot: Snapshot {
                presence: vec![],
                health: json!({}),
                state_version: StateVersion {
                    presence: 0,
                    health: 0,
                },
                uptime_ms: 0,
                config_path: None,
                state_dir: None,
            },
            policy: None,
            auth: None,
        };

        let payload = serde_json::to_value(&hello).unwrap();
        let frame = super::super::frames::GatewayFrame::Response(
            super::super::frames::ResponseFrame::ok("handshake-1", payload),
        );
        let json_str = serde_json::to_string(&frame).unwrap();
        // Verify the envelope wraps correctly
        let parsed: serde_json::Value = serde_json::from_str(&json_str).unwrap();
        assert_eq!(parsed["type"], "res");
        assert_eq!(parsed["ok"], true);
        assert_eq!(parsed["payload"]["protocol"], 1);
        assert_eq!(parsed["payload"]["serverName"], "claw");
    }
}
