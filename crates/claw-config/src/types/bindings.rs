//! Binding (multi-agent routing) configuration types.

use serde::{Deserialize, Serialize};

/// Maps an agent to a channel/account via pattern matching.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct BindingEntry {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<String>,

    #[serde(rename = "match", skip_serializing_if = "Option::is_none")]
    pub match_rule: Option<BindingMatch>,
}

/// Match criteria for a binding rule.
///
/// All specified fields use AND semantics — every non-`None` field
/// must match the inbound message for the binding to apply.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct BindingMatch {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub channel: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub account_id: Option<String>,

    /// Exact peer (DM/group/channel) match.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub peer: Option<BindingPeer>,

    /// Discord guild (server) ID.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub guild_id: Option<String>,

    /// Discord roles required for this binding (AND with guild_id).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub roles: Option<Vec<String>>,

    /// Slack team (workspace) ID.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub team_id: Option<String>,
}

/// Peer identifier in a binding match rule.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct BindingPeer {
    /// Chat type kind (e.g. "direct", "group", "channel").
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,

    /// Peer/chat identifier.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
}
