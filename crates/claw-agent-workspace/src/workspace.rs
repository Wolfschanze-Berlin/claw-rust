//! Core agent workspace — loads and caches identity, skills, and memory files.
//!
//! Ports OpenClaw's `workspace.ts` `readWorkspaceFile()` behavior:
//! - Files are cached in memory after first read
//! - Missing files return empty string (not an error)
//! - Cache can be invalidated per-file or globally for on-demand reload

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::SystemTime;

use tokio::sync::RwLock;
use tracing::{debug, warn};

use crate::workspace_dir::WorkspaceDir;

/// Well-known workspace file names.
pub const IDENTITY_FILE: &str = "IDENTITY.md";
pub const SKILLS_FILE: &str = "SKILLS.md";
pub const MEMORY_FILE: &str = "MEMORY.md";

/// A single cached file entry.
#[derive(Debug, Clone)]
struct CachedEntry {
    /// File content (empty string if file was missing on disk).
    content: String,
    /// Modification time at the moment we read the file.
    /// `None` if the file did not exist when cached.
    /// Stored for future staleness-detection (e.g. auto-reload on mtime change).
    #[allow(dead_code)]
    modified_at: Option<SystemTime>,
}

/// An agent's workspace: isolated directory containing persona files,
/// session data, auth profiles, and tool policies.
///
/// Workspace files (IDENTITY.md, SKILLS.md, MEMORY.md) are cached after
/// first read. Use [`reload_file`] or [`reload_all`] to refresh the cache.
#[derive(Debug, Clone)]
pub struct AgentWorkspace {
    agent_id: String,
    dir: WorkspaceDir,
    /// File cache keyed by filename (e.g. "IDENTITY.md").
    /// Wrapped in Arc so clones share the same cache.
    cache: Arc<RwLock<HashMap<String, CachedEntry>>>,
}

impl AgentWorkspace {
    /// Open a workspace for the given agent under `base_dir`.
    pub fn new(base_dir: impl Into<PathBuf>, agent_id: &str) -> Self {
        Self {
            agent_id: agent_id.to_owned(),
            dir: WorkspaceDir::new(base_dir, agent_id),
            cache: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Open a workspace at an exact path.
    pub fn from_dir(dir: WorkspaceDir, agent_id: &str) -> Self {
        Self {
            agent_id: agent_id.to_owned(),
            dir,
            cache: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// The agent's identifier.
    pub fn agent_id(&self) -> &str {
        &self.agent_id
    }

    /// The underlying workspace directory resolver.
    pub fn dir(&self) -> &WorkspaceDir {
        &self.dir
    }

    // -----------------------------------------------------------------
    // Core file loading API (ports OpenClaw's readWorkspaceFile)
    // -----------------------------------------------------------------

    /// Read a workspace file by name, returning its content.
    ///
    /// Returns an empty string if the file does not exist.
    /// Results are cached — subsequent calls return the cached value
    /// until [`reload_file`] or [`reload_all`] is called.
    ///
    /// This is the Rust equivalent of OpenClaw's `readWorkspaceFile()`.
    pub async fn read_workspace_file(&self, filename: &str) -> String {
        // Fast path: check cache under a read lock.
        {
            let cache = self.cache.read().await;
            if let Some(entry) = cache.get(filename) {
                return entry.content.clone();
            }
        }

        // Cache miss — read from disk and populate cache.
        let path = self.dir.root().join(filename);
        let entry = Self::read_and_cache_file(&self.agent_id, &path).await;
        let content = entry.content.clone();

        let mut cache = self.cache.write().await;
        cache.insert(filename.to_owned(), entry);

        content
    }

    /// Force-reload a single workspace file from disk, updating the cache.
    ///
    /// Returns the fresh content (empty string if the file is missing).
    pub async fn reload_file(&self, filename: &str) -> String {
        let path = self.dir.root().join(filename);
        let entry = Self::read_and_cache_file(&self.agent_id, &path).await;
        let content = entry.content.clone();

        let mut cache = self.cache.write().await;
        cache.insert(filename.to_owned(), entry);

        content
    }

    /// Invalidate the entire cache, forcing all files to be re-read
    /// on the next access.
    pub async fn reload_all(&self) {
        let mut cache = self.cache.write().await;
        cache.clear();
    }

    // -----------------------------------------------------------------
    // Convenience accessors for well-known files
    // -----------------------------------------------------------------

    /// Load the agent's identity (persona / character sheet).
    ///
    /// Returns an empty string if `IDENTITY.md` does not exist.
    pub async fn load_identity(&self) -> String {
        self.read_workspace_file(IDENTITY_FILE).await
    }

    /// Load the agent's skill descriptions.
    ///
    /// Returns an empty string if `SKILLS.md` does not exist.
    pub async fn load_skills(&self) -> String {
        self.read_workspace_file(SKILLS_FILE).await
    }

    /// Load the agent's persistent memory.
    ///
    /// Returns an empty string if `MEMORY.md` does not exist.
    pub async fn load_memory(&self) -> String {
        self.read_workspace_file(MEMORY_FILE).await
    }

    // -----------------------------------------------------------------
    // Internal helpers
    // -----------------------------------------------------------------

    /// Read a file from disk and produce a cache entry.
    ///
    /// Missing files produce a `CachedEntry` with empty content.
    /// I/O errors (other than not-found) are logged and treated as missing.
    async fn read_and_cache_file(agent_id: &str, path: &Path) -> CachedEntry {
        // Attempt to read modification time first (for staleness tracking).
        let modified_at = tokio::fs::metadata(path)
            .await
            .ok()
            .and_then(|m| m.modified().ok());

        match tokio::fs::read_to_string(path).await {
            Ok(content) => {
                debug!(
                    agent_id = %agent_id,
                    path = %path.display(),
                    len = content.len(),
                    "loaded workspace file"
                );
                CachedEntry {
                    content,
                    modified_at,
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                debug!(
                    agent_id = %agent_id,
                    path = %path.display(),
                    "workspace file not found, returning empty"
                );
                CachedEntry {
                    content: String::new(),
                    modified_at: None,
                }
            }
            Err(e) => {
                warn!(
                    agent_id = %agent_id,
                    path = %path.display(),
                    error = %e,
                    "failed to read workspace file, returning empty"
                );
                CachedEntry {
                    content: String::new(),
                    modified_at: None,
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[tokio::test]
    async fn read_workspace_file_returns_content() {
        let tmp = tempfile::tempdir().unwrap();
        let ws = AgentWorkspace::new(tmp.path(), "test");
        ws.dir().ensure_dirs().await.unwrap();

        fs::write(ws.dir().identity_path(), "I am a helpful assistant.").unwrap();

        let content = ws.read_workspace_file(IDENTITY_FILE).await;
        assert_eq!(content, "I am a helpful assistant.");
    }

    #[tokio::test]
    async fn read_workspace_file_returns_empty_when_missing() {
        let tmp = tempfile::tempdir().unwrap();
        let ws = AgentWorkspace::new(tmp.path(), "test");
        ws.dir().ensure_dirs().await.unwrap();

        let content = ws.read_workspace_file(IDENTITY_FILE).await;
        assert_eq!(content, "");
    }

    #[tokio::test]
    async fn load_identity_returns_content() {
        let tmp = tempfile::tempdir().unwrap();
        let ws = AgentWorkspace::new(tmp.path(), "test");
        ws.dir().ensure_dirs().await.unwrap();

        fs::write(ws.dir().identity_path(), "I am a helpful assistant.").unwrap();

        let identity = ws.load_identity().await;
        assert_eq!(identity, "I am a helpful assistant.");
    }

    #[tokio::test]
    async fn load_identity_returns_empty_when_missing() {
        let tmp = tempfile::tempdir().unwrap();
        let ws = AgentWorkspace::new(tmp.path(), "test");
        ws.dir().ensure_dirs().await.unwrap();

        assert_eq!(ws.load_identity().await, "");
    }

    #[tokio::test]
    async fn load_skills_returns_content() {
        let tmp = tempfile::tempdir().unwrap();
        let ws = AgentWorkspace::new(tmp.path(), "test");
        ws.dir().ensure_dirs().await.unwrap();

        fs::write(ws.dir().skills_path(), "I can search the web.").unwrap();

        let skills = ws.load_skills().await;
        assert_eq!(skills, "I can search the web.");
    }

    #[tokio::test]
    async fn load_memory_returns_content() {
        let tmp = tempfile::tempdir().unwrap();
        let ws = AgentWorkspace::new(tmp.path(), "test");
        ws.dir().ensure_dirs().await.unwrap();

        fs::write(ws.dir().memory_path(), "User prefers concise answers.").unwrap();

        let memory = ws.load_memory().await;
        assert_eq!(memory, "User prefers concise answers.");
    }

    #[tokio::test]
    async fn all_files_missing_returns_empty() {
        let tmp = tempfile::tempdir().unwrap();
        let ws = AgentWorkspace::new(tmp.path(), "empty");
        ws.dir().ensure_dirs().await.unwrap();

        assert_eq!(ws.load_identity().await, "");
        assert_eq!(ws.load_skills().await, "");
        assert_eq!(ws.load_memory().await, "");
    }

    #[tokio::test]
    async fn caching_returns_same_value_without_reread() {
        let tmp = tempfile::tempdir().unwrap();
        let ws = AgentWorkspace::new(tmp.path(), "test");
        ws.dir().ensure_dirs().await.unwrap();

        fs::write(ws.dir().identity_path(), "version 1").unwrap();
        assert_eq!(ws.load_identity().await, "version 1");

        // Overwrite on disk — cached value should still be returned.
        fs::write(ws.dir().identity_path(), "version 2").unwrap();
        assert_eq!(ws.load_identity().await, "version 1");
    }

    #[tokio::test]
    async fn reload_file_refreshes_cache() {
        let tmp = tempfile::tempdir().unwrap();
        let ws = AgentWorkspace::new(tmp.path(), "test");
        ws.dir().ensure_dirs().await.unwrap();

        fs::write(ws.dir().identity_path(), "version 1").unwrap();
        assert_eq!(ws.load_identity().await, "version 1");

        // Overwrite and reload.
        fs::write(ws.dir().identity_path(), "version 2").unwrap();
        let refreshed = ws.reload_file(IDENTITY_FILE).await;
        assert_eq!(refreshed, "version 2");

        // Subsequent cached read should also return the new value.
        assert_eq!(ws.load_identity().await, "version 2");
    }

    #[tokio::test]
    async fn reload_all_clears_entire_cache() {
        let tmp = tempfile::tempdir().unwrap();
        let ws = AgentWorkspace::new(tmp.path(), "test");
        ws.dir().ensure_dirs().await.unwrap();

        fs::write(ws.dir().identity_path(), "old identity").unwrap();
        fs::write(ws.dir().skills_path(), "old skills").unwrap();

        // Populate cache.
        assert_eq!(ws.load_identity().await, "old identity");
        assert_eq!(ws.load_skills().await, "old skills");

        // Update files on disk.
        fs::write(ws.dir().identity_path(), "new identity").unwrap();
        fs::write(ws.dir().skills_path(), "new skills").unwrap();

        // Without reload, cache returns old values.
        assert_eq!(ws.load_identity().await, "old identity");

        // After reload_all, fresh values are read.
        ws.reload_all().await;
        assert_eq!(ws.load_identity().await, "new identity");
        assert_eq!(ws.load_skills().await, "new skills");
    }

    #[tokio::test]
    async fn clones_share_cache() {
        let tmp = tempfile::tempdir().unwrap();
        let ws1 = AgentWorkspace::new(tmp.path(), "test");
        ws1.dir().ensure_dirs().await.unwrap();

        fs::write(ws1.dir().identity_path(), "shared content").unwrap();

        // Read through ws1 to populate cache.
        assert_eq!(ws1.load_identity().await, "shared content");

        // Clone should see the same cached value.
        let ws2 = ws1.clone();
        assert_eq!(ws2.load_identity().await, "shared content");

        // Reload through ws2 should be visible to ws1.
        fs::write(ws1.dir().identity_path(), "updated").unwrap();
        ws2.reload_file(IDENTITY_FILE).await;
        assert_eq!(ws1.load_identity().await, "updated");
    }

    #[tokio::test]
    async fn read_arbitrary_workspace_file() {
        let tmp = tempfile::tempdir().unwrap();
        let ws = AgentWorkspace::new(tmp.path(), "test");
        ws.dir().ensure_dirs().await.unwrap();

        fs::write(ws.dir().root().join("CUSTOM.md"), "custom content").unwrap();

        let content = ws.read_workspace_file("CUSTOM.md").await;
        assert_eq!(content, "custom content");
    }

    #[test]
    fn agent_id_accessor() {
        let tmp = tempfile::tempdir().unwrap();
        let ws = AgentWorkspace::new(tmp.path(), "my-agent");
        assert_eq!(ws.agent_id(), "my-agent");
    }
}
