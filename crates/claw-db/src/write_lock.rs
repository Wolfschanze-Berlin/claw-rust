//! File-based session write lock with RAII guard.
//!
//! Provides exclusive write access to a session using lock files.
//! Stale locks (from crashed processes) are detected and broken
//! automatically based on file modification time.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use serde::{Deserialize, Serialize};
use tracing::warn;

use crate::lock_error::LockError;

/// Default timeout waiting for lock acquisition.
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);

/// Default age after which a lock is considered stale.
const DEFAULT_STALE_THRESHOLD: Duration = Duration::from_secs(60);

/// Polling interval when waiting for a held lock.
const POLL_INTERVAL: Duration = Duration::from_millis(100);

/// Metadata written inside each lock file.
#[derive(Debug, Serialize, Deserialize)]
struct LockMetadata {
    holder_pid: u32,
    acquired_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    holder_info: Option<String>,
}

/// Session write lock manager.
///
/// Create one instance per lock directory, then call [`acquire`] or
/// [`try_acquire`] with a session key.
#[derive(Debug, Clone)]
pub struct SessionWriteLock {
    lock_dir: PathBuf,
    lock_timeout: Duration,
    stale_threshold: Duration,
}

/// RAII guard that releases the lock file on drop.
#[derive(Debug)]
pub struct LockGuard {
    lock_path: PathBuf,
}

impl Drop for LockGuard {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.lock_path);
    }
}

impl LockGuard {
    /// Path to the lock file held by this guard.
    pub fn lock_path(&self) -> &Path {
        &self.lock_path
    }
}

impl SessionWriteLock {
    /// Create a new lock manager storing lock files in `lock_dir`.
    pub fn new(lock_dir: impl Into<PathBuf>) -> Self {
        Self {
            lock_dir: lock_dir.into(),
            lock_timeout: DEFAULT_TIMEOUT,
            stale_threshold: DEFAULT_STALE_THRESHOLD,
        }
    }

    /// Set the maximum time to wait for lock acquisition.
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.lock_timeout = timeout;
        self
    }

    /// Set the age after which a lock file is considered stale.
    pub fn with_stale_threshold(mut self, threshold: Duration) -> Self {
        self.stale_threshold = threshold;
        self
    }

    /// Acquire exclusive write lock for a session.
    ///
    /// Blocks (async-sleeping) until the lock is acquired or the timeout
    /// is reached.  Stale locks are broken automatically.
    pub async fn acquire(&self, session_key: &str) -> Result<LockGuard, LockError> {
        let start = tokio::time::Instant::now();

        loop {
            match self.try_acquire_inner(session_key)? {
                TryResult::Acquired(guard) => return Ok(guard),
                TryResult::StaleBroken(guard, info) => {
                    warn!(
                        session_key,
                        age_secs = info.age_secs,
                        "broke stale lock for session"
                    );
                    return Ok(guard);
                }
                TryResult::Held => {}
            }

            let elapsed = start.elapsed();
            if elapsed >= self.lock_timeout {
                return Err(LockError::Timeout {
                    session_key: session_key.to_string(),
                    elapsed_ms: elapsed.as_millis() as u64,
                });
            }

            tokio::time::sleep(POLL_INTERVAL).await;
        }
    }

    /// Try to acquire the lock without blocking.
    ///
    /// Returns `Ok(Some(guard))` if acquired, `Ok(None)` if already held
    /// by another process.
    pub fn try_acquire(&self, session_key: &str) -> Result<Option<LockGuard>, LockError> {
        match self.try_acquire_inner(session_key)? {
            TryResult::Acquired(guard) | TryResult::StaleBroken(guard, _) => Ok(Some(guard)),
            TryResult::Held => Ok(None),
        }
    }

    /// Check whether a session is currently locked.
    pub fn is_locked(&self, session_key: &str) -> bool {
        self.lock_path(session_key).exists()
    }

    // ------------------------------------------------------------------
    // Internal helpers
    // ------------------------------------------------------------------

    fn lock_path(&self, session_key: &str) -> PathBuf {
        self.lock_dir
            .join(format!("{}.lock", sanitize_session_key(session_key)))
    }

    /// Attempt a single lock acquisition cycle.
    fn try_acquire_inner(&self, session_key: &str) -> Result<TryResult, LockError> {
        let path = self.lock_path(session_key);

        // Ensure lock directory exists.
        if !self.lock_dir.exists() {
            fs::create_dir_all(&self.lock_dir).map_err(|e| LockError::io(&self.lock_dir, e))?;
        }

        // Check for existing lock.
        if path.exists() {
            // Is it stale?
            if let Some(age) = self.lock_age(&path) {
                if age >= self.stale_threshold {
                    let age_secs = age.as_secs();
                    fs::remove_file(&path).map_err(|e| LockError::io(&path, e))?;
                    let guard = self.write_lock_file(&path)?;
                    return Ok(TryResult::StaleBroken(guard, StaleInfo { age_secs }));
                }
            }
            return Ok(TryResult::Held);
        }

        // No lock exists — create it.
        let guard = self.write_lock_file(&path)?;
        Ok(TryResult::Acquired(guard))
    }

    /// Write the lock file and return a guard.
    fn write_lock_file(&self, path: &Path) -> Result<LockGuard, LockError> {
        let metadata = LockMetadata {
            holder_pid: std::process::id(),
            acquired_at: chrono::Utc::now().to_rfc3339(),
            holder_info: None,
        };
        let json = serde_json::to_string_pretty(&metadata)
            .expect("LockMetadata serialization cannot fail");
        fs::write(path, json).map_err(|e| LockError::io(path, e))?;
        Ok(LockGuard {
            lock_path: path.to_path_buf(),
        })
    }

    /// How old is the lock file? Returns `None` on metadata errors.
    fn lock_age(&self, path: &Path) -> Option<Duration> {
        let meta = fs::metadata(path).ok()?;
        let modified = meta.modified().ok()?;
        SystemTime::now().duration_since(modified).ok()
    }
}

/// Replace characters that are invalid in filenames.
fn sanitize_session_key(key: &str) -> String {
    key.replace(':', "_").replace('/', "_")
}

/// Internal result of a single try-acquire attempt.
enum TryResult {
    Acquired(LockGuard),
    StaleBroken(LockGuard, StaleInfo),
    Held,
}

struct StaleInfo {
    age_secs: u64,
}

// =====================================================================
// Tests
// =====================================================================

#[cfg(test)]
mod tests {
    use std::fs;
    use std::time::Duration;

    use super::*;

    fn temp_lock_dir() -> tempfile::TempDir {
        tempfile::tempdir().expect("create temp dir")
    }

    #[tokio::test]
    async fn acquire_and_release() {
        let dir = temp_lock_dir();
        let locker = SessionWriteLock::new(dir.path());

        let guard = locker.acquire("sess:1").await.unwrap();
        assert!(locker.is_locked("sess:1"));

        drop(guard);
        assert!(!locker.is_locked("sess:1"));
    }

    #[tokio::test]
    async fn try_acquire_returns_none_when_held() {
        let dir = temp_lock_dir();
        let locker = SessionWriteLock::new(dir.path());

        let _guard = locker.acquire("sess:1").await.unwrap();
        let result = locker.try_acquire("sess:1").unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn is_locked_true_when_held() {
        let dir = temp_lock_dir();
        let locker = SessionWriteLock::new(dir.path());

        assert!(!locker.is_locked("sess:1"));
        let _guard = locker.acquire("sess:1").await.unwrap();
        assert!(locker.is_locked("sess:1"));
    }

    #[tokio::test]
    async fn lock_file_cleanup_on_drop() {
        let dir = temp_lock_dir();
        let locker = SessionWriteLock::new(dir.path());

        let guard = locker.acquire("sess:1").await.unwrap();
        let path = guard.lock_path().to_path_buf();
        assert!(path.exists());

        drop(guard);
        assert!(!path.exists());
    }

    #[tokio::test]
    async fn double_acquire_times_out() {
        let dir = temp_lock_dir();
        let locker = SessionWriteLock::new(dir.path())
            .with_timeout(Duration::from_millis(250));

        let _guard = locker.acquire("sess:1").await.unwrap();

        let result = locker.acquire("sess:1").await;
        assert!(matches!(result, Err(LockError::Timeout { .. })));
    }

    #[tokio::test]
    async fn stale_lock_detection_and_breaking() {
        let dir = temp_lock_dir();
        let locker = SessionWriteLock::new(dir.path())
            .with_stale_threshold(Duration::from_millis(50));

        // Manually create a lock file and backdate its mtime.
        let lock_path = dir.path().join("sess_1.lock");
        fs::write(&lock_path, r#"{"holder_pid":99999,"acquired_at":"2020-01-01T00:00:00Z"}"#)
            .unwrap();

        // Backdate modification time by setting a very old timestamp.
        let old_time = filetime::FileTime::from_unix_time(0, 0);
        filetime::set_file_mtime(&lock_path, old_time).unwrap();

        // Should break stale lock and acquire.
        let guard = locker.acquire("sess:1").await.unwrap();
        assert!(locker.is_locked("sess:1"));
        drop(guard);
    }

    #[tokio::test]
    async fn concurrent_acquire_from_two_tasks() {
        let dir = temp_lock_dir();
        let locker = SessionWriteLock::new(dir.path())
            .with_timeout(Duration::from_secs(2));

        let guard = locker.acquire("sess:1").await.unwrap();

        let locker2 = locker.clone();
        let handle = tokio::spawn(async move {
            // This will block until the lock is released.
            locker2.acquire("sess:1").await
        });

        // Release after a short delay so the spawned task can acquire.
        tokio::time::sleep(Duration::from_millis(200)).await;
        drop(guard);

        let result = handle.await.unwrap();
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn different_session_keys_dont_conflict() {
        let dir = temp_lock_dir();
        let locker = SessionWriteLock::new(dir.path());

        let _guard_a = locker.acquire("sess:a").await.unwrap();
        let _guard_b = locker.acquire("sess:b").await.unwrap();

        assert!(locker.is_locked("sess:a"));
        assert!(locker.is_locked("sess:b"));
    }

    #[test]
    fn sanitize_session_key_replaces_colons_and_slashes() {
        assert_eq!(sanitize_session_key("agent:gpt:telegram:group:123"), "agent_gpt_telegram_group_123");
        assert_eq!(sanitize_session_key("a/b:c"), "a_b_c");
    }

    #[tokio::test]
    async fn with_timeout_builder() {
        let dir = temp_lock_dir();
        let locker = SessionWriteLock::new(dir.path())
            .with_timeout(Duration::from_millis(100))
            .with_stale_threshold(Duration::from_secs(120));

        let _guard = locker.acquire("sess:1").await.unwrap();

        // Second acquire should time out quickly.
        let start = tokio::time::Instant::now();
        let result = locker.acquire("sess:1").await;
        let elapsed = start.elapsed();

        assert!(matches!(result, Err(LockError::Timeout { .. })));
        assert!(elapsed < Duration::from_millis(500), "should timeout quickly");
    }

    #[tokio::test]
    async fn lock_file_contains_valid_json() {
        let dir = temp_lock_dir();
        let locker = SessionWriteLock::new(dir.path());

        let guard = locker.acquire("sess:1").await.unwrap();
        let content = fs::read_to_string(guard.lock_path()).unwrap();
        let meta: serde_json::Value = serde_json::from_str(&content).unwrap();

        assert!(meta["holder_pid"].is_number());
        assert!(meta["acquired_at"].is_string());

        drop(guard);
    }

    #[tokio::test]
    async fn creates_lock_dir_if_missing() {
        let dir = temp_lock_dir();
        let nested = dir.path().join("sub").join("locks");
        let locker = SessionWriteLock::new(&nested);

        let guard = locker.acquire("sess:1").await.unwrap();
        assert!(nested.exists());
        drop(guard);
    }
}
