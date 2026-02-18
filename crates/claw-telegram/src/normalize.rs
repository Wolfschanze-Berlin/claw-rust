//! Telegram message normalizer — converts Telegram updates to MsgContext.
//!
//! The normalizer is the core translation layer between platform-specific
//! teloxide types and the platform-agnostic MsgContext envelope used
//! throughout the claw-rust dispatch pipeline.

use claw_channels::msg_context::{MsgContext, StickerMetadata};
use teloxide::types::{
    MediaKind, Message, MessageCommon, MessageKind, MessageOrigin, Update, UpdateKind,
};

/// Normalize a Telegram update into a unified MsgContext.
pub fn normalize_update(update: &Update) -> Option<MsgContext> {
    let message = match &update.kind {
        UpdateKind::Message(msg) => msg,
        _ => return None,
    };

    // Only handle regular text/media messages.
    let common = match &message.kind {
        MessageKind::Common(c) => c,
        _ => return None,
    };

    let from = message.from.as_ref()?;
    let chat_id = message.chat.id.0.to_string();
    let user_id = from.id.0.to_string();
    let display_name = from.full_name();

    let mut ctx = MsgContext::default();

    // Core identity fields.
    ctx.from = Some(user_id.clone());
    ctx.sender_id = Some(user_id);
    ctx.sender_name = Some(display_name);
    ctx.sender_username = from.username.clone();
    ctx.to = Some(chat_id);
    ctx.provider = Some("telegram".into());
    ctx.message_sid = Some(message.id.0.to_string());
    ctx.timestamp = Some(message.date.timestamp());

    // Extract body + media from media kind.
    extract_media(common, &mut ctx);

    // Chat type mapping.
    let chat_type = if message.chat.is_private() {
        "direct"
    } else if message.chat.is_channel() {
        "channel"
    } else {
        "group"
    };
    ctx.chat_type = Some(chat_type.into());

    // Reply context.
    if let Some(reply) = &common.reply_to_message {
        ctx.reply_to_id = Some(reply.id.0.to_string());
        extract_reply_body(reply, &mut ctx);
    }

    // Forward metadata.
    if let Some(origin) = &common.forward_origin {
        extract_forward(origin, message, &mut ctx);
    }

    // Telegram topic/forum threads.
    if let Some(thread_id) = message.thread_id {
        // ThreadId(MessageId(i32)) — unwrap both newtype wrappers.
        ctx.message_thread_id =
            Some(serde_json::Value::Number((thread_id.0 .0 as i64).into()));
    }
    ctx.is_forum = Some(message.is_topic_message);

    Some(ctx)
}

/// Extract body text and media metadata from the message's media kind.
fn extract_media(common: &MessageCommon, ctx: &mut MsgContext) {
    match &common.media_kind {
        MediaKind::Text(t) => {
            ctx.body = Some(t.text.clone());
        }
        MediaKind::Photo(p) => {
            ctx.body = p.caption.clone();
            ctx.media_type = Some("photo".into());
            // Telegram sends multiple sizes; pick the largest (last).
            if let Some(largest) = p.photo.last() {
                ctx.media_url = Some(format!("tg://file/{}", largest.file.id));
            }
        }
        MediaKind::Document(d) => {
            ctx.body = d.caption.clone();
            ctx.media_type = Some("document".into());
            ctx.media_url = Some(format!("tg://file/{}", d.document.file.id));
            ctx.media_file_name = d.document.file_name.clone();
            ctx.media_mime_type = d.document.mime_type.as_ref().map(|m| m.to_string());
        }
        MediaKind::Audio(a) => {
            ctx.body = a.caption.clone();
            ctx.media_type = Some("audio".into());
            ctx.media_url = Some(format!("tg://file/{}", a.audio.file.id));
            ctx.media_file_name = a.audio.file_name.clone();
            ctx.media_mime_type = a.audio.mime_type.as_ref().map(|m| m.to_string());
        }
        MediaKind::Video(v) => {
            ctx.body = v.caption.clone();
            ctx.media_type = Some("video".into());
            ctx.media_url = Some(format!("tg://file/{}", v.video.file.id));
            ctx.media_mime_type = v.video.mime_type.as_ref().map(|m| m.to_string());
        }
        MediaKind::Voice(v) => {
            ctx.body = v.caption.clone();
            ctx.media_type = Some("voice".into());
            ctx.media_url = Some(format!("tg://file/{}", v.voice.file.id));
        }
        MediaKind::Sticker(s) => {
            ctx.media_type = Some("sticker".into());
            ctx.sticker = Some(StickerMetadata {
                emoji: s.sticker.emoji.clone(),
                set_name: s.sticker.set_name.clone(),
                file_id: Some(s.sticker.file.id.to_string()),
                file_unique_id: Some(s.sticker.file.unique_id.to_string()),
                description: None,
            });
        }
        MediaKind::Animation(a) => {
            ctx.body = a.caption.clone();
            ctx.media_type = Some("animation".into());
            ctx.media_url = Some(format!("tg://file/{}", a.animation.file.id));
            ctx.media_file_name = a.animation.file_name.clone();
            ctx.media_mime_type = a.animation.mime_type.as_ref().map(|m| m.to_string());
        }
        _ => {}
    }
}

/// Extract reply body text from the replied-to message.
fn extract_reply_body(reply: &Message, ctx: &mut MsgContext) {
    if let MessageKind::Common(ref c) = reply.kind {
        if let MediaKind::Text(ref t) = c.media_kind {
            ctx.reply_to_body = Some(t.text.clone());
        }
        // Also capture reply sender for attribution.
        if let Some(ref sender) = reply.from {
            ctx.reply_to_sender = Some(sender.full_name());
        }
    }
}

/// Extract forward origin metadata into MsgContext forwarded_from fields.
fn extract_forward(origin: &MessageOrigin, message: &Message, ctx: &mut MsgContext) {
    ctx.forwarded_date = Some(origin.date().timestamp());

    match origin {
        MessageOrigin::User { sender_user, .. } => {
            ctx.forwarded_from = Some(sender_user.full_name());
            ctx.forwarded_from_type = Some("user".into());
            ctx.forwarded_from_id = Some(sender_user.id.0.to_string());
            ctx.forwarded_from_username = sender_user.username.clone();
        }
        MessageOrigin::HiddenUser {
            sender_user_name, ..
        } => {
            ctx.forwarded_from = Some(sender_user_name.clone());
            ctx.forwarded_from_type = Some("hidden_user".into());
        }
        MessageOrigin::Chat {
            sender_chat,
            author_signature,
            ..
        } => {
            ctx.forwarded_from = Some(sender_chat.title().unwrap_or("Unknown").to_string());
            ctx.forwarded_from_type = Some("chat".into());
            ctx.forwarded_from_id = Some(sender_chat.id.0.to_string());
            ctx.forwarded_from_chat_type =
                Some(format!("{:?}", sender_chat.kind).to_lowercase());
            ctx.forwarded_from_signature = author_signature.clone();
        }
        MessageOrigin::Channel {
            chat,
            message_id,
            author_signature,
            ..
        } => {
            ctx.forwarded_from = Some(chat.title().unwrap_or("Unknown").to_string());
            ctx.forwarded_from_type = Some("channel".into());
            ctx.forwarded_from_id = Some(chat.id.0.to_string());
            ctx.forwarded_from_title = chat.title().map(String::from);
            ctx.forwarded_from_message_id = Some(message_id.0 as i64);
            ctx.forwarded_from_signature = author_signature.clone();
        }
    }

    // Legacy forward_from_user on Message (if present).
    if let Some(fwd) = message.forward_from_user() {
        ctx.forwarded_from_username = fwd.username.clone();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper: deserialize a raw JSON string into a teloxide Update.
    /// Uses from_str (not from_value) to match teloxide's custom deserializer.
    fn make_update(json: &str) -> Update {
        serde_json::from_str(json).expect("valid update JSON")
    }

    #[test]
    fn normalizes_text_message() {
        let update = make_update(r#"{
            "update_id": 1,
            "message": {
                "message_id": 42,
                "date": 1700000000,
                "chat": { "id": 12345, "type": "private" },
                "from": {
                    "id": 99,
                    "is_bot": false,
                    "first_name": "Alice",
                    "last_name": "Smith",
                    "username": "alice_s"
                },
                "text": "Hello, world!"
            }
        }"#);

        let ctx = normalize_update(&update).expect("should normalize");
        assert_eq!(ctx.body.as_deref(), Some("Hello, world!"));
        assert_eq!(ctx.from.as_deref(), Some("99"));
        assert_eq!(ctx.sender_name.as_deref(), Some("Alice Smith"));
        assert_eq!(ctx.sender_username.as_deref(), Some("alice_s"));
        assert_eq!(ctx.sender_id.as_deref(), Some("99"));
        assert_eq!(ctx.to.as_deref(), Some("12345"));
        assert_eq!(ctx.provider.as_deref(), Some("telegram"));
        assert_eq!(ctx.message_sid.as_deref(), Some("42"));
        assert_eq!(ctx.chat_type.as_deref(), Some("direct"));
    }

    #[test]
    fn normalizes_group_message() {
        let update = make_update(r#"{
            "update_id": 2,
            "message": {
                "message_id": 100,
                "date": 1700000000,
                "chat": { "id": -100123, "type": "supergroup", "title": "Test Group" },
                "from": {
                    "id": 55,
                    "is_bot": false,
                    "first_name": "Bob"
                },
                "text": "group msg"
            }
        }"#);

        let ctx = normalize_update(&update).expect("should normalize");
        assert_eq!(ctx.chat_type.as_deref(), Some("group"));
        assert_eq!(ctx.body.as_deref(), Some("group msg"));
    }

    #[test]
    fn normalizes_reply() {
        let update = make_update(r#"{
            "update_id": 3,
            "message": {
                "message_id": 200,
                "date": 1700000000,
                "chat": { "id": 12345, "type": "private" },
                "from": {
                    "id": 99,
                    "is_bot": false,
                    "first_name": "Alice"
                },
                "text": "reply text",
                "reply_to_message": {
                    "message_id": 199,
                    "date": 1699999000,
                    "chat": { "id": 12345, "type": "private" },
                    "from": {
                        "id": 88,
                        "is_bot": false,
                        "first_name": "Charlie"
                    },
                    "text": "original text"
                }
            }
        }"#);

        let ctx = normalize_update(&update).expect("should normalize");
        assert_eq!(ctx.reply_to_id.as_deref(), Some("199"));
        assert_eq!(ctx.reply_to_body.as_deref(), Some("original text"));
        assert_eq!(ctx.reply_to_sender.as_deref(), Some("Charlie"));
    }

    #[test]
    fn normalizes_channel_message() {
        let update = make_update(r#"{
            "update_id": 4,
            "message": {
                "message_id": 300,
                "date": 1700000000,
                "chat": { "id": -1001234, "type": "channel", "title": "News" },
                "from": {
                    "id": 77,
                    "is_bot": false,
                    "first_name": "Admin"
                },
                "text": "announcement"
            }
        }"#);

        let ctx = normalize_update(&update).expect("should normalize");
        assert_eq!(ctx.chat_type.as_deref(), Some("channel"));
    }

    #[test]
    fn skips_non_message_updates() {
        // edited_message is UpdateKind::EditedMessage, not Message.
        let update = make_update(r#"{
            "update_id": 5,
            "edited_message": {
                "message_id": 1,
                "date": 1700000000,
                "chat": { "id": 1, "type": "private" },
                "from": { "id": 1, "is_bot": false, "first_name": "X" },
                "text": "edited"
            }
        }"#);

        assert!(normalize_update(&update).is_none());
    }

    #[test]
    fn normalizes_photo_message() {
        let update = make_update(r#"{
            "update_id": 6,
            "message": {
                "message_id": 400,
                "date": 1700000000,
                "chat": { "id": 12345, "type": "private" },
                "from": { "id": 99, "is_bot": false, "first_name": "Alice" },
                "photo": [
                    { "file_id": "small_id", "file_unique_id": "small_uid", "width": 90, "height": 90 },
                    { "file_id": "large_id", "file_unique_id": "large_uid", "width": 800, "height": 600 }
                ],
                "caption": "Look at this!"
            }
        }"#);

        let ctx = normalize_update(&update).expect("should normalize");
        assert_eq!(ctx.body.as_deref(), Some("Look at this!"));
        assert_eq!(ctx.media_type.as_deref(), Some("photo"));
        assert_eq!(ctx.media_url.as_deref(), Some("tg://file/large_id"));
    }

    #[test]
    fn normalizes_document_message_with_metadata() {
        let update = make_update(r#"{
            "update_id": 10,
            "message": {
                "message_id": 600,
                "date": 1700000000,
                "chat": { "id": 12345, "type": "private" },
                "from": { "id": 99, "is_bot": false, "first_name": "Alice" },
                "document": {
                    "file_id": "doc_file_id_123",
                    "file_unique_id": "doc_uid",
                    "file_name": "report.pdf",
                    "mime_type": "application/pdf",
                    "file_size": 1024
                },
                "caption": "Here is the report"
            }
        }"#);

        let ctx = normalize_update(&update).expect("should normalize");
        assert_eq!(ctx.body.as_deref(), Some("Here is the report"));
        assert_eq!(ctx.media_type.as_deref(), Some("document"));
        assert_eq!(ctx.media_url.as_deref(), Some("tg://file/doc_file_id_123"));
        assert_eq!(ctx.media_file_name.as_deref(), Some("report.pdf"));
        assert_eq!(ctx.media_mime_type.as_deref(), Some("application/pdf"));
    }

    #[test]
    fn normalizes_audio_message_with_metadata() {
        let update = make_update(r#"{
            "update_id": 11,
            "message": {
                "message_id": 601,
                "date": 1700000000,
                "chat": { "id": 12345, "type": "private" },
                "from": { "id": 99, "is_bot": false, "first_name": "Alice" },
                "audio": {
                    "file_id": "audio_file_id",
                    "file_unique_id": "audio_uid",
                    "duration": 180,
                    "file_name": "song.mp3",
                    "mime_type": "audio/mpeg"
                },
                "caption": "Listen to this"
            }
        }"#);

        let ctx = normalize_update(&update).expect("should normalize");
        assert_eq!(ctx.body.as_deref(), Some("Listen to this"));
        assert_eq!(ctx.media_type.as_deref(), Some("audio"));
        assert_eq!(ctx.media_url.as_deref(), Some("tg://file/audio_file_id"));
        assert_eq!(ctx.media_file_name.as_deref(), Some("song.mp3"));
        assert_eq!(ctx.media_mime_type.as_deref(), Some("audio/mpeg"));
    }

    #[test]
    fn normalizes_video_message_with_mime() {
        let update = make_update(r#"{
            "update_id": 12,
            "message": {
                "message_id": 602,
                "date": 1700000000,
                "chat": { "id": 12345, "type": "private" },
                "from": { "id": 99, "is_bot": false, "first_name": "Alice" },
                "video": {
                    "file_id": "video_file_id",
                    "file_unique_id": "video_uid",
                    "width": 1920,
                    "height": 1080,
                    "duration": 30,
                    "mime_type": "video/mp4"
                },
                "caption": "Cool video"
            }
        }"#);

        let ctx = normalize_update(&update).expect("should normalize");
        assert_eq!(ctx.body.as_deref(), Some("Cool video"));
        assert_eq!(ctx.media_type.as_deref(), Some("video"));
        assert_eq!(ctx.media_url.as_deref(), Some("tg://file/video_file_id"));
        assert_eq!(ctx.media_file_name, None); // Video has no file_name in Telegram API
        assert_eq!(ctx.media_mime_type.as_deref(), Some("video/mp4"));
    }

    #[test]
    fn normalizes_voice_message() {
        let update = make_update(r#"{
            "update_id": 13,
            "message": {
                "message_id": 603,
                "date": 1700000000,
                "chat": { "id": 12345, "type": "private" },
                "from": { "id": 99, "is_bot": false, "first_name": "Alice" },
                "voice": {
                    "file_id": "voice_file_id",
                    "file_unique_id": "voice_uid",
                    "duration": 5,
                    "mime_type": "audio/ogg"
                }
            }
        }"#);

        let ctx = normalize_update(&update).expect("should normalize");
        assert_eq!(ctx.media_type.as_deref(), Some("voice"));
        assert_eq!(ctx.media_url.as_deref(), Some("tg://file/voice_file_id"));
        // Voice messages have no file_name or explicit mime in Telegram API
        assert_eq!(ctx.media_file_name, None);
        assert_eq!(ctx.media_mime_type, None);
    }

    #[test]
    fn normalizes_animation_message_with_metadata() {
        let update = make_update(r#"{
            "update_id": 14,
            "message": {
                "message_id": 604,
                "date": 1700000000,
                "chat": { "id": 12345, "type": "private" },
                "from": { "id": 99, "is_bot": false, "first_name": "Alice" },
                "animation": {
                    "file_id": "anim_file_id",
                    "file_unique_id": "anim_uid",
                    "width": 320,
                    "height": 240,
                    "duration": 3,
                    "file_name": "funny.gif",
                    "mime_type": "video/mp4"
                },
                "document": {
                    "file_id": "anim_file_id",
                    "file_unique_id": "anim_uid",
                    "file_name": "funny.gif",
                    "mime_type": "video/mp4"
                },
                "caption": "LOL"
            }
        }"#);

        let ctx = normalize_update(&update).expect("should normalize");
        assert_eq!(ctx.body.as_deref(), Some("LOL"));
        assert_eq!(ctx.media_type.as_deref(), Some("animation"));
        assert_eq!(ctx.media_url.as_deref(), Some("tg://file/anim_file_id"));
        assert_eq!(ctx.media_file_name.as_deref(), Some("funny.gif"));
        assert_eq!(ctx.media_mime_type.as_deref(), Some("video/mp4"));
    }

    #[test]
    fn normalizes_sticker_message() {
        let update = make_update(r#"{
            "update_id": 7,
            "message": {
                "message_id": 500,
                "date": 1700000000,
                "chat": { "id": 12345, "type": "private" },
                "from": { "id": 99, "is_bot": false, "first_name": "Alice" },
                "sticker": {
                    "file_id": "sticker_file_id",
                    "file_unique_id": "sticker_uid",
                    "width": 512,
                    "height": 512,
                    "type": "regular",
                    "emoji": "😊",
                    "set_name": "HappyStickers",
                    "is_animated": false,
                    "is_video": false
                }
            }
        }"#);

        let ctx = normalize_update(&update).expect("should normalize");
        assert_eq!(ctx.media_type.as_deref(), Some("sticker"));
        let sticker = ctx.sticker.as_ref().expect("sticker metadata");
        assert_eq!(sticker.emoji.as_deref(), Some("😊"));
        assert_eq!(sticker.set_name.as_deref(), Some("HappyStickers"));
        assert_eq!(sticker.file_id.as_deref(), Some("sticker_file_id"));
        assert_eq!(sticker.file_unique_id.as_deref(), Some("sticker_uid"));
    }
}
