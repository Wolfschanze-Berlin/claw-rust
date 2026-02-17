//! Core agent workspace — loads identity, skills, and memory files.

use tracing::debug;

use crate::workspace_dir::WorkspaceDir;

/// An agent's workspace: isolated directory containing persona files,
/// session data, auth profiles, and tool policies.
#[derive(Debug, Clone)]
pub struct AgentWorkspace {
    agent_id: String,
    dir: WorkspaceDir,
}

impl AgentWorkspace {
    /// Open a workspace for the given agent under `base_dir`.
    pub fn new(base_dir: impl Into<std::path::PathBuf>, agent_id: &str) -> Self {
        Self {
            agent_id: agent_id.to_owned(),
            dir: WorkspaceDir::new(base_dir, agent_id),
        }
    }

    /// Open a workspace at an exact path.
    pub fn from_dir(dir: WorkspaceDir, agent_id: &str) -> Self {
        Self {
            agent_id: agent_id.to_owned(),
            dir,
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

    /// Load the agent's identity (persona / character sheet).
    ///
    /// Returns `None` if `IDENTITY.md` does not exist.
    pub async fn load_identity(&self) -> Option<String> {
        self.read_optional_file(&self.dir.identity_path()).await
    }

    /// Load the agent's skill descriptions.
    ///
    /// Returns `None` if `SKILLS.md` does not exist.
    pub async fn load_skills(&self) -> Option<String> {
        self.read_optional_file(&self.dir.skills_path()).await
    }

    /// Load the agent's persistent memory.
    ///
    /// Returns `None` if `MEMORY.md` does not exist.
    pub async fn load_memory(&self) -> Option<String> {
        self.read_optional_file(&self.dir.memory_path()).await
    }

    /// Read a file, returning `None` if it doesn't exist (other errors logged).
    async fn read_optional_file(&self, path: &std::path::Path) -> Option<String> {
        match tokio::fs::read_to_string(path).await {
            Ok(content) => Some(content),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                debug!(agent_id = %self.agent_id, path = %path.display(), "workspace file not found");
                None
            }
            Err(e) => {
                tracing::warn!(
                    agent_id = %self.agent_id,
                    path = %path.display(),
                    error = %e,
                    "failed to read workspace file"
                );
                None
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
    async fn load_identity_returns_content() {
        let tmp = tempfile::tempdir().unwrap();
        let ws = AgentWorkspace::new(tmp.path(), "test");
        ws.dir().ensure_dirs().await.unwrap();

        fs::write(ws.dir().identity_path(), "I am a helpful assistant.").unwrap();

        let identity = ws.load_identity().await;
        assert_eq!(identity.as_deref(), Some("I am a helpful assistant."));
    }

    #[tokio::test]
    async fn load_identity_returns_none_when_missing() {
        let tmp = tempfile::tempdir().unwrap();
        let ws = AgentWorkspace::new(tmp.path(), "test");
        ws.dir().ensure_dirs().await.unwrap();

        assert!(ws.load_identity().await.is_none());
    }

    #[tokio::test]
    async fn load_skills_returns_content() {
        let tmp = tempfile::tempdir().unwrap();
        let ws = AgentWorkspace::new(tmp.path(), "test");
        ws.dir().ensure_dirs().await.unwrap();

        fs::write(ws.dir().skills_path(), "I can search the web.").unwrap();

        let skills = ws.load_skills().await;
        assert_eq!(skills.as_deref(), Some("I can search the web."));
    }

    #[tokio::test]
    async fn load_memory_returns_content() {
        let tmp = tempfile::tempdir().unwrap();
        let ws = AgentWorkspace::new(tmp.path(), "test");
        ws.dir().ensure_dirs().await.unwrap();

        fs::write(ws.dir().memory_path(), "User prefers concise answers.").unwrap();

        let memory = ws.load_memory().await;
        assert_eq!(memory.as_deref(), Some("User prefers concise answers."));
    }

    #[tokio::test]
    async fn all_files_missing_returns_none() {
        let tmp = tempfile::tempdir().unwrap();
        let ws = AgentWorkspace::new(tmp.path(), "empty");
        ws.dir().ensure_dirs().await.unwrap();

        assert!(ws.load_identity().await.is_none());
        assert!(ws.load_skills().await.is_none());
        assert!(ws.load_memory().await.is_none());
    }

    #[test]
    fn agent_id_accessor() {
        let tmp = tempfile::tempdir().unwrap();
        let ws = AgentWorkspace::new(tmp.path(), "my-agent");
        assert_eq!(ws.agent_id(), "my-agent");
    }
}
