//! Configuration type definitions for OpenClaw.
//!
//! Ports the config schema from OpenClaw's TypeScript types to Rust structs.
//! All fields use `Option<T>` with serde defaults since configs are highly
//! flexible and most fields are optional.

pub mod agents;
pub mod bindings;
pub mod channels;
pub mod commands;
pub mod gateway;
pub mod helpers;
pub mod messages;
pub mod misc;
pub mod models;
pub mod plugins;
pub mod session;
pub mod tools;
pub mod top_level;

pub use agents::*;
pub use bindings::*;
pub use channels::*;
pub use commands::*;
pub use gateway::*;
pub use helpers::*;
pub use messages::*;
pub use misc::*;
pub use models::*;
pub use plugins::*;
pub use session::*;
pub use tools::*;
pub use top_level::*;

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_config_deserializes() {
        let config: OpenClawConfig = serde_json::from_str("{}").unwrap();
        assert!(config.agents.is_none());
        assert!(config.gateway.is_none());
        assert!(config.channels.is_none());
    }

    #[test]
    fn default_config_serializes_to_empty_object() {
        let config = OpenClawConfig::default();
        let json = serde_json::to_value(&config).unwrap();
        assert_eq!(json, serde_json::json!({}));
    }

    #[test]
    fn gateway_bind_mode_roundtrip() {
        let mode = BindMode::Lan;
        let json = serde_json::to_string(&mode).unwrap();
        assert_eq!(json, r#""lan""#);
        let parsed: BindMode = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, BindMode::Lan);
    }

    #[test]
    fn gateway_config_parses() {
        let json = r#"{
            "mode": "local",
            "bind": "lan",
            "auth": {
                "mode": "token",
                "token": "secret123"
            }
        }"#;
        let gw: GatewayConfig = serde_json::from_str(json).unwrap();
        assert_eq!(gw.mode.as_deref(), Some("local"));
        assert_eq!(gw.bind, Some(BindMode::Lan));
        assert_eq!(gw.auth.as_ref().unwrap().mode.as_deref(), Some("token"));
    }

    #[test]
    fn channel_config_with_accounts() {
        let json = r#"{
            "enabled": true,
            "dmPolicy": "pairing",
            "accounts": {
                "main": { "botToken": "tok123" }
            }
        }"#;
        let ch: ChannelConfig = serde_json::from_str(json).unwrap();
        assert_eq!(ch.enabled, Some(true));
        assert_eq!(ch.dm_policy.as_deref(), Some("pairing"));
        let accts = ch.accounts.unwrap();
        assert_eq!(
            accts.get("main").unwrap().bot_token.as_deref(),
            Some("tok123")
        );
    }

    #[test]
    fn binding_entry_parses() {
        let json = r#"{
            "agentId": "main",
            "match": {
                "channel": "telegram",
                "accountId": "main"
            }
        }"#;
        let b: BindingEntry = serde_json::from_str(json).unwrap();
        assert_eq!(b.agent_id.as_deref(), Some("main"));
        let m = b.match_rule.unwrap();
        assert_eq!(m.channel.as_deref(), Some("telegram"));
        assert_eq!(m.account_id.as_deref(), Some("main"));
    }

    #[test]
    fn agent_entry_parses() {
        let json = r#"{
            "id": "einstein",
            "name": "einstein",
            "model": "anthropic/claude-haiku-4-5-20251001",
            "subagents": { "allowAgents": [] }
        }"#;
        let a: AgentEntry = serde_json::from_str(json).unwrap();
        assert_eq!(a.id.as_deref(), Some("einstein"));
        assert_eq!(
            a.model.as_deref(),
            Some("anthropic/claude-haiku-4-5-20251001")
        );
        assert!(a.subagents.unwrap().allow_agents.unwrap().is_empty());
    }

    #[test]
    fn model_entry_parses() {
        let json = r#"{
            "id": "claude-opus-4-6",
            "name": "anthropic/claude-opus-4-6",
            "input": ["text", "image"],
            "reasoning": true,
            "contextWindow": 200000,
            "maxTokens": 128000
        }"#;
        let m: ModelEntry = serde_json::from_str(json).unwrap();
        assert_eq!(m.id.as_deref(), Some("claude-opus-4-6"));
        assert_eq!(m.reasoning, Some(true));
        assert_eq!(m.context_window, Some(200000));
        assert_eq!(m.max_tokens, Some(128000));
    }

    #[test]
    fn full_config_roundtrip() {
        let json = include_str!("../../../../config/config.json");
        let config: OpenClawConfig = serde_json::from_str(json).unwrap();
        assert!(config.agents.is_some());
        assert!(config.channels.is_some());
        assert!(config.gateway.is_some());
        assert!(config.models.is_some());
        assert!(config.bindings.is_some());

        // Round-trip
        let serialized = serde_json::to_string(&config).unwrap();
        let reparsed: OpenClawConfig = serde_json::from_str(&serialized).unwrap();
        assert!(reparsed.agents.is_some());
    }

    #[test]
    fn extra_fields_preserved() {
        let json = r#"{"customField": 42, "anotherOne": "hello"}"#;
        let config: OpenClawConfig = serde_json::from_str(json).unwrap();
        assert_eq!(
            config.extra.get("customField").unwrap(),
            &serde_json::json!(42)
        );
        assert_eq!(
            config.extra.get("anotherOne").unwrap(),
            &serde_json::json!("hello")
        );
    }

    #[test]
    fn telegram_account_config_parses() {
        let json = r#"{
            "botToken": "123:ABC",
            "webhookUrl": "https://example.com/hook",
            "webhookSecret": "sec123",
            "allowedUpdates": ["message", "callback_query"],
            "pollTimeoutSecs": 60
        }"#;
        let cfg: TelegramAccountConfig = serde_json::from_str(json).unwrap();
        assert_eq!(cfg.bot_token.as_deref(), Some("123:ABC"));
        assert_eq!(
            cfg.webhook_url.as_deref(),
            Some("https://example.com/hook")
        );
        assert_eq!(cfg.webhook_secret.as_deref(), Some("sec123"));
        assert_eq!(cfg.allowed_updates.as_ref().unwrap().len(), 2);
        assert_eq!(cfg.poll_timeout_secs, Some(60));
    }

    #[test]
    fn telegram_account_from_generic() {
        let json = r#"{"botToken": "123:ABC", "webhookUrl": "https://example.com"}"#;
        let generic: ChannelAccountConfig = serde_json::from_str(json).unwrap();
        let tg = TelegramAccountConfig::try_from(&generic).unwrap();
        assert_eq!(tg.bot_token.as_deref(), Some("123:ABC"));
        assert_eq!(tg.webhook_url.as_deref(), Some("https://example.com"));
    }

    #[test]
    fn whatsapp_account_config_parses() {
        let json = r#"{
            "storePath": "/data/wa-store.db",
            "phoneNumber": "+1234567890",
            "usePairingCode": true,
            "heartbeatIntervalSecs": 15,
            "maxReconnectAttempts": 5
        }"#;
        let cfg: WhatsAppAccountConfig = serde_json::from_str(json).unwrap();
        assert_eq!(cfg.store_path.as_deref(), Some("/data/wa-store.db"));
        assert_eq!(cfg.phone_number.as_deref(), Some("+1234567890"));
        assert_eq!(cfg.use_pairing_code, Some(true));
        assert_eq!(cfg.heartbeat_interval_secs, Some(15));
        assert_eq!(cfg.max_reconnect_attempts, Some(5));
    }

    #[test]
    fn whatsapp_account_from_generic() {
        let json = r#"{"storePath": "/data/wa.db"}"#;
        let generic: ChannelAccountConfig = serde_json::from_str(json).unwrap();
        let wa = WhatsAppAccountConfig::try_from(&generic).unwrap();
        assert_eq!(wa.store_path.as_deref(), Some("/data/wa.db"));
        assert!(wa.phone_number.is_none());
    }

    #[test]
    fn discord_account_config_parses() {
        let json = r#"{
            "botToken": "discord-tok",
            "applicationId": "1234567890",
            "intents": 3276799,
            "shardCount": 2,
            "syncCommands": true
        }"#;
        let cfg: DiscordAccountConfig = serde_json::from_str(json).unwrap();
        assert_eq!(cfg.bot_token.as_deref(), Some("discord-tok"));
        assert_eq!(cfg.application_id.as_deref(), Some("1234567890"));
        assert_eq!(cfg.intents, Some(3276799));
        assert_eq!(cfg.shard_count, Some(2));
        assert_eq!(cfg.sync_commands, Some(true));
    }

    #[test]
    fn discord_account_from_generic() {
        let json = r#"{"botToken": "disc-tok", "applicationId": "app123"}"#;
        let generic: ChannelAccountConfig = serde_json::from_str(json).unwrap();
        let dc = DiscordAccountConfig::try_from(&generic).unwrap();
        assert_eq!(dc.bot_token.as_deref(), Some("disc-tok"));
        assert_eq!(dc.application_id.as_deref(), Some("app123"));
    }

    #[test]
    fn platform_config_defaults_are_empty() {
        let tg = TelegramAccountConfig::default();
        let json = serde_json::to_value(&tg).unwrap();
        assert_eq!(json, serde_json::json!({}));

        let wa = WhatsAppAccountConfig::default();
        let json = serde_json::to_value(&wa).unwrap();
        assert_eq!(json, serde_json::json!({}));

        let dc = DiscordAccountConfig::default();
        let json = serde_json::to_value(&dc).unwrap();
        assert_eq!(json, serde_json::json!({}));
    }
}
