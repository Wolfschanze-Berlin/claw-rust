//! WhatsApp gateway adapter — session-based connection with QR/pairing login.

use async_trait::async_trait;
use tracing::{info, warn};

use claw_channels::plugin::ChannelGatewayAdapter;
use claw_channels::types::{ChannelError, ChannelGatewayContext};
use claw_config::WhatsAppAccountConfig;

/// Manages WhatsApp session lifecycle — QR login, event streaming, reconnection.
pub struct WhatsAppGateway {
    // Session state will be managed per-account.
}

impl WhatsAppGateway {
    pub fn new() -> Self {
        Self {}
    }
}

#[async_trait]
impl ChannelGatewayAdapter for WhatsAppGateway {
    async fn start_account(&self, ctx: ChannelGatewayContext) -> Result<(), ChannelError> {
        let config = extract_config(&ctx.account_config)?;

        let store_path = config.store_path.unwrap_or_else(|| {
            format!("data/whatsapp-{}.db", ctx.account_id)
        });

        info!(
            account_id = %ctx.account_id,
            store_path = %store_path,
            use_pairing_code = config.use_pairing_code.unwrap_or(false),
            "starting whatsapp gateway"
        );

        // Main event loop — runs until cancelled.
        let cancel = ctx.cancel.clone();
        let heartbeat_interval = config.heartbeat_interval_secs.unwrap_or(30);
        let max_reconnects = config.max_reconnect_attempts.unwrap_or(5);

        let mut reconnect_count: u32 = 0;
        loop {
            if cancel.is_cancelled() {
                info!(account_id = %ctx.account_id, "whatsapp gateway cancelled");
                break;
            }

            // TODO: connect to WhatsApp via whatsapp-rust SDK.
            // For now, simulate the event loop structure.
            info!(
                account_id = %ctx.account_id,
                "whatsapp: awaiting connection (stub)"
            );

            // Wait for either cancellation or a simulated disconnect.
            tokio::select! {
                _ = cancel.cancelled() => {
                    info!(account_id = %ctx.account_id, "whatsapp shutdown requested");
                    break;
                }
                _ = tokio::time::sleep(std::time::Duration::from_secs(heartbeat_interval as u64)) => {
                    // Heartbeat tick — check connection health.
                    tracing::debug!(account_id = %ctx.account_id, "whatsapp heartbeat tick");
                }
            }

            // Reconnection logic.
            reconnect_count += 1;
            if reconnect_count >= max_reconnects {
                warn!(
                    account_id = %ctx.account_id,
                    max_reconnects,
                    "whatsapp max reconnect attempts reached"
                );
                return Err(ChannelError::GatewayError(
                    "whatsapp: max reconnect attempts reached".into(),
                ));
            }

            let backoff = std::time::Duration::from_secs(2u64.pow(reconnect_count.min(5)));
            warn!(
                account_id = %ctx.account_id,
                attempt = reconnect_count,
                backoff_secs = backoff.as_secs(),
                "whatsapp reconnecting"
            );
            tokio::time::sleep(backoff).await;
        }

        Ok(())
    }

    async fn stop_account(&self, account_id: &str) -> Result<(), ChannelError> {
        info!(account_id, "stopping whatsapp gateway");
        Ok(())
    }

    async fn login_with_qr_start(
        &self,
        account_id: &str,
    ) -> Result<Option<String>, ChannelError> {
        info!(account_id, "whatsapp QR login requested");
        // TODO: generate QR code data via whatsapp-rust SDK.
        Ok(Some("QR_CODE_PLACEHOLDER".into()))
    }

    async fn login_with_qr_wait(&self, account_id: &str) -> Result<bool, ChannelError> {
        info!(account_id, "waiting for whatsapp QR scan");
        // TODO: poll for QR scan completion.
        Ok(false)
    }

    async fn logout_account(&self, account_id: &str) -> Result<(), ChannelError> {
        info!(account_id, "whatsapp logout");
        // TODO: disconnect and clear session store.
        Ok(())
    }
}

/// Extract WhatsApp config from a generic JSON value.
fn extract_config(value: &serde_json::Value) -> Result<WhatsAppAccountConfig, ChannelError> {
    serde_json::from_value(value.clone())
        .map_err(|e| ChannelError::ConfigError(format!("invalid whatsapp config: {e}")))
}
