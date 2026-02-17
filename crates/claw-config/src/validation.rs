//! Configuration validation for OpenClaw configs.
//!
//! Validates semantic correctness of a deserialized [`OpenClawConfig`]:
//! port ranges, required tokens, duplicate IDs, etc. Produces a
//! [`ValidationResult`] with categorized issues rather than failing
//! on the first error.

use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use crate::types::{BindingEntry, OpenClawConfig};

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// Severity level for validation issues.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum IssueSeverity {
    Error,
    Warning,
    Info,
}

/// A single validation finding.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigValidationIssue {
    /// JSON-style path to the offending field (e.g. `"gateway.port"`).
    pub path: String,
    /// Human-readable description of the issue.
    pub message: String,
    /// How severe this issue is.
    pub severity: IssueSeverity,
}

/// Aggregated validation result.
#[derive(Debug, Clone, Default)]
pub struct ValidationResult {
    pub issues: Vec<ConfigValidationIssue>,
}

impl ValidationResult {
    /// Returns `true` if any issue has [`IssueSeverity::Error`].
    pub fn has_errors(&self) -> bool {
        self.issues.iter().any(|i| i.severity == IssueSeverity::Error)
    }

    /// All error-level issues.
    pub fn errors(&self) -> Vec<&ConfigValidationIssue> {
        self.issues
            .iter()
            .filter(|i| i.severity == IssueSeverity::Error)
            .collect()
    }

    /// All warning-level issues.
    pub fn warnings(&self) -> Vec<&ConfigValidationIssue> {
        self.issues
            .iter()
            .filter(|i| i.severity == IssueSeverity::Warning)
            .collect()
    }

    fn push(&mut self, path: impl Into<String>, message: impl Into<String>, severity: IssueSeverity) {
        self.issues.push(ConfigValidationIssue {
            path: path.into(),
            message: message.into(),
            severity,
        });
    }

    fn error(&mut self, path: impl Into<String>, message: impl Into<String>) {
        self.push(path, message, IssueSeverity::Error);
    }

    fn warning(&mut self, path: impl Into<String>, message: impl Into<String>) {
        self.push(path, message, IssueSeverity::Warning);
    }
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Validate an [`OpenClawConfig`] for semantic correctness.
///
/// Returns all issues found (errors, warnings, info) rather than stopping
/// at the first failure.
pub fn validate_config(config: &OpenClawConfig) -> ValidationResult {
    let mut result = ValidationResult::default();

    validate_gateway(config, &mut result);
    validate_channels(config, &mut result);
    validate_agents(config, &mut result);
    validate_bindings(config, &mut result);
    validate_models(config, &mut result);
    validate_extra_keys(config, &mut result);

    result
}

/// Apply basic legacy config migrations.
///
/// Converts deprecated field names found in `extra` catch-all maps and
/// emits warnings for deprecated patterns. Mutates the config in place.
pub fn migrate_legacy_config(config: &mut OpenClawConfig, issues: &mut Vec<ConfigValidationIssue>) {
    // Migrate deprecated top-level "server" key → "gateway"
    if let Some(server_val) = config.extra.remove("server") {
        if config.gateway.is_none() {
            if let Ok(gw) = serde_json::from_value(server_val) {
                config.gateway = Some(gw);
                issues.push(ConfigValidationIssue {
                    path: "server".into(),
                    message: "deprecated key 'server' migrated to 'gateway'".into(),
                    severity: IssueSeverity::Warning,
                });
            }
        }
    }

    // Migrate deprecated "bot" key → "agents"
    if let Some(bot_val) = config.extra.remove("bot") {
        if config.agents.is_none() {
            if let Ok(agents) = serde_json::from_value(bot_val) {
                config.agents = Some(agents);
                issues.push(ConfigValidationIssue {
                    path: "bot".into(),
                    message: "deprecated key 'bot' migrated to 'agents'".into(),
                    severity: IssueSeverity::Warning,
                });
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Validation helpers
// ---------------------------------------------------------------------------

fn validate_gateway(config: &OpenClawConfig, result: &mut ValidationResult) {
    let Some(gw) = &config.gateway else { return };

    // Port range (u16 already constrains 0..65535, but 0 is invalid for binding)
    if let Some(port) = gw.port {
        if port == 0 {
            result.error("gateway.port", "port must be between 1 and 65535");
        }
    }

    // Auth mode "token" requires a token value
    if let Some(auth) = &gw.auth {
        if let Some(mode) = &auth.mode {
            if mode == "token" {
                match &auth.token {
                    None => result.error(
                        "gateway.auth.token",
                        "auth mode 'token' requires a non-empty token",
                    ),
                    Some(t) if t.is_empty() => result.error(
                        "gateway.auth.token",
                        "auth mode 'token' requires a non-empty token",
                    ),
                    _ => {}
                }
            }
        }
    }
}

fn validate_channels(config: &OpenClawConfig, result: &mut ValidationResult) {
    let Some(channels) = &config.channels else { return };

    for (name, ch) in channels {
        let prefix = format!("channels.{name}");

        // Telegram accounts need a bot_token
        if name == "telegram" {
            if let Some(accounts) = &ch.accounts {
                for (acct_id, acct) in accounts {
                    if acct.bot_token.as_ref().is_none_or(|t| t.is_empty()) {
                        result.error(
                            format!("{prefix}.accounts.{acct_id}.botToken"),
                            "telegram account requires a bot_token",
                        );
                    }
                }
            }
        }

        // Warn if channel is explicitly disabled
        if ch.enabled == Some(false) {
            result.warning(format!("{prefix}.enabled"), format!("channel '{name}' is disabled"));
        }
    }
}

fn validate_agents(config: &OpenClawConfig, result: &mut ValidationResult) {
    let Some(agents) = &config.agents else { return };
    let Some(list) = &agents.list else { return };

    let mut seen_ids = HashSet::new();

    for (idx, agent) in list.iter().enumerate() {
        let prefix = format!("agents.list[{idx}]");

        match &agent.id {
            None => result.error(format!("{prefix}.id"), "agent entry must have a non-empty id"),
            Some(id) if id.is_empty() => {
                result.error(format!("{prefix}.id"), "agent entry must have a non-empty id")
            }
            Some(id) => {
                if !seen_ids.insert(id.clone()) {
                    result.error(format!("{prefix}.id"), format!("duplicate agent id '{id}'"));
                }
            }
        }
    }
}

fn validate_bindings(config: &OpenClawConfig, result: &mut ValidationResult) {
    let Some(bindings) = &config.bindings else { return };

    let mut seen_patterns = HashSet::new();

    for (idx, binding) in bindings.iter().enumerate() {
        let prefix = format!("bindings[{idx}]");

        // Binding should reference an agent
        if binding.agent_id.as_ref().is_none_or(|id| id.is_empty()) {
            result.error(format!("{prefix}.agentId"), "binding must reference an agent_id");
        }

        // Check for duplicate match patterns
        let pattern_key = binding_pattern_key(binding);
        if !seen_patterns.insert(pattern_key.clone()) {
            result.warning(
                format!("{prefix}.match"),
                format!("duplicate binding pattern: {pattern_key}"),
            );
        }
    }
}

fn validate_models(config: &OpenClawConfig, result: &mut ValidationResult) {
    let Some(models) = &config.models else { return };
    let Some(providers) = &models.providers else { return };

    for (provider_name, provider) in providers {
        let Some(entries) = &provider.models else { continue };

        for (idx, entry) in entries.iter().enumerate() {
            let prefix = format!("models.providers.{provider_name}.models[{idx}]");

            if entry.id.as_ref().is_none_or(|id| id.is_empty()) {
                result.error(format!("{prefix}.id"), "model entry must have a non-empty id");
            }
        }
    }
}

fn validate_extra_keys(config: &OpenClawConfig, result: &mut ValidationResult) {
    // Warn about known deprecated keys in the catch-all
    for key in config.extra.keys() {
        if key == "server" || key == "bot" {
            result.warning(
                key.clone(),
                format!("deprecated top-level key '{key}'; run migration to update"),
            );
        }
    }
}

/// Build a dedup key from a binding's match criteria.
fn binding_pattern_key(binding: &BindingEntry) -> String {
    let channel = binding
        .match_rule
        .as_ref()
        .and_then(|m| m.channel.as_deref())
        .unwrap_or("*");
    let account = binding
        .match_rule
        .as_ref()
        .and_then(|m| m.account_id.as_deref())
        .unwrap_or("*");
    format!("{channel}:{account}")
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::*;
    use std::collections::HashMap;

    /// Helper: build a minimal valid config.
    fn valid_config() -> OpenClawConfig {
        OpenClawConfig {
            gateway: Some(GatewayConfig {
                port: Some(3000),
                bind: Some(BindMode::Localhost),
                ..Default::default()
            }),
            agents: Some(AgentsConfig {
                list: Some(vec![AgentEntry {
                    id: Some("main".into()),
                    name: Some("Main Agent".into()),
                    ..Default::default()
                }]),
                ..Default::default()
            }),
            bindings: Some(vec![BindingEntry {
                agent_id: Some("main".into()),
                match_rule: Some(BindingMatch {
                    channel: Some("telegram".into()),
                    account_id: Some("default".into()),
                    ..Default::default()
                }),
            }]),
            channels: Some({
                let mut m = HashMap::new();
                m.insert("telegram".into(), ChannelConfig {
                    enabled: Some(true),
                    accounts: Some({
                        let mut a = HashMap::new();
                        a.insert("default".into(), ChannelAccountConfig {
                            bot_token: Some("tok_123".into()),
                            ..Default::default()
                        });
                        a
                    }),
                    ..Default::default()
                });
                m
            }),
            models: Some(ModelsConfig {
                providers: Some({
                    let mut m = HashMap::new();
                    m.insert("anthropic".into(), ModelProvider {
                        models: Some(vec![ModelEntry {
                            id: Some("claude-sonnet-4-5-20250929".into()),
                            ..Default::default()
                        }]),
                        ..Default::default()
                    });
                    m
                }),
                ..Default::default()
            }),
            ..Default::default()
        }
    }

    #[test]
    fn valid_config_produces_no_errors() {
        let result = validate_config(&valid_config());
        assert!(!result.has_errors(), "unexpected errors: {:?}", result.errors());
    }

    #[test]
    fn empty_config_is_valid() {
        let result = validate_config(&OpenClawConfig::default());
        assert!(!result.has_errors());
    }

    #[test]
    fn invalid_port_zero_caught() {
        let mut config = valid_config();
        config.gateway.as_mut().unwrap().port = Some(0);
        let result = validate_config(&config);
        assert!(result.has_errors());
        assert!(result.errors().iter().any(|i| i.path == "gateway.port"));
    }

    #[test]
    fn missing_auth_token_caught() {
        let mut config = valid_config();
        config.gateway.as_mut().unwrap().auth = Some(GatewayAuthConfig {
            mode: Some("token".into()),
            token: None,
            ..Default::default()
        });
        let result = validate_config(&config);
        assert!(result.has_errors());
        assert!(result.errors().iter().any(|i| i.path == "gateway.auth.token"));
    }

    #[test]
    fn empty_auth_token_caught() {
        let mut config = valid_config();
        config.gateway.as_mut().unwrap().auth = Some(GatewayAuthConfig {
            mode: Some("token".into()),
            token: Some(String::new()),
            ..Default::default()
        });
        let result = validate_config(&config);
        assert!(result.has_errors());
        assert!(result.errors().iter().any(|i| i.path == "gateway.auth.token"));
    }

    #[test]
    fn duplicate_agent_ids_caught() {
        let mut config = valid_config();
        let agents = config.agents.as_mut().unwrap();
        agents.list = Some(vec![
            AgentEntry { id: Some("dup".into()), ..Default::default() },
            AgentEntry { id: Some("dup".into()), ..Default::default() },
        ]);
        let result = validate_config(&config);
        assert!(result.has_errors());
        assert!(result.errors().iter().any(|i| i.message.contains("duplicate agent id")));
    }

    #[test]
    fn empty_agent_id_caught() {
        let mut config = valid_config();
        let agents = config.agents.as_mut().unwrap();
        agents.list = Some(vec![AgentEntry {
            id: Some(String::new()),
            ..Default::default()
        }]);
        let result = validate_config(&config);
        assert!(result.has_errors());
        assert!(result.errors().iter().any(|i| i.path.contains("agents.list")));
    }

    #[test]
    fn missing_agent_id_caught() {
        let mut config = valid_config();
        let agents = config.agents.as_mut().unwrap();
        agents.list = Some(vec![AgentEntry::default()]);
        let result = validate_config(&config);
        assert!(result.has_errors());
    }

    #[test]
    fn telegram_missing_bot_token_caught() {
        let mut config = valid_config();
        let channels = config.channels.as_mut().unwrap();
        channels.insert("telegram".into(), ChannelConfig {
            enabled: Some(true),
            accounts: Some({
                let mut a = HashMap::new();
                a.insert("bot1".into(), ChannelAccountConfig {
                    bot_token: None,
                    ..Default::default()
                });
                a
            }),
            ..Default::default()
        });
        let result = validate_config(&config);
        assert!(result.has_errors());
        assert!(result.errors().iter().any(|i| i.path.contains("botToken")));
    }

    #[test]
    fn disabled_channel_produces_warning() {
        let mut config = valid_config();
        let channels = config.channels.as_mut().unwrap();
        channels.insert("discord".into(), ChannelConfig {
            enabled: Some(false),
            ..Default::default()
        });
        let result = validate_config(&config);
        assert!(!result.warnings().is_empty());
        assert!(result.warnings().iter().any(|i| i.message.contains("disabled")));
    }

    #[test]
    fn duplicate_binding_patterns_warned() {
        let mut config = valid_config();
        config.bindings = Some(vec![
            BindingEntry {
                agent_id: Some("a".into()),
                match_rule: Some(BindingMatch {
                    channel: Some("telegram".into()),
                    account_id: Some("main".into()),
                    ..Default::default()
                }),
            },
            BindingEntry {
                agent_id: Some("b".into()),
                match_rule: Some(BindingMatch {
                    channel: Some("telegram".into()),
                    account_id: Some("main".into()),
                    ..Default::default()
                }),
            },
        ]);
        let result = validate_config(&config);
        assert!(result.warnings().iter().any(|i| i.message.contains("duplicate binding")));
    }

    #[test]
    fn model_entry_without_id_caught() {
        let mut config = valid_config();
        let models = config.models.as_mut().unwrap();
        let providers = models.providers.as_mut().unwrap();
        providers.insert("test".into(), ModelProvider {
            models: Some(vec![ModelEntry::default()]),
            ..Default::default()
        });
        let result = validate_config(&config);
        assert!(result.has_errors());
        assert!(result.errors().iter().any(|i| i.path.contains("models.providers")));
    }

    #[test]
    fn binding_without_agent_id_caught() {
        let mut config = valid_config();
        config.bindings = Some(vec![BindingEntry {
            agent_id: None,
            match_rule: Some(BindingMatch {
                channel: Some("telegram".into()),
                ..Default::default()
            }),
        }]);
        let result = validate_config(&config);
        assert!(result.has_errors());
        assert!(result.errors().iter().any(|i| i.path.contains("agentId")));
    }

    #[test]
    fn deprecated_extra_keys_warned() {
        let mut config = OpenClawConfig::default();
        config.extra.insert("server".into(), serde_json::json!({}));
        let result = validate_config(&config);
        assert!(result.warnings().iter().any(|i| i.message.contains("deprecated")));
    }

    #[test]
    fn legacy_migration_server_to_gateway() {
        let mut config = OpenClawConfig::default();
        config.extra.insert(
            "server".into(),
            serde_json::json!({ "port": 5000, "bind": "lan" }),
        );
        let mut issues = Vec::new();
        migrate_legacy_config(&mut config, &mut issues);

        assert!(config.gateway.is_some());
        assert_eq!(config.gateway.unwrap().port, Some(5000));
        assert!(issues.iter().any(|i| i.message.contains("migrated")));
    }

    #[test]
    fn legacy_migration_bot_to_agents() {
        let mut config = OpenClawConfig::default();
        config.extra.insert(
            "bot".into(),
            serde_json::json!({ "list": [{ "id": "migrated" }] }),
        );
        let mut issues = Vec::new();
        migrate_legacy_config(&mut config, &mut issues);

        assert!(config.agents.is_some());
        assert!(issues.iter().any(|i| i.message.contains("migrated")));
    }

    #[test]
    fn legacy_migration_does_not_overwrite_existing() {
        let mut config = valid_config();
        config.extra.insert(
            "server".into(),
            serde_json::json!({ "port": 9999 }),
        );
        let original_port = config.gateway.as_ref().unwrap().port;
        let mut issues = Vec::new();
        migrate_legacy_config(&mut config, &mut issues);

        // Should NOT overwrite existing gateway
        assert_eq!(config.gateway.as_ref().unwrap().port, original_port);
        assert!(issues.is_empty());
    }
}
