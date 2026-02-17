//! WhatsApp message normalizer — converts WhatsApp events to MsgContext.
//!
//! WhatsApp events from the whatsapp-rust SDK arrive as JSON objects.
//! This normalizer extracts sender, chat, message body, media,
//! and reply context into the platform-agnostic MsgContext envelope.

use claw_channels::msg_context::MsgContext;

/// Normalize a raw WhatsApp event (JSON) into a unified MsgContext.
pub fn normalize_event(event: &serde_json::Value) -> Option<MsgContext> {
    let msg = event.get("message")?;

    let key = msg.get("key")?;
    let chat_id = key.get("remoteJid")?.as_str()?.to_string();
    let from_me = key
        .get("fromMe")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    // Skip our own messages.
    if from_me {
        return None;
    }

    let message_id = key
        .get("id")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string();

    // In group chats, participant is the actual sender JID.
    let participant = key.get("participant").and_then(|v| v.as_str()).map(String::from);
    let sender_jid = participant.clone().unwrap_or_else(|| chat_id.clone());

    // Extract display name from pushName field.
    let push_name = msg
        .get("pushName")
        .and_then(|v| v.as_str())
        .map(String::from);

    // Extract text body from various WhatsApp message types.
    let text = extract_text(msg);

    let mut ctx = MsgContext::default();
    ctx.body = text;
    ctx.from = Some(sender_jid.clone());
    ctx.sender_id = Some(sender_jid.clone());
    ctx.sender_name = push_name;
    ctx.to = Some(chat_id.clone());
    ctx.provider = Some("whatsapp".into());
    ctx.message_sid = Some(message_id);

    // Extract phone number (E.164 format) from JID.
    if let Some(phone) = extract_e164(&sender_jid) {
        ctx.sender_e164 = Some(phone);
    }

    // Determine chat type from JID format.
    let chat_type = if chat_id.ends_with("@g.us") {
        "group"
    } else {
        "direct"
    };
    ctx.chat_type = Some(chat_type.into());

    // Extract media type from message content.
    extract_media(msg, &mut ctx);

    // Handle quoted/reply messages.
    if let Some(context_info) = find_context_info(msg) {
        if let Some(stanza_id) = context_info.get("stanzaId").and_then(|v| v.as_str()) {
            ctx.reply_to_id = Some(stanza_id.to_string());
        }
        if let Some(quoted_msg) = context_info.get("quotedMessage") {
            if let Some(text) = quoted_msg
                .get("conversation")
                .and_then(|v| v.as_str())
            {
                ctx.reply_to_body = Some(text.to_string());
            }
        }
    }

    // Timestamp (WhatsApp sends epoch seconds as messageTimestamp).
    if let Some(ts) = msg
        .get("messageTimestamp")
        .and_then(|v| v.as_i64().or_else(|| v.as_str().and_then(|s| s.parse().ok())))
    {
        ctx.timestamp = Some(ts);
    }

    Some(ctx)
}

/// Extract text from various WhatsApp message types.
fn extract_text(msg: &serde_json::Value) -> Option<String> {
    let message = msg.get("message")?;

    // Plain text conversation.
    if let Some(text) = message.get("conversation").and_then(|v| v.as_str()) {
        return Some(text.to_string());
    }

    // Extended text message (with link preview, mentions, etc).
    if let Some(text) = message
        .get("extendedTextMessage")
        .and_then(|e| e.get("text"))
        .and_then(|v| v.as_str())
    {
        return Some(text.to_string());
    }

    // Image/video/document caption.
    for key in &["imageMessage", "videoMessage", "documentMessage"] {
        if let Some(caption) = message
            .get(*key)
            .and_then(|m| m.get("caption"))
            .and_then(|v| v.as_str())
        {
            return Some(caption.to_string());
        }
    }

    None
}

/// Extract media type from the message content.
fn extract_media(msg: &serde_json::Value, ctx: &mut MsgContext) {
    let message = match msg.get("message") {
        Some(m) => m,
        None => return,
    };

    let media_types = [
        ("imageMessage", "image"),
        ("videoMessage", "video"),
        ("audioMessage", "audio"),
        ("documentMessage", "document"),
        ("stickerMessage", "sticker"),
    ];

    for (key, media_type) in &media_types {
        if message.get(*key).is_some() {
            ctx.media_type = Some((*media_type).into());
            return;
        }
    }
}

/// Find contextInfo in any message type (for reply detection).
fn find_context_info(msg: &serde_json::Value) -> Option<&serde_json::Value> {
    let message = msg.get("message")?;

    // Check various message types for contextInfo.
    let containers = [
        "extendedTextMessage",
        "imageMessage",
        "videoMessage",
        "audioMessage",
        "documentMessage",
        "stickerMessage",
    ];

    for container in &containers {
        if let Some(ci) = message.get(*container).and_then(|m| m.get("contextInfo")) {
            return Some(ci);
        }
    }

    None
}

/// Extract E.164 phone number from a WhatsApp JID.
/// JID format: `<phone>@s.whatsapp.net` or `<phone>@g.us` for groups.
fn extract_e164(jid: &str) -> Option<String> {
    let phone = jid.split('@').next()?;
    // Only return if it looks like a phone number (digits only).
    if phone.chars().all(|c| c.is_ascii_digit()) && phone.len() >= 7 {
        Some(format!("+{phone}"))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn normalizes_text_dm() {
        let event = json!({
            "message": {
                "key": {
                    "remoteJid": "491234567890@s.whatsapp.net",
                    "fromMe": false,
                    "id": "ABC123"
                },
                "pushName": "Alice",
                "messageTimestamp": 1700000000,
                "message": {
                    "conversation": "Hello from WhatsApp!"
                }
            }
        });

        let ctx = normalize_event(&event).expect("should normalize");
        assert_eq!(ctx.body.as_deref(), Some("Hello from WhatsApp!"));
        assert_eq!(ctx.from.as_deref(), Some("491234567890@s.whatsapp.net"));
        assert_eq!(ctx.sender_name.as_deref(), Some("Alice"));
        assert_eq!(ctx.sender_e164.as_deref(), Some("+491234567890"));
        assert_eq!(ctx.provider.as_deref(), Some("whatsapp"));
        assert_eq!(ctx.message_sid.as_deref(), Some("ABC123"));
        assert_eq!(ctx.chat_type.as_deref(), Some("direct"));
        assert_eq!(ctx.timestamp, Some(1700000000));
    }

    #[test]
    fn normalizes_group_message() {
        let event = json!({
            "message": {
                "key": {
                    "remoteJid": "120363012345@g.us",
                    "fromMe": false,
                    "id": "GRP456",
                    "participant": "491234567890@s.whatsapp.net"
                },
                "pushName": "Bob",
                "message": {
                    "conversation": "group chat"
                }
            }
        });

        let ctx = normalize_event(&event).expect("should normalize");
        assert_eq!(ctx.chat_type.as_deref(), Some("group"));
        assert_eq!(ctx.from.as_deref(), Some("491234567890@s.whatsapp.net"));
        assert_eq!(ctx.to.as_deref(), Some("120363012345@g.us"));
    }

    #[test]
    fn skips_own_messages() {
        let event = json!({
            "message": {
                "key": {
                    "remoteJid": "491234567890@s.whatsapp.net",
                    "fromMe": true,
                    "id": "OWN789"
                },
                "message": { "conversation": "my own message" }
            }
        });

        assert!(normalize_event(&event).is_none());
    }

    #[test]
    fn normalizes_extended_text() {
        let event = json!({
            "message": {
                "key": {
                    "remoteJid": "491234567890@s.whatsapp.net",
                    "fromMe": false,
                    "id": "EXT001"
                },
                "message": {
                    "extendedTextMessage": {
                        "text": "Check this out: https://example.com",
                        "contextInfo": {
                            "stanzaId": "REPLY_TO_ID",
                            "quotedMessage": {
                                "conversation": "original message"
                            }
                        }
                    }
                }
            }
        });

        let ctx = normalize_event(&event).expect("should normalize");
        assert_eq!(
            ctx.body.as_deref(),
            Some("Check this out: https://example.com")
        );
        assert_eq!(ctx.reply_to_id.as_deref(), Some("REPLY_TO_ID"));
        assert_eq!(ctx.reply_to_body.as_deref(), Some("original message"));
    }

    #[test]
    fn normalizes_image_with_caption() {
        let event = json!({
            "message": {
                "key": {
                    "remoteJid": "491234567890@s.whatsapp.net",
                    "fromMe": false,
                    "id": "IMG001"
                },
                "message": {
                    "imageMessage": {
                        "caption": "Look at this photo!",
                        "mimetype": "image/jpeg"
                    }
                }
            }
        });

        let ctx = normalize_event(&event).expect("should normalize");
        assert_eq!(ctx.body.as_deref(), Some("Look at this photo!"));
        assert_eq!(ctx.media_type.as_deref(), Some("image"));
    }

    #[test]
    fn returns_none_for_missing_message() {
        let event = json!({ "something_else": {} });
        assert!(normalize_event(&event).is_none());
    }
}
