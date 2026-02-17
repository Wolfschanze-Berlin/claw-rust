//! Discord message normalizer — converts Discord gateway events to MsgContext.
//!
//! Discord events arrive as JSON payloads from the gateway WebSocket.
//! This normalizer handles MESSAGE_CREATE events, extracting sender,
//! channel, guild, content, and thread context.

use claw_channels::msg_context::MsgContext;

/// Normalize a raw Discord gateway event (JSON) into a unified MsgContext.
pub fn normalize_event(event: &serde_json::Value) -> Option<MsgContext> {
    let event_type = event.get("t")?.as_str()?;
    let data = event.get("d")?;

    match event_type {
        "MESSAGE_CREATE" => normalize_message_create(data),
        _ => None,
    }
}

fn normalize_message_create(data: &serde_json::Value) -> Option<MsgContext> {
    let author = data.get("author")?;

    // Skip bot messages (including our own).
    if author.get("bot").and_then(|v| v.as_bool()).unwrap_or(false) {
        return None;
    }

    let channel_id = data.get("channel_id")?.as_str()?.to_string();
    let user_id = author.get("id")?.as_str()?.to_string();
    let username = author
        .get("username")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown")
        .to_string();
    let display_name = author
        .get("global_name")
        .and_then(|v| v.as_str())
        .unwrap_or(&username)
        .to_string();
    let content = data
        .get("content")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string();
    let message_id = data.get("id")?.as_str()?.to_string();
    let guild_id = data
        .get("guild_id")
        .and_then(|v| v.as_str())
        .map(String::from);

    let mut ctx = MsgContext::default();
    ctx.body = Some(content);
    ctx.from = Some(user_id.clone());
    ctx.sender_id = Some(user_id);
    ctx.sender_name = Some(display_name);
    ctx.sender_username = Some(username);
    ctx.to = Some(channel_id);
    ctx.provider = Some("discord".into());
    ctx.message_sid = Some(message_id);

    // Timestamp from ISO 8601 string.
    if let Some(ts_str) = data.get("timestamp").and_then(|v| v.as_str()) {
        if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(ts_str) {
            ctx.timestamp = Some(dt.timestamp());
        }
    }

    // Guild presence determines chat type.
    if guild_id.is_some() {
        let is_thread = data.get("thread").is_some()
            || matches!(
                data.get("type").and_then(|v| v.as_u64()),
                Some(11) | Some(12) // PUBLIC_THREAD or PRIVATE_THREAD
            );

        if is_thread {
            ctx.chat_type = Some("thread".into());
        } else {
            ctx.chat_type = Some("group".into());
        }
    } else {
        ctx.chat_type = Some("direct".into());
    }

    // Handle replies.
    if let Some(ref_msg) = data.get("referenced_message") {
        if let Some(ref_id) = ref_msg.get("id").and_then(|v| v.as_str()) {
            ctx.reply_to_id = Some(ref_id.to_string());
        }
        // Extract reply body for context.
        if let Some(ref_content) = ref_msg.get("content").and_then(|v| v.as_str()) {
            ctx.reply_to_body = Some(ref_content.to_string());
        }
    }

    // Handle thread context — message_thread_id is a serde_json::Value.
    if let Some(thread) = data.get("thread") {
        if let Some(thread_id) = thread.get("id") {
            ctx.message_thread_id = Some(thread_id.clone());
        }
    }

    // Extract attachment URLs.
    if let Some(attachments) = data.get("attachments").and_then(|v| v.as_array()) {
        let urls: Vec<String> = attachments
            .iter()
            .filter_map(|a| a.get("url").and_then(|v| v.as_str()).map(String::from))
            .collect();
        if !urls.is_empty() {
            ctx.media_urls = Some(urls.clone());
            ctx.media_url = urls.into_iter().next();
            ctx.media_type = Some("attachment".into());
        }
    }

    Some(ctx)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn normalizes_guild_text_message() {
        let event = json!({
            "t": "MESSAGE_CREATE",
            "d": {
                "id": "1234567890",
                "channel_id": "999888777",
                "guild_id": "111222333",
                "author": {
                    "id": "444555666",
                    "username": "alice",
                    "global_name": "Alice Smith"
                },
                "content": "Hello from Discord!",
                "timestamp": "2024-01-15T10:30:00+00:00"
            }
        });

        let ctx = normalize_event(&event).expect("should normalize");
        assert_eq!(ctx.body.as_deref(), Some("Hello from Discord!"));
        assert_eq!(ctx.from.as_deref(), Some("444555666"));
        assert_eq!(ctx.sender_name.as_deref(), Some("Alice Smith"));
        assert_eq!(ctx.sender_username.as_deref(), Some("alice"));
        assert_eq!(ctx.sender_id.as_deref(), Some("444555666"));
        assert_eq!(ctx.provider.as_deref(), Some("discord"));
        assert_eq!(ctx.message_sid.as_deref(), Some("1234567890"));
        assert_eq!(ctx.chat_type.as_deref(), Some("group"));
        assert_eq!(ctx.timestamp, Some(1705314600));
    }

    #[test]
    fn normalizes_dm_message() {
        let event = json!({
            "t": "MESSAGE_CREATE",
            "d": {
                "id": "9876543210",
                "channel_id": "dm_channel_id",
                "author": {
                    "id": "777888999",
                    "username": "bob"
                },
                "content": "hey there"
            }
        });

        let ctx = normalize_event(&event).expect("should normalize");
        assert_eq!(ctx.chat_type.as_deref(), Some("direct"));
        // No global_name falls back to username.
        assert_eq!(ctx.sender_name.as_deref(), Some("bob"));
    }

    #[test]
    fn skips_bot_messages() {
        let event = json!({
            "t": "MESSAGE_CREATE",
            "d": {
                "id": "111",
                "channel_id": "222",
                "author": {
                    "id": "333",
                    "username": "mybot",
                    "bot": true
                },
                "content": "automated message"
            }
        });

        assert!(normalize_event(&event).is_none());
    }

    #[test]
    fn normalizes_reply() {
        let event = json!({
            "t": "MESSAGE_CREATE",
            "d": {
                "id": "200",
                "channel_id": "100",
                "guild_id": "50",
                "author": { "id": "10", "username": "user" },
                "content": "my reply",
                "referenced_message": {
                    "id": "199",
                    "content": "original text"
                }
            }
        });

        let ctx = normalize_event(&event).expect("should normalize");
        assert_eq!(ctx.reply_to_id.as_deref(), Some("199"));
        assert_eq!(ctx.reply_to_body.as_deref(), Some("original text"));
    }

    #[test]
    fn normalizes_thread_message() {
        let event = json!({
            "t": "MESSAGE_CREATE",
            "d": {
                "id": "300",
                "channel_id": "400",
                "guild_id": "500",
                "author": { "id": "10", "username": "user" },
                "content": "in a thread",
                "type": 11,
                "thread": {
                    "id": "400"
                }
            }
        });

        let ctx = normalize_event(&event).expect("should normalize");
        assert_eq!(ctx.chat_type.as_deref(), Some("thread"));
        assert_eq!(ctx.message_thread_id, Some(json!("400")));
    }

    #[test]
    fn normalizes_attachment() {
        let event = json!({
            "t": "MESSAGE_CREATE",
            "d": {
                "id": "500",
                "channel_id": "600",
                "guild_id": "700",
                "author": { "id": "10", "username": "user" },
                "content": "check this file",
                "attachments": [
                    { "url": "https://cdn.discord.com/file1.png", "filename": "file1.png" }
                ]
            }
        });

        let ctx = normalize_event(&event).expect("should normalize");
        assert_eq!(ctx.media_url.as_deref(), Some("https://cdn.discord.com/file1.png"));
        assert_eq!(ctx.media_type.as_deref(), Some("attachment"));
    }

    #[test]
    fn skips_non_message_create() {
        let event = json!({
            "t": "TYPING_START",
            "d": { "channel_id": "123" }
        });

        assert!(normalize_event(&event).is_none());
    }
}
