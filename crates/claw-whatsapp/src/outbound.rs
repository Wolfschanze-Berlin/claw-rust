//! WhatsApp outbound adapter — sends messages via WhatsApp API.

use async_trait::async_trait;
use tracing::info;

use claw_channels::plugin::ChannelOutboundAdapter;
use claw_channels::types::{
    ChannelError, ChannelOutboundContext, DeliveryMode, OutboundDeliveryResult,
};

/// Sends messages to WhatsApp chats.
pub struct WhatsAppOutbound {
    // Connection handles are resolved per-account.
}

impl WhatsAppOutbound {
    pub fn new() -> Self {
        Self {}
    }
}

#[async_trait]
impl ChannelOutboundAdapter for WhatsAppOutbound {
    fn delivery_mode(&self) -> DeliveryMode {
        // WhatsApp doesn't support edit-in-place streaming.
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
            "whatsapp send_text"
        );

        // WhatsApp message limit is ~65,000 chars. Chunking handled upstream.
        // TODO: send via whatsapp-rust SDK.
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
            "whatsapp send_media"
        );

        let _ = caption;
        // TODO: detect media type and send via whatsapp-rust.
        Ok(OutboundDeliveryResult {
            success: Some(true),
            message_id: None,
            error: None,
            metadata: None,
        })
    }
}
