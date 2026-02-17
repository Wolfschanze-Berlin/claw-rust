//! Config file loading with JSON5 parsing, `$include` resolution, and `${ENV}` substitution.
//!
//! Ports OpenClaw's config loader to Rust. Supports:
//! - JSON5 file parsing (comments, trailing commas, unquoted keys)
//! - `$include` directive for recursive file inclusion and deep merge
//! - `${ENV_VAR}` substitution in raw config text before parsing
//! - SHA-256 hash computation for change detection
//! - Snapshot creation for config file state tracking

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use regex::Regex;
use sha2::{Digest, Sha256};
use std::sync::LazyLock;
use thiserror::Error;
use tracing::{debug, warn};

use crate::OpenClawConfig;

// ---------------------------------------------------------------------------
// Error types
// ---------------------------------------------------------------------------

/// Errors that can occur during config loading.
#[derive(Error, Debug)]
pub enum ConfigError {
    #[error("failed to read config file {path}: {source}")]
    ReadFile {
        path: PathBuf,
        source: std::io::Error,
    },

    #[error("failed to write config file {path}: {source}")]
    WriteFile {
        path: PathBuf,
        source: std::io::Error,
    },

    #[error("failed to parse JSON5 in {path}: {source}")]
    ParseJson5 {
        path: PathBuf,
        source: json5::Error,
    },

    #[error("failed to deserialize config: {0}")]
    Deserialize(#[from] serde_json::Error),

    #[error("circular $include detected: {path}")]
    CircularInclude { path: PathBuf },

    #[error("$include path is not a string in {parent}")]
    InvalidIncludePath { parent: PathBuf },

    #[error("failed to create parent directory for {path}: {source}")]
    CreateDir {
        path: PathBuf,
        source: std::io::Error,
    },
}

// ---------------------------------------------------------------------------
// ConfigFileSnapshot
// ---------------------------------------------------------------------------

/// A snapshot of a config file's state at a point in time.
///
/// Used for change detection and hot-reloading.
#[derive(Debug, Clone)]
pub struct ConfigFileSnapshot {
    /// Path to the config file.
    pub path: PathBuf,
    /// Whether the file existed when the snapshot was taken.
    pub exists: bool,
    /// Raw file contents (after env substitution, before JSON5 parsing).
    pub raw: String,
    /// Parsed JSON value tree.
    pub parsed: serde_json::Value,
    /// Deserialized config.
    pub config: OpenClawConfig,
    /// SHA-256 hex digest of the raw contents.
    pub hash: String,
    /// Non-fatal issues encountered during loading.
    pub issues: Vec<String>,
}

// ---------------------------------------------------------------------------
// ENV substitution
// ---------------------------------------------------------------------------

/// Matches `${VAR_NAME}` patterns for environment variable substitution.
static ENV_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\$\{([A-Za-z_][A-Za-z0-9_]*)\}").expect("env regex must compile"));

/// Replace all `${VAR}` occurrences in `input` with their environment values.
///
/// Missing variables are replaced with empty string and a warning is logged.
fn substitute_env_vars(input: &str) -> (String, Vec<String>) {
    let mut issues = Vec::new();
    let result = ENV_RE
        .replace_all(input, |caps: &regex::Captures<'_>| {
            let var_name = &caps[1];
            match std::env::var(var_name) {
                Ok(val) => val,
                Err(_) => {
                    let msg = format!("environment variable ${{{var_name}}} is not set");
                    warn!("{}", msg);
                    issues.push(msg);
                    String::new()
                }
            }
        })
        .into_owned();
    (result, issues)
}

// ---------------------------------------------------------------------------
// $include resolution
// ---------------------------------------------------------------------------

/// Deep-merge `source` into `target`. Objects are recursively merged;
/// all other types in `source` overwrite `target`.
fn deep_merge(target: &mut serde_json::Value, source: serde_json::Value) {
    match (target, source) {
        (serde_json::Value::Object(t), serde_json::Value::Object(s)) => {
            for (key, value) in s {
                deep_merge(t.entry(key).or_insert(serde_json::Value::Null), value);
            }
        }
        (target, source) => {
            *target = source;
        }
    }
}

/// Recursively resolve `$include` directives in a JSON value tree.
///
/// An `$include` directive is an object key `"$include"` whose value is a
/// file path (relative to the current file's directory). The included file
/// is parsed and deep-merged into the parent object, replacing the
/// `$include` key.
fn resolve_includes(
    value: &mut serde_json::Value,
    base_dir: &Path,
    seen: &mut HashSet<PathBuf>,
    issues: &mut Vec<String>,
) -> Result<(), ConfigError> {
    match value {
        serde_json::Value::Object(map) => {
            // Check for $include at this level
            if let Some(include_val) = map.remove("$include") {
                let include_path_str = include_val
                    .as_str()
                    .ok_or_else(|| ConfigError::InvalidIncludePath {
                        parent: base_dir.to_path_buf(),
                    })?;

                let include_path = base_dir.join(include_path_str);
                let canonical = include_path
                    .canonicalize()
                    .unwrap_or_else(|_| include_path.clone());

                if !seen.insert(canonical.clone()) {
                    return Err(ConfigError::CircularInclude {
                        path: canonical,
                    });
                }

                match load_and_parse_file(&include_path) {
                    Ok((mut included_value, file_issues)) => {
                        issues.extend(file_issues);

                        let included_dir = include_path
                            .parent()
                            .unwrap_or(base_dir);
                        resolve_includes(&mut included_value, included_dir, seen, issues)?;

                        // Deep-merge included content into current object
                        let mut current = serde_json::Value::Object(map.clone());
                        deep_merge(&mut current, included_value);
                        if let serde_json::Value::Object(merged) = current {
                            *map = merged;
                        }
                    }
                    Err(e) => {
                        let msg = format!("failed to load $include {include_path_str}: {e}");
                        warn!("{}", msg);
                        issues.push(msg);
                    }
                }

                seen.remove(&canonical);
            }

            // Recurse into remaining values
            for val in map.values_mut() {
                resolve_includes(val, base_dir, seen, issues)?;
            }
        }
        serde_json::Value::Array(arr) => {
            for item in arr.iter_mut() {
                resolve_includes(item, base_dir, seen, issues)?;
            }
        }
        _ => {}
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// File I/O helpers
// ---------------------------------------------------------------------------

/// Read a file, substitute env vars, and parse as JSON5.
fn load_and_parse_file(path: &Path) -> Result<(serde_json::Value, Vec<String>), ConfigError> {
    let raw = std::fs::read_to_string(path).map_err(|e| ConfigError::ReadFile {
        path: path.to_path_buf(),
        source: e,
    })?;

    let (substituted, issues) = substitute_env_vars(&raw);

    let value: serde_json::Value =
        json5::from_str(&substituted).map_err(|e| ConfigError::ParseJson5 {
            path: path.to_path_buf(),
            source: e,
        })?;

    Ok((value, issues))
}

/// Compute SHA-256 hex digest of a string.
fn sha256_hex(input: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(input.as_bytes());
    format!("{:x}", hasher.finalize())
}

/// Returns the default config file path (`~/.openclaw/config.json5`).
pub fn default_config_path() -> PathBuf {
    dirs_or_home()
        .join(".openclaw")
        .join("config.json5")
}

/// Get user home directory, falling back to `/tmp` if unavailable.
fn dirs_or_home() -> PathBuf {
    std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("/tmp"))
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Load and parse a config file into a fully resolved [`OpenClawConfig`].
///
/// Steps:
/// 1. Read the file from disk
/// 2. Substitute `${ENV_VAR}` patterns
/// 3. Parse as JSON5
/// 4. Resolve `$include` directives (recursive, with cycle detection)
/// 5. Deserialize into [`OpenClawConfig`]
pub fn load_config(path: &Path) -> Result<OpenClawConfig, ConfigError> {
    let snapshot = read_config_file_snapshot(path)?;
    Ok(snapshot.config)
}

/// Read and parse a config file, returning a full [`ConfigFileSnapshot`].
///
/// This is useful when you need the raw text, hash, or parse issues
/// in addition to the deserialized config (e.g. for hot-reload change
/// detection).
pub fn read_config_file_snapshot(path: &Path) -> Result<ConfigFileSnapshot, ConfigError> {
    debug!("loading config from {}", path.display());

    if !path.exists() {
        // Return a default snapshot for missing files
        let config = OpenClawConfig::default();
        let raw = String::new();
        return Ok(ConfigFileSnapshot {
            path: path.to_path_buf(),
            exists: false,
            hash: sha256_hex(&raw),
            raw,
            parsed: serde_json::to_value(&config)?,
            config,
            issues: vec![format!("config file not found: {}", path.display())],
        });
    }

    let raw_original = std::fs::read_to_string(path).map_err(|e| ConfigError::ReadFile {
        path: path.to_path_buf(),
        source: e,
    })?;

    let (substituted, mut issues) = substitute_env_vars(&raw_original);
    let hash = sha256_hex(&raw_original);

    let mut parsed: serde_json::Value =
        json5::from_str(&substituted).map_err(|e| ConfigError::ParseJson5 {
            path: path.to_path_buf(),
            source: e,
        })?;

    let base_dir = path.parent().unwrap_or(Path::new("."));
    let mut seen = HashSet::new();
    if let Ok(canonical) = path.canonicalize() {
        seen.insert(canonical);
    }
    resolve_includes(&mut parsed, base_dir, &mut seen, &mut issues)?;

    let config: OpenClawConfig = serde_json::from_value(parsed.clone())?;

    Ok(ConfigFileSnapshot {
        path: path.to_path_buf(),
        exists: true,
        raw: raw_original,
        parsed,
        config,
        hash,
        issues,
    })
}

/// Write a config to a JSON file (pretty-printed).
///
/// Creates parent directories if they don't exist.
pub fn write_config_file(path: &Path, config: &OpenClawConfig) -> Result<(), ConfigError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| ConfigError::CreateDir {
            path: parent.to_path_buf(),
            source: e,
        })?;
    }

    let json = serde_json::to_string_pretty(config)?;
    std::fs::write(path, json).map_err(|e| ConfigError::WriteFile {
        path: path.to_path_buf(),
        source: e,
    })?;

    debug!("wrote config to {}", path.display());
    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::TempDir;

    /// Helper to create a temp config file with given contents.
    fn write_temp_config(dir: &TempDir, name: &str, content: &str) -> PathBuf {
        let path = dir.path().join(name);
        let mut f = std::fs::File::create(&path).unwrap();
        f.write_all(content.as_bytes()).unwrap();
        path
    }

    // -- Empty / minimal config -------------------------------------------

    #[test]
    fn load_empty_config() {
        let dir = TempDir::new().unwrap();
        let path = write_temp_config(&dir, "config.json5", "{}");

        let config = load_config(&path).unwrap();
        assert!(config.agents.is_none());
        assert!(config.gateway.is_none());
        assert!(config.channels.is_none());
    }

    #[test]
    fn load_missing_file_returns_default_snapshot() {
        let path = Path::new("/tmp/nonexistent_claw_config_12345.json5");
        let snapshot = read_config_file_snapshot(path).unwrap();

        assert!(!snapshot.exists);
        assert!(snapshot.config.agents.is_none());
        assert!(!snapshot.issues.is_empty());
    }

    // -- ENV var substitution ---------------------------------------------

    #[test]
    fn env_var_substitution() {
        unsafe { std::env::set_var("CLAW_TEST_PORT", "8080") };

        let dir = TempDir::new().unwrap();
        let path = write_temp_config(
            &dir,
            "config.json5",
            r#"{ "gateway": { "port": ${CLAW_TEST_PORT} } }"#,
        );

        let config = load_config(&path).unwrap();
        assert_eq!(config.gateway.unwrap().port, Some(8080));

        unsafe { std::env::remove_var("CLAW_TEST_PORT") };
    }

    #[test]
    fn missing_env_var_replaced_with_empty_string() {
        let (result, issues) = substitute_env_vars("hello ${DEFINITELY_MISSING_VAR_XYZ} world");
        assert_eq!(result, "hello  world");
        assert_eq!(issues.len(), 1);
        assert!(issues[0].contains("DEFINITELY_MISSING_VAR_XYZ"));
    }

    // -- $include resolution ----------------------------------------------

    #[test]
    fn include_merges_files() {
        let dir = TempDir::new().unwrap();

        write_temp_config(
            &dir,
            "gateway.json5",
            r#"{ "gateway": { "port": 3000, "bind": "lan" } }"#,
        );

        let path = write_temp_config(
            &dir,
            "config.json5",
            r#"{
                "$include": "gateway.json5",
                "gateway": { "mode": "local" }
            }"#,
        );

        let config = load_config(&path).unwrap();
        let gw = config.gateway.unwrap();
        // Included values
        assert_eq!(gw.port, Some(3000));
        // Local override (deep-merged on top)
        assert_eq!(gw.mode.as_deref(), Some("local"));
    }

    #[test]
    fn circular_include_detected() {
        let dir = TempDir::new().unwrap();

        write_temp_config(
            &dir,
            "a.json5",
            r#"{ "$include": "b.json5" }"#,
        );
        write_temp_config(
            &dir,
            "b.json5",
            r#"{ "$include": "a.json5" }"#,
        );

        let path = dir.path().join("a.json5");
        let result = load_config(&path);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("circular"));
    }

    // -- ConfigFileSnapshot -----------------------------------------------

    #[test]
    fn snapshot_has_correct_hash() {
        let dir = TempDir::new().unwrap();
        let content = r#"{ "gateway": { "port": 9090 } }"#;
        let path = write_temp_config(&dir, "config.json5", content);

        let snapshot = read_config_file_snapshot(&path).unwrap();
        assert!(snapshot.exists);
        assert_eq!(snapshot.raw, content);
        assert_eq!(snapshot.hash, sha256_hex(content));
        assert!(snapshot.issues.is_empty());
    }

    #[test]
    fn snapshot_hash_changes_with_content() {
        let dir = TempDir::new().unwrap();
        let path1 = write_temp_config(&dir, "a.json5", r#"{ "gateway": { "port": 1 } }"#);
        let path2 = write_temp_config(&dir, "b.json5", r#"{ "gateway": { "port": 2 } }"#);

        let snap1 = read_config_file_snapshot(&path1).unwrap();
        let snap2 = read_config_file_snapshot(&path2).unwrap();
        assert_ne!(snap1.hash, snap2.hash);
    }

    // -- write_config_file ------------------------------------------------

    #[test]
    fn write_and_reload_config() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("out.json");

        let mut config = OpenClawConfig::default();
        config.gateway = Some(crate::GatewayConfig {
            port: Some(4000),
            ..Default::default()
        });

        write_config_file(&path, &config).unwrap();

        let reloaded = load_config(&path).unwrap();
        assert_eq!(reloaded.gateway.unwrap().port, Some(4000));
    }

    #[test]
    fn write_creates_parent_dirs() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("nested").join("deep").join("config.json");

        let config = OpenClawConfig::default();
        write_config_file(&path, &config).unwrap();
        assert!(path.exists());
    }

    // -- deep_merge -------------------------------------------------------

    #[test]
    fn deep_merge_objects() {
        let mut target = serde_json::json!({
            "a": 1,
            "b": { "x": 10, "y": 20 }
        });
        let source = serde_json::json!({
            "b": { "y": 99, "z": 30 },
            "c": 3
        });

        deep_merge(&mut target, source);

        assert_eq!(target["a"], 1);
        assert_eq!(target["b"]["x"], 10);
        assert_eq!(target["b"]["y"], 99);
        assert_eq!(target["b"]["z"], 30);
        assert_eq!(target["c"], 3);
    }

    // -- JSON5 features ---------------------------------------------------

    #[test]
    fn json5_comments_and_trailing_commas() {
        let dir = TempDir::new().unwrap();
        let path = write_temp_config(
            &dir,
            "config.json5",
            r#"{
                // This is a comment
                gateway: {
                    port: 5000,
                    bind: "localhost",  // trailing comma
                },
            }"#,
        );

        let config = load_config(&path).unwrap();
        let gw = config.gateway.unwrap();
        assert_eq!(gw.port, Some(5000));
    }

    // -- default_config_path ----------------------------------------------

    #[test]
    fn default_path_ends_with_expected_segments() {
        let path = default_config_path();
        assert!(path.ends_with(".openclaw/config.json5"));
    }
}
