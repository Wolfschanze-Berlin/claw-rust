//! Schema migrations for the claw database.

use rusqlite::Connection;

use crate::error::{DbError, DbResult};

/// Current schema version.
pub const CURRENT_VERSION: u32 = 1;

/// V1 migration: core tables for sessions, transcripts, messages, and channel state.
const MIGRATION_V1: &str = "
    CREATE TABLE IF NOT EXISTS sessions (
        session_key TEXT PRIMARY KEY,
        agent_id TEXT NOT NULL,
        channel TEXT,
        account_id TEXT,
        created_at TEXT NOT NULL,
        updated_at TEXT NOT NULL,
        metadata TEXT
    );

    CREATE TABLE IF NOT EXISTS transcript_events (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        session_key TEXT NOT NULL,
        event_type TEXT NOT NULL,
        role TEXT,
        content TEXT,
        metadata TEXT,
        created_at TEXT NOT NULL,
        FOREIGN KEY (session_key) REFERENCES sessions(session_key)
    );

    CREATE TABLE IF NOT EXISTS messages (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        session_key TEXT NOT NULL,
        message_sid TEXT,
        direction TEXT NOT NULL,
        body TEXT,
        sender_id TEXT,
        sender_name TEXT,
        channel TEXT,
        created_at TEXT NOT NULL,
        metadata TEXT,
        FOREIGN KEY (session_key) REFERENCES sessions(session_key)
    );

    CREATE TABLE IF NOT EXISTS channel_state (
        channel_id TEXT NOT NULL,
        account_id TEXT NOT NULL,
        state TEXT NOT NULL,
        updated_at TEXT NOT NULL,
        PRIMARY KEY (channel_id, account_id)
    );

    CREATE INDEX IF NOT EXISTS idx_transcript_session ON transcript_events(session_key);
    CREATE INDEX IF NOT EXISTS idx_messages_session ON messages(session_key);
    CREATE INDEX IF NOT EXISTS idx_messages_created ON messages(created_at);
";

/// Run all pending migrations on the given connection.
///
/// Uses a `schema_version` PRAGMA to track the current version and applies
/// migrations sequentially up to [`CURRENT_VERSION`].
pub fn run_migrations(conn: &Connection) -> DbResult<()> {
    let current: u32 = conn.pragma_query_value(None, "user_version", |row| row.get(0))?;

    if current < 1 {
        conn.execute_batch(MIGRATION_V1)
            .map_err(|e| DbError::Migration(format!("v1 migration failed: {e}")))?;
    }

    conn.pragma_update(None, "user_version", CURRENT_VERSION)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migration_creates_tables() {
        let conn = Connection::open_in_memory().unwrap();
        run_migrations(&conn).unwrap();

        // Verify all tables exist by querying sqlite_master
        let tables: Vec<String> = conn
            .prepare("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")
            .unwrap()
            .query_map([], |row| row.get(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();

        assert!(tables.contains(&"sessions".to_string()));
        assert!(tables.contains(&"transcript_events".to_string()));
        assert!(tables.contains(&"messages".to_string()));
        assert!(tables.contains(&"channel_state".to_string()));
    }

    #[test]
    fn migration_is_idempotent() {
        let conn = Connection::open_in_memory().unwrap();
        run_migrations(&conn).unwrap();
        run_migrations(&conn).unwrap(); // should not fail
    }

    #[test]
    fn migration_sets_version() {
        let conn = Connection::open_in_memory().unwrap();
        run_migrations(&conn).unwrap();

        let version: u32 = conn
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .unwrap();
        assert_eq!(version, CURRENT_VERSION);
    }
}
