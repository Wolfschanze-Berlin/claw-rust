//! Subagent spawning and registry.
//!
//! Ports OpenClaw's `subagent-spawn.ts` and `subagent-registry.ts`.
//!
//! - [`SubagentSpawner`] creates isolated child sessions that inherit parent
//!   context but run their own execution lifecycle independently.
//! - [`SubagentRegistry`] tracks active subagents with lookup by session key
//!   or parent, lifecycle state tracking, and orphan cleanup.
//! - Depth limiting prevents infinite recursion (configurable, default 5).
//! - An announce queue (`tokio::sync::mpsc`) carries child-to-parent results.

use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::{mpsc, RwLock};
use tracing::{debug, info, warn};

use crate::error::RuntimeError;

// ---------------------------------------------------------------------------
// Subagent state
// ---------------------------------------------------------------------------

/// Lifecycle state of a subagent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SubagentState {
    /// The subagent is actively executing.
    Running,
    /// The subagent completed successfully with a result string.
    Completed(String),
    /// The subagent failed with an error description.
    Failed(String),
}

// ---------------------------------------------------------------------------
// Registry entry
// ---------------------------------------------------------------------------

/// A single entry in the subagent registry.
#[derive(Debug, Clone)]
pub struct SubagentEntry {
    /// Unique session key for this subagent.
    pub session_key: String,
    /// Session key of the parent that spawned this subagent.
    pub parent_key: String,
    /// Current nesting depth (parent depth + 1).
    pub depth: usize,
    /// Current lifecycle state.
    pub state: SubagentState,
    /// Index of this subagent among its parent's children.
    pub index: usize,
}

// ---------------------------------------------------------------------------
// Announce message
// ---------------------------------------------------------------------------

/// A result message sent from a child subagent to its parent via the
/// announce queue.
#[derive(Debug, Clone)]
pub struct AnnounceMessage {
    /// Session key of the child that produced this result.
    pub child_key: String,
    /// Session key of the parent that should receive this result.
    pub parent_key: String,
    /// The result payload (final output or error description).
    pub result: SubagentState,
}

// ---------------------------------------------------------------------------
// SubagentRegistry
// ---------------------------------------------------------------------------

/// Thread-safe registry of active subagents.
///
/// Uses `Arc<RwLock<...>>` for concurrent reads and exclusive writes.
#[derive(Debug, Clone)]
pub struct SubagentRegistry {
    entries: Arc<RwLock<HashMap<String, SubagentEntry>>>,
    /// Tracks the next child index per parent key.
    child_counters: Arc<RwLock<HashMap<String, usize>>>,
}

impl SubagentRegistry {
    /// Create an empty registry.
    pub fn new() -> Self {
        Self {
            entries: Arc::new(RwLock::new(HashMap::new())),
            child_counters: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Insert a new subagent entry. Returns the generated session key.
    pub async fn register(&self, entry: SubagentEntry) -> String {
        let key = entry.session_key.clone();
        self.entries.write().await.insert(key.clone(), entry);
        debug!(session_key = %key, "subagent registered");
        key
    }

    /// Look up a subagent by its session key.
    pub async fn get(&self, session_key: &str) -> Option<SubagentEntry> {
        self.entries.read().await.get(session_key).cloned()
    }

    /// Return all subagents whose parent is `parent_key`.
    pub async fn children_of(&self, parent_key: &str) -> Vec<SubagentEntry> {
        self.entries
            .read()
            .await
            .values()
            .filter(|e| e.parent_key == parent_key)
            .cloned()
            .collect()
    }

    /// Update the lifecycle state of a subagent.
    ///
    /// Returns `false` if the session key was not found.
    pub async fn set_state(&self, session_key: &str, state: SubagentState) -> bool {
        let mut entries = self.entries.write().await;
        if let Some(entry) = entries.get_mut(session_key) {
            debug!(session_key, ?state, "subagent state updated");
            entry.state = state;
            true
        } else {
            warn!(session_key, "set_state called for unknown subagent");
            false
        }
    }

    /// Remove a subagent entry by session key.
    pub async fn remove(&self, session_key: &str) -> Option<SubagentEntry> {
        self.entries.write().await.remove(session_key)
    }

    /// Remove all children (and their descendants) of `parent_key`.
    ///
    /// Returns the number of entries removed.
    pub async fn cleanup_orphans(&self, parent_key: &str) -> usize {
        // Collect all keys to remove (direct children and transitive descendants).
        let keys_to_remove = self.collect_descendants(parent_key).await;
        let count = keys_to_remove.len();

        if count > 0 {
            let mut entries = self.entries.write().await;
            for key in &keys_to_remove {
                entries.remove(key);
            }
            // Also clean up child counters for the parent and removed entries.
            let mut counters = self.child_counters.write().await;
            counters.remove(parent_key);
            for key in &keys_to_remove {
                counters.remove(key);
            }
            info!(parent_key, removed = count, "orphan subagents cleaned up");
        }

        count
    }

    /// Recursively collect all descendant session keys of `parent_key`.
    async fn collect_descendants(&self, parent_key: &str) -> Vec<String> {
        let entries = self.entries.read().await;
        let mut result = Vec::new();
        let mut stack = vec![parent_key.to_owned()];

        while let Some(current) = stack.pop() {
            for entry in entries.values() {
                if entry.parent_key == current {
                    result.push(entry.session_key.clone());
                    stack.push(entry.session_key.clone());
                }
            }
        }
        result
    }

    /// Allocate the next child index for a parent and return the generated
    /// session key: `{parent_key}:sub:{index}`.
    pub async fn next_child_key(&self, parent_key: &str) -> (String, usize) {
        let mut counters = self.child_counters.write().await;
        let index = counters.entry(parent_key.to_owned()).or_insert(0);
        let current = *index;
        *index += 1;
        let key = format!("{parent_key}:sub:{current}");
        (key, current)
    }

    /// Return the total number of entries in the registry.
    pub async fn len(&self) -> usize {
        self.entries.read().await.len()
    }

    /// Return `true` if the registry is empty.
    pub async fn is_empty(&self) -> bool {
        self.entries.read().await.is_empty()
    }
}

impl Default for SubagentRegistry {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// SubagentSpawner
// ---------------------------------------------------------------------------

/// Configuration for the subagent spawner.
#[derive(Debug, Clone)]
pub struct SpawnerConfig {
    /// Maximum nesting depth for subagents (default: 5).
    pub max_depth: usize,
    /// Buffer size for the announce channel (default: 64).
    pub announce_buffer: usize,
}

impl Default for SpawnerConfig {
    fn default() -> Self {
        Self {
            max_depth: 5,
            announce_buffer: 64,
        }
    }
}

/// Creates isolated child sessions inheriting parent context.
///
/// Each spawner owns a shared registry and an announce channel sender so
/// children can push results back to the parent.
pub struct SubagentSpawner {
    registry: SubagentRegistry,
    config: SpawnerConfig,
    announce_tx: mpsc::Sender<AnnounceMessage>,
}

/// Handle returned to the parent for receiving child results.
pub struct AnnounceReceiver {
    rx: mpsc::Receiver<AnnounceMessage>,
}

impl AnnounceReceiver {
    /// Receive the next announce message, blocking until one arrives.
    ///
    /// Returns `None` if all senders have been dropped.
    pub async fn recv(&mut self) -> Option<AnnounceMessage> {
        self.rx.recv().await
    }

    /// Try to receive without blocking.
    pub fn try_recv(&mut self) -> Result<AnnounceMessage, mpsc::error::TryRecvError> {
        self.rx.try_recv()
    }
}

impl SubagentSpawner {
    /// Create a new spawner with the given config.
    ///
    /// Returns the spawner and an [`AnnounceReceiver`] the parent should poll
    /// for child results.
    pub fn new(config: SpawnerConfig) -> (Self, AnnounceReceiver) {
        let (tx, rx) = mpsc::channel(config.announce_buffer);
        let spawner = Self {
            registry: SubagentRegistry::new(),
            config,
            announce_tx: tx,
        };
        (spawner, AnnounceReceiver { rx })
    }

    /// Create a spawner with default config.
    pub fn with_defaults() -> (Self, AnnounceReceiver) {
        Self::new(SpawnerConfig::default())
    }

    /// Spawn a new subagent under `parent_key`.
    ///
    /// Returns the child's session key and a clone of the announce sender
    /// the child should use to report its result.
    ///
    /// # Errors
    ///
    /// Returns [`RuntimeError::DepthLimitExceeded`] if `parent_depth + 1`
    /// would exceed `max_depth`.
    pub async fn spawn(
        &self,
        parent_key: &str,
        parent_depth: usize,
    ) -> Result<SpawnedSubagent, RuntimeError> {
        let child_depth = parent_depth + 1;
        if child_depth > self.config.max_depth {
            return Err(RuntimeError::DepthLimitExceeded {
                depth: child_depth,
                max: self.config.max_depth,
            });
        }

        let (session_key, index) = self.registry.next_child_key(parent_key).await;

        let entry = SubagentEntry {
            session_key: session_key.clone(),
            parent_key: parent_key.to_owned(),
            depth: child_depth,
            state: SubagentState::Running,
            index,
        };

        self.registry.register(entry).await;

        info!(
            parent_key,
            child_key = %session_key,
            depth = child_depth,
            "subagent spawned"
        );

        Ok(SpawnedSubagent {
            session_key,
            depth: child_depth,
            announce_tx: self.announce_tx.clone(),
        })
    }

    /// Access the underlying registry.
    pub fn registry(&self) -> &SubagentRegistry {
        &self.registry
    }

    /// Return the configured max depth.
    pub fn max_depth(&self) -> usize {
        self.config.max_depth
    }
}

/// Handle returned after a successful spawn.
#[derive(Debug)]
pub struct SpawnedSubagent {
    /// The unique session key for this child.
    pub session_key: String,
    /// Nesting depth of this child.
    pub depth: usize,
    /// Channel sender for announcing results back to the parent.
    announce_tx: mpsc::Sender<AnnounceMessage>,
}

impl SpawnedSubagent {
    /// Announce a completion result to the parent.
    pub async fn announce_completed(
        &self,
        parent_key: &str,
        result: String,
    ) -> Result<(), mpsc::error::SendError<AnnounceMessage>> {
        self.announce_tx
            .send(AnnounceMessage {
                child_key: self.session_key.clone(),
                parent_key: parent_key.to_owned(),
                result: SubagentState::Completed(result),
            })
            .await
    }

    /// Announce a failure to the parent.
    pub async fn announce_failed(
        &self,
        parent_key: &str,
        error: String,
    ) -> Result<(), mpsc::error::SendError<AnnounceMessage>> {
        self.announce_tx
            .send(AnnounceMessage {
                child_key: self.session_key.clone(),
                parent_key: parent_key.to_owned(),
                result: SubagentState::Failed(error),
            })
            .await
    }
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // -----------------------------------------------------------------------
    // Registry: create and retrieve
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn registry_create_and_retrieve() {
        let registry = SubagentRegistry::new();

        let entry = SubagentEntry {
            session_key: "parent:sub:0".into(),
            parent_key: "parent".into(),
            depth: 1,
            state: SubagentState::Running,
            index: 0,
        };

        registry.register(entry.clone()).await;

        let found = registry.get("parent:sub:0").await.unwrap();
        assert_eq!(found.session_key, "parent:sub:0");
        assert_eq!(found.parent_key, "parent");
        assert_eq!(found.depth, 1);
        assert_eq!(found.state, SubagentState::Running);
        assert_eq!(found.index, 0);

        // Non-existent key returns None.
        assert!(registry.get("nonexistent").await.is_none());
    }

    #[tokio::test]
    async fn registry_children_of() {
        let registry = SubagentRegistry::new();

        for i in 0..3 {
            registry
                .register(SubagentEntry {
                    session_key: format!("parent:sub:{i}"),
                    parent_key: "parent".into(),
                    depth: 1,
                    state: SubagentState::Running,
                    index: i,
                })
                .await;
        }
        // Another parent's child.
        registry
            .register(SubagentEntry {
                session_key: "other:sub:0".into(),
                parent_key: "other".into(),
                depth: 1,
                state: SubagentState::Running,
                index: 0,
            })
            .await;

        let children = registry.children_of("parent").await;
        assert_eq!(children.len(), 3);
        assert!(children.iter().all(|c| c.parent_key == "parent"));

        let other_children = registry.children_of("other").await;
        assert_eq!(other_children.len(), 1);

        assert!(registry.children_of("nobody").await.is_empty());
    }

    // -----------------------------------------------------------------------
    // Depth limit enforcement
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn depth_limit_enforced() {
        let config = SpawnerConfig {
            max_depth: 2,
            announce_buffer: 8,
        };
        let (spawner, _rx) = SubagentSpawner::new(config);

        // Depth 0 -> 1: ok
        let child1 = spawner.spawn("root", 0).await.unwrap();
        assert_eq!(child1.depth, 1);

        // Depth 1 -> 2: ok (at the limit)
        let child2 = spawner.spawn(&child1.session_key, 1).await.unwrap();
        assert_eq!(child2.depth, 2);

        // Depth 2 -> 3: should fail
        let result = spawner.spawn(&child2.session_key, 2).await;
        assert!(result.is_err());
        match result.unwrap_err() {
            RuntimeError::DepthLimitExceeded { depth, max } => {
                assert_eq!(depth, 3);
                assert_eq!(max, 2);
            }
            other => panic!("expected DepthLimitExceeded, got: {other:?}"),
        }
    }

    #[tokio::test]
    async fn default_depth_limit_is_five() {
        let (spawner, _rx) = SubagentSpawner::with_defaults();
        assert_eq!(spawner.max_depth(), 5);
    }

    // -----------------------------------------------------------------------
    // Orphan cleanup
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn orphan_cleanup_removes_children_and_descendants() {
        let registry = SubagentRegistry::new();

        // parent -> child1 -> grandchild1
        //        -> child2
        registry
            .register(SubagentEntry {
                session_key: "parent:sub:0".into(),
                parent_key: "parent".into(),
                depth: 1,
                state: SubagentState::Running,
                index: 0,
            })
            .await;
        registry
            .register(SubagentEntry {
                session_key: "parent:sub:1".into(),
                parent_key: "parent".into(),
                depth: 1,
                state: SubagentState::Running,
                index: 1,
            })
            .await;
        registry
            .register(SubagentEntry {
                session_key: "parent:sub:0:sub:0".into(),
                parent_key: "parent:sub:0".into(),
                depth: 2,
                state: SubagentState::Completed("done".into()),
                index: 0,
            })
            .await;

        assert_eq!(registry.len().await, 3);

        let removed = registry.cleanup_orphans("parent").await;
        assert_eq!(removed, 3);
        assert!(registry.is_empty().await);
    }

    #[tokio::test]
    async fn orphan_cleanup_does_not_affect_other_parents() {
        let registry = SubagentRegistry::new();

        registry
            .register(SubagentEntry {
                session_key: "a:sub:0".into(),
                parent_key: "a".into(),
                depth: 1,
                state: SubagentState::Running,
                index: 0,
            })
            .await;
        registry
            .register(SubagentEntry {
                session_key: "b:sub:0".into(),
                parent_key: "b".into(),
                depth: 1,
                state: SubagentState::Running,
                index: 0,
            })
            .await;

        let removed = registry.cleanup_orphans("a").await;
        assert_eq!(removed, 1);
        assert_eq!(registry.len().await, 1);
        assert!(registry.get("b:sub:0").await.is_some());
    }

    // -----------------------------------------------------------------------
    // Announce queue message delivery
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn announce_queue_delivers_messages() {
        let (spawner, mut rx) = SubagentSpawner::with_defaults();

        let child = spawner.spawn("parent", 0).await.unwrap();
        child
            .announce_completed("parent", "result-42".into())
            .await
            .unwrap();

        let msg = rx.recv().await.unwrap();
        assert_eq!(msg.child_key, child.session_key);
        assert_eq!(msg.parent_key, "parent");
        assert_eq!(msg.result, SubagentState::Completed("result-42".into()));
    }

    #[tokio::test]
    async fn announce_queue_delivers_failure() {
        let (spawner, mut rx) = SubagentSpawner::with_defaults();

        let child = spawner.spawn("parent", 0).await.unwrap();
        child
            .announce_failed("parent", "something broke".into())
            .await
            .unwrap();

        let msg = rx.recv().await.unwrap();
        assert_eq!(
            msg.result,
            SubagentState::Failed("something broke".into())
        );
    }

    #[tokio::test]
    async fn announce_queue_multiple_children() {
        let (spawner, mut rx) = SubagentSpawner::with_defaults();

        let c1 = spawner.spawn("parent", 0).await.unwrap();
        let c2 = spawner.spawn("parent", 0).await.unwrap();

        c1.announce_completed("parent", "r1".into()).await.unwrap();
        c2.announce_completed("parent", "r2".into()).await.unwrap();

        let m1 = rx.recv().await.unwrap();
        let m2 = rx.recv().await.unwrap();

        let results: Vec<String> = vec![m1, m2]
            .into_iter()
            .map(|m| match m.result {
                SubagentState::Completed(s) => s,
                _ => panic!("expected completed"),
            })
            .collect();
        assert!(results.contains(&"r1".to_owned()));
        assert!(results.contains(&"r2".to_owned()));
    }

    // -----------------------------------------------------------------------
    // Lifecycle state transitions
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn lifecycle_state_transitions() {
        let registry = SubagentRegistry::new();

        registry
            .register(SubagentEntry {
                session_key: "s1".into(),
                parent_key: "p".into(),
                depth: 1,
                state: SubagentState::Running,
                index: 0,
            })
            .await;

        // Running -> Completed
        assert!(
            registry
                .set_state("s1", SubagentState::Completed("done".into()))
                .await
        );
        let entry = registry.get("s1").await.unwrap();
        assert_eq!(entry.state, SubagentState::Completed("done".into()));

        // Completed -> Failed (allowed — registry doesn't enforce ordering)
        assert!(
            registry
                .set_state("s1", SubagentState::Failed("oops".into()))
                .await
        );
        let entry = registry.get("s1").await.unwrap();
        assert_eq!(entry.state, SubagentState::Failed("oops".into()));

        // Non-existent key
        assert!(!registry.set_state("nope", SubagentState::Running).await);
    }

    // -----------------------------------------------------------------------
    // Concurrent access safety
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn registry_concurrent_access() {
        let registry = SubagentRegistry::new();
        let reg = Arc::new(registry);

        let mut handles = Vec::new();
        for i in 0..50 {
            let r = reg.clone();
            handles.push(tokio::spawn(async move {
                r.register(SubagentEntry {
                    session_key: format!("p:sub:{i}"),
                    parent_key: "p".into(),
                    depth: 1,
                    state: SubagentState::Running,
                    index: i,
                })
                .await;
            }));
        }

        for h in handles {
            h.await.unwrap();
        }

        assert_eq!(reg.len().await, 50);

        // Concurrent reads.
        let mut read_handles = Vec::new();
        for i in 0..50 {
            let r = reg.clone();
            read_handles.push(tokio::spawn(async move {
                let entry = r.get(&format!("p:sub:{i}")).await;
                assert!(entry.is_some());
            }));
        }
        for h in read_handles {
            h.await.unwrap();
        }
    }

    // -----------------------------------------------------------------------
    // Session key generation
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn session_key_format() {
        let (spawner, _rx) = SubagentSpawner::with_defaults();

        let c0 = spawner.spawn("parent", 0).await.unwrap();
        assert_eq!(c0.session_key, "parent:sub:0");

        let c1 = spawner.spawn("parent", 0).await.unwrap();
        assert_eq!(c1.session_key, "parent:sub:1");

        // Different parent gets its own counter.
        let c_other = spawner.spawn("other", 0).await.unwrap();
        assert_eq!(c_other.session_key, "other:sub:0");
    }

    // -----------------------------------------------------------------------
    // Registry remove
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn registry_remove() {
        let registry = SubagentRegistry::new();

        registry
            .register(SubagentEntry {
                session_key: "k".into(),
                parent_key: "p".into(),
                depth: 1,
                state: SubagentState::Running,
                index: 0,
            })
            .await;

        let removed = registry.remove("k").await;
        assert!(removed.is_some());
        assert!(registry.get("k").await.is_none());
        assert!(registry.remove("k").await.is_none());
    }

    // -----------------------------------------------------------------------
    // SpawnerConfig defaults
    // -----------------------------------------------------------------------

    #[test]
    fn spawner_config_defaults() {
        let config = SpawnerConfig::default();
        assert_eq!(config.max_depth, 5);
        assert_eq!(config.announce_buffer, 64);
    }

    // -----------------------------------------------------------------------
    // Error variant display
    // -----------------------------------------------------------------------

    #[test]
    fn depth_limit_error_display() {
        let err = RuntimeError::DepthLimitExceeded { depth: 6, max: 5 };
        let msg = err.to_string();
        assert!(msg.contains("6"));
        assert!(msg.contains("5"));
        assert!(msg.contains("depth limit"));
    }
}
