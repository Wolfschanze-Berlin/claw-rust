//! Channel state persistence.

use rusqlite::params;
use serde::{Deserialize, Serialize};

use crate::database::Database;
use crate::error::DbResult;

/// A row in the `channel_state` table.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelStateRow {
    pub channel_id: String,
    pub account_id: String,
    pub state: String,
    pub updated_at: String,
}

impl Database {
    /// Upsert channel state (insert or replace on conflict).
    pub fn upsert_channel_state(&self, row: &ChannelStateRow) -> DbResult<()> {
        self.conn().execute(
            "INSERT INTO channel_state (channel_id, account_id, state, updated_at)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(channel_id, account_id) DO UPDATE SET state = ?3, updated_at = ?4",
            params![row.channel_id, row.account_id, row.state, row.updated_at],
        )?;
        Ok(())
    }

    /// Get channel state by composite key.
    pub fn get_channel_state(
        &self,
        channel_id: &str,
        account_id: &str,
    ) -> DbResult<Option<ChannelStateRow>> {
        let mut stmt = self.conn().prepare(
            "SELECT channel_id, account_id, state, updated_at
             FROM channel_state WHERE channel_id = ?1 AND account_id = ?2",
        )?;
        let mut rows = stmt.query_map(params![channel_id, account_id], |row| {
            Ok(ChannelStateRow {
                channel_id: row.get(0)?,
                account_id: row.get(1)?,
                state: row.get(2)?,
                updated_at: row.get(3)?,
            })
        })?;
        match rows.next() {
            Some(row) => Ok(Some(row?)),
            None => Ok(None),
        }
    }

    /// Delete channel state. Returns `true` if a row was deleted.
    pub fn delete_channel_state(
        &self,
        channel_id: &str,
        account_id: &str,
    ) -> DbResult<bool> {
        let count = self.conn().execute(
            "DELETE FROM channel_state WHERE channel_id = ?1 AND account_id = ?2",
            params![channel_id, account_id],
        )?;
        Ok(count > 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_db() -> Database {
        let db = Database::open_in_memory().unwrap();
        db.initialize().unwrap();
        db
    }

    #[test]
    fn upsert_and_get_channel_state() {
        let db = test_db();
        let row = ChannelStateRow {
            channel_id: "telegram".to_string(),
            account_id: "acct-1".to_string(),
            state: r#"{"connected":true}"#.to_string(),
            updated_at: "2026-01-01T00:00:00Z".to_string(),
        };
        db.upsert_channel_state(&row).unwrap();

        let fetched = db
            .get_channel_state("telegram", "acct-1")
            .unwrap()
            .unwrap();
        assert_eq!(fetched.state, r#"{"connected":true}"#);
    }

    #[test]
    fn upsert_overwrites_existing() {
        let db = test_db();
        let row = ChannelStateRow {
            channel_id: "telegram".to_string(),
            account_id: "acct-1".to_string(),
            state: "old".to_string(),
            updated_at: "2026-01-01T00:00:00Z".to_string(),
        };
        db.upsert_channel_state(&row).unwrap();

        let updated = ChannelStateRow {
            state: "new".to_string(),
            updated_at: "2026-01-02T00:00:00Z".to_string(),
            ..row
        };
        db.upsert_channel_state(&updated).unwrap();

        let fetched = db
            .get_channel_state("telegram", "acct-1")
            .unwrap()
            .unwrap();
        assert_eq!(fetched.state, "new");
        assert_eq!(fetched.updated_at, "2026-01-02T00:00:00Z");
    }

    #[test]
    fn delete_channel_state() {
        let db = test_db();
        let row = ChannelStateRow {
            channel_id: "telegram".to_string(),
            account_id: "acct-1".to_string(),
            state: "data".to_string(),
            updated_at: "2026-01-01T00:00:00Z".to_string(),
        };
        db.upsert_channel_state(&row).unwrap();

        assert!(db.delete_channel_state("telegram", "acct-1").unwrap());
        assert!(!db.delete_channel_state("telegram", "acct-1").unwrap());
        assert!(db.get_channel_state("telegram", "acct-1").unwrap().is_none());
    }

    #[test]
    fn get_nonexistent_channel_state() {
        let db = test_db();
        assert!(db.get_channel_state("nope", "nope").unwrap().is_none());
    }
}
