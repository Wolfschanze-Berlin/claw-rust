//! Tool profiles — named presets that bundle commonly-used tool sets.

use std::collections::HashSet;

use serde::{Deserialize, Serialize};

/// A named tool access preset.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ToolProfile {
    /// Read-only tools: search, file reading, listing.
    Minimal,
    /// Developer tools: file read/write, shell, git.
    Coding,
    /// Communication tools: channel messaging, DMs, reactions.
    Messaging,
    /// All available tools (admin only).
    Full,
}

impl ToolProfile {
    /// The set of tool names included in this profile.
    pub fn tools(&self) -> HashSet<String> {
        match self {
            Self::Minimal => [
                "read_file",
                "search",
                "list_files",
                "get_info",
            ]
            .into_iter()
            .map(String::from)
            .collect(),

            Self::Coding => {
                let mut tools = Self::Minimal.tools();
                tools.extend(
                    [
                        "write_file",
                        "edit_file",
                        "shell",
                        "git_status",
                        "git_commit",
                        "git_diff",
                    ]
                    .into_iter()
                    .map(String::from),
                );
                tools
            }

            Self::Messaging => [
                "send_message",
                "send_dm",
                "add_reaction",
                "remove_reaction",
                "list_channels",
            ]
            .into_iter()
            .map(String::from)
            .collect(),

            Self::Full => {
                let mut tools = Self::Coding.tools();
                tools.extend(Self::Messaging.tools());
                tools.extend(
                    [
                        "exec_process",
                        "network_request",
                        "manage_users",
                        "admin_config",
                    ]
                    .into_iter()
                    .map(String::from),
                );
                tools
            }
        }
    }
}

impl std::str::FromStr for ToolProfile {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "minimal" => Ok(Self::Minimal),
            "coding" => Ok(Self::Coding),
            "messaging" => Ok(Self::Messaging),
            "full" => Ok(Self::Full),
            other => Err(format!("unknown tool profile: `{other}`")),
        }
    }
}

impl std::fmt::Display for ToolProfile {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Minimal => write!(f, "minimal"),
            Self::Coding => write!(f, "coding"),
            Self::Messaging => write!(f, "messaging"),
            Self::Full => write!(f, "full"),
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn minimal_contains_read_only_tools() {
        let tools = ToolProfile::Minimal.tools();
        assert!(tools.contains("read_file"));
        assert!(tools.contains("search"));
        assert!(tools.contains("list_files"));
        assert!(!tools.contains("write_file"));
        assert!(!tools.contains("shell"));
    }

    #[test]
    fn coding_extends_minimal() {
        let minimal = ToolProfile::Minimal.tools();
        let coding = ToolProfile::Coding.tools();
        assert!(minimal.is_subset(&coding));
        assert!(coding.contains("write_file"));
        assert!(coding.contains("shell"));
    }

    #[test]
    fn messaging_is_separate() {
        let messaging = ToolProfile::Messaging.tools();
        assert!(messaging.contains("send_message"));
        assert!(messaging.contains("send_dm"));
        assert!(!messaging.contains("read_file"));
    }

    #[test]
    fn full_contains_everything() {
        let full = ToolProfile::Full.tools();
        assert!(ToolProfile::Coding.tools().is_subset(&full));
        assert!(ToolProfile::Messaging.tools().is_subset(&full));
        assert!(full.contains("exec_process"));
        assert!(full.contains("admin_config"));
    }

    #[test]
    fn from_str_case_insensitive() {
        assert_eq!("minimal".parse::<ToolProfile>().unwrap(), ToolProfile::Minimal);
        assert_eq!("CODING".parse::<ToolProfile>().unwrap(), ToolProfile::Coding);
        assert_eq!("Messaging".parse::<ToolProfile>().unwrap(), ToolProfile::Messaging);
        assert_eq!("FULL".parse::<ToolProfile>().unwrap(), ToolProfile::Full);
    }

    #[test]
    fn from_str_unknown_errors() {
        assert!("unknown".parse::<ToolProfile>().is_err());
    }

    #[test]
    fn display_roundtrip() {
        for profile in [ToolProfile::Minimal, ToolProfile::Coding, ToolProfile::Messaging, ToolProfile::Full] {
            let s = profile.to_string();
            let parsed: ToolProfile = s.parse().unwrap();
            assert_eq!(parsed, profile);
        }
    }

    #[test]
    fn serde_roundtrip() {
        for profile in [ToolProfile::Minimal, ToolProfile::Coding, ToolProfile::Messaging, ToolProfile::Full] {
            let json = serde_json::to_string(&profile).unwrap();
            let parsed: ToolProfile = serde_json::from_str(&json).unwrap();
            assert_eq!(parsed, profile);
        }
    }
}
