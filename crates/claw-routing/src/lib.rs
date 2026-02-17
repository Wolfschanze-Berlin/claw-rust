//! Routing and session key system for claw-rust.
//!
//! Implements agent binding resolution with 8-level priority cascade
//! and session key generation/parsing for agent-channel-peer scoping.

pub mod resolve;
pub mod session_key;

pub use resolve::{
    MatchedBy, ResolvedAgentRoute, RouteContext,
    resolve_agent_route,
};

pub use session_key::{
    DmScope, ParsedAgentSessionKey, PeerSessionKeyParams, SessionKeyShape,
    DEFAULT_ACCOUNT_ID, DEFAULT_AGENT_ID, DEFAULT_MAIN_KEY,
    build_agent_main_session_key, build_agent_peer_session_key,
    build_group_history_key, classify_session_key_shape,
    normalize_account_id, normalize_agent_id, normalize_main_key,
    parse_agent_session_key, resolve_agent_id_from_session_key,
    resolve_thread_session_keys, to_agent_request_session_key,
    to_agent_store_session_key,
};
