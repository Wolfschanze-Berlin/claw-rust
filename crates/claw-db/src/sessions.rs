//! Session CRUD operations.

use rusqlite::params;
use serde::{Deserialize, Serialize};

use crate::database::Database;
use crate::error::DbResult;

/// A row in the `sessions` table.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionRow {
    pub session_key: String,
    pub agent_id: String,
    pub channel: Option<String>,
    pub account_id: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_json::Value>,
}

impl Database {
    /// Insert a new session.
    pub fn create_session(&self, row: &SessionRow) -> DbResult<()> {
        let metadata = row.metadata.as_ref().map(|v| v.to_string());
        self.conn().execute(
            "INSERT INTO sessions (session_key, agent_id, channel, account_id, created_at, updated_at, metadata)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                row.session_key,
                row.agent_id,
                row.channel,
                row.account_id,
                row.created_at,
                row.updated_at,
                metadata,
            ],
        )?;
        Ok(())
    }

    /// Fetch a session by key, returning `None` if not found.
    pub fn get_session(&self, session_key: &str) -> DbResult<Option<SessionRow>> {
        let mut stmt = self.conn().prepare(
            "SELECT session_key, agent_id, channel, account_id, created_at, updated_at, metadata
             FROM sessions WHERE session_key = ?1",
        )?;

        let mut rows = stmt.query_map(params![session_key], row_to_session)?;
        match rows.next() {
            Some(row) => Ok(Some(row?)),
            None => Ok(None),
        }
    }

    /// Update the `updated_at` timestamp to now (ISO 8601).
    pub fn update_session_timestamp(&self, session_key: &str) -> DbResult<()> {
        let now = chrono::Utc::now().to_rfc3339();
        self.conn().execute(
            "UPDATE sessions SET updated_at = ?1 WHERE session_key = ?2",
            params![now, session_key],
        )?;
        Ok(())
    }

    /// Delete a session. Returns `true` if a row was deleted.
    pub fn delete_session(&self, session_key: &str) -> DbResult<bool> {
        let count = self.conn().execute(
            "DELETE FROM sessions WHERE session_key = ?1",
            params![session_key],
        )?;
        Ok(count > 0)
    }

    /// List all sessions.
    pub fn list_sessions(&self) -> DbResult<Vec<SessionRow>> {
        let mut stmt = self.conn().prepare(
            "SELECT session_key, agent_id, channel, account_id, created_at, updated_at, metadata
             FROM sessions ORDER BY created_at",
        )?;
        let rows = stmt.query_map([], row_to_session)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    /// List sessions belonging to a specific agent.
    pub fn list_sessions_by_agent(&self, agent_id: &str) -> DbResult<Vec<SessionRow>> {
        let mut stmt = self.conn().prepare(
            "SELECT session_key, agent_id, channel, account_id, created_at, updated_at, metadata
             FROM sessions WHERE agent_id = ?1 ORDER BY created_at",
        )?;
        let rows = stmt.query_map(params![agent_id], row_to_session)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }
}

/// Map a rusqlite row to a `SessionRow`.
fn row_to_session(row: &rusqlite::Row<'_>) -> rusqlite::Result<SessionRow> {
    let metadata_str: Option<String> = row.get(6)?;
    let metadata = metadata_str
        .as_deref()
        .map(serde_json::from_str)
        .transpose()
        .unwrap_or(None);

    Ok(SessionRow {
        session_key: row.get(0)?,
        agent_id: row.get(1)?,
        channel: row.get(2)?,
        account_id: row.get(3)?,
        created_at: row.get(4)?,
        updated_at: row.get(5)?,
        metadata,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_db() -> Database {
        let db = Database::open_in_memory().unwrap();
        db.initialize().unwrap();
        db
    }

    fn sample_session(key: &str) -> SessionRow {
        SessionRow {
            session_key: key.to_string(),
            agent_id: "agent-1".to_string(),
            channel: Some("telegram".to_string()),
            account_id: Some("acct-1".to_string()),
            created_at: "2026-01-01T00:00:00Z".to_string(),
            updated_at: "2026-01-01T00:00:00Z".to_string(),
            metadata: Some(serde_json::json!({"foo": "bar"})),
        }
    }

    #[test]
    fn create_and_get_session() {
        let db = test_db();
        let session = sample_session("sess-1");
        db.create_session(&session).unwrap();

        let fetched = db.get_session("sess-1").unwrap().unwrap();
        assert_eq!(fetched.session_key, "sess-1");
        assert_eq!(fetched.agent_id, "agent-1");
        assert_eq!(fetched.channel, Some("telegram".to_string()));
        assert_eq!(fetched.metadata, Some(serde_json::json!({"foo": "bar"})));
    }

    #[test]
    fn get_nonexistent_session_returns_none() {
        let db = test_db();
        assert!(db.get_session("nope").unwrap().is_none());
    }

    #[test]
    fn update_session_timestamp() {
        let db = test_db();
        db.create_session(&sample_session("sess-1")).unwrap();

        db.update_session_timestamp("sess-1").unwrap();

        let fetched = db.get_session("sess-1").unwrap().unwrap();
        assert_ne!(fetched.updated_at, "2026-01-01T00:00:00Z");
    }

    #[test]
    fn delete_session() {
        let db = test_db();
        db.create_session(&sample_session("sess-1")).unwrap();

        assert!(db.delete_session("sess-1").unwrap());
        assert!(!db.delete_session("sess-1").unwrap()); // already deleted
        assert!(db.get_session("sess-1").unwrap().is_none());
    }

    #[test]
    fn list_sessions() {
        let db = test_db();
        db.create_session(&sample_session("sess-1")).unwrap();
        db.create_session(&sample_session("sess-2")).unwrap();

        let all = db.list_sessions().unwrap();
        assert_eq!(all.len(), 2);
    }

    #[test]
    fn list_sessions_by_agent() {
        let db = test_db();
        db.create_session(&sample_session("sess-1")).unwrap();

        let mut other = sample_session("sess-2");
        other.agent_id = "agent-2".to_string();
        db.create_session(&other).unwrap();

        let agent1 = db.list_sessions_by_agent("agent-1").unwrap();
        assert_eq!(agent1.len(), 1);
        assert_eq!(agent1[0].session_key, "sess-1");
    }
}
