//! SQLite session persistence.
//!
//! Implements [`SessionStore`] for SQLite, storing Claude Code session
//! mappings in a `claude_code_sessions` table. The table is created
//! idempotently (no dependency on claw-db's migration system).

use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use rusqlite::Connection;
use tokio::sync::Mutex;
use tracing::debug;

use crate::error::{ClaudeCodeError, ClaudeCodeResult};
use crate::session::{SessionMapping, SessionStore};

/// SQL to create the session table (idempotent).
const CREATE_TABLE_SQL: &str = "
    CREATE TABLE IF NOT EXISTS claude_code_sessions (
        claw_session_key  TEXT PRIMARY KEY,
        claude_session_id TEXT NOT NULL DEFAULT '',
        created_at        TEXT NOT NULL,
        updated_at        TEXT NOT NULL,
        message_count     INTEGER NOT NULL DEFAULT 0,
        last_summary      TEXT,
        model             TEXT
    );

    CREATE INDEX IF NOT EXISTS idx_cc_sessions_updated
        ON claude_code_sessions(updated_at);
";

// ---------------------------------------------------------------------------
// SqliteSessionStore
// ---------------------------------------------------------------------------

/// SQLite-backed implementation of [`SessionStore`].
///
/// Wraps a shared `rusqlite::Connection` in `Arc<Mutex<_>>` for async
/// access. The `claude_code_sessions` table is created on construction.
pub struct SqliteSessionStore {
    conn: Arc<Mutex<Connection>>,
}

impl SqliteSessionStore {
    /// Create a new store, ensuring the table exists.
    pub async fn new(conn: Arc<Mutex<Connection>>) -> ClaudeCodeResult<Self> {
        {
            let c = conn.lock().await;
            c.execute_batch(CREATE_TABLE_SQL)
                .map_err(|e| ClaudeCodeError::StoreError(format!("table creation failed: {e}")))?;
        }
        debug!("claude_code_sessions table ensured");
        Ok(Self { conn })
    }

    /// Create a store from an in-memory connection (for testing).
    #[cfg(test)]
    pub async fn new_in_memory() -> ClaudeCodeResult<Self> {
        let conn = Connection::open_in_memory()
            .map_err(|e| ClaudeCodeError::StoreError(e.to_string()))?;
        Self::new(Arc::new(Mutex::new(conn))).await
    }
}

#[async_trait]
impl SessionStore for SqliteSessionStore {
    async fn get(&self, claw_session_key: &str) -> ClaudeCodeResult<Option<SessionMapping>> {
        let conn = self.conn.lock().await;
        let mut stmt = conn.prepare(
            "SELECT claw_session_key, claude_session_id, created_at, updated_at,
                    message_count, last_summary, model
             FROM claude_code_sessions
             WHERE claw_session_key = ?1",
        )?;

        let result = stmt.query_row([claw_session_key], |row| {
            let created_str: String = row.get(2)?;
            let updated_str: String = row.get(3)?;

            Ok(SessionMapping {
                claw_session_key: row.get(0)?,
                claude_session_id: row.get(1)?,
                created_at: created_str
                    .parse::<DateTime<Utc>>()
                    .unwrap_or_else(|_| Utc::now()),
                updated_at: updated_str
                    .parse::<DateTime<Utc>>()
                    .unwrap_or_else(|_| Utc::now()),
                message_count: row.get::<_, u32>(4)?,
                last_summary: row.get(5)?,
                model: row.get(6)?,
            })
        });

        match result {
            Ok(mapping) => Ok(Some(mapping)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(ClaudeCodeError::DbError(e)),
        }
    }

    async fn insert(&self, mapping: &SessionMapping) -> ClaudeCodeResult<()> {
        let conn = self.conn.lock().await;
        conn.execute(
            "INSERT INTO claude_code_sessions
             (claw_session_key, claude_session_id, created_at, updated_at, message_count, last_summary, model)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            rusqlite::params![
                mapping.claw_session_key,
                mapping.claude_session_id,
                mapping.created_at.to_rfc3339(),
                mapping.updated_at.to_rfc3339(),
                mapping.message_count,
                mapping.last_summary,
                mapping.model,
            ],
        )?;
        Ok(())
    }

    async fn update(
        &self,
        claw_session_key: &str,
        claude_session_id: &str,
        message_count: u32,
        last_summary: Option<&str>,
        model: Option<&str>,
    ) -> ClaudeCodeResult<()> {
        let conn = self.conn.lock().await;
        let updated_at = Utc::now().to_rfc3339();

        let rows = conn.execute(
            "UPDATE claude_code_sessions
             SET claude_session_id = ?1,
                 updated_at = ?2,
                 message_count = ?3,
                 last_summary = ?4,
                 model = ?5
             WHERE claw_session_key = ?6",
            rusqlite::params![
                claude_session_id,
                updated_at,
                message_count,
                last_summary,
                model,
                claw_session_key,
            ],
        )?;

        if rows == 0 {
            return Err(ClaudeCodeError::SessionNotMapped {
                session_key: claw_session_key.to_owned(),
            });
        }
        Ok(())
    }

    async fn delete(&self, claw_session_key: &str) -> ClaudeCodeResult<()> {
        let conn = self.conn.lock().await;
        conn.execute(
            "DELETE FROM claude_code_sessions WHERE claw_session_key = ?1",
            [claw_session_key],
        )?;
        Ok(())
    }
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn roundtrip_insert_get() {
        let store = SqliteSessionStore::new_in_memory().await.unwrap();

        let mapping = SessionMapping::new("sk1");
        store.insert(&mapping).await.unwrap();

        let retrieved = store.get("sk1").await.unwrap().unwrap();
        assert_eq!(retrieved.claw_session_key, "sk1");
        assert_eq!(retrieved.claude_session_id, "");
        assert_eq!(retrieved.message_count, 0);
    }

    #[tokio::test]
    async fn get_nonexistent_returns_none() {
        let store = SqliteSessionStore::new_in_memory().await.unwrap();
        assert!(store.get("unknown").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn update_session_id() {
        let store = SqliteSessionStore::new_in_memory().await.unwrap();

        let mapping = SessionMapping::new("sk1");
        store.insert(&mapping).await.unwrap();

        store
            .update("sk1", "cc-abc-123", 5, Some("summary"), Some("opus"))
            .await
            .unwrap();

        let updated = store.get("sk1").await.unwrap().unwrap();
        assert_eq!(updated.claude_session_id, "cc-abc-123");
        assert_eq!(updated.message_count, 5);
        assert_eq!(updated.last_summary.as_deref(), Some("summary"));
        assert_eq!(updated.model.as_deref(), Some("opus"));
    }

    #[tokio::test]
    async fn update_nonexistent_errors() {
        let store = SqliteSessionStore::new_in_memory().await.unwrap();
        let err = store.update("ghost", "cc-1", 0, None, None).await.unwrap_err();
        assert!(matches!(err, ClaudeCodeError::SessionNotMapped { .. }));
    }

    #[tokio::test]
    async fn delete_existing() {
        let store = SqliteSessionStore::new_in_memory().await.unwrap();

        let mapping = SessionMapping::new("sk1");
        store.insert(&mapping).await.unwrap();
        store.delete("sk1").await.unwrap();

        assert!(store.get("sk1").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn delete_nonexistent_is_noop() {
        let store = SqliteSessionStore::new_in_memory().await.unwrap();
        store.delete("ghost").await.unwrap(); // should not error
    }

    #[tokio::test]
    async fn table_creation_is_idempotent() {
        let conn = Arc::new(Mutex::new(
            Connection::open_in_memory().unwrap(),
        ));

        // Create twice — should not error.
        SqliteSessionStore::new(conn.clone()).await.unwrap();
        SqliteSessionStore::new(conn).await.unwrap();
    }

    #[tokio::test]
    async fn multiple_sessions() {
        let store = SqliteSessionStore::new_in_memory().await.unwrap();

        store.insert(&SessionMapping::new("sk1")).await.unwrap();
        store.insert(&SessionMapping::new("sk2")).await.unwrap();
        store.insert(&SessionMapping::new("sk3")).await.unwrap();

        assert!(store.get("sk1").await.unwrap().is_some());
        assert!(store.get("sk2").await.unwrap().is_some());
        assert!(store.get("sk3").await.unwrap().is_some());

        store.delete("sk2").await.unwrap();
        assert!(store.get("sk2").await.unwrap().is_none());
        assert!(store.get("sk1").await.unwrap().is_some());
    }
}
