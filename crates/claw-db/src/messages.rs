//! Message history operations.

use rusqlite::params;
use serde::{Deserialize, Serialize};

use crate::database::Database;
use crate::error::DbResult;

/// A row in the `messages` table.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageRow {
    pub id: Option<i64>,
    pub session_key: String,
    pub message_sid: Option<String>,
    pub direction: String,
    pub body: Option<String>,
    pub sender_id: Option<String>,
    pub sender_name: Option<String>,
    pub channel: Option<String>,
    pub created_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_json::Value>,
}

impl Database {
    /// Insert a message and return its auto-generated ID.
    pub fn insert_message(&self, msg: &MessageRow) -> DbResult<i64> {
        let metadata = msg.metadata.as_ref().map(|v| v.to_string());
        self.conn().execute(
            "INSERT INTO messages (session_key, message_sid, direction, body, sender_id, sender_name, channel, created_at, metadata)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                msg.session_key,
                msg.message_sid,
                msg.direction,
                msg.body,
                msg.sender_id,
                msg.sender_name,
                msg.channel,
                msg.created_at,
                metadata,
            ],
        )?;
        Ok(self.conn().last_insert_rowid())
    }

    /// Fetch messages for a session, optionally limited.
    pub fn get_messages_by_session(
        &self,
        session_key: &str,
        limit: Option<u32>,
    ) -> DbResult<Vec<MessageRow>> {
        let sql = match limit {
            Some(_) => {
                "SELECT id, session_key, message_sid, direction, body, sender_id, sender_name, channel, created_at, metadata
                 FROM messages WHERE session_key = ?1
                 ORDER BY created_at, id LIMIT ?2"
            }
            None => {
                "SELECT id, session_key, message_sid, direction, body, sender_id, sender_name, channel, created_at, metadata
                 FROM messages WHERE session_key = ?1
                 ORDER BY created_at, id"
            }
        };

        let mut stmt = self.conn().prepare(sql)?;
        let rows = match limit {
            Some(n) => stmt.query_map(params![session_key, n], row_to_message)?,
            None => stmt.query_map(params![session_key], row_to_message)?,
        };
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    /// Fetch messages within a time range (inclusive, ISO 8601 strings).
    pub fn get_messages_by_time_range(
        &self,
        from: &str,
        to: &str,
    ) -> DbResult<Vec<MessageRow>> {
        let mut stmt = self.conn().prepare(
            "SELECT id, session_key, message_sid, direction, body, sender_id, sender_name, channel, created_at, metadata
             FROM messages
             WHERE created_at >= ?1 AND created_at <= ?2
             ORDER BY created_at, id",
        )?;
        let rows = stmt.query_map(params![from, to], row_to_message)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }
}

fn row_to_message(row: &rusqlite::Row<'_>) -> rusqlite::Result<MessageRow> {
    let metadata_str: Option<String> = row.get(9)?;
    let metadata = metadata_str
        .as_deref()
        .map(serde_json::from_str)
        .transpose()
        .unwrap_or(None);

    Ok(MessageRow {
        id: row.get(0)?,
        session_key: row.get(1)?,
        message_sid: row.get(2)?,
        direction: row.get(3)?,
        body: row.get(4)?,
        sender_id: row.get(5)?,
        sender_name: row.get(6)?,
        channel: row.get(7)?,
        created_at: row.get(8)?,
        metadata,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sessions::SessionRow;

    fn test_db() -> Database {
        let db = Database::open_in_memory().unwrap();
        db.initialize().unwrap();
        db.create_session(&SessionRow {
            session_key: "sess-1".to_string(),
            agent_id: "agent-1".to_string(),
            channel: None,
            account_id: None,
            created_at: "2026-01-01T00:00:00Z".to_string(),
            updated_at: "2026-01-01T00:00:00Z".to_string(),
            metadata: None,
        })
        .unwrap();
        db
    }

    fn sample_message(session_key: &str, created_at: &str) -> MessageRow {
        MessageRow {
            id: None,
            session_key: session_key.to_string(),
            message_sid: Some("msg-001".to_string()),
            direction: "inbound".to_string(),
            body: Some("hello world".to_string()),
            sender_id: Some("user-1".to_string()),
            sender_name: Some("Alice".to_string()),
            channel: Some("telegram".to_string()),
            created_at: created_at.to_string(),
            metadata: None,
        }
    }

    #[test]
    fn insert_and_get_messages() {
        let db = test_db();
        let id = db
            .insert_message(&sample_message("sess-1", "2026-01-01T00:00:00Z"))
            .unwrap();
        assert!(id > 0);

        let msgs = db.get_messages_by_session("sess-1", None).unwrap();
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0].direction, "inbound");
        assert_eq!(msgs[0].body, Some("hello world".to_string()));
    }

    #[test]
    fn get_messages_with_limit() {
        let db = test_db();
        for i in 0..5 {
            db.insert_message(&sample_message(
                "sess-1",
                &format!("2026-01-0{}T00:00:00Z", i + 1),
            ))
            .unwrap();
        }

        let msgs = db.get_messages_by_session("sess-1", Some(3)).unwrap();
        assert_eq!(msgs.len(), 3);
    }

    #[test]
    fn get_messages_by_time_range() {
        let db = test_db();
        db.insert_message(&sample_message("sess-1", "2026-01-01T00:00:00Z"))
            .unwrap();
        db.insert_message(&sample_message("sess-1", "2026-01-02T00:00:00Z"))
            .unwrap();
        db.insert_message(&sample_message("sess-1", "2026-01-03T00:00:00Z"))
            .unwrap();

        let msgs = db
            .get_messages_by_time_range("2026-01-01T12:00:00Z", "2026-01-02T12:00:00Z")
            .unwrap();
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0].created_at, "2026-01-02T00:00:00Z");
    }

    #[test]
    fn empty_messages() {
        let db = test_db();
        let msgs = db.get_messages_by_session("sess-1", None).unwrap();
        assert!(msgs.is_empty());
    }
}
