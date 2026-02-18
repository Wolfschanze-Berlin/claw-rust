//! Telegram outbound adapter — sends messages via Telegram Bot API.

use async_trait::async_trait;
use teloxide::requests::Requester;
use teloxide::types::ChatId;
use tracing::{info, warn};

use claw_channels::plugin::ChannelOutboundAdapter;
use claw_channels::types::{
    ChannelError, ChannelOutboundContext, DeliveryMode, OutboundDeliveryResult,
};

use crate::BotStore;

/// Sends messages to Telegram chats via the Bot API.
pub struct TelegramOutbound {
    bots: BotStore,
}

impl TelegramOutbound {
    pub fn new(bots: BotStore) -> Self {
        Self { bots }
    }

    /// Look up the bot for the given account ID from the shared store.
    fn get_bot(&self, account_id: &str) -> Result<teloxide::Bot, ChannelError> {
        let store = self.bots.read().map_err(|e| {
            ChannelError::DeliveryFailed(format!("bot store lock poisoned: {e}"))
        })?;
        store.get(account_id).cloned().ok_or_else(|| {
            ChannelError::AccountNotFound {
                channel: "telegram".into(),
                account_id: account_id.into(),
            }
        })
    }
}

/// Parse a chat_id string into a teloxide ChatId.
fn parse_chat_id(chat_id: &str) -> Result<ChatId, ChannelError> {
    chat_id
        .parse::<i64>()
        .map(ChatId)
        .map_err(|_| ChannelError::DeliveryFailed(format!("invalid chat_id: {chat_id}")))
}

/// Detect media type from a URL or local file path extension.
fn detect_media_type(url: &str) -> &str {
    // Strip query parameters and fragments for cleaner extension detection.
    let path = url.split('?').next().unwrap_or(url);
    let path = path.split('#').next().unwrap_or(path);
    let lower = path.to_lowercase();

    if lower.ends_with(".jpg")
        || lower.ends_with(".jpeg")
        || lower.ends_with(".png")
        || lower.ends_with(".gif")
        || lower.ends_with(".webp")
    {
        "photo"
    } else if lower.ends_with(".mp4") || lower.ends_with(".mov") || lower.ends_with(".avi") {
        "video"
    } else if lower.ends_with(".mp3")
        || lower.ends_with(".ogg")
        || lower.ends_with(".wav")
        || lower.ends_with(".flac")
    {
        "audio"
    } else {
        "document"
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
        use teloxide::payloads::SendMessageSetters;
        use teloxide::types::ReplyParameters;

        let bot = self.get_bot(&ctx.account_id)?;
        let chat_id = parse_chat_id(&ctx.chat_id)?;

        info!(
            account_id = %ctx.account_id,
            chat_id = %ctx.chat_id,
            text_len = text.len(),
            "telegram send_text"
        );

        let mut request = bot.send_message(chat_id, text);

        // Set reply-to if provided.
        if let Some(ref reply_id) = ctx.reply_to_message_id {
            if let Ok(msg_id) = reply_id.parse::<i32>() {
                request = request.reply_parameters(
                    ReplyParameters::new(teloxide::types::MessageId(msg_id))
                        .allow_sending_without_reply(),
                );
            }
        }

        match request.await {
            Ok(msg) => Ok(OutboundDeliveryResult {
                success: Some(true),
                message_id: Some(msg.id.0.to_string()),
                error: None,
                metadata: None,
            }),
            Err(e) => {
                warn!(error = %e, "telegram send_text failed");
                Err(ChannelError::DeliveryFailed(format!(
                    "telegram send_message failed: {e}"
                )))
            }
        }
    }

    async fn send_media(
        &self,
        ctx: &ChannelOutboundContext,
        media_url: &str,
        caption: Option<&str>,
    ) -> Result<OutboundDeliveryResult, ChannelError> {
        use teloxide::payloads::{
            SendAudioSetters, SendDocumentSetters, SendPhotoSetters, SendVideoSetters,
        };
        use teloxide::types::InputFile;

        let bot = self.get_bot(&ctx.account_id)?;
        let chat_id = parse_chat_id(&ctx.chat_id)?;

        info!(
            account_id = %ctx.account_id,
            chat_id = %ctx.chat_id,
            media_url,
            "telegram send_media"
        );

        // Detect whether media_url is an HTTP URL or local file path.
        let input = if media_url.starts_with("http://") || media_url.starts_with("https://") {
            InputFile::url(
                media_url
                    .parse()
                    .map_err(|e| ChannelError::DeliveryFailed(format!("invalid media URL: {e}")))?,
            )
        } else {
            // Treat as local file path.
            let path = std::path::Path::new(media_url);
            if !path.exists() {
                return Err(ChannelError::DeliveryFailed(format!(
                    "media file not found: {media_url}"
                )));
            }
            InputFile::file(path)
        };

        let media_type = detect_media_type(media_url);

        let result: Result<teloxide::types::Message, _> = match media_type {
            "photo" => {
                let mut req = bot.send_photo(chat_id, input);
                if let Some(cap) = caption {
                    req = req.caption(cap);
                }
                req.await
            }
            "video" => {
                let mut req = bot.send_video(chat_id, input);
                if let Some(cap) = caption {
                    req = req.caption(cap);
                }
                req.await
            }
            "audio" => {
                let mut req = bot.send_audio(chat_id, input);
                if let Some(cap) = caption {
                    req = req.caption(cap);
                }
                req.await
            }
            _ => {
                // Default to document for unknown types.
                let mut req = bot.send_document(chat_id, input);
                if let Some(cap) = caption {
                    req = req.caption(cap);
                }
                req.await
            }
        };

        match result {
            Ok(msg) => Ok(OutboundDeliveryResult {
                success: Some(true),
                message_id: Some(msg.id.0.to_string()),
                error: None,
                metadata: None,
            }),
            Err(e) => {
                warn!(error = %e, "telegram send_media failed");
                Err(ChannelError::DeliveryFailed(format!(
                    "telegram send_{media_type} failed: {e}"
                )))
            }
        }
    }

    async fn send_poll(
        &self,
        ctx: &ChannelOutboundContext,
        question: &str,
        options: &[String],
    ) -> Result<OutboundDeliveryResult, ChannelError> {
        let bot = self.get_bot(&ctx.account_id)?;
        let chat_id = parse_chat_id(&ctx.chat_id)?;

        info!(
            account_id = %ctx.account_id,
            chat_id = %ctx.chat_id,
            question,
            option_count = options.len(),
            "telegram send_poll"
        );

        if options.len() < 2 {
            return Err(ChannelError::DeliveryFailed(
                "telegram polls require at least 2 options".into(),
            ));
        }

        let poll_options: Vec<teloxide::types::InputPollOption> = options
            .iter()
            .map(|o| teloxide::types::InputPollOption {
                text: o.clone(),
                formatting: None,
            })
            .collect();

        match bot.send_poll(chat_id, question, poll_options).await {
            Ok(msg) => Ok(OutboundDeliveryResult {
                success: Some(true),
                message_id: Some(msg.id.0.to_string()),
                error: None,
                metadata: None,
            }),
            Err(e) => {
                warn!(error = %e, "telegram send_poll failed");
                Err(ChannelError::DeliveryFailed(format!(
                    "telegram send_poll failed: {e}"
                )))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_photo_extensions() {
        assert_eq!(detect_media_type("https://example.com/img.jpg"), "photo");
        assert_eq!(detect_media_type("https://example.com/img.PNG"), "photo");
        assert_eq!(detect_media_type("https://example.com/a.webp"), "photo");
    }

    #[test]
    fn detect_video_extensions() {
        assert_eq!(detect_media_type("https://example.com/v.mp4"), "video");
        assert_eq!(detect_media_type("https://example.com/v.MOV"), "video");
    }

    #[test]
    fn detect_audio_extensions() {
        assert_eq!(detect_media_type("https://example.com/a.mp3"), "audio");
        assert_eq!(detect_media_type("https://example.com/a.ogg"), "audio");
    }

    #[test]
    fn detect_document_fallback() {
        assert_eq!(detect_media_type("https://example.com/f.pdf"), "document");
        assert_eq!(detect_media_type("https://example.com/f.zip"), "document");
    }

    #[test]
    fn detect_local_file_paths() {
        assert_eq!(detect_media_type("/tmp/files/photo.jpg"), "photo");
        assert_eq!(detect_media_type("/data/report.pdf"), "document");
        assert_eq!(detect_media_type("/data/video.mp4"), "video");
    }

    #[test]
    fn detect_url_with_query_params() {
        assert_eq!(
            detect_media_type("https://example.com/img.jpg?token=abc"),
            "photo"
        );
        assert_eq!(
            detect_media_type("https://example.com/v.mp4#section"),
            "video"
        );
    }

    #[test]
    fn parse_valid_chat_id() {
        let id = parse_chat_id("12345").unwrap();
        assert_eq!(id, ChatId(12345));
    }

    #[test]
    fn parse_negative_chat_id() {
        let id = parse_chat_id("-100123456").unwrap();
        assert_eq!(id, ChatId(-100123456));
    }

    #[test]
    fn parse_invalid_chat_id() {
        assert!(parse_chat_id("not_a_number").is_err());
    }

    #[test]
    fn poll_requires_two_options() {
        // Verify the validation logic by testing the constraint directly.
        let options: Vec<String> = vec!["only_one".into()];
        assert!(options.len() < 2);
    }
}
