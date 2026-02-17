//! Claude Code subprocess spawn, stream, and lifecycle management.
//!
//! Wraps `tokio::process::Child` to spawn the `claude` CLI, stream NDJSON
//! messages from stdout, and manage process lifecycle (cancellation, timeout,
//! cleanup). Uses `kill_on_drop(true)` for safety.

use std::process::Stdio;

use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use tracing::{debug, info, warn};

use crate::config::ClaudeCodeConfig;
use crate::error::{ClaudeCodeError, ClaudeCodeResult};
use crate::ndjson::NdjsonParser;
use crate::types::ClaudeMessage;

// ---------------------------------------------------------------------------
// ClaudeCodeProcess
// ---------------------------------------------------------------------------

/// A spawned Claude Code CLI subprocess with NDJSON streaming.
///
/// The process is configured to pipe stdout (NDJSON messages) and stderr
/// (diagnostics). `kill_on_drop(true)` ensures the child is terminated
/// if this struct is dropped without explicit cleanup.
pub struct ClaudeCodeProcess {
    child: Child,
    cancel: CancellationToken,
    kill_timeout: std::time::Duration,
}

impl ClaudeCodeProcess {
    /// Spawn a new Claude Code process for a fresh session.
    pub async fn spawn_new(
        config: &ClaudeCodeConfig,
        prompt: &str,
        cancel: CancellationToken,
    ) -> ClaudeCodeResult<Self> {
        let args = config.build_args(prompt);
        Self::spawn_with_args(config, &args, cancel).await
    }

    /// Spawn a Claude Code process resuming an existing session.
    pub async fn spawn_resume(
        config: &ClaudeCodeConfig,
        session_id: &str,
        prompt: &str,
        cancel: CancellationToken,
    ) -> ClaudeCodeResult<Self> {
        let args = config.build_resume_args(session_id, prompt);
        Self::spawn_with_args(config, &args, cancel).await
    }

    /// Internal: spawn with pre-built argument vector.
    async fn spawn_with_args(
        config: &ClaudeCodeConfig,
        args: &[std::ffi::OsString],
        cancel: CancellationToken,
    ) -> ClaudeCodeResult<Self> {
        let mut cmd = Command::new(&config.cli_path);
        cmd.args(args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);

        if let Some(ref wd) = config.working_directory {
            cmd.current_dir(wd);
        }

        for (key, val) in &config.env {
            cmd.env(key, val);
        }

        debug!(
            cli = %config.cli_path.display(),
            args = ?args.iter().map(|a| a.to_string_lossy()).collect::<Vec<_>>(),
            "spawning Claude Code process"
        );

        let child = cmd.spawn().map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                ClaudeCodeError::CliNotFound {
                    path: config.cli_path.display().to_string(),
                }
            } else {
                ClaudeCodeError::SpawnFailed {
                    reason: e.to_string(),
                }
            }
        })?;

        info!(pid = ?child.id(), "Claude Code process spawned");

        Ok(Self {
            child,
            cancel,
            kill_timeout: config.kill_timeout,
        })
    }

    /// Begin streaming parsed NDJSON messages from stdout.
    ///
    /// Returns an `mpsc::UnboundedReceiver` that yields [`ClaudeMessage`]
    /// values as they arrive. A background task reads lines from stdout,
    /// feeds them to the NDJSON parser, and forwards parsed messages.
    ///
    /// The stream ends when:
    /// - stdout is closed (process exits)
    /// - the cancellation token fires
    /// - the receiver is dropped
    pub fn stream_messages(&mut self) -> ClaudeCodeResult<mpsc::UnboundedReceiver<ClaudeMessage>> {
        let stdout = self.child.stdout.take().ok_or_else(|| {
            ClaudeCodeError::Internal("stdout already taken or not captured".into())
        })?;

        let (tx, rx) = mpsc::unbounded_channel();
        let cancel = self.cancel.clone();

        tokio::spawn(async move {
            let reader = BufReader::new(stdout);
            let mut lines = reader.lines();
            let mut parser = NdjsonParser::new();

            loop {
                tokio::select! {
                    _ = cancel.cancelled() => {
                        debug!("NDJSON stream cancelled");
                        break;
                    }
                    line_result = lines.next_line() => {
                        match line_result {
                            Ok(Some(line)) => {
                                // Feed line + newline to parser.
                                let input = format!("{line}\n");
                                match parser.feed(&input) {
                                    Ok(messages) => {
                                        for msg in messages {
                                            if tx.send(msg).is_err() {
                                                return; // receiver dropped
                                            }
                                        }
                                    }
                                    Err(e) => {
                                        warn!(error = %e, "NDJSON parse error, continuing");
                                    }
                                }
                            }
                            Ok(None) => {
                                debug!("Claude Code stdout closed (EOF)");
                                break;
                            }
                            Err(e) => {
                                warn!(error = %e, "error reading Claude Code stdout");
                                break;
                            }
                        }
                    }
                }
            }

            // Flush remaining parser buffer.
            if let Ok(remaining) = parser.finish() {
                for msg in remaining {
                    let _ = tx.send(msg);
                }
            }
        });

        Ok(rx)
    }

    /// Spawn a background task that logs stderr lines.
    ///
    /// Should be called after `stream_messages()` since both take
    /// child I/O handles.
    pub fn spawn_stderr_logger(&mut self) {
        if let Some(stderr) = self.child.stderr.take() {
            tokio::spawn(async move {
                let reader = BufReader::new(stderr);
                let mut lines = reader.lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    debug!(target: "claude_code::stderr", "{}", line);
                }
            });
        }
    }

    /// Wait for the process to exit naturally.
    pub async fn wait(&mut self) -> ClaudeCodeResult<std::process::ExitStatus> {
        self.child.wait().await.map_err(ClaudeCodeError::ProcessIo)
    }

    /// Cancel the process: signal termination, then force-kill after timeout.
    pub async fn cancel(&mut self) -> ClaudeCodeResult<()> {
        self.cancel.cancel();

        // On Windows, there's no SIGTERM — go straight to kill.
        // On Unix, we could try SIGTERM first, but tokio's kill() sends
        // SIGKILL which is reliable. For a CLI tool, this is acceptable.
        let timeout = self.kill_timeout;
        match tokio::time::timeout(timeout, self.child.wait()).await {
            Ok(Ok(status)) => {
                info!(status = ?status, "Claude Code process exited");
                Ok(())
            }
            Ok(Err(e)) => Err(ClaudeCodeError::ProcessIo(e)),
            Err(_) => {
                warn!("Claude Code process did not exit within {:?}, force killing", timeout);
                self.child.kill().await.map_err(ClaudeCodeError::ProcessIo)?;
                Ok(())
            }
        }
    }

    /// Get the process ID (if still running).
    pub fn pid(&self) -> Option<u32> {
        self.child.id()
    }
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_creates_valid_args() {
        let cfg = ClaudeCodeConfig::default();
        let args = cfg.build_args("test prompt");
        // Should at minimum have --print and --output-format
        let strs: Vec<String> = args.iter().map(|a| a.to_string_lossy().into_owned()).collect();
        assert!(strs.contains(&"--print".to_owned()));
        assert!(strs.contains(&"stream-json".to_owned()));
    }

    #[tokio::test]
    async fn spawn_nonexistent_binary_returns_cli_not_found() {
        let cfg = ClaudeCodeConfig {
            cli_path: "nonexistent-binary-12345".into(),
            ..Default::default()
        };
        let cancel = CancellationToken::new();
        let result = ClaudeCodeProcess::spawn_new(&cfg, "test", cancel).await;
        assert!(result.is_err());
        let err = result.err().unwrap();
        assert!(matches!(err, ClaudeCodeError::CliNotFound { .. }));
    }
}
