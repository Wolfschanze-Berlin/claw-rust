//! Transcript event operations.

use rusqlite::params;
use serde::{Deserialize, Serialize};

use crate::database::Database;
use crate::error::DbResult;

/// A row in the `transcript_events` table.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranscriptEventRow {
    pub id: Option<i64>,
    pub session_key: String,
    pub event_type: String,
    pub role: Option<String>,
    pub content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_json::Value>,
    pub created_at: String,
}

impl Database {
    /// Insert a transcript event and return its auto-generated ID.
    pub fn insert_transcript_event(&self, event: &TranscriptEventRow) -> DbResult<i64> {
        let metadata = event.metadata.as_ref().map(|v| v.to_string());
        self.conn().execute(
            "INSERT INTO transcript_events (session_key, event_type, role, content, metadata, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                event.session_key,
                event.event_type,
                event.role,
                event.content,
                metadata,
                event.created_at,
            ],
        )?;
        Ok(self.conn().last_insert_rowid())
    }

    /// Fetch all transcript events for a session, ordered by creation time.
    pub fn get_transcript(&self, session_key: &str) -> DbResult<Vec<TranscriptEventRow>> {
        let mut stmt = self.conn().prepare(
            "SELECT id, session_key, event_type, role, content, metadata, created_at
             FROM transcript_events WHERE session_key = ?1
             ORDER BY created_at, id",
        )?;
        let rows = stmt.query_map(params![session_key], row_to_event)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    /// Fetch transcript events within a time range (inclusive, ISO 8601 strings).
    pub fn get_transcript_range(
        &self,
        session_key: &str,
        from: &str,
        to: &str,
    ) -> DbResult<Vec<TranscriptEventRow>> {
        let mut stmt = self.conn().prepare(
            "SELECT id, session_key, event_type, role, content, metadata, created_at
             FROM transcript_events
             WHERE session_key = ?1 AND created_at >= ?2 AND created_at <= ?3
             ORDER BY created_at, id",
        )?;
        let rows = stmt.query_map(params![session_key, from, to], row_to_event)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    /// Delete all transcript events for a session. Returns the number of deleted rows.
    pub fn delete_transcript(&self, session_key: &str) -> DbResult<u64> {
        let count = self.conn().execute(
            "DELETE FROM transcript_events WHERE session_key = ?1",
            params![session_key],
        )?;
        Ok(count as u64)
    }
}

fn row_to_event(row: &rusqlite::Row<'_>) -> rusqlite::Result<TranscriptEventRow> {
    let metadata_str: Option<String> = row.get(5)?;
    let metadata = metadata_str
        .as_deref()
        .map(serde_json::from_str)
        .transpose()
        .unwrap_or(None);

    Ok(TranscriptEventRow {
        id: row.get(0)?,
        session_key: row.get(1)?,
        event_type: row.get(2)?,
        role: row.get(3)?,
        content: row.get(4)?,
        metadata,
        created_at: row.get(6)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sessions::SessionRow;

    fn test_db() -> Database {
        let db = Database::open_in_memory().unwrap();
        db.initialize().unwrap();
        // Insert a parent session for FK constraint
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

    fn sample_event(session_key: &str, created_at: &str) -> TranscriptEventRow {
        TranscriptEventRow {
            id: None,
            session_key: session_key.to_string(),
            event_type: "message".to_string(),
            role: Some("user".to_string()),
            content: Some("hello".to_string()),
            metadata: None,
            created_at: created_at.to_string(),
        }
    }

    #[test]
    fn insert_and_get_transcript() {
        let db = test_db();
        let id = db
            .insert_transcript_event(&sample_event("sess-1", "2026-01-01T00:00:00Z"))
            .unwrap();
        assert!(id > 0);

        let events = db.get_transcript("sess-1").unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event_type, "message");
        assert_eq!(events[0].content, Some("hello".to_string()));
    }

    #[test]
    fn get_transcript_range() {
        let db = test_db();
        db.insert_transcript_event(&sample_event("sess-1", "2026-01-01T00:00:00Z"))
            .unwrap();
        db.insert_transcript_event(&sample_event("sess-1", "2026-01-02T00:00:00Z"))
            .unwrap();
        db.insert_transcript_event(&sample_event("sess-1", "2026-01-03T00:00:00Z"))
            .unwrap();

        let range = db
            .get_transcript_range("sess-1", "2026-01-01T12:00:00Z", "2026-01-02T12:00:00Z")
            .unwrap();
        assert_eq!(range.len(), 1);
        assert_eq!(range[0].created_at, "2026-01-02T00:00:00Z");
    }

    #[test]
    fn delete_transcript() {
        let db = test_db();
        db.insert_transcript_event(&sample_event("sess-1", "2026-01-01T00:00:00Z"))
            .unwrap();
        db.insert_transcript_event(&sample_event("sess-1", "2026-01-02T00:00:00Z"))
            .unwrap();

        let deleted = db.delete_transcript("sess-1").unwrap();
        assert_eq!(deleted, 2);
        assert!(db.get_transcript("sess-1").unwrap().is_empty());
    }

    #[test]
    fn empty_transcript() {
        let db = test_db();
        let events = db.get_transcript("sess-1").unwrap();
        assert!(events.is_empty());
    }
}
