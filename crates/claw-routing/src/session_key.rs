//! Session key generation and parsing.
//!
//! Ports OpenClaw's `src/routing/session-key.ts`. Session keys encode the
//! full routing context (agent, channel, peer, DM scope) into a single
//! colon-delimited string used as the primary session bucket identifier.
//!
//! # Key format
//!
//! ```text
//! agent:<agentId>:<mainKey>                              (main DM)
//! agent:<agentId>:direct:<peerId>                        (per-peer DM)
//! agent:<agentId>:<channel>:direct:<peerId>              (per-channel-peer DM)
//! agent:<agentId>:<channel>:<accountId>:direct:<peerId>  (per-account-channel-peer DM)
//! agent:<agentId>:<channel>:group:<peerId>               (group chat)
//! agent:<agentId>:<channel>:channel:<peerId>             (broadcast channel)
//! ```

use std::collections::HashMap;
use std::sync::LazyLock;

use regex::Regex;
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Default agent ID when none is specified.
pub const DEFAULT_AGENT_ID: &str = "main";

/// Default main session key segment.
pub const DEFAULT_MAIN_KEY: &str = "main";

/// Default account ID when none is specified.
pub const DEFAULT_ACCOUNT_ID: &str = "default";

// ---------------------------------------------------------------------------
// DmScope
// ---------------------------------------------------------------------------

/// How direct-message sessions are scoped/isolated.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DmScope {
    /// All DMs share the agent's main session.
    #[default]
    Main,
    /// Isolate by sender ID across all channels.
    PerPeer,
    /// Isolate by channel + sender.
    PerChannelPeer,
    /// Isolate by account + channel + sender.
    PerAccountChannelPeer,
}

// ---------------------------------------------------------------------------
// Regex
// ---------------------------------------------------------------------------

/// Valid ID: starts with alnum, then up to 63 more alnum/underscore/dash chars.
static VALID_ID_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[a-zA-Z0-9][a-zA-Z0-9_-]{0,63}$").unwrap());

/// Characters that are not valid in an ID.
static INVALID_CHARS_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"[^a-z0-9_-]+").unwrap());

// ---------------------------------------------------------------------------
// Normalization
// ---------------------------------------------------------------------------

/// Normalize a token: trim + lowercase.
fn normalize_token(value: &str) -> String {
    value.trim().to_lowercase()
}

/// Normalize a main key — empty/whitespace-only becomes [`DEFAULT_MAIN_KEY`].
pub fn normalize_main_key(value: Option<&str>) -> String {
    let trimmed = value.unwrap_or("").trim();
    if trimmed.is_empty() {
        DEFAULT_MAIN_KEY.to_string()
    } else {
        trimmed.to_lowercase()
    }
}

/// Normalize an agent ID to a path-safe, shell-friendly form.
///
/// Empty/whitespace-only values return [`DEFAULT_AGENT_ID`]. Valid IDs are
/// lowercased. Invalid characters are collapsed to `-` with leading/trailing
/// dashes stripped.
pub fn normalize_agent_id(value: Option<&str>) -> String {
    normalize_id_impl(value, DEFAULT_AGENT_ID)
}

/// Normalize an account ID to a path-safe, shell-friendly form.
///
/// Same rules as [`normalize_agent_id`] but defaults to [`DEFAULT_ACCOUNT_ID`].
pub fn normalize_account_id(value: Option<&str>) -> String {
    normalize_id_impl(value, DEFAULT_ACCOUNT_ID)
}

/// Shared normalization logic for agent/account IDs.
fn normalize_id_impl(value: Option<&str>, default: &str) -> String {
    let trimmed = value.unwrap_or("").trim();
    if trimmed.is_empty() {
        return default.to_string();
    }
    if VALID_ID_RE.is_match(trimmed) {
        return trimmed.to_lowercase();
    }
    // Best-effort: collapse invalid chars to dash, strip edges, truncate
    let cleaned = INVALID_CHARS_RE
        .replace_all(&trimmed.to_lowercase(), "-")
        .to_string();
    let cleaned = cleaned.trim_start_matches('-').trim_end_matches('-');
    let cleaned = &cleaned[..cleaned.len().min(64)];
    if cleaned.is_empty() {
        default.to_string()
    } else {
        cleaned.to_string()
    }
}

// ---------------------------------------------------------------------------
// Session key builders
// ---------------------------------------------------------------------------

/// Build the main (default DM) session key for an agent.
///
/// Format: `agent:<agentId>:<mainKey>`
pub fn build_agent_main_session_key(agent_id: &str, main_key: Option<&str>) -> String {
    let agent_id = normalize_agent_id(Some(agent_id));
    let main_key = normalize_main_key(main_key);
    format!("agent:{agent_id}:{main_key}")
}

/// Parameters for building a peer session key.
pub struct PeerSessionKeyParams<'a> {
    pub agent_id: &'a str,
    pub main_key: Option<&'a str>,
    pub channel: &'a str,
    pub account_id: Option<&'a str>,
    pub peer_kind: Option<&'a str>,
    pub peer_id: Option<&'a str>,
    pub dm_scope: Option<DmScope>,
    pub identity_links: Option<&'a HashMap<String, Vec<String>>>,
}

/// Build a peer session key with full routing context.
///
/// For direct chats, the DM scope determines the key format. For groups,
/// channels, and threads the key always includes the channel and peer ID.
pub fn build_agent_peer_session_key(params: &PeerSessionKeyParams<'_>) -> String {
    let peer_kind = params.peer_kind.unwrap_or("direct");
    let agent_id = normalize_agent_id(Some(params.agent_id));

    if peer_kind == "direct" {
        let dm_scope = params.dm_scope.unwrap_or_default();
        let mut peer_id = params.peer_id.unwrap_or("").trim().to_string();

        // Resolve identity links (unless main scope, which ignores peer)
        if dm_scope != DmScope::Main {
            if let Some(linked) = resolve_linked_peer_id(
                params.identity_links,
                params.channel,
                &peer_id,
            ) {
                peer_id = linked;
            }
        }
        peer_id = peer_id.to_lowercase();

        match dm_scope {
            DmScope::PerAccountChannelPeer if !peer_id.is_empty() => {
                let channel = normalize_token(params.channel);
                let channel = if channel.is_empty() { "unknown" } else { &channel };
                let account_id = normalize_account_id(params.account_id);
                format!("agent:{agent_id}:{channel}:{account_id}:direct:{peer_id}")
            }
            DmScope::PerChannelPeer if !peer_id.is_empty() => {
                let channel = normalize_token(params.channel);
                let channel = if channel.is_empty() { "unknown" } else { &channel };
                format!("agent:{agent_id}:{channel}:direct:{peer_id}")
            }
            DmScope::PerPeer if !peer_id.is_empty() => {
                format!("agent:{agent_id}:direct:{peer_id}")
            }
            _ => {
                // Main scope or empty peer ID → fall back to main key
                build_agent_main_session_key(params.agent_id, params.main_key)
            }
        }
    } else {
        // Group, channel, thread — always include channel + peer
        let channel = normalize_token(params.channel);
        let channel = if channel.is_empty() { "unknown" } else { &channel };
        let peer_id = params
            .peer_id
            .map(|p| p.trim().to_lowercase())
            .unwrap_or_else(|| "unknown".to_string());
        let peer_id = if peer_id.is_empty() { "unknown" } else { &peer_id };
        format!("agent:{agent_id}:{channel}:{peer_kind}:{peer_id}")
    }
}

/// Extract the agent ID from a session key string.
///
/// Returns [`DEFAULT_AGENT_ID`] if the key is empty or unparseable.
pub fn resolve_agent_id_from_session_key(session_key: Option<&str>) -> String {
    let raw = session_key.unwrap_or("").trim();
    if raw.is_empty() {
        return normalize_agent_id(Some(DEFAULT_AGENT_ID));
    }
    match parse_agent_session_key(raw) {
        Some(parsed) => normalize_agent_id(Some(&parsed.agent_id)),
        None => normalize_agent_id(Some(DEFAULT_AGENT_ID)),
    }
}

// ---------------------------------------------------------------------------
// Parsing
// ---------------------------------------------------------------------------

/// Parsed components of an `agent:...` session key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedAgentSessionKey {
    pub agent_id: String,
    pub rest: String,
}

/// Parse an `agent:<agentId>:<rest>` session key.
///
/// Returns `None` if the key doesn't start with `agent:` or has fewer
/// than 3 colon-separated segments.
pub fn parse_agent_session_key(key: &str) -> Option<ParsedAgentSessionKey> {
    let trimmed = key.trim().to_lowercase();
    if !trimmed.starts_with("agent:") {
        return None;
    }
    let rest = &trimmed["agent:".len()..];
    let colon_pos = rest.find(':')?;
    let agent_id = &rest[..colon_pos];
    let remainder = &rest[colon_pos + 1..];
    if agent_id.is_empty() || remainder.is_empty() {
        return None;
    }
    Some(ParsedAgentSessionKey {
        agent_id: agent_id.to_string(),
        rest: remainder.to_string(),
    })
}

/// Classify the shape of a session key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionKeyShape {
    Missing,
    Agent,
    LegacyOrAlias,
    MalformedAgent,
}

/// Classify a session key string into a shape category.
pub fn classify_session_key_shape(session_key: Option<&str>) -> SessionKeyShape {
    let raw = session_key.unwrap_or("").trim();
    if raw.is_empty() {
        return SessionKeyShape::Missing;
    }
    if parse_agent_session_key(raw).is_some() {
        return SessionKeyShape::Agent;
    }
    if raw.to_lowercase().starts_with("agent:") {
        SessionKeyShape::MalformedAgent
    } else {
        SessionKeyShape::LegacyOrAlias
    }
}

/// Convert a store-format session key to a request key (strips `agent:<id>:` prefix).
pub fn to_agent_request_session_key(store_key: Option<&str>) -> Option<String> {
    let raw = store_key.unwrap_or("").trim();
    if raw.is_empty() {
        return None;
    }
    parse_agent_session_key(raw)
        .map(|p| p.rest)
        .or_else(|| Some(raw.to_string()))
}

/// Convert a request key to store format by prepending `agent:<agentId>:`.
pub fn to_agent_store_session_key(
    agent_id: &str,
    request_key: Option<&str>,
    main_key: Option<&str>,
) -> String {
    let raw = request_key.unwrap_or("").trim();
    if raw.is_empty() || raw == DEFAULT_MAIN_KEY {
        return build_agent_main_session_key(agent_id, main_key);
    }
    let lowered = raw.to_lowercase();
    if lowered.starts_with("agent:") {
        return lowered;
    }
    let agent_id = normalize_agent_id(Some(agent_id));
    format!("agent:{agent_id}:{lowered}")
}

/// Build a group history key (channel:account:kind:peer).
pub fn build_group_history_key(
    channel: &str,
    account_id: Option<&str>,
    peer_kind: &str,
    peer_id: &str,
) -> String {
    let channel = normalize_token(channel);
    let channel = if channel.is_empty() { "unknown" } else { &channel };
    let account_id = normalize_account_id(account_id);
    let peer_id = peer_id.trim().to_lowercase();
    let peer_id = if peer_id.is_empty() { "unknown" } else { &peer_id };
    format!("{channel}:{account_id}:{peer_kind}:{peer_id}")
}

/// Resolve thread session keys by appending `:thread:<threadId>` suffix.
pub fn resolve_thread_session_keys(
    base_session_key: &str,
    thread_id: Option<&str>,
    parent_session_key: Option<&str>,
    use_suffix: bool,
) -> (String, Option<String>) {
    let thread_id = thread_id.unwrap_or("").trim();
    if thread_id.is_empty() {
        return (base_session_key.to_string(), None);
    }
    let normalized = thread_id.to_lowercase();
    let session_key = if use_suffix {
        format!("{base_session_key}:thread:{normalized}")
    } else {
        base_session_key.to_string()
    };
    (session_key, parent_session_key.map(|s| s.to_string()))
}

// ---------------------------------------------------------------------------
// Identity linking
// ---------------------------------------------------------------------------

/// Resolve a peer ID through identity links.
///
/// Checks both the raw peer ID and the channel-scoped form
/// (`<channel>:<peerId>`) against the identity link map. Returns the
/// canonical identity name if found.
fn resolve_linked_peer_id(
    identity_links: Option<&HashMap<String, Vec<String>>>,
    channel: &str,
    peer_id: &str,
) -> Option<String> {
    let links = identity_links?;
    let peer_id = peer_id.trim();
    if peer_id.is_empty() {
        return None;
    }

    let raw_candidate = normalize_token(peer_id);
    let channel_norm = normalize_token(channel);
    let scoped_candidate = if channel_norm.is_empty() {
        None
    } else {
        Some(format!("{channel_norm}:{}", normalize_token(peer_id)))
    };

    for (canonical, ids) in links {
        let canonical_name = canonical.trim();
        if canonical_name.is_empty() {
            continue;
        }
        for id in ids {
            let normalized = normalize_token(id);
            if normalized.is_empty() {
                continue;
            }
            if normalized == raw_candidate {
                return Some(canonical_name.to_string());
            }
            if let Some(ref scoped) = scoped_candidate {
                if &normalized == scoped {
                    return Some(canonical_name.to_string());
                }
            }
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -- Constants --

    #[test]
    fn default_constants() {
        assert_eq!(DEFAULT_AGENT_ID, "main");
        assert_eq!(DEFAULT_MAIN_KEY, "main");
        assert_eq!(DEFAULT_ACCOUNT_ID, "default");
    }

    // -- normalize_agent_id --

    #[test]
    fn normalize_agent_id_basic() {
        assert_eq!(normalize_agent_id(Some("Main")), "main");
        assert_eq!(normalize_agent_id(Some("GPT-4o")), "gpt-4o");
        assert_eq!(normalize_agent_id(Some("")), DEFAULT_AGENT_ID);
        assert_eq!(normalize_agent_id(None), DEFAULT_AGENT_ID);
        assert_eq!(normalize_agent_id(Some("  spaced  ")), "spaced");
    }

    #[test]
    fn normalize_agent_id_invalid_chars() {
        assert_eq!(normalize_agent_id(Some("hello world!")), "hello-world");
        assert_eq!(normalize_agent_id(Some("---")), DEFAULT_AGENT_ID);
    }

    // -- normalize_account_id --

    #[test]
    fn normalize_account_id_basic() {
        assert_eq!(normalize_account_id(Some("Main")), "main");
        assert_eq!(normalize_account_id(Some("")), DEFAULT_ACCOUNT_ID);
        assert_eq!(normalize_account_id(None), DEFAULT_ACCOUNT_ID);
    }

    // -- build_agent_main_session_key --

    #[test]
    fn main_session_key_default() {
        assert_eq!(
            build_agent_main_session_key("main", None),
            "agent:main:main"
        );
    }

    #[test]
    fn main_session_key_custom() {
        assert_eq!(
            build_agent_main_session_key("GPT", Some("custom")),
            "agent:gpt:custom"
        );
    }

    // -- build_agent_peer_session_key: DM scopes --

    #[test]
    fn peer_key_dm_main_scope() {
        let key = build_agent_peer_session_key(&PeerSessionKeyParams {
            agent_id: "gpt",
            main_key: None,
            channel: "telegram",
            account_id: None,
            peer_kind: Some("direct"),
            peer_id: Some("user123"),
            dm_scope: Some(DmScope::Main),
            identity_links: None,
        });
        // Main scope ignores peer → falls back to main key
        assert_eq!(key, "agent:gpt:main");
    }

    #[test]
    fn peer_key_dm_per_peer() {
        let key = build_agent_peer_session_key(&PeerSessionKeyParams {
            agent_id: "gpt",
            main_key: None,
            channel: "telegram",
            account_id: None,
            peer_kind: Some("direct"),
            peer_id: Some("user123"),
            dm_scope: Some(DmScope::PerPeer),
            identity_links: None,
        });
        assert_eq!(key, "agent:gpt:direct:user123");
    }

    #[test]
    fn peer_key_dm_per_channel_peer() {
        let key = build_agent_peer_session_key(&PeerSessionKeyParams {
            agent_id: "gpt",
            main_key: None,
            channel: "telegram",
            account_id: None,
            peer_kind: Some("direct"),
            peer_id: Some("user123"),
            dm_scope: Some(DmScope::PerChannelPeer),
            identity_links: None,
        });
        assert_eq!(key, "agent:gpt:telegram:direct:user123");
    }

    #[test]
    fn peer_key_dm_per_account_channel_peer() {
        let key = build_agent_peer_session_key(&PeerSessionKeyParams {
            agent_id: "gpt",
            main_key: None,
            channel: "telegram",
            account_id: Some("bot1"),
            peer_kind: Some("direct"),
            peer_id: Some("user123"),
            dm_scope: Some(DmScope::PerAccountChannelPeer),
            identity_links: None,
        });
        assert_eq!(key, "agent:gpt:telegram:bot1:direct:user123");
    }

    #[test]
    fn peer_key_dm_empty_peer_falls_back_to_main() {
        let key = build_agent_peer_session_key(&PeerSessionKeyParams {
            agent_id: "gpt",
            main_key: None,
            channel: "telegram",
            account_id: None,
            peer_kind: Some("direct"),
            peer_id: Some(""),
            dm_scope: Some(DmScope::PerPeer),
            identity_links: None,
        });
        assert_eq!(key, "agent:gpt:main");
    }

    // -- build_agent_peer_session_key: group/channel --

    #[test]
    fn peer_key_group() {
        let key = build_agent_peer_session_key(&PeerSessionKeyParams {
            agent_id: "gpt",
            main_key: None,
            channel: "telegram",
            account_id: None,
            peer_kind: Some("group"),
            peer_id: Some("-100123456"),
            dm_scope: None,
            identity_links: None,
        });
        assert_eq!(key, "agent:gpt:telegram:group:-100123456");
    }

    #[test]
    fn peer_key_channel_chat() {
        let key = build_agent_peer_session_key(&PeerSessionKeyParams {
            agent_id: "gpt",
            main_key: None,
            channel: "discord",
            account_id: None,
            peer_kind: Some("channel"),
            peer_id: Some("ch_999"),
            dm_scope: None,
            identity_links: None,
        });
        assert_eq!(key, "agent:gpt:discord:channel:ch_999");
    }

    #[test]
    fn peer_key_defaults_to_direct() {
        let key = build_agent_peer_session_key(&PeerSessionKeyParams {
            agent_id: "gpt",
            main_key: None,
            channel: "telegram",
            account_id: None,
            peer_kind: None, // defaults to direct
            peer_id: Some("user1"),
            dm_scope: Some(DmScope::PerPeer),
            identity_links: None,
        });
        assert_eq!(key, "agent:gpt:direct:user1");
    }

    // -- resolve_agent_id_from_session_key --

    #[test]
    fn resolve_agent_id_basic() {
        assert_eq!(
            resolve_agent_id_from_session_key(Some("agent:gpt:telegram:group:123")),
            "gpt"
        );
        assert_eq!(
            resolve_agent_id_from_session_key(Some("agent:main:main")),
            "main"
        );
        assert_eq!(
            resolve_agent_id_from_session_key(None),
            "main"
        );
        assert_eq!(
            resolve_agent_id_from_session_key(Some("")),
            "main"
        );
    }

    // -- parse_agent_session_key --

    #[test]
    fn parse_valid_key() {
        let parsed = parse_agent_session_key("agent:gpt:telegram:group:123").unwrap();
        assert_eq!(parsed.agent_id, "gpt");
        assert_eq!(parsed.rest, "telegram:group:123");
    }

    #[test]
    fn parse_missing_prefix() {
        assert!(parse_agent_session_key("cron:daily").is_none());
    }

    #[test]
    fn parse_malformed() {
        assert!(parse_agent_session_key("agent:").is_none());
        assert!(parse_agent_session_key("agent:gpt").is_none());
        assert!(parse_agent_session_key("agent::rest").is_none());
    }

    // -- classify_session_key_shape --

    #[test]
    fn classify_shapes() {
        assert_eq!(classify_session_key_shape(None), SessionKeyShape::Missing);
        assert_eq!(classify_session_key_shape(Some("")), SessionKeyShape::Missing);
        assert_eq!(
            classify_session_key_shape(Some("agent:gpt:main")),
            SessionKeyShape::Agent
        );
        assert_eq!(
            classify_session_key_shape(Some("agent:")),
            SessionKeyShape::MalformedAgent
        );
        assert_eq!(
            classify_session_key_shape(Some("legacy-key")),
            SessionKeyShape::LegacyOrAlias
        );
    }

    // -- to_agent_request_session_key --

    #[test]
    fn request_key_strips_prefix() {
        assert_eq!(
            to_agent_request_session_key(Some("agent:gpt:telegram:direct:user1")),
            Some("telegram:direct:user1".to_string())
        );
        assert_eq!(to_agent_request_session_key(Some("")), None);
        assert_eq!(to_agent_request_session_key(None), None);
    }

    // -- to_agent_store_session_key --

    #[test]
    fn store_key_prepends_prefix() {
        assert_eq!(
            to_agent_store_session_key("gpt", Some("telegram:direct:user1"), None),
            "agent:gpt:telegram:direct:user1"
        );
        assert_eq!(
            to_agent_store_session_key("gpt", Some(""), None),
            "agent:gpt:main"
        );
        assert_eq!(
            to_agent_store_session_key("gpt", Some("main"), None),
            "agent:gpt:main"
        );
        // Already prefixed → passthrough
        assert_eq!(
            to_agent_store_session_key("gpt", Some("agent:other:x"), None),
            "agent:other:x"
        );
    }

    // -- identity links --

    #[test]
    fn identity_link_resolves_raw_peer_id() {
        let mut links = HashMap::new();
        links.insert("alice".to_string(), vec!["telegram:12345".to_string(), "user12345".to_string()]);

        let key = build_agent_peer_session_key(&PeerSessionKeyParams {
            agent_id: "gpt",
            main_key: None,
            channel: "telegram",
            account_id: None,
            peer_kind: Some("direct"),
            peer_id: Some("user12345"),
            dm_scope: Some(DmScope::PerPeer),
            identity_links: Some(&links),
        });
        // Should resolve to canonical "alice"
        assert_eq!(key, "agent:gpt:direct:alice");
    }

    #[test]
    fn identity_link_resolves_scoped_id() {
        let mut links = HashMap::new();
        links.insert("bob".to_string(), vec!["telegram:99999".to_string()]);

        let key = build_agent_peer_session_key(&PeerSessionKeyParams {
            agent_id: "gpt",
            main_key: None,
            channel: "telegram",
            account_id: None,
            peer_kind: Some("direct"),
            peer_id: Some("99999"),
            dm_scope: Some(DmScope::PerChannelPeer),
            identity_links: Some(&links),
        });
        // "telegram:99999" matches scoped candidate → resolves to "bob"
        assert_eq!(key, "agent:gpt:telegram:direct:bob");
    }

    // -- build_group_history_key --

    #[test]
    fn group_history_key() {
        assert_eq!(
            build_group_history_key("telegram", Some("bot1"), "group", "123"),
            "telegram:bot1:group:123"
        );
        assert_eq!(
            build_group_history_key("", None, "channel", ""),
            "unknown:default:channel:unknown"
        );
    }

    // -- resolve_thread_session_keys --

    #[test]
    fn thread_key_with_suffix() {
        let (key, parent) =
            resolve_thread_session_keys("agent:gpt:telegram:group:123", Some("thread-42"), None, true);
        assert_eq!(key, "agent:gpt:telegram:group:123:thread:thread-42");
        assert!(parent.is_none());
    }

    #[test]
    fn thread_key_without_thread_id() {
        let (key, parent) =
            resolve_thread_session_keys("agent:gpt:main", None, None, true);
        assert_eq!(key, "agent:gpt:main");
        assert!(parent.is_none());
    }

    #[test]
    fn thread_key_no_suffix_mode() {
        let (key, parent) = resolve_thread_session_keys(
            "agent:gpt:telegram:group:123",
            Some("t1"),
            Some("agent:gpt:telegram:group:123"),
            false,
        );
        assert_eq!(key, "agent:gpt:telegram:group:123");
        assert_eq!(parent.unwrap(), "agent:gpt:telegram:group:123");
    }

    // -- DmScope serde --

    #[test]
    fn dm_scope_serde_roundtrip() {
        let scope = DmScope::PerChannelPeer;
        let json = serde_json::to_string(&scope).unwrap();
        assert_eq!(json, r#""per-channel-peer""#);
        let parsed: DmScope = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, scope);
    }

    #[test]
    fn dm_scope_default_is_main() {
        assert_eq!(DmScope::default(), DmScope::Main);
    }
}
