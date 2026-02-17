//! Agent binding resolution with 8-level priority cascade.
//!
//! Ports OpenClaw's `src/routing/resolve-route.ts`. Given an inbound message
//! context, resolves which agent should handle it by scanning configured
//! bindings from most-specific to least-specific:
//!
//! 1. Peer (exact DM/group/channel match)
//! 2. Parent peer (thread inheritance)
//! 3. Guild + roles (Discord server with role requirements)
//! 4. Guild (Discord server)
//! 5. Team (Slack workspace)
//! 6. Account (channel account instance)
//! 7. Channel (channel type)
//! 8. Default (fallback agent)

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use claw_config::{AgentsConfig, BindingEntry, BindingMatch, OpenClawConfig};

use crate::session_key::{
    self, DmScope, DEFAULT_AGENT_ID,
};

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// How an agent route was matched — encodes the priority level.
///
/// Variants are ordered from highest priority (most specific) to lowest.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MatchedBy {
    /// Level 1: Exact peer (DM user, group, or channel) match.
    BindingPeer,
    /// Level 2: Parent peer match (thread inherits parent's binding).
    BindingPeerParent,
    /// Level 3: Discord guild + roles combination.
    BindingGuildRoles,
    /// Level 4: Discord guild alone.
    BindingGuild,
    /// Level 5: Slack team (workspace).
    BindingTeam,
    /// Level 6: Channel account instance.
    BindingAccount,
    /// Level 7: Channel type.
    BindingChannel,
    /// Level 8: Default fallback.
    Default,
}

/// The result of resolving an agent route.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedAgentRoute {
    /// The agent that should handle this message.
    pub agent_id: String,
    /// The channel the message came from.
    pub channel: String,
    /// The account the message was received on.
    pub account_id: String,
    /// The fully-scoped session key for this conversation.
    pub session_key: String,
    /// The agent's main (DM-merged) session key.
    pub main_session_key: String,
    /// Which priority level matched.
    pub matched_by: MatchedBy,
}

/// Inbound message context used for route resolution.
///
/// Carries the fields extracted from a message that are needed to
/// evaluate binding match rules and build session keys.
#[derive(Debug, Clone, Default)]
pub struct RouteContext {
    pub channel: String,
    pub account_id: String,
    pub peer_kind: String,
    pub peer_id: String,
    pub parent_peer_id: Option<String>,
    pub guild_id: Option<String>,
    pub team_id: Option<String>,
    pub roles: Vec<String>,
    pub dm_scope: DmScope,
    pub main_key: Option<String>,
    pub identity_links: Option<HashMap<String, Vec<String>>>,
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Resolve which agent should handle an inbound message.
///
/// Scans bindings in priority order (peer → parent → guild+roles → guild →
/// team → account → channel → default). The first match at the highest
/// priority level wins. If no binding matches, falls back to the default
/// agent.
pub fn resolve_agent_route(
    config: &OpenClawConfig,
    ctx: &RouteContext,
) -> ResolvedAgentRoute {
    let bindings = config.bindings.as_deref().unwrap_or(&[]);

    // Try each priority level in order
    if let Some(route) = try_match_peer(bindings, ctx) {
        return route;
    }
    if let Some(route) = try_match_parent_peer(bindings, ctx) {
        return route;
    }
    if let Some(route) = try_match_guild_roles(bindings, ctx) {
        return route;
    }
    if let Some(route) = try_match_guild(bindings, ctx) {
        return route;
    }
    if let Some(route) = try_match_team(bindings, ctx) {
        return route;
    }
    if let Some(route) = try_match_account(bindings, ctx) {
        return route;
    }
    if let Some(route) = try_match_channel(bindings, ctx) {
        return route;
    }

    // Level 8: Default fallback
    let agent_id = resolve_default_agent_id(config.agents.as_ref());
    build_route(&agent_id, ctx, MatchedBy::Default)
}

// ---------------------------------------------------------------------------
// Priority matchers
// ---------------------------------------------------------------------------

/// Level 1: Exact peer match. Binding must specify `peer.id` (and optionally
/// `peer.kind`), and may also require channel/account to match.
fn try_match_peer(bindings: &[BindingEntry], ctx: &RouteContext) -> Option<ResolvedAgentRoute> {
    for binding in bindings {
        let Some(m) = &binding.match_rule else { continue };
        let Some(peer) = &m.peer else { continue };
        let Some(peer_id) = peer.id.as_deref() else { continue };

        if !eq_ci(peer_id, &ctx.peer_id) {
            continue;
        }
        if let Some(kind) = &peer.kind {
            if !eq_ci(kind, &ctx.peer_kind) {
                continue;
            }
        }
        if !matches_channel_account(m, ctx) {
            continue;
        }
        let agent_id = binding.agent_id.as_deref().unwrap_or(DEFAULT_AGENT_ID);
        return Some(build_route(agent_id, ctx, MatchedBy::BindingPeer));
    }
    None
}

/// Level 2: Parent peer match (thread inheritance).
fn try_match_parent_peer(bindings: &[BindingEntry], ctx: &RouteContext) -> Option<ResolvedAgentRoute> {
    let parent_id = ctx.parent_peer_id.as_deref()?;
    if parent_id.is_empty() {
        return None;
    }

    for binding in bindings {
        let Some(m) = &binding.match_rule else { continue };
        let Some(peer) = &m.peer else { continue };
        let Some(peer_id) = peer.id.as_deref() else { continue };

        if !eq_ci(peer_id, parent_id) {
            continue;
        }
        if !matches_channel_account(m, ctx) {
            continue;
        }
        let agent_id = binding.agent_id.as_deref().unwrap_or(DEFAULT_AGENT_ID);
        return Some(build_route(agent_id, ctx, MatchedBy::BindingPeerParent));
    }
    None
}

/// Level 3: Guild + roles (Discord). Binding must specify `guild_id` AND
/// `roles`, and all specified roles must be present in the context.
fn try_match_guild_roles(bindings: &[BindingEntry], ctx: &RouteContext) -> Option<ResolvedAgentRoute> {
    let ctx_guild = ctx.guild_id.as_deref()?;
    if ctx_guild.is_empty() {
        return None;
    }

    for binding in bindings {
        let Some(m) = &binding.match_rule else { continue };
        let Some(guild_id) = &m.guild_id else { continue };
        let Some(roles) = &m.roles else { continue };

        if roles.is_empty() {
            continue; // No roles specified → not a guild+roles binding
        }
        if !eq_ci(guild_id, ctx_guild) {
            continue;
        }
        // All required roles must be present in context
        if !roles.iter().all(|r| ctx.roles.iter().any(|cr| eq_ci(r, cr))) {
            continue;
        }
        if !matches_channel_account(m, ctx) {
            continue;
        }
        let agent_id = binding.agent_id.as_deref().unwrap_or(DEFAULT_AGENT_ID);
        return Some(build_route(agent_id, ctx, MatchedBy::BindingGuildRoles));
    }
    None
}

/// Level 4: Guild alone (Discord). Binding has `guild_id` but no `roles`.
fn try_match_guild(bindings: &[BindingEntry], ctx: &RouteContext) -> Option<ResolvedAgentRoute> {
    let ctx_guild = ctx.guild_id.as_deref()?;
    if ctx_guild.is_empty() {
        return None;
    }

    for binding in bindings {
        let Some(m) = &binding.match_rule else { continue };
        let Some(guild_id) = &m.guild_id else { continue };

        // Skip if roles are specified (that's level 3)
        if m.roles.as_ref().is_some_and(|r| !r.is_empty()) {
            continue;
        }
        if !eq_ci(guild_id, ctx_guild) {
            continue;
        }
        if !matches_channel_account(m, ctx) {
            continue;
        }
        let agent_id = binding.agent_id.as_deref().unwrap_or(DEFAULT_AGENT_ID);
        return Some(build_route(agent_id, ctx, MatchedBy::BindingGuild));
    }
    None
}

/// Level 5: Slack team (workspace).
fn try_match_team(bindings: &[BindingEntry], ctx: &RouteContext) -> Option<ResolvedAgentRoute> {
    let ctx_team = ctx.team_id.as_deref()?;
    if ctx_team.is_empty() {
        return None;
    }

    for binding in bindings {
        let Some(m) = &binding.match_rule else { continue };
        let Some(team_id) = &m.team_id else { continue };

        if !eq_ci(team_id, ctx_team) {
            continue;
        }
        if !matches_channel_account(m, ctx) {
            continue;
        }
        let agent_id = binding.agent_id.as_deref().unwrap_or(DEFAULT_AGENT_ID);
        return Some(build_route(agent_id, ctx, MatchedBy::BindingTeam));
    }
    None
}

/// Level 6: Account match. Binding specifies `account_id` (and optionally
/// `channel`) but no peer/guild/team.
fn try_match_account(bindings: &[BindingEntry], ctx: &RouteContext) -> Option<ResolvedAgentRoute> {
    for binding in bindings {
        let Some(m) = &binding.match_rule else { continue };

        // Must have account_id specified
        let Some(acct_id) = &m.account_id else { continue };
        // Skip if this binding also has peer/guild/team (higher priority)
        if has_specific_match(m) {
            continue;
        }
        if !eq_ci(acct_id, &ctx.account_id) {
            continue;
        }
        // Channel must match if specified
        if let Some(ch) = &m.channel {
            if !eq_ci(ch, &ctx.channel) {
                continue;
            }
        }
        let agent_id = binding.agent_id.as_deref().unwrap_or(DEFAULT_AGENT_ID);
        return Some(build_route(agent_id, ctx, MatchedBy::BindingAccount));
    }
    None
}

/// Level 7: Channel match. Binding specifies only `channel`.
fn try_match_channel(bindings: &[BindingEntry], ctx: &RouteContext) -> Option<ResolvedAgentRoute> {
    for binding in bindings {
        let Some(m) = &binding.match_rule else { continue };

        let Some(ch) = &m.channel else { continue };
        // Skip if this binding also has account/peer/guild/team
        if m.account_id.is_some() || has_specific_match(m) {
            continue;
        }
        if !eq_ci(ch, &ctx.channel) {
            continue;
        }
        let agent_id = binding.agent_id.as_deref().unwrap_or(DEFAULT_AGENT_ID);
        return Some(build_route(agent_id, ctx, MatchedBy::BindingChannel));
    }
    None
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Check if a binding match has peer, guild, or team (i.e. is more specific
/// than just channel/account).
fn has_specific_match(m: &BindingMatch) -> bool {
    m.peer.is_some() || m.guild_id.is_some() || m.team_id.is_some()
}

/// Case-insensitive string equality.
fn eq_ci(a: &str, b: &str) -> bool {
    a.eq_ignore_ascii_case(b)
}

/// Check if the binding's optional channel/account match the context.
fn matches_channel_account(m: &BindingMatch, ctx: &RouteContext) -> bool {
    if let Some(ch) = &m.channel {
        if !eq_ci(ch, &ctx.channel) {
            return false;
        }
    }
    if let Some(acct) = &m.account_id {
        if !eq_ci(acct, &ctx.account_id) {
            return false;
        }
    }
    true
}

/// Determine the default agent ID from config.
///
/// Priority: agent with `default: true` → first agent in list → DEFAULT_AGENT_ID.
fn resolve_default_agent_id(agents: Option<&AgentsConfig>) -> String {
    let Some(agents) = agents else {
        return DEFAULT_AGENT_ID.to_string();
    };
    let Some(list) = &agents.list else {
        return DEFAULT_AGENT_ID.to_string();
    };

    // Look for agent with default=true
    for agent in list {
        if agent.default == Some(true) {
            if let Some(id) = &agent.id {
                return session_key::normalize_agent_id(Some(id));
            }
        }
    }

    // Fall back to first agent
    if let Some(first) = list.first() {
        if let Some(id) = &first.id {
            return session_key::normalize_agent_id(Some(id));
        }
    }

    DEFAULT_AGENT_ID.to_string()
}

/// Build a [`ResolvedAgentRoute`] from context and match level.
fn build_route(agent_id: &str, ctx: &RouteContext, matched_by: MatchedBy) -> ResolvedAgentRoute {
    let agent_id = session_key::normalize_agent_id(Some(agent_id));
    let main_key = ctx.main_key.as_deref();

    let main_session_key = session_key::build_agent_main_session_key(&agent_id, main_key);

    let session_key = session_key::build_agent_peer_session_key(
        &session_key::PeerSessionKeyParams {
            agent_id: &agent_id,
            main_key,
            channel: &ctx.channel,
            account_id: Some(&ctx.account_id),
            peer_kind: Some(&ctx.peer_kind),
            peer_id: Some(&ctx.peer_id),
            dm_scope: Some(ctx.dm_scope),
            identity_links: ctx.identity_links.as_ref(),
        },
    );

    ResolvedAgentRoute {
        agent_id,
        channel: ctx.channel.clone(),
        account_id: ctx.account_id.clone(),
        session_key,
        main_session_key,
        matched_by,
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use claw_config::{AgentEntry, BindingPeer};

    /// Helper: build config with given bindings and agents.
    fn config_with(bindings: Vec<BindingEntry>, agents: Vec<AgentEntry>) -> OpenClawConfig {
        OpenClawConfig {
            bindings: Some(bindings),
            agents: Some(AgentsConfig {
                list: Some(agents),
                ..Default::default()
            }),
            ..Default::default()
        }
    }

    fn default_ctx() -> RouteContext {
        RouteContext {
            channel: "telegram".into(),
            account_id: "default".into(),
            peer_kind: "direct".into(),
            peer_id: "user123".into(),
            dm_scope: DmScope::PerPeer,
            ..Default::default()
        }
    }

    // -- Level 8: Default fallback --

    #[test]
    fn no_bindings_falls_back_to_default() {
        let config = config_with(vec![], vec![
            AgentEntry { id: Some("first".into()), ..Default::default() },
        ]);
        let route = resolve_agent_route(&config, &default_ctx());
        assert_eq!(route.agent_id, "first");
        assert_eq!(route.matched_by, MatchedBy::Default);
    }

    #[test]
    fn empty_config_uses_main_agent() {
        let config = OpenClawConfig::default();
        let route = resolve_agent_route(&config, &default_ctx());
        assert_eq!(route.agent_id, DEFAULT_AGENT_ID);
        assert_eq!(route.matched_by, MatchedBy::Default);
    }

    #[test]
    fn default_agent_flag_wins_over_first() {
        let config = config_with(vec![], vec![
            AgentEntry { id: Some("first".into()), ..Default::default() },
            AgentEntry { id: Some("chosen".into()), default: Some(true), ..Default::default() },
        ]);
        let route = resolve_agent_route(&config, &default_ctx());
        assert_eq!(route.agent_id, "chosen");
        assert_eq!(route.matched_by, MatchedBy::Default);
    }

    // -- Level 7: Channel match --

    #[test]
    fn channel_binding_matches() {
        let config = config_with(
            vec![BindingEntry {
                agent_id: Some("telegram-bot".into()),
                match_rule: Some(BindingMatch {
                    channel: Some("telegram".into()),
                    ..Default::default()
                }),
            }],
            vec![],
        );
        let route = resolve_agent_route(&config, &default_ctx());
        assert_eq!(route.agent_id, "telegram-bot");
        assert_eq!(route.matched_by, MatchedBy::BindingChannel);
    }

    #[test]
    fn channel_binding_no_match() {
        let config = config_with(
            vec![BindingEntry {
                agent_id: Some("discord-bot".into()),
                match_rule: Some(BindingMatch {
                    channel: Some("discord".into()),
                    ..Default::default()
                }),
            }],
            vec![],
        );
        let route = resolve_agent_route(&config, &default_ctx());
        assert_eq!(route.matched_by, MatchedBy::Default);
    }

    // -- Level 6: Account match --

    #[test]
    fn account_binding_matches() {
        let config = config_with(
            vec![BindingEntry {
                agent_id: Some("acct-agent".into()),
                match_rule: Some(BindingMatch {
                    channel: Some("telegram".into()),
                    account_id: Some("default".into()),
                    ..Default::default()
                }),
            }],
            vec![],
        );
        let route = resolve_agent_route(&config, &default_ctx());
        assert_eq!(route.agent_id, "acct-agent");
        assert_eq!(route.matched_by, MatchedBy::BindingAccount);
    }

    // -- Level 5: Team match --

    #[test]
    fn team_binding_matches() {
        let config = config_with(
            vec![BindingEntry {
                agent_id: Some("slack-agent".into()),
                match_rule: Some(BindingMatch {
                    team_id: Some("T12345".into()),
                    ..Default::default()
                }),
            }],
            vec![],
        );
        let mut ctx = default_ctx();
        ctx.channel = "slack".into();
        ctx.team_id = Some("T12345".into());
        let route = resolve_agent_route(&config, &ctx);
        assert_eq!(route.agent_id, "slack-agent");
        assert_eq!(route.matched_by, MatchedBy::BindingTeam);
    }

    // -- Level 4: Guild match --

    #[test]
    fn guild_binding_matches() {
        let config = config_with(
            vec![BindingEntry {
                agent_id: Some("guild-agent".into()),
                match_rule: Some(BindingMatch {
                    guild_id: Some("G999".into()),
                    ..Default::default()
                }),
            }],
            vec![],
        );
        let mut ctx = default_ctx();
        ctx.channel = "discord".into();
        ctx.guild_id = Some("G999".into());
        let route = resolve_agent_route(&config, &ctx);
        assert_eq!(route.agent_id, "guild-agent");
        assert_eq!(route.matched_by, MatchedBy::BindingGuild);
    }

    // -- Level 3: Guild + roles --

    #[test]
    fn guild_roles_binding_matches() {
        let config = config_with(
            vec![BindingEntry {
                agent_id: Some("admin-agent".into()),
                match_rule: Some(BindingMatch {
                    guild_id: Some("G999".into()),
                    roles: Some(vec!["admin".into()]),
                    ..Default::default()
                }),
            }],
            vec![],
        );
        let mut ctx = default_ctx();
        ctx.channel = "discord".into();
        ctx.guild_id = Some("G999".into());
        ctx.roles = vec!["admin".into(), "member".into()];
        let route = resolve_agent_route(&config, &ctx);
        assert_eq!(route.agent_id, "admin-agent");
        assert_eq!(route.matched_by, MatchedBy::BindingGuildRoles);
    }

    #[test]
    fn guild_roles_missing_role_no_match() {
        let config = config_with(
            vec![BindingEntry {
                agent_id: Some("admin-agent".into()),
                match_rule: Some(BindingMatch {
                    guild_id: Some("G999".into()),
                    roles: Some(vec!["admin".into()]),
                    ..Default::default()
                }),
            }],
            vec![],
        );
        let mut ctx = default_ctx();
        ctx.channel = "discord".into();
        ctx.guild_id = Some("G999".into());
        ctx.roles = vec!["member".into()]; // No "admin" role
        let route = resolve_agent_route(&config, &ctx);
        // Should fall through to default since guild+roles doesn't match
        // and guild-alone skips bindings with roles
        assert_eq!(route.matched_by, MatchedBy::Default);
    }

    // -- Level 1: Peer match --

    #[test]
    fn peer_binding_matches() {
        let config = config_with(
            vec![BindingEntry {
                agent_id: Some("vip-agent".into()),
                match_rule: Some(BindingMatch {
                    peer: Some(BindingPeer {
                        id: Some("user123".into()),
                        kind: Some("direct".into()),
                    }),
                    channel: Some("telegram".into()),
                    ..Default::default()
                }),
            }],
            vec![],
        );
        let route = resolve_agent_route(&config, &default_ctx());
        assert_eq!(route.agent_id, "vip-agent");
        assert_eq!(route.matched_by, MatchedBy::BindingPeer);
    }

    #[test]
    fn peer_binding_wrong_channel_no_match() {
        let config = config_with(
            vec![BindingEntry {
                agent_id: Some("vip-agent".into()),
                match_rule: Some(BindingMatch {
                    peer: Some(BindingPeer {
                        id: Some("user123".into()),
                        ..Default::default()
                    }),
                    channel: Some("discord".into()), // Wrong channel
                    ..Default::default()
                }),
            }],
            vec![],
        );
        let route = resolve_agent_route(&config, &default_ctx());
        assert_eq!(route.matched_by, MatchedBy::Default); // No match due to AND
    }

    // -- Level 2: Parent peer match --

    #[test]
    fn parent_peer_binding_matches() {
        let config = config_with(
            vec![BindingEntry {
                agent_id: Some("thread-agent".into()),
                match_rule: Some(BindingMatch {
                    peer: Some(BindingPeer {
                        id: Some("group456".into()),
                        ..Default::default()
                    }),
                    channel: Some("telegram".into()),
                    ..Default::default()
                }),
            }],
            vec![],
        );
        let mut ctx = default_ctx();
        ctx.peer_id = "thread789".into();
        ctx.peer_kind = "thread".into();
        ctx.parent_peer_id = Some("group456".into());
        let route = resolve_agent_route(&config, &ctx);
        assert_eq!(route.agent_id, "thread-agent");
        assert_eq!(route.matched_by, MatchedBy::BindingPeerParent);
    }

    // -- Priority ordering --

    #[test]
    fn peer_beats_channel() {
        let config = config_with(
            vec![
                BindingEntry {
                    agent_id: Some("channel-agent".into()),
                    match_rule: Some(BindingMatch {
                        channel: Some("telegram".into()),
                        ..Default::default()
                    }),
                },
                BindingEntry {
                    agent_id: Some("peer-agent".into()),
                    match_rule: Some(BindingMatch {
                        peer: Some(BindingPeer {
                            id: Some("user123".into()),
                            ..Default::default()
                        }),
                        ..Default::default()
                    }),
                },
            ],
            vec![],
        );
        let route = resolve_agent_route(&config, &default_ctx());
        assert_eq!(route.agent_id, "peer-agent");
        assert_eq!(route.matched_by, MatchedBy::BindingPeer);
    }

    #[test]
    fn account_beats_channel() {
        let config = config_with(
            vec![
                BindingEntry {
                    agent_id: Some("channel-agent".into()),
                    match_rule: Some(BindingMatch {
                        channel: Some("telegram".into()),
                        ..Default::default()
                    }),
                },
                BindingEntry {
                    agent_id: Some("acct-agent".into()),
                    match_rule: Some(BindingMatch {
                        channel: Some("telegram".into()),
                        account_id: Some("default".into()),
                        ..Default::default()
                    }),
                },
            ],
            vec![],
        );
        let route = resolve_agent_route(&config, &default_ctx());
        assert_eq!(route.agent_id, "acct-agent");
        assert_eq!(route.matched_by, MatchedBy::BindingAccount);
    }

    // -- Session key generation --

    #[test]
    fn route_generates_correct_session_keys() {
        let config = OpenClawConfig::default();
        let ctx = RouteContext {
            channel: "telegram".into(),
            account_id: "default".into(),
            peer_kind: "direct".into(),
            peer_id: "user123".into(),
            dm_scope: DmScope::PerChannelPeer,
            ..Default::default()
        };
        let route = resolve_agent_route(&config, &ctx);
        assert_eq!(route.main_session_key, "agent:main:main");
        assert_eq!(route.session_key, "agent:main:telegram:direct:user123");
    }

    #[test]
    fn route_group_session_key() {
        let config = OpenClawConfig::default();
        let ctx = RouteContext {
            channel: "telegram".into(),
            account_id: "default".into(),
            peer_kind: "group".into(),
            peer_id: "-100123456".into(),
            dm_scope: DmScope::Main,
            ..Default::default()
        };
        let route = resolve_agent_route(&config, &ctx);
        assert_eq!(route.session_key, "agent:main:telegram:group:-100123456");
    }

    // -- Case insensitivity --

    #[test]
    fn matching_is_case_insensitive() {
        let config = config_with(
            vec![BindingEntry {
                agent_id: Some("tg-agent".into()),
                match_rule: Some(BindingMatch {
                    channel: Some("Telegram".into()),
                    ..Default::default()
                }),
            }],
            vec![],
        );
        let route = resolve_agent_route(&config, &default_ctx());
        assert_eq!(route.agent_id, "tg-agent");
        assert_eq!(route.matched_by, MatchedBy::BindingChannel);
    }
}
