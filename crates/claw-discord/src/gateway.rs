//! Discord gateway adapter — WebSocket shard connection to Discord API.

use async_trait::async_trait;
use tracing::info;

use claw_channels::plugin::ChannelGatewayAdapter;
use claw_channels::types::{ChannelError, ChannelGatewayContext};
use claw_config::DiscordAccountConfig;

/// Manages Discord WebSocket gateway shards for receiving events.
pub struct DiscordGateway {
    // Shard handles will be managed per-account.
}

impl DiscordGateway {
    pub fn new() -> Self {
        Self {}
    }
}

#[async_trait]
impl ChannelGatewayAdapter for DiscordGateway {
    async fn start_account(&self, ctx: ChannelGatewayContext) -> Result<(), ChannelError> {
        let config = extract_discord_config(&ctx.account_config)?;

        let _bot_token = config.bot_token.ok_or_else(|| {
            ChannelError::ConfigError("discord: botToken is required".into())
        })?;

        let intents = config.intents.unwrap_or(
            // Default: GUILDS | GUILD_MESSAGES | GUILD_MESSAGE_REACTIONS
            // | DIRECT_MESSAGES | MESSAGE_CONTENT
            (1 << 0) | (1 << 9) | (1 << 10) | (1 << 12) | (1 << 15),
        );

        info!(
            account_id = %ctx.account_id,
            intents,
            shard_count = config.shard_count.unwrap_or(1),
            sync_commands = config.sync_commands.unwrap_or(false),
            "starting discord gateway"
        );

        let cancel = ctx.cancel.clone();

        // TODO: connect to Discord gateway via tokio-tungstenite.
        info!(account_id = %ctx.account_id, "discord: connecting to gateway (stub)");

        // Wait for cancellation.
        cancel.cancelled().await;
        info!(account_id = %ctx.account_id, "discord gateway shutdown");

        Ok(())
    }

    async fn stop_account(&self, account_id: &str) -> Result<(), ChannelError> {
        info!(account_id, "stopping discord gateway");
        Ok(())
    }

    async fn logout_account(&self, account_id: &str) -> Result<(), ChannelError> {
        info!(account_id, "discord logout — closing shard connections");
        Ok(())
    }
}

/// Extract Discord config from a generic JSON value.
fn extract_discord_config(value: &serde_json::Value) -> Result<DiscordAccountConfig, ChannelError> {
    serde_json::from_value(value.clone())
        .map_err(|e| ChannelError::ConfigError(format!("invalid discord config: {e}")))
}
