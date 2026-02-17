//! System prompt constructor for the agent runtime.
//!
//! Assembles the full system prompt from workspace sections (identity, skills,
//! memory), available tool definitions, and runtime context (channel, session).
//! Each section is wrapped with clear boundary markers so the model can
//! distinguish prompt regions.

use claw_agent_models::ToolDefinition;
use claw_agent_workspace::AgentWorkspace;

// ---------------------------------------------------------------------------
// PromptContext
// ---------------------------------------------------------------------------

/// Runtime context injected into the system prompt.
///
/// Contains channel and session information that varies per request.
#[derive(Debug, Clone)]
pub struct PromptContext {
    /// Channel platform name (e.g. "telegram", "discord").
    pub platform: String,
    /// Channel or chat name / identifier.
    pub channel_name: Option<String>,
    /// Session identifier.
    pub session_id: String,
    /// User identity string.
    pub user_identity: Option<String>,
    /// Current timestamp (ISO 8601).
    pub timestamp: String,
    /// Channel-specific formatting rules.
    pub formatting_rules: Option<String>,
}

// ---------------------------------------------------------------------------
// PromptBuilder
// ---------------------------------------------------------------------------

/// Builds the system prompt from workspace files and runtime context.
///
/// Sections are assembled in a fixed order:
/// 1. Identity (from `IDENTITY.md`)
/// 2. Skills (from `SKILLS.md`)
/// 3. Memory (from `MEMORY.md`)
/// 4. Tool descriptions (filtered by policy)
/// 5. Channel context
/// 6. Session context
///
/// Missing workspace files are silently omitted. Empty tool lists produce
/// no TOOLS section.
pub struct PromptBuilder;

impl PromptBuilder {
    /// Assemble the full system prompt.
    ///
    /// Workspace file reads are async because [`AgentWorkspace`] caches
    /// lazily from disk on first access.
    pub async fn build(
        workspace: &AgentWorkspace,
        tools: &[ToolDefinition],
        context: &PromptContext,
    ) -> String {
        let mut sections = Vec::new();

        // 1. Identity
        let identity = workspace.load_identity().await;
        if !identity.is_empty() {
            sections.push(Self::wrap_section("IDENTITY", &identity));
        }

        // 2. Skills
        let skills = workspace.load_skills().await;
        if !skills.is_empty() {
            sections.push(Self::wrap_section("SKILLS", &skills));
        }

        // 3. Memory
        let memory = workspace.load_memory().await;
        if !memory.is_empty() {
            sections.push(Self::wrap_section("MEMORY", &memory));
        }

        // 4. Tool descriptions
        if !tools.is_empty() {
            let tool_desc = Self::format_tools(tools);
            sections.push(Self::wrap_section("TOOLS", &tool_desc));
        }

        // 5. Channel context
        sections.push(Self::format_channel_context(context));

        // 6. Session context
        sections.push(Self::format_session_context(context));

        sections.join("\n\n")
    }

    /// Wrap content with named boundary markers.
    fn wrap_section(name: &str, content: &str) -> String {
        format!("--- {name} ---\n{content}\n--- END {name} ---")
    }

    /// Format tool definitions into a human-readable list.
    fn format_tools(tools: &[ToolDefinition]) -> String {
        tools
            .iter()
            .map(|t| {
                let params = serde_json::to_string_pretty(&t.parameters)
                    .unwrap_or_else(|_| "{}".to_owned());
                format!("### {}\n{}\nParameters: {}", t.name, t.description, params)
            })
            .collect::<Vec<_>>()
            .join("\n\n")
    }

    /// Format channel context section with boundary markers.
    fn format_channel_context(ctx: &PromptContext) -> String {
        let mut lines = vec![format!("Platform: {}", ctx.platform)];

        if let Some(ref name) = ctx.channel_name {
            lines.push(format!("Channel: {name}"));
        }

        if let Some(ref rules) = ctx.formatting_rules {
            lines.push(format!("Formatting: {rules}"));
        }

        Self::wrap_section("CHANNEL", &lines.join("\n"))
    }

    /// Format session context section with boundary markers.
    fn format_session_context(ctx: &PromptContext) -> String {
        let mut lines = vec![format!("Session: {}", ctx.session_id)];

        if let Some(ref user) = ctx.user_identity {
            lines.push(format!("User: {user}"));
        }

        lines.push(format!("Timestamp: {}", ctx.timestamp));

        Self::wrap_section("SESSION", &lines.join("\n"))
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn test_context() -> PromptContext {
        PromptContext {
            platform: "telegram".to_owned(),
            channel_name: Some("general".to_owned()),
            session_id: "sess-123".to_owned(),
            user_identity: Some("alice".to_owned()),
            timestamp: "2026-02-18T12:00:00Z".to_owned(),
            formatting_rules: Some("Markdown supported".to_owned()),
        }
    }

    fn test_tools() -> Vec<ToolDefinition> {
        vec![
            ToolDefinition {
                name: "search".to_owned(),
                description: "Search the web".to_owned(),
                parameters: serde_json::json!({"type": "object", "properties": {"query": {"type": "string"}}}),
            },
            ToolDefinition {
                name: "calculate".to_owned(),
                description: "Evaluate math expressions".to_owned(),
                parameters: serde_json::json!({"type": "object", "properties": {"expr": {"type": "string"}}}),
            },
        ]
    }

    #[tokio::test]
    async fn build_with_all_sections() {
        let tmp = tempfile::tempdir().unwrap();
        let ws = AgentWorkspace::new(tmp.path(), "test");
        ws.dir().ensure_dirs().await.unwrap();

        fs::write(ws.dir().identity_path(), "I am a helpful assistant.").unwrap();
        fs::write(ws.dir().skills_path(), "I can search and calculate.").unwrap();
        fs::write(ws.dir().memory_path(), "User prefers short answers.").unwrap();

        let prompt = PromptBuilder::build(&ws, &test_tools(), &test_context()).await;

        assert!(prompt.contains("--- IDENTITY ---"));
        assert!(prompt.contains("I am a helpful assistant."));
        assert!(prompt.contains("--- END IDENTITY ---"));

        assert!(prompt.contains("--- SKILLS ---"));
        assert!(prompt.contains("I can search and calculate."));
        assert!(prompt.contains("--- END SKILLS ---"));

        assert!(prompt.contains("--- MEMORY ---"));
        assert!(prompt.contains("User prefers short answers."));
        assert!(prompt.contains("--- END MEMORY ---"));

        assert!(prompt.contains("--- TOOLS ---"));
        assert!(prompt.contains("### search"));
        assert!(prompt.contains("### calculate"));
        assert!(prompt.contains("--- END TOOLS ---"));

        assert!(prompt.contains("--- CHANNEL ---"));
        assert!(prompt.contains("Platform: telegram"));
        assert!(prompt.contains("Channel: general"));
        assert!(prompt.contains("--- END CHANNEL ---"));

        assert!(prompt.contains("--- SESSION ---"));
        assert!(prompt.contains("Session: sess-123"));
        assert!(prompt.contains("User: alice"));
        assert!(prompt.contains("Timestamp: 2026-02-18T12:00:00Z"));
        assert!(prompt.contains("--- END SESSION ---"));
    }

    #[tokio::test]
    async fn build_with_missing_workspace_files() {
        let tmp = tempfile::tempdir().unwrap();
        let ws = AgentWorkspace::new(tmp.path(), "empty");
        ws.dir().ensure_dirs().await.unwrap();

        let prompt = PromptBuilder::build(&ws, &test_tools(), &test_context()).await;

        // Workspace sections should be omitted.
        assert!(!prompt.contains("--- IDENTITY ---"));
        assert!(!prompt.contains("--- SKILLS ---"));
        assert!(!prompt.contains("--- MEMORY ---"));

        // Tools, channel, and session should still be present.
        assert!(prompt.contains("--- TOOLS ---"));
        assert!(prompt.contains("--- CHANNEL ---"));
        assert!(prompt.contains("--- SESSION ---"));
    }

    #[tokio::test]
    async fn build_with_empty_tools() {
        let tmp = tempfile::tempdir().unwrap();
        let ws = AgentWorkspace::new(tmp.path(), "test");
        ws.dir().ensure_dirs().await.unwrap();

        fs::write(ws.dir().identity_path(), "I am an agent.").unwrap();

        let prompt = PromptBuilder::build(&ws, &[], &test_context()).await;

        assert!(prompt.contains("--- IDENTITY ---"));
        assert!(!prompt.contains("--- TOOLS ---"));
        assert!(prompt.contains("--- CHANNEL ---"));
        assert!(prompt.contains("--- SESSION ---"));
    }

    #[tokio::test]
    async fn build_with_minimal_context() {
        let tmp = tempfile::tempdir().unwrap();
        let ws = AgentWorkspace::new(tmp.path(), "test");
        ws.dir().ensure_dirs().await.unwrap();

        let ctx = PromptContext {
            platform: "discord".to_owned(),
            channel_name: None,
            session_id: "s1".to_owned(),
            user_identity: None,
            timestamp: "2026-01-01T00:00:00Z".to_owned(),
            formatting_rules: None,
        };

        let prompt = PromptBuilder::build(&ws, &[], &ctx).await;

        assert!(prompt.contains("Platform: discord"));
        assert!(!prompt.contains("Channel:"));
        assert!(!prompt.contains("Formatting:"));
        assert!(prompt.contains("Session: s1"));
        assert!(!prompt.contains("User:"));
        assert!(prompt.contains("Timestamp: 2026-01-01T00:00:00Z"));
    }

    #[test]
    fn wrap_section_format() {
        let result = PromptBuilder::wrap_section("TEST", "content here");
        assert_eq!(result, "--- TEST ---\ncontent here\n--- END TEST ---");
    }

    #[test]
    fn format_tools_lists_all() {
        let tools = test_tools();
        let formatted = PromptBuilder::format_tools(&tools);

        assert!(formatted.contains("### search"));
        assert!(formatted.contains("Search the web"));
        assert!(formatted.contains("### calculate"));
        assert!(formatted.contains("Evaluate math expressions"));
        assert!(formatted.contains("Parameters:"));
    }

    #[test]
    fn format_channel_context_with_all_fields() {
        let ctx = test_context();
        let section = PromptBuilder::format_channel_context(&ctx);

        assert!(section.contains("--- CHANNEL ---"));
        assert!(section.contains("Platform: telegram"));
        assert!(section.contains("Channel: general"));
        assert!(section.contains("Formatting: Markdown supported"));
        assert!(section.contains("--- END CHANNEL ---"));
    }

    #[test]
    fn format_session_context_with_all_fields() {
        let ctx = test_context();
        let section = PromptBuilder::format_session_context(&ctx);

        assert!(section.contains("--- SESSION ---"));
        assert!(section.contains("Session: sess-123"));
        assert!(section.contains("User: alice"));
        assert!(section.contains("Timestamp: 2026-02-18T12:00:00Z"));
        assert!(section.contains("--- END SESSION ---"));
    }

    #[tokio::test]
    async fn section_ordering() {
        let tmp = tempfile::tempdir().unwrap();
        let ws = AgentWorkspace::new(tmp.path(), "test");
        ws.dir().ensure_dirs().await.unwrap();

        fs::write(ws.dir().identity_path(), "identity").unwrap();
        fs::write(ws.dir().skills_path(), "skills").unwrap();
        fs::write(ws.dir().memory_path(), "memory").unwrap();

        let prompt = PromptBuilder::build(&ws, &test_tools(), &test_context()).await;

        // Verify ordering: IDENTITY < SKILLS < MEMORY < TOOLS < CHANNEL < SESSION
        let idx_identity = prompt.find("--- IDENTITY ---").unwrap();
        let idx_skills = prompt.find("--- SKILLS ---").unwrap();
        let idx_memory = prompt.find("--- MEMORY ---").unwrap();
        let idx_tools = prompt.find("--- TOOLS ---").unwrap();
        let idx_channel = prompt.find("--- CHANNEL ---").unwrap();
        let idx_session = prompt.find("--- SESSION ---").unwrap();

        assert!(idx_identity < idx_skills);
        assert!(idx_skills < idx_memory);
        assert!(idx_memory < idx_tools);
        assert!(idx_tools < idx_channel);
        assert!(idx_channel < idx_session);
    }
}
