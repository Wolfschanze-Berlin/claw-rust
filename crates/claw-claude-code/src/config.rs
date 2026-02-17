//! Claude Code CLI configuration and argument builder.
//!
//! Provides [`ClaudeCodeConfig`] which holds all settings for spawning a
//! Claude Code subprocess, and methods to convert those settings into CLI
//! argument vectors for new sessions and session resumptions.

use std::ffi::OsString;
use std::path::PathBuf;
use std::time::Duration;

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// OutputFormat
// ---------------------------------------------------------------------------

/// Output format for the Claude Code CLI.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum OutputFormat {
    /// Streaming JSON (NDJSON) — one JSON object per line.
    StreamJson,
    /// Single JSON result at the end.
    Json,
    /// Plain text output.
    Text,
}

impl OutputFormat {
    fn as_cli_value(self) -> &'static str {
        match self {
            OutputFormat::StreamJson => "stream-json",
            OutputFormat::Json => "json",
            OutputFormat::Text => "text",
        }
    }
}

// ---------------------------------------------------------------------------
// PermissionMode
// ---------------------------------------------------------------------------

/// Claude Code permission mode controlling tool approval behavior.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PermissionMode {
    /// Default interactive mode.
    Default,
    /// Auto-accept file edits.
    AcceptEdits,
    /// Bypass all permission checks.
    BypassPermissions,
    /// Plan mode (read-only exploration).
    Plan,
}

impl PermissionMode {
    fn as_cli_value(self) -> &'static str {
        match self {
            PermissionMode::Default => "default",
            PermissionMode::AcceptEdits => "acceptEdits",
            PermissionMode::BypassPermissions => "bypassPermissions",
            PermissionMode::Plan => "plan",
        }
    }
}

// ---------------------------------------------------------------------------
// ClaudeCodeConfig
// ---------------------------------------------------------------------------

/// Configuration for spawning a Claude Code CLI subprocess.
///
/// Covers binary location, model selection, tool permissions, session
/// management, and process lifecycle settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClaudeCodeConfig {
    /// Path to the `claude` binary. Defaults to `"claude"` (resolved via PATH).
    #[serde(default = "default_cli_path")]
    pub cli_path: PathBuf,

    /// Output format. Always `StreamJson` for NDJSON parsing.
    #[serde(default = "default_output_format")]
    pub output_format: OutputFormat,

    /// Enable verbose output (diagnostics on stderr).
    #[serde(default = "default_true")]
    pub verbose: bool,

    /// System prompt prepended to the conversation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub system_prompt: Option<String>,

    /// System prompt appended after the default system prompt.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub append_system_prompt: Option<String>,

    /// Tools the model is allowed to use (e.g. `["Read", "Edit", "Bash"]`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub allowed_tools: Option<Vec<String>>,

    /// Tools the model is NOT allowed to use.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub disallowed_tools: Option<Vec<String>>,

    /// Maximum number of agentic turns.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_turns: Option<usize>,

    /// Model to use (e.g. `"claude-sonnet-4-20250514"`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,

    /// Permission mode for tool approvals.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub permission_mode: Option<PermissionMode>,

    /// Working directory for the Claude Code process.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub working_directory: Option<PathBuf>,

    /// Timeout before force-killing the process after SIGTERM.
    #[serde(default = "default_kill_timeout")]
    pub kill_timeout: Duration,

    /// Additional environment variables to set on the subprocess.
    #[serde(default)]
    pub env: Vec<(String, String)>,
}

fn default_cli_path() -> PathBuf {
    PathBuf::from("claude")
}

fn default_output_format() -> OutputFormat {
    OutputFormat::StreamJson
}

fn default_true() -> bool {
    true
}

fn default_kill_timeout() -> Duration {
    Duration::from_secs(5)
}

impl Default for ClaudeCodeConfig {
    fn default() -> Self {
        Self {
            cli_path: default_cli_path(),
            output_format: OutputFormat::StreamJson,
            verbose: true,
            system_prompt: None,
            append_system_prompt: None,
            allowed_tools: None,
            disallowed_tools: None,
            max_turns: None,
            model: None,
            permission_mode: None,
            working_directory: None,
            kill_timeout: default_kill_timeout(),
            env: vec![(
                "CLAUDE_CODE_ENTRYPOINT".into(),
                "sdk-rust".into(),
            )],
        }
    }
}

impl ClaudeCodeConfig {
    /// Build the CLI argument list for a new session (no --resume).
    pub fn build_args(&self, prompt: &str) -> Vec<OsString> {
        let mut args = Vec::new();

        args.push("--print".into());
        args.push(prompt.into());

        args.push("--output-format".into());
        args.push(self.output_format.as_cli_value().into());

        if self.verbose {
            args.push("--verbose".into());
        }

        self.push_common_args(&mut args);
        args
    }

    /// Build the CLI argument list for resuming an existing session.
    pub fn build_resume_args(&self, session_id: &str, prompt: &str) -> Vec<OsString> {
        let mut args = Vec::new();

        args.push("--resume".into());
        args.push(session_id.into());

        args.push("--print".into());
        args.push(prompt.into());

        args.push("--output-format".into());
        args.push(self.output_format.as_cli_value().into());

        self.push_common_args(&mut args);
        args
    }

    /// Push arguments shared between new and resume invocations.
    fn push_common_args(&self, args: &mut Vec<OsString>) {
        if let Some(ref model) = self.model {
            args.push("--model".into());
            args.push(model.into());
        }

        if let Some(ref sp) = self.system_prompt {
            args.push("--system-prompt".into());
            args.push(sp.into());
        }

        if let Some(ref asp) = self.append_system_prompt {
            args.push("--append-system-prompt".into());
            args.push(asp.into());
        }

        if let Some(ref tools) = self.allowed_tools {
            for tool in tools {
                args.push("--allowedTools".into());
                args.push(tool.into());
            }
        }

        if let Some(ref tools) = self.disallowed_tools {
            for tool in tools {
                args.push("--disallowedTools".into());
                args.push(tool.into());
            }
        }

        if let Some(turns) = self.max_turns {
            args.push("--max-turns".into());
            args.push(turns.to_string().into());
        }

        if let Some(mode) = self.permission_mode {
            args.push("--permission-mode".into());
            args.push(mode.as_cli_value().into());
        }
    }
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn args_to_strings(args: &[OsString]) -> Vec<String> {
        args.iter()
            .map(|s| s.to_string_lossy().into_owned())
            .collect()
    }

    #[test]
    fn default_config() {
        let cfg = ClaudeCodeConfig::default();
        assert_eq!(cfg.cli_path, PathBuf::from("claude"));
        assert_eq!(cfg.output_format, OutputFormat::StreamJson);
        assert!(cfg.verbose);
        assert_eq!(cfg.kill_timeout, Duration::from_secs(5));
        assert_eq!(cfg.env.len(), 1);
        assert_eq!(cfg.env[0].0, "CLAUDE_CODE_ENTRYPOINT");
    }

    #[test]
    fn build_args_minimal() {
        let cfg = ClaudeCodeConfig::default();
        let args = args_to_strings(&cfg.build_args("hello"));
        assert!(args.contains(&"--print".to_owned()));
        assert!(args.contains(&"hello".to_owned()));
        assert!(args.contains(&"stream-json".to_owned()));
        assert!(args.contains(&"--verbose".to_owned()));
    }

    #[test]
    fn build_args_with_model() {
        let cfg = ClaudeCodeConfig {
            model: Some("claude-sonnet-4-20250514".into()),
            ..Default::default()
        };
        let args = args_to_strings(&cfg.build_args("test"));
        assert!(args.contains(&"--model".to_owned()));
        assert!(args.contains(&"claude-sonnet-4-20250514".to_owned()));
    }

    #[test]
    fn build_args_with_tools() {
        let cfg = ClaudeCodeConfig {
            allowed_tools: Some(vec!["Read".into(), "Edit".into()]),
            max_turns: Some(25),
            ..Default::default()
        };
        let args = args_to_strings(&cfg.build_args("test"));
        let tool_count = args.iter().filter(|a| a.as_str() == "--allowedTools").count();
        assert_eq!(tool_count, 2);
        assert!(args.contains(&"--max-turns".to_owned()));
        assert!(args.contains(&"25".to_owned()));
    }

    #[test]
    fn build_resume_args() {
        let cfg = ClaudeCodeConfig::default();
        let args = args_to_strings(&cfg.build_resume_args("sess-abc", "continue"));
        assert!(args.contains(&"--resume".to_owned()));
        assert!(args.contains(&"sess-abc".to_owned()));
        assert!(args.contains(&"--print".to_owned()));
        assert!(args.contains(&"continue".to_owned()));
    }

    #[test]
    fn resume_args_include_model() {
        let cfg = ClaudeCodeConfig {
            model: Some("opus".into()),
            ..Default::default()
        };
        let args = args_to_strings(&cfg.build_resume_args("s1", "hi"));
        assert!(args.contains(&"--model".to_owned()));
        assert!(args.contains(&"opus".to_owned()));
    }

    #[test]
    fn permission_mode_cli_value() {
        assert_eq!(PermissionMode::Default.as_cli_value(), "default");
        assert_eq!(PermissionMode::AcceptEdits.as_cli_value(), "acceptEdits");
        assert_eq!(
            PermissionMode::BypassPermissions.as_cli_value(),
            "bypassPermissions"
        );
        assert_eq!(PermissionMode::Plan.as_cli_value(), "plan");
    }

    #[test]
    fn config_with_permission_mode() {
        let cfg = ClaudeCodeConfig {
            permission_mode: Some(PermissionMode::BypassPermissions),
            ..Default::default()
        };
        let args = args_to_strings(&cfg.build_args("test"));
        assert!(args.contains(&"--permission-mode".to_owned()));
        assert!(args.contains(&"bypassPermissions".to_owned()));
    }
}
