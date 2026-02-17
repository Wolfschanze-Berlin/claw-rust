//! Telegram gateway adapter — long polling and webhook inbound.

use async_trait::async_trait;
use tracing::{info, warn};

use claw_channels::plugin::ChannelGatewayAdapter;
use claw_channels::types::{ChannelError, ChannelGatewayContext};
use claw_config::TelegramAccountConfig;

use crate::normalize::normalize_update;
use crate::BotStore;

/// Handles inbound Telegram updates via long polling or webhook.
pub struct TelegramGateway {
    bots: BotStore,
}

impl TelegramGateway {
    pub fn new(bots: BotStore) -> Self {
        Self { bots }
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

        let bot_token = config
            .bot_token
            .filter(|t| !t.is_empty())
            .ok_or_else(|| {
                ChannelError::ConfigError(
                    "telegram: botToken is required (check .env or config)".into(),
                )
            })?;

        // Log a masked token so operators can verify the right credential is loaded.
        let masked = if bot_token.len() > 10 {
            format!("{}…{}", &bot_token[..6], &bot_token[bot_token.len() - 4..])
        } else {
            "***".into()
        };
        info!(
            account_id = %ctx.account_id,
            bot_token_hint = %masked,
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
        // Remove the bot from the shared store on shutdown.
        if let Ok(mut store) = self.bots.write() {
            store.remove(account_id);
        }
        Ok(())
    }
}

impl TelegramGateway {
    /// Build a `Bot` with a reqwest client whose timeout exceeds the poll duration.
    fn build_bot(&self, account_id: &str, bot_token: &str, poll_timeout_secs: u32) -> teloxide::Bot {
        let client_timeout = std::time::Duration::from_secs(u64::from(poll_timeout_secs) + 10);
        let client = reqwest::Client::builder()
            .timeout(client_timeout)
            .build()
            .expect("failed to build reqwest client");
        let bot = teloxide::Bot::with_client(bot_token, client);

        // Register bot in the shared store for outbound/adapter use.
        if let Ok(mut store) = self.bots.write() {
            store.insert(account_id.to_string(), bot.clone());
        }

        bot
    }

    async fn start_polling(
        &self,
        ctx: &ChannelGatewayContext,
        bot_token: &str,
        timeout_secs: u32,
    ) -> Result<(), ChannelError> {
        use teloxide::payloads::GetUpdatesSetters;
        use teloxide::requests::Requester;

        let bot = self.build_bot(&ctx.account_id, bot_token, timeout_secs);
        let cancel = ctx.cancel.clone();

        info!(
            account_id = %ctx.account_id,
            timeout_secs,
            "telegram long-polling started"
        );

        // Long-polling loop — runs until cancelled.
        let mut offset: i32 = 0;
        let mut poll_count: u64 = 0;
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
                    poll_count += 1;
                    if poll_count == 1 {
                        info!(
                            account_id = %ctx.account_id,
                            "telegram polling connected — listening for updates"
                        );
                    }
                    if !updates.is_empty() {
                        info!(
                            account_id = %ctx.account_id,
                            count = updates.len(),
                            "received telegram updates"
                        );
                    }
                    for update in &updates {
                        offset = update.id.as_offset();

                        // Normalize the teloxide Update into a platform-agnostic MsgContext.
                        match normalize_update(update) {
                            Some(msg_ctx) => {
                                tracing::debug!(
                                    update_id = update.id.0,
                                    sender = ?msg_ctx.sender_name,
                                    body = ?msg_ctx.body,
                                    chat_type = ?msg_ctx.chat_type,
                                    "normalized telegram update"
                                );

                                // Forward to the dispatch pipeline if wired.
                                if let Some(ref tx) = ctx.dispatch_tx {
                                    let inbound = claw_channels::types::InboundMessage {
                                        msg: msg_ctx,
                                        channel_id: ctx.channel_id.clone(),
                                        account_id: ctx.account_id.clone(),
                                    };
                                    if let Err(e) = tx.send(inbound) {
                                        warn!(
                                            update_id = update.id.0,
                                            error = %e,
                                            "failed to dispatch inbound message"
                                        );
                                    }
                                } else {
                                    tracing::debug!(
                                        update_id = update.id.0,
                                        "dispatch not wired, message logged only"
                                    );
                                }
                            }
                            None => {
                                tracing::debug!(
                                    update_id = update.id.0,
                                    "skipped non-message telegram update"
                                );
                            }
                        }
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

        let bot = self.build_bot(&ctx.account_id, bot_token, 30);

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
