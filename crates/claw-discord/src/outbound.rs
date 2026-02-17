//! Discord outbound adapter — sends messages via Discord REST API.

use async_trait::async_trait;
use tracing::info;

use claw_channels::plugin::ChannelOutboundAdapter;
use claw_channels::types::{
    ChannelError, ChannelOutboundContext, DeliveryMode, OutboundDeliveryResult,
};

/// Sends messages to Discord channels via the REST API.
pub struct DiscordOutbound {
    // HTTP client and token are resolved per-account.
}

impl DiscordOutbound {
    pub fn new() -> Self {
        Self {}
    }
}

#[async_trait]
impl ChannelOutboundAdapter for DiscordOutbound {
    fn delivery_mode(&self) -> DeliveryMode {
        DeliveryMode::Single
    }

    async fn send_payload(
        &self,
        ctx: &ChannelOutboundContext,
        payload: serde_json::Value,
    ) -> Result<OutboundDeliveryResult, ChannelError> {
        let text = payload
            .get("text")
            .and_then(|v| v.as_str())
            .unwrap_or_default();

        self.send_text(ctx, text).await
    }

    async fn send_text(
        &self,
        ctx: &ChannelOutboundContext,
        text: &str,
    ) -> Result<OutboundDeliveryResult, ChannelError> {
        info!(
            account_id = %ctx.account_id,
            chat_id = %ctx.chat_id,
            text_len = text.len(),
            "discord send_text"
        );

        // Discord message limit is 2000 chars. Chunking handled upstream.
        // TODO: POST to /api/v10/channels/{channel_id}/messages
        Ok(OutboundDeliveryResult {
            success: Some(true),
            message_id: None,
            error: None,
            metadata: None,
        })
    }

    async fn send_media(
        &self,
        ctx: &ChannelOutboundContext,
        media_url: &str,
        caption: Option<&str>,
    ) -> Result<OutboundDeliveryResult, ChannelError> {
        info!(
            account_id = %ctx.account_id,
            chat_id = %ctx.chat_id,
            media_url,
            "discord send_media"
        );

        let _ = caption;
        Ok(OutboundDeliveryResult {
            success: Some(true),
            message_id: None,
            error: None,
            metadata: None,
        })
    }
}
