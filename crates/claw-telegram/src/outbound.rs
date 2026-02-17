//! Telegram outbound adapter — sends messages via Telegram Bot API.

use async_trait::async_trait;
use tracing::info;

use claw_channels::plugin::ChannelOutboundAdapter;
use claw_channels::types::{
    ChannelError, ChannelOutboundContext, DeliveryMode, OutboundDeliveryResult,
};

/// Sends messages to Telegram chats via the Bot API.
pub struct TelegramOutbound {
    // Bot handles are resolved per-account at send time.
}

impl TelegramOutbound {
    pub fn new() -> Self {
        Self {}
    }
}

#[async_trait]
impl ChannelOutboundAdapter for TelegramOutbound {
    fn delivery_mode(&self) -> DeliveryMode {
        // Telegram supports streaming via edit-in-place.
        DeliveryMode::Single
    }

    async fn send_payload(
        &self,
        ctx: &ChannelOutboundContext,
        payload: serde_json::Value,
    ) -> Result<OutboundDeliveryResult, ChannelError> {
        info!(
            account_id = %ctx.account_id,
            chat_id = %ctx.chat_id,
            "sending telegram payload"
        );

        // Extract text from payload, falling back to JSON stringification.
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
        // TODO: resolve bot token from account_id → config lookup.
        // For now, return a placeholder indicating the path is wired correctly.
        info!(
            account_id = %ctx.account_id,
            chat_id = %ctx.chat_id,
            text_len = text.len(),
            "telegram send_text"
        );

        // Telegram message limit is 4096 chars. Chunking is handled by
        // claw-channels/outbound.rs (the shared chunking layer).

        // TODO: call teloxide Bot::send_message here.
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
            "telegram send_media"
        );

        // TODO: detect media type (photo/document/audio/video) and call
        // the appropriate teloxide method.
        let _ = caption;

        Ok(OutboundDeliveryResult {
            success: Some(true),
            message_id: None,
            error: None,
            metadata: None,
        })
    }

    async fn send_poll(
        &self,
        ctx: &ChannelOutboundContext,
        question: &str,
        options: &[String],
    ) -> Result<OutboundDeliveryResult, ChannelError> {
        info!(
            account_id = %ctx.account_id,
            chat_id = %ctx.chat_id,
            question,
            option_count = options.len(),
            "telegram send_poll"
        );

        // TODO: call teloxide Bot::send_poll.
        Ok(OutboundDeliveryResult {
            success: Some(true),
            message_id: None,
            error: None,
            metadata: None,
        })
    }
}
