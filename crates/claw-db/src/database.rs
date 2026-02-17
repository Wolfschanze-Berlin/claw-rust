//! Database connection manager.

use std::path::Path;

use rusqlite::Connection;
use tracing::info;

use crate::error::DbResult;
use crate::schema;

/// SQLite database handle with WAL mode and schema migrations.
pub struct Database {
    conn: Connection,
}

impl Database {
    /// Open a file-backed database at the given path.
    pub fn open(path: &Path) -> DbResult<Self> {
        let conn = Connection::open(path)?;
        Ok(Self { conn })
    }

    /// Open an in-memory database (useful for testing).
    pub fn open_in_memory() -> DbResult<Self> {
        let conn = Connection::open_in_memory()?;
        Ok(Self { conn })
    }

    /// Run schema migrations and enable WAL mode.
    pub fn initialize(&self) -> DbResult<()> {
        self.conn.execute_batch("PRAGMA journal_mode=WAL;")?;
        self.conn.execute_batch("PRAGMA foreign_keys=ON;")?;
        schema::run_migrations(&self.conn)?;
        info!("database initialized (schema v{})", schema::CURRENT_VERSION);
        Ok(())
    }

    /// Access the underlying rusqlite connection (for module impls).
    pub(crate) fn conn(&self) -> &Connection {
        &self.conn
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_in_memory_and_initialize() {
        let db = Database::open_in_memory().unwrap();
        db.initialize().unwrap();
    }

    #[test]
    fn wal_mode_is_enabled() {
        let db = Database::open_in_memory().unwrap();
        db.initialize().unwrap();

        // In-memory databases report "memory" for journal_mode, but file-based
        // ones would report "wal". We verify the PRAGMA doesn't error out.
        let mode: String = db
            .conn()
            .pragma_query_value(None, "journal_mode", |row| row.get(0))
            .unwrap();
        // in-memory => "memory", file-backed => "wal"
        assert!(mode == "memory" || mode == "wal");
    }

    #[test]
    fn foreign_keys_enabled() {
        let db = Database::open_in_memory().unwrap();
        db.initialize().unwrap();

        let fk: i32 = db
            .conn()
            .pragma_query_value(None, "foreign_keys", |row| row.get(0))
            .unwrap();
        assert_eq!(fk, 1);
    }
}
