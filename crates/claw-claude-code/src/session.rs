//! Session manager — claw session key to Claude Code session mapping.
//!
//! Maps claw-rust session keys (e.g. `"agent:bot1:telegram:user123"`) to
//! Claude Code session IDs (e.g. `"cc-sess-abc123"`) so multi-turn
//! conversations can be resumed with `--resume`.

use std::collections::HashMap;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use tokio::sync::RwLock;

use crate::error::{ClaudeCodeError, ClaudeCodeResult};

// ---------------------------------------------------------------------------
// SessionMapping
// ---------------------------------------------------------------------------

/// A mapping between a claw session key and a Claude Code session ID.
#[derive(Debug, Clone)]
pub struct SessionMapping {
    /// The claw-rust session key (e.g. `"agent:bot1:telegram:user123"`).
    pub claw_session_key: String,

    /// The Claude Code session ID (for `--resume`). Empty until first run.
    pub claude_session_id: String,

    /// When this mapping was created.
    pub created_at: DateTime<Utc>,

    /// When this mapping was last used.
    pub updated_at: DateTime<Utc>,

    /// Number of messages processed in this session.
    pub message_count: u32,

    /// Summary from the last ResultMessage.
    pub last_summary: Option<String>,

    /// Model used in the last run.
    pub model: Option<String>,
}

impl SessionMapping {
    /// Create a new empty mapping (no Claude Code session yet).
    pub fn new(claw_session_key: &str) -> Self {
        let now = Utc::now();
        Self {
            claw_session_key: claw_session_key.to_owned(),
            claude_session_id: String::new(),
            created_at: now,
            updated_at: now,
            message_count: 0,
            last_summary: None,
            model: None,
        }
    }

    /// Whether this mapping has an associated Claude Code session.
    pub fn has_session(&self) -> bool {
        !self.claude_session_id.is_empty()
    }
}

// ---------------------------------------------------------------------------
// SessionStore trait
// ---------------------------------------------------------------------------

/// Abstract session persistence layer.
///
/// Implementations include [`InMemorySessionStore`] (for testing) and
/// [`SqliteSessionStore`](crate::store::SqliteSessionStore) (for production).
#[async_trait]
pub trait SessionStore: Send + Sync {
    /// Look up a session mapping by claw session key.
    async fn get(&self, claw_session_key: &str) -> ClaudeCodeResult<Option<SessionMapping>>;

    /// Insert a new session mapping.
    async fn insert(&self, mapping: &SessionMapping) -> ClaudeCodeResult<()>;

    /// Update the Claude Code session ID and metadata for an existing mapping.
    async fn update(
        &self,
        claw_session_key: &str,
        claude_session_id: &str,
        message_count: u32,
        last_summary: Option<&str>,
        model: Option<&str>,
    ) -> ClaudeCodeResult<()>;

    /// Delete a session mapping (for /reset).
    async fn delete(&self, claw_session_key: &str) -> ClaudeCodeResult<()>;
}

// ---------------------------------------------------------------------------
// SessionManager
// ---------------------------------------------------------------------------

/// Coordinates claw ↔ Claude Code session mappings.
///
/// Wraps a [`SessionStore`] and provides high-level session lifecycle
/// operations: get-or-create, update after run, reset.
pub struct SessionManager {
    store: Box<dyn SessionStore>,
}

impl SessionManager {
    /// Create a new session manager backed by the given store.
    pub fn new(store: impl SessionStore + 'static) -> Self {
        Self {
            store: Box::new(store),
        }
    }

    /// Get an existing session mapping, or create a new empty one.
    pub async fn get_or_create(&self, claw_session_key: &str) -> ClaudeCodeResult<SessionMapping> {
        if let Some(mapping) = self.store.get(claw_session_key).await? {
            return Ok(mapping);
        }

        let mapping = SessionMapping::new(claw_session_key);
        self.store.insert(&mapping).await?;
        Ok(mapping)
    }

    /// Look up an existing session mapping (returns None if not found).
    pub async fn get(&self, claw_session_key: &str) -> ClaudeCodeResult<Option<SessionMapping>> {
        self.store.get(claw_session_key).await
    }

    /// Update session metadata after a Claude Code run completes.
    pub async fn update_after_run(
        &self,
        claw_session_key: &str,
        claude_session_id: &str,
        message_count: u32,
        last_summary: Option<&str>,
        model: Option<&str>,
    ) -> ClaudeCodeResult<()> {
        self.store
            .update(
                claw_session_key,
                claude_session_id,
                message_count,
                last_summary,
                model,
            )
            .await
    }

    /// Delete a session mapping (for /reset command).
    pub async fn reset(&self, claw_session_key: &str) -> ClaudeCodeResult<()> {
        self.store.delete(claw_session_key).await
    }
}

// ---------------------------------------------------------------------------
// InMemorySessionStore (for testing)
// ---------------------------------------------------------------------------

/// In-memory session store for unit tests.
pub struct InMemorySessionStore {
    sessions: RwLock<HashMap<String, SessionMapping>>,
}

impl InMemorySessionStore {
    pub fn new() -> Self {
        Self {
            sessions: RwLock::new(HashMap::new()),
        }
    }
}

impl Default for InMemorySessionStore {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl SessionStore for InMemorySessionStore {
    async fn get(&self, claw_session_key: &str) -> ClaudeCodeResult<Option<SessionMapping>> {
        Ok(self.sessions.read().await.get(claw_session_key).cloned())
    }

    async fn insert(&self, mapping: &SessionMapping) -> ClaudeCodeResult<()> {
        self.sessions
            .write()
            .await
            .insert(mapping.claw_session_key.clone(), mapping.clone());
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
        let mut sessions = self.sessions.write().await;
        let mapping = sessions.get_mut(claw_session_key).ok_or_else(|| {
            ClaudeCodeError::SessionNotMapped {
                session_key: claw_session_key.to_owned(),
            }
        })?;

        mapping.claude_session_id = claude_session_id.to_owned();
        mapping.updated_at = Utc::now();
        mapping.message_count = message_count;
        mapping.last_summary = last_summary.map(|s| s.to_owned());
        mapping.model = model.map(|s| s.to_owned());
        Ok(())
    }

    async fn delete(&self, claw_session_key: &str) -> ClaudeCodeResult<()> {
        self.sessions.write().await.remove(claw_session_key);
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
    async fn get_or_create_new_session() {
        let mgr = SessionManager::new(InMemorySessionStore::new());
        let mapping = mgr.get_or_create("sk1").await.unwrap();
        assert_eq!(mapping.claw_session_key, "sk1");
        assert!(!mapping.has_session());
        assert_eq!(mapping.message_count, 0);
    }

    #[tokio::test]
    async fn get_or_create_returns_existing() {
        let mgr = SessionManager::new(InMemorySessionStore::new());
        let m1 = mgr.get_or_create("sk1").await.unwrap();
        mgr.update_after_run("sk1", "cc-123", 1, None, None)
            .await
            .unwrap();
        let m2 = mgr.get_or_create("sk1").await.unwrap();
        assert_eq!(m2.claude_session_id, "cc-123");
        assert!(m2.has_session());
        // created_at should remain the same
        assert_eq!(m1.created_at, m2.created_at);
    }

    #[tokio::test]
    async fn update_after_run_stores_metadata() {
        let mgr = SessionManager::new(InMemorySessionStore::new());
        mgr.get_or_create("sk1").await.unwrap();

        mgr.update_after_run("sk1", "cc-abc", 3, Some("summary text"), Some("opus"))
            .await
            .unwrap();

        let mapping = mgr.get("sk1").await.unwrap().unwrap();
        assert_eq!(mapping.claude_session_id, "cc-abc");
        assert_eq!(mapping.message_count, 3);
        assert_eq!(mapping.last_summary.as_deref(), Some("summary text"));
        assert_eq!(mapping.model.as_deref(), Some("opus"));
    }

    #[tokio::test]
    async fn update_nonexistent_session_errors() {
        let mgr = SessionManager::new(InMemorySessionStore::new());
        let err = mgr
            .update_after_run("nonexistent", "cc-1", 0, None, None)
            .await
            .unwrap_err();
        assert!(matches!(err, ClaudeCodeError::SessionNotMapped { .. }));
    }

    #[tokio::test]
    async fn reset_deletes_session() {
        let mgr = SessionManager::new(InMemorySessionStore::new());
        mgr.get_or_create("sk1").await.unwrap();
        mgr.reset("sk1").await.unwrap();
        assert!(mgr.get("sk1").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn reset_nonexistent_is_ok() {
        let mgr = SessionManager::new(InMemorySessionStore::new());
        // Should not error — deleting a nonexistent key is idempotent.
        mgr.reset("nonexistent").await.unwrap();
    }

    #[tokio::test]
    async fn get_returns_none_for_unknown() {
        let mgr = SessionManager::new(InMemorySessionStore::new());
        assert!(mgr.get("unknown").await.unwrap().is_none());
    }
}
