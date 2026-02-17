//! WebSocket JSON-RPC frame types for the gateway protocol.
//!
//! Ports OpenClaw's `src/gateway/protocol/schema/frames.ts` to Rust.
//! All types serialize to byte-identical JSON as the TypeScript version.

use claw_core::ErrorShape;
use serde::{Deserialize, Serialize};

use super::handshake::StateVersion;

// ---------------------------------------------------------------------------
// GatewayFrame — discriminated union over `type` field
// ---------------------------------------------------------------------------

/// Top-level WebSocket frame envelope.
///
/// Discriminated on the `"type"` field:
/// - `"req"` for client-to-server RPC requests
/// - `"res"` for server-to-client RPC responses
/// - `"event"` for server-to-client event broadcasts
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type")]
pub enum GatewayFrame {
    #[serde(rename = "req")]
    Request(RequestFrame),

    #[serde(rename = "res")]
    Response(ResponseFrame),

    #[serde(rename = "event")]
    Event(EventFrame),
}

// ---------------------------------------------------------------------------
// RequestFrame
// ---------------------------------------------------------------------------

/// Client-to-server RPC request.
///
/// ```json
/// { "type": "req", "id": "abc-123", "method": "chat.send", "params": {...} }
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RequestFrame {
    pub id: String,
    pub method: String,
    pub params: serde_json::Value,
}

// ---------------------------------------------------------------------------
// ResponseFrame
// ---------------------------------------------------------------------------

/// Server-to-client RPC response.
///
/// ```json
/// { "type": "res", "id": "abc-123", "ok": true, "payload": {...} }
/// ```
///
/// On error:
/// ```json
/// { "type": "res", "id": "abc-123", "ok": false, "error": {...} }
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ResponseFrame {
    pub id: String,
    pub ok: bool,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub payload: Option<serde_json::Value>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<ErrorShape>,
}

impl ResponseFrame {
    /// Build a success response with a payload.
    pub fn ok(id: impl Into<String>, payload: serde_json::Value) -> Self {
        Self {
            id: id.into(),
            ok: true,
            payload: Some(payload),
            error: None,
        }
    }

    /// Build an error response.
    pub fn err(id: impl Into<String>, error: ErrorShape) -> Self {
        Self {
            id: id.into(),
            ok: false,
            payload: None,
            error: Some(error),
        }
    }
}

// ---------------------------------------------------------------------------
// EventFrame
// ---------------------------------------------------------------------------

/// Server-to-client event broadcast.
///
/// ```json
/// { "type": "event", "event": "presence.update", "payload": {...}, "seq": 1 }
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EventFrame {
    pub event: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub payload: Option<serde_json::Value>,

    pub seq: u64,

    #[serde(rename = "stateVersion", skip_serializing_if = "Option::is_none")]
    pub state_version: Option<StateVersion>,
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // -- RequestFrame -----------------------------------------------------

    #[test]
    fn request_frame_serialization() {
        let frame = GatewayFrame::Request(RequestFrame {
            id: "req-1".into(),
            method: "chat.send".into(),
            params: json!({"text": "hello"}),
        });

        let json = serde_json::to_value(&frame).unwrap();
        assert_eq!(json["type"], "req");
        assert_eq!(json["id"], "req-1");
        assert_eq!(json["method"], "chat.send");
        assert_eq!(json["params"]["text"], "hello");
    }

    #[test]
    fn request_frame_exact_json() {
        let frame = GatewayFrame::Request(RequestFrame {
            id: "r1".into(),
            method: "ping".into(),
            params: json!({}),
        });

        let json_str = serde_json::to_string(&frame).unwrap();
        assert_eq!(
            json_str,
            r#"{"type":"req","id":"r1","method":"ping","params":{}}"#
        );
    }

    #[test]
    fn request_frame_deserialization() {
        let input = r#"{"type":"req","id":"r1","method":"ping","params":{}}"#;
        let frame: GatewayFrame = serde_json::from_str(input).unwrap();

        match frame {
            GatewayFrame::Request(req) => {
                assert_eq!(req.id, "r1");
                assert_eq!(req.method, "ping");
                assert_eq!(req.params, json!({}));
            }
            _ => panic!("expected Request variant"),
        }
    }

    // -- ResponseFrame (success) ------------------------------------------

    #[test]
    fn response_frame_ok_serialization() {
        let frame = GatewayFrame::Response(ResponseFrame::ok("r1", json!({"status": "ok"})));

        let json_str = serde_json::to_string(&frame).unwrap();
        assert_eq!(
            json_str,
            r#"{"type":"res","id":"r1","ok":true,"payload":{"status":"ok"}}"#
        );
    }

    #[test]
    fn response_frame_ok_deserialization() {
        let input = r#"{"type":"res","id":"r1","ok":true,"payload":{"status":"ok"}}"#;
        let frame: GatewayFrame = serde_json::from_str(input).unwrap();

        match frame {
            GatewayFrame::Response(res) => {
                assert!(res.ok);
                assert_eq!(res.payload, Some(json!({"status": "ok"})));
                assert!(res.error.is_none());
            }
            _ => panic!("expected Response variant"),
        }
    }

    // -- ResponseFrame (error) --------------------------------------------

    #[test]
    fn response_frame_err_serialization() {
        let error = ErrorShape {
            code: "UNAVAILABLE".into(),
            message: "service down".into(),
            details: None,
            retryable: None,
            retry_after_ms: None,
        };
        let frame = GatewayFrame::Response(ResponseFrame::err("r2", error));

        let json_str = serde_json::to_string(&frame).unwrap();
        assert_eq!(
            json_str,
            r#"{"type":"res","id":"r2","ok":false,"error":{"code":"UNAVAILABLE","message":"service down"}}"#
        );
    }

    #[test]
    fn response_frame_err_with_retry() {
        let error = ErrorShape {
            code: "AGENT_TIMEOUT".into(),
            message: "timed out".into(),
            details: None,
            retryable: Some(true),
            retry_after_ms: Some(5000),
        };
        let frame = GatewayFrame::Response(ResponseFrame::err("r3", error));

        let json_str = serde_json::to_string(&frame).unwrap();
        assert_eq!(
            json_str,
            r#"{"type":"res","id":"r3","ok":false,"error":{"code":"AGENT_TIMEOUT","message":"timed out","retryable":true,"retryAfterMs":5000}}"#
        );
    }

    #[test]
    fn response_frame_err_deserialization() {
        let input = r#"{"type":"res","id":"r2","ok":false,"error":{"code":"UNAVAILABLE","message":"down"}}"#;
        let frame: GatewayFrame = serde_json::from_str(input).unwrap();

        match frame {
            GatewayFrame::Response(res) => {
                assert!(!res.ok);
                assert!(res.payload.is_none());
                let err = res.error.unwrap();
                assert_eq!(err.code, "UNAVAILABLE");
                assert_eq!(err.message, "down");
            }
            _ => panic!("expected Response variant"),
        }
    }

    // -- EventFrame -------------------------------------------------------

    #[test]
    fn event_frame_serialization() {
        let frame = GatewayFrame::Event(EventFrame {
            event: "presence.update".into(),
            payload: Some(json!({"user": "alice"})),
            seq: 42,
            state_version: None,
        });

        let json_str = serde_json::to_string(&frame).unwrap();
        assert_eq!(
            json_str,
            r#"{"type":"event","event":"presence.update","payload":{"user":"alice"},"seq":42}"#
        );
    }

    #[test]
    fn event_frame_with_state_version() {
        let frame = GatewayFrame::Event(EventFrame {
            event: "health.changed".into(),
            payload: None,
            seq: 1,
            state_version: Some(StateVersion {
                presence: 10,
                health: 5,
            }),
        });

        let json_str = serde_json::to_string(&frame).unwrap();
        assert_eq!(
            json_str,
            r#"{"type":"event","event":"health.changed","seq":1,"stateVersion":{"presence":10,"health":5}}"#
        );
    }

    #[test]
    fn event_frame_deserialization() {
        let input = r#"{"type":"event","event":"test","seq":7,"stateVersion":{"presence":1,"health":2}}"#;
        let frame: GatewayFrame = serde_json::from_str(input).unwrap();

        match frame {
            GatewayFrame::Event(evt) => {
                assert_eq!(evt.event, "test");
                assert_eq!(evt.seq, 7);
                assert!(evt.payload.is_none());
                let sv = evt.state_version.unwrap();
                assert_eq!(sv.presence, 1);
                assert_eq!(sv.health, 2);
            }
            _ => panic!("expected Event variant"),
        }
    }

    // -- Round-trip --------------------------------------------------------

    #[test]
    fn request_roundtrip() {
        let original = GatewayFrame::Request(RequestFrame {
            id: "rt-1".into(),
            method: "test.method".into(),
            params: json!({"a": 1, "b": [2, 3]}),
        });
        let json_str = serde_json::to_string(&original).unwrap();
        let decoded: GatewayFrame = serde_json::from_str(&json_str).unwrap();
        assert_eq!(original, decoded);
    }

    #[test]
    fn response_roundtrip() {
        let original = GatewayFrame::Response(ResponseFrame::ok("rt-2", json!("done")));
        let json_str = serde_json::to_string(&original).unwrap();
        let decoded: GatewayFrame = serde_json::from_str(&json_str).unwrap();
        assert_eq!(original, decoded);
    }

    #[test]
    fn event_roundtrip() {
        let original = GatewayFrame::Event(EventFrame {
            event: "rt.event".into(),
            payload: Some(json!({"key": "value"})),
            seq: 99,
            state_version: Some(StateVersion {
                presence: 100,
                health: 200,
            }),
        });
        let json_str = serde_json::to_string(&original).unwrap();
        let decoded: GatewayFrame = serde_json::from_str(&json_str).unwrap();
        assert_eq!(original, decoded);
    }
}
