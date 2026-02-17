//! Path resolver for agent workspace directories.
//!
//! Each agent's workspace lives at `<base_dir>/agents/<agent_id>/` and
//! contains well-known subdirectories and files.

use std::path::{Path, PathBuf};

use crate::error::WorkspaceError;

/// Resolves paths within an agent's workspace directory.
///
/// Layout:
/// ```text
/// <base_dir>/agents/<agent_id>/
///   IDENTITY.md
///   SKILLS.md
///   MEMORY.md
///   sessions/
///   auth/
///   tools/
///   models/
/// ```
#[derive(Debug, Clone)]
pub struct WorkspaceDir {
    /// Root directory for this agent's workspace.
    root: PathBuf,
}

impl WorkspaceDir {
    /// Create a workspace dir rooted at `<base_dir>/agents/<agent_id>/`.
    pub fn new(base_dir: impl Into<PathBuf>, agent_id: &str) -> Self {
        let root = base_dir.into().join("agents").join(agent_id);
        Self { root }
    }

    /// Create a workspace dir at an exact path (no `agents/<id>` nesting).
    pub fn from_path(path: impl Into<PathBuf>) -> Self {
        Self { root: path.into() }
    }

    /// The root directory of this workspace.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Path to `IDENTITY.md`.
    pub fn identity_path(&self) -> PathBuf {
        self.root.join("IDENTITY.md")
    }

    /// Path to `SKILLS.md`.
    pub fn skills_path(&self) -> PathBuf {
        self.root.join("SKILLS.md")
    }

    /// Path to `MEMORY.md`.
    pub fn memory_path(&self) -> PathBuf {
        self.root.join("MEMORY.md")
    }

    /// Path to the `sessions/` directory.
    pub fn sessions_dir(&self) -> PathBuf {
        self.root.join("sessions")
    }

    /// Path to the `auth/` directory.
    pub fn auth_dir(&self) -> PathBuf {
        self.root.join("auth")
    }

    /// Path to the `tools/` directory.
    pub fn tools_dir(&self) -> PathBuf {
        self.root.join("tools")
    }

    /// Path to the `models/` directory.
    pub fn models_dir(&self) -> PathBuf {
        self.root.join("models")
    }

    /// Create the workspace directory structure if it doesn't exist.
    pub async fn ensure_dirs(&self) -> Result<(), WorkspaceError> {
        for dir in [
            &self.root,
            &self.sessions_dir(),
            &self.auth_dir(),
            &self.tools_dir(),
            &self.models_dir(),
        ] {
            tokio::fs::create_dir_all(dir)
                .await
                .map_err(|e| WorkspaceError::io(dir, e))?;
        }
        Ok(())
    }

    /// Whether the workspace root directory exists on disk.
    pub fn exists(&self) -> bool {
        self.root.exists()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_resolution() {
        let dir = WorkspaceDir::new("/data", "gpt");
        assert_eq!(dir.root(), Path::new("/data/agents/gpt"));
        assert_eq!(dir.identity_path(), PathBuf::from("/data/agents/gpt/IDENTITY.md"));
        assert_eq!(dir.skills_path(), PathBuf::from("/data/agents/gpt/SKILLS.md"));
        assert_eq!(dir.memory_path(), PathBuf::from("/data/agents/gpt/MEMORY.md"));
        assert_eq!(dir.sessions_dir(), PathBuf::from("/data/agents/gpt/sessions"));
        assert_eq!(dir.auth_dir(), PathBuf::from("/data/agents/gpt/auth"));
        assert_eq!(dir.tools_dir(), PathBuf::from("/data/agents/gpt/tools"));
        assert_eq!(dir.models_dir(), PathBuf::from("/data/agents/gpt/models"));
    }

    #[test]
    fn from_path_skips_nesting() {
        let dir = WorkspaceDir::from_path("/custom/path");
        assert_eq!(dir.root(), Path::new("/custom/path"));
        assert_eq!(dir.identity_path(), PathBuf::from("/custom/path/IDENTITY.md"));
    }

    #[tokio::test]
    async fn ensure_dirs_creates_structure() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = WorkspaceDir::new(tmp.path(), "test-agent");

        assert!(!dir.exists());
        dir.ensure_dirs().await.unwrap();
        assert!(dir.exists());
        assert!(dir.sessions_dir().exists());
        assert!(dir.auth_dir().exists());
        assert!(dir.tools_dir().exists());
        assert!(dir.models_dir().exists());
    }

    #[test]
    fn exists_false_for_missing_dir() {
        let dir = WorkspaceDir::new("/nonexistent/path", "ghost");
        assert!(!dir.exists());
    }
}
