//! Telegram gateway adapter — long polling and webhook inbound.

use async_trait::async_trait;
use tracing::{info, warn};

use claw_channels::plugin::ChannelGatewayAdapter;
use claw_channels::types::{ChannelError, ChannelGatewayContext};
use claw_config::TelegramAccountConfig;

/// Handles inbound Telegram updates via long polling or webhook.
pub struct TelegramGateway {
    // Will hold per-account bot handles once started.
}

impl TelegramGateway {
    pub fn new() -> Self {
        Self {}
    }
}

/// Extract Telegram config from a generic JSON value.
fn extract_config(value: &serde_json::Value) -> Result<TelegramAccountConfig, ChannelError> {
    serde_json::from_value(value.clone())
        .map_err(|e| ChannelError::ConfigError(format!("invalid telegram config: {e}")))
}

#[async_trait]
impl ChannelGatewayAdapter for TelegramGateway {
    async fn start_account(&self, ctx: ChannelGatewayContext) -> Result<(), ChannelError> {
        let config = extract_config(&ctx.account_config)?;

        let bot_token = config.bot_token.ok_or_else(|| {
            ChannelError::ConfigError("telegram: botToken is required".into())
        })?;

        info!(
            account_id = %ctx.account_id,
            has_webhook = config.webhook_url.is_some(),
            "starting telegram gateway"
        );

        if let Some(webhook_url) = &config.webhook_url {
            self.start_webhook(&ctx, &bot_token, webhook_url).await
        } else {
            let timeout = config.poll_timeout_secs.unwrap_or(30);
            self.start_polling(&ctx, &bot_token, timeout).await
        }
    }

    async fn stop_account(&self, account_id: &str) -> Result<(), ChannelError> {
        info!(account_id, "stopping telegram gateway");
        Ok(())
    }
}

impl TelegramGateway {
    /// Build a `Bot` with a reqwest client whose timeout exceeds the poll duration.
    fn build_bot(bot_token: &str, poll_timeout_secs: u32) -> teloxide::Bot {
        let client_timeout = std::time::Duration::from_secs(u64::from(poll_timeout_secs) + 10);
        let client = reqwest::Client::builder()
            .timeout(client_timeout)
            .build()
            .expect("failed to build reqwest client");
        teloxide::Bot::with_client(bot_token, client)
    }

    async fn start_polling(
        &self,
        ctx: &ChannelGatewayContext,
        bot_token: &str,
        timeout_secs: u32,
    ) -> Result<(), ChannelError> {
        use teloxide::payloads::GetUpdatesSetters;
        use teloxide::requests::Requester;

        let bot = Self::build_bot(bot_token, timeout_secs);
        let cancel = ctx.cancel.clone();

        info!(
            account_id = %ctx.account_id,
            timeout_secs,
            "telegram long-polling started"
        );

        // Long-polling loop — runs until cancelled.
        let mut offset: i32 = 0;
        loop {
            if cancel.is_cancelled() {
                info!(account_id = %ctx.account_id, "telegram polling cancelled");
                break;
            }

            match bot
                .get_updates()
                .offset(offset)
                .timeout(timeout_secs)
                .await
            {
                Ok(updates) => {
                    for update in &updates {
                        offset = update.id.as_offset();
                        // TODO: normalize update → MsgContext and dispatch
                        tracing::debug!(update_id = update.id.0, "received telegram update");
                    }
                }
                Err(e) => {
                    warn!(error = %e, "telegram polling error, will retry");
                    tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                }
            }
        }

        Ok(())
    }

    async fn start_webhook(
        &self,
        ctx: &ChannelGatewayContext,
        bot_token: &str,
        webhook_url: &str,
    ) -> Result<(), ChannelError> {
        use teloxide::requests::Requester;

        let bot = Self::build_bot(bot_token, 30);

        // Register webhook URL with Telegram.
        bot.set_webhook(
            webhook_url
                .parse()
                .map_err(|e| ChannelError::ConfigError(format!("invalid webhook URL: {e}")))?,
        )
        .await
        .map_err(|e| ChannelError::GatewayError(format!("failed to set webhook: {e}")))?;

        info!(
            account_id = %ctx.account_id,
            webhook_url,
            "telegram webhook registered"
        );

        // Webhook mode: wait for cancel signal. HTTP handling is done by claw-gateway.
        ctx.cancel.cancelled().await;

        // Clean up: delete webhook on shutdown.
        let _ = bot.delete_webhook().await;
        info!(account_id = %ctx.account_id, "telegram webhook removed on shutdown");

        Ok(())
    }
}
