//! Channel lifecycle manager.
//!
//! Manages starting, stopping, and auto-restarting channel accounts.
//! Each account runs in its own tokio task with a dedicated
//! [`CancellationToken`] for graceful shutdown.
//!
//! Ports OpenClaw's `src/gateway/server-channels.ts`.

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use chrono::Utc;
use serde::{Deserialize, Serialize};
use tokio_util::sync::CancellationToken;
use tracing::{error, info, warn};

use claw_core::backoff::{BackoffPolicy, compute_backoff, sleep_with_abort};

use crate::plugin::ChannelPlugin;
use crate::registry::ChannelRegistry;
use crate::types::ChannelGatewayContext;

// ---------------------------------------------------------------------------
// Account status
// ---------------------------------------------------------------------------

/// Status of a channel account.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AccountStatus {
    /// Account is running normally.
    Running,
    /// Account is starting up.
    Starting,
    /// Account has stopped (cleanly or by request).
    Stopped,
    /// Account encountered an error and stopped.
    Errored,
    /// Account was manually stopped by the user.
    ManuallyStopped,
    /// Account was logged out by the platform.
    LoggedOut,
}

impl std::fmt::Display for AccountStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Running => write!(f, "running"),
            Self::Starting => write!(f, "starting"),
            Self::Stopped => write!(f, "stopped"),
            Self::Errored => write!(f, "errored"),
            Self::ManuallyStopped => write!(f, "manually_stopped"),
            Self::LoggedOut => write!(f, "logged_out"),
        }
    }
}

// ---------------------------------------------------------------------------
// Snapshot types
// ---------------------------------------------------------------------------

/// Point-in-time snapshot of a single channel account's state.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChannelAccountSnapshot {
    /// Current status.
    pub status: AccountStatus,
    /// Last error message, if any.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// ISO 8601 timestamp of last activity.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_activity: Option<String>,
    /// Number of restart attempts since last successful start.
    pub restart_attempts: u32,
}

/// Point-in-time snapshot of all channel runtime state.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChannelRuntimeSnapshot {
    /// Map of channel_id → (account_id → account snapshot).
    pub channels: HashMap<String, HashMap<String, ChannelAccountSnapshot>>,
}

// ---------------------------------------------------------------------------
// Internal account state
// ---------------------------------------------------------------------------

/// Internal mutable state tracked per channel account.
#[derive(Debug)]
struct AccountState {
    status: AccountStatus,
    error: Option<String>,
    last_activity: Option<String>,
    restart_attempts: u32,
    cancel: CancellationToken,
}

impl AccountState {
    fn new(cancel: CancellationToken) -> Self {
        Self {
            status: AccountStatus::Stopped,
            error: None,
            last_activity: None,
            restart_attempts: 0,
            cancel,
        }
    }

    fn snapshot(&self) -> ChannelAccountSnapshot {
        ChannelAccountSnapshot {
            status: self.status,
            error: self.error.clone(),
            last_activity: self.last_activity.clone(),
            restart_attempts: self.restart_attempts,
        }
    }

    fn touch(&mut self) {
        self.last_activity = Some(Utc::now().to_rfc3339());
    }
}

// ---------------------------------------------------------------------------
// ChannelManager
// ---------------------------------------------------------------------------

/// Manages the lifecycle of channel accounts.
///
/// Each channel+account pair gets its own tokio task and cancellation
/// token. Failed accounts are automatically restarted with exponential
/// backoff.
#[derive(Clone)]
pub struct ChannelManager {
    /// Shared mutable state: channel_id → account_id → state.
    state: Arc<RwLock<HashMap<String, HashMap<String, AccountState>>>>,
    /// Channel registry for looking up plugins.
    registry: ChannelRegistry,
    /// Parent cancellation token — cancelling this stops all accounts.
    cancel: CancellationToken,
    /// Backoff policy for auto-restarts.
    backoff: BackoffPolicy,
}

impl std::fmt::Debug for ChannelManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ChannelManager")
            .field("registry", &self.registry)
            .field("cancel", &self.cancel)
            .finish_non_exhaustive()
    }
}

impl ChannelManager {
    /// Create a new channel manager.
    pub fn new(registry: ChannelRegistry, cancel: CancellationToken) -> Self {
        Self {
            state: Arc::new(RwLock::new(HashMap::new())),
            registry,
            cancel,
            backoff: BackoffPolicy::default(),
        }
    }

    /// Create a channel manager with a custom backoff policy.
    pub fn with_backoff(mut self, backoff: BackoffPolicy) -> Self {
        self.backoff = backoff;
        self
    }

    /// Get a runtime snapshot of all channel account states.
    pub fn runtime_snapshot(&self) -> ChannelRuntimeSnapshot {
        let state = self.state.read().expect("state lock poisoned");
        let channels = state
            .iter()
            .map(|(ch_id, accounts)| {
                let acct_snaps = accounts
                    .iter()
                    .map(|(acct_id, s)| (acct_id.clone(), s.snapshot()))
                    .collect();
                (ch_id.clone(), acct_snaps)
            })
            .collect();
        ChannelRuntimeSnapshot { channels }
    }

    /// Start all accounts for all registered channels that have a gateway adapter.
    ///
    /// Reads the channel registry and config to discover which accounts exist,
    /// then starts each one in a separate tokio task.
    pub fn start_channels(
        &self,
        channel_configs: &HashMap<String, claw_config::ChannelConfig>,
        runtime: &claw_core::RuntimeEnv,
    ) {
        for (channel_id, channel_config) in channel_configs {
            if channel_config.enabled != Some(true) {
                info!(channel = %channel_id, "channel disabled, skipping");
                continue;
            }

            let Some(accounts) = &channel_config.accounts else {
                warn!(channel = %channel_id, "channel enabled but has no accounts");
                continue;
            };

            let dock = self.registry.get(channel_id);
            let plugin = dock.as_ref().and_then(|d| d.plugin().cloned());

            let Some(plugin) = plugin else {
                warn!(channel = %channel_id, "no plugin registered, skipping");
                continue;
            };

            for (account_id, _account_config) in accounts {
                self.start_account(
                    channel_id,
                    account_id,
                    plugin.clone(),
                    channel_config.clone(),
                    runtime.clone(),
                );
            }
        }
    }

    /// Start a single channel account.
    ///
    /// Creates a child cancellation token and spawns a tokio task that
    /// runs the account's gateway adapter. If the task exits with an error,
    /// it will be automatically restarted with backoff.
    pub fn start_account(
        &self,
        channel_id: &str,
        account_id: &str,
        plugin: Arc<dyn ChannelPlugin>,
        channel_config: claw_config::ChannelConfig,
        runtime: claw_core::RuntimeEnv,
    ) {
        let account_cancel = self.cancel.child_token();

        // Initialize state
        {
            let mut state = self.state.write().expect("state lock poisoned");
            let accounts = state.entry(channel_id.to_string()).or_default();
            let acct_state = accounts
                .entry(account_id.to_string())
                .or_insert_with(|| AccountState::new(account_cancel.clone()));
            acct_state.status = AccountStatus::Starting;
            acct_state.cancel = account_cancel.clone();
            acct_state.touch();
        }

        let manager = self.clone();
        let channel_id = channel_id.to_string();
        let account_id = account_id.to_string();
        let parent_cancel = self.cancel.clone();

        info!(channel = %channel_id, account = %account_id, "account start requested");

        tokio::spawn(async move {
            manager.run_account_loop(
                &channel_id,
                &account_id,
                plugin,
                channel_config,
                runtime,
                account_cancel,
                parent_cancel,
            )
            .await;
        });
    }

    /// Stop a channel account by cancelling its token.
    pub fn stop_account(&self, channel_id: &str, account_id: &str) {
        let mut state = self.state.write().expect("state lock poisoned");
        if let Some(accounts) = state.get_mut(channel_id) {
            if let Some(acct) = accounts.get_mut(account_id) {
                acct.cancel.cancel();
                acct.status = AccountStatus::Stopped;
                acct.touch();
                info!(channel = %channel_id, account = %account_id, "account stopped");
            }
        }
    }

    /// Stop a channel account and mark it as manually stopped (no auto-restart).
    pub fn stop_account_manual(&self, channel_id: &str, account_id: &str) {
        let mut state = self.state.write().expect("state lock poisoned");
        if let Some(accounts) = state.get_mut(channel_id) {
            if let Some(acct) = accounts.get_mut(account_id) {
                acct.cancel.cancel();
                acct.status = AccountStatus::ManuallyStopped;
                acct.touch();
                info!(channel = %channel_id, account = %account_id, "account manually stopped");
            }
        }
    }

    /// Mark a channel account as logged out.
    pub fn mark_channel_logged_out(&self, channel_id: &str, account_id: &str) {
        let mut state = self.state.write().expect("state lock poisoned");
        if let Some(accounts) = state.get_mut(channel_id) {
            if let Some(acct) = accounts.get_mut(account_id) {
                acct.cancel.cancel();
                acct.status = AccountStatus::LoggedOut;
                acct.touch();
                warn!(channel = %channel_id, account = %account_id, "account logged out");
            }
        }
    }

    /// Check if an account was manually stopped.
    pub fn is_manually_stopped(&self, channel_id: &str, account_id: &str) -> bool {
        let state = self.state.read().expect("state lock poisoned");
        state
            .get(channel_id)
            .and_then(|a| a.get(account_id))
            .is_some_and(|s| s.status == AccountStatus::ManuallyStopped)
    }

    /// Reset restart attempts for an account (e.g. after a successful reconnect).
    pub fn reset_restart_attempts(&self, channel_id: &str, account_id: &str) {
        let mut state = self.state.write().expect("state lock poisoned");
        if let Some(accounts) = state.get_mut(channel_id) {
            if let Some(acct) = accounts.get_mut(account_id) {
                acct.restart_attempts = 0;
            }
        }
    }

    /// Stop all channel accounts by cancelling the parent token.
    pub fn stop_all(&self) {
        self.cancel.cancel();
        info!("all channels stopping");
    }

    // -- Internal loop --------------------------------------------------------

    /// Run the account's gateway adapter in a loop with auto-restart.
    async fn run_account_loop(
        &self,
        channel_id: &str,
        account_id: &str,
        plugin: Arc<dyn ChannelPlugin>,
        channel_config: claw_config::ChannelConfig,
        runtime: claw_core::RuntimeEnv,
        account_cancel: CancellationToken,
        parent_cancel: CancellationToken,
    ) {
        loop {
            // Check if we should stop
            if parent_cancel.is_cancelled() || account_cancel.is_cancelled() {
                break;
            }

            // Check if manually stopped
            if self.is_manually_stopped(channel_id, account_id) {
                break;
            }

            let Some(gateway) = plugin.gateway_adapter() else {
                warn!(channel = %channel_id, account = %account_id, "no gateway adapter");
                self.set_status(channel_id, account_id, AccountStatus::Errored, Some("no gateway adapter".into()));
                break;
            };

            // Build the gateway context
            let acct_config = channel_config
                .accounts
                .as_ref()
                .and_then(|a| a.get(account_id))
                .map(|c| serde_json::to_value(c).unwrap_or_default())
                .unwrap_or_default();

            let ctx = ChannelGatewayContext {
                account_id: account_id.to_string(),
                account_config: acct_config,
                channel_config: channel_config.clone(),
                runtime: runtime.clone(),
                cancel: account_cancel.clone(),
            };

            // Mark as running
            self.set_status(channel_id, account_id, AccountStatus::Running, None);
            self.reset_restart_attempts(channel_id, account_id);

            info!(channel = %channel_id, account = %account_id, "account starting");

            // Run the gateway adapter
            let result = gateway.start_account(ctx).await;

            match result {
                Ok(()) => {
                    info!(channel = %channel_id, account = %account_id, "account exited cleanly");
                    self.set_status(channel_id, account_id, AccountStatus::Stopped, None);
                    break;
                }
                Err(e) => {
                    let err_msg = e.to_string();
                    error!(channel = %channel_id, account = %account_id, error = %err_msg, "account crashed");
                    self.set_status(channel_id, account_id, AccountStatus::Errored, Some(err_msg));
                }
            }

            // Check stop conditions before restart
            if parent_cancel.is_cancelled() || account_cancel.is_cancelled() {
                break;
            }
            if self.is_manually_stopped(channel_id, account_id) {
                break;
            }

            // Compute backoff and wait
            let attempts = self.increment_restart_attempts(channel_id, account_id);
            let delay = compute_backoff(&self.backoff, attempts.saturating_sub(1));

            warn!(
                channel = %channel_id,
                account = %account_id,
                attempt = attempts,
                delay_ms = delay.as_millis() as u64,
                "restarting account after backoff"
            );

            // Sleep with abort — cancellation breaks out of the wait
            if sleep_with_abort(delay, account_cancel.clone()).await.is_err() {
                break;
            }
        }
    }

    fn set_status(
        &self,
        channel_id: &str,
        account_id: &str,
        status: AccountStatus,
        error: Option<String>,
    ) {
        let mut state = self.state.write().expect("state lock poisoned");
        if let Some(accounts) = state.get_mut(channel_id) {
            if let Some(acct) = accounts.get_mut(account_id) {
                acct.status = status;
                acct.error = error;
                acct.touch();
            }
        }
    }

    fn increment_restart_attempts(&self, channel_id: &str, account_id: &str) -> u32 {
        let mut state = self.state.write().expect("state lock poisoned");
        if let Some(accounts) = state.get_mut(channel_id) {
            if let Some(acct) = accounts.get_mut(account_id) {
                acct.restart_attempts += 1;
                return acct.restart_attempts;
            }
        }
        0
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -- Snapshot types -------------------------------------------------------

    #[test]
    fn account_status_display() {
        assert_eq!(AccountStatus::Running.to_string(), "running");
        assert_eq!(AccountStatus::ManuallyStopped.to_string(), "manually_stopped");
        assert_eq!(AccountStatus::LoggedOut.to_string(), "logged_out");
    }

    #[test]
    fn account_status_serde_roundtrip() {
        let status = AccountStatus::Running;
        let json = serde_json::to_string(&status).unwrap();
        assert_eq!(json, r#""running""#);
        let parsed: AccountStatus = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, status);
    }

    #[test]
    fn channel_runtime_snapshot_serde() {
        let mut snapshot = ChannelRuntimeSnapshot::default();
        let mut accounts = HashMap::new();
        accounts.insert(
            "main".to_string(),
            ChannelAccountSnapshot {
                status: AccountStatus::Running,
                error: None,
                last_activity: Some("2026-02-17T00:00:00Z".to_string()),
                restart_attempts: 0,
            },
        );
        snapshot.channels.insert("telegram".to_string(), accounts);

        let json = serde_json::to_value(&snapshot).unwrap();
        assert_eq!(json["channels"]["telegram"]["main"]["status"], "running");
        assert_eq!(json["channels"]["telegram"]["main"]["restartAttempts"], 0);
    }

    // -- ChannelManager state -------------------------------------------------

    fn make_manager() -> (ChannelManager, CancellationToken) {
        let cancel = CancellationToken::new();
        let registry = ChannelRegistry::new();
        let mgr = ChannelManager::new(registry, cancel.clone());
        (mgr, cancel)
    }

    #[test]
    fn empty_runtime_snapshot() {
        let (mgr, _) = make_manager();
        let snap = mgr.runtime_snapshot();
        assert!(snap.channels.is_empty());
    }

    #[test]
    fn stop_account_sets_stopped() {
        let (mgr, _) = make_manager();

        // Manually insert state
        {
            let mut state = mgr.state.write().unwrap();
            let mut accounts = HashMap::new();
            accounts.insert(
                "main".to_string(),
                AccountState::new(CancellationToken::new()),
            );
            state.insert("telegram".to_string(), accounts);
        }

        mgr.stop_account("telegram", "main");

        let snap = mgr.runtime_snapshot();
        let acct = &snap.channels["telegram"]["main"];
        assert_eq!(acct.status, AccountStatus::Stopped);
    }

    #[test]
    fn manual_stop_prevents_restart() {
        let (mgr, _) = make_manager();

        {
            let mut state = mgr.state.write().unwrap();
            let mut accounts = HashMap::new();
            accounts.insert(
                "bot1".to_string(),
                AccountState::new(CancellationToken::new()),
            );
            state.insert("discord".to_string(), accounts);
        }

        assert!(!mgr.is_manually_stopped("discord", "bot1"));

        mgr.stop_account_manual("discord", "bot1");

        assert!(mgr.is_manually_stopped("discord", "bot1"));
        let snap = mgr.runtime_snapshot();
        assert_eq!(snap.channels["discord"]["bot1"].status, AccountStatus::ManuallyStopped);
    }

    #[test]
    fn mark_logged_out() {
        let (mgr, _) = make_manager();

        {
            let mut state = mgr.state.write().unwrap();
            let mut accounts = HashMap::new();
            accounts.insert(
                "main".to_string(),
                AccountState::new(CancellationToken::new()),
            );
            state.insert("whatsapp".to_string(), accounts);
        }

        mgr.mark_channel_logged_out("whatsapp", "main");

        let snap = mgr.runtime_snapshot();
        assert_eq!(snap.channels["whatsapp"]["main"].status, AccountStatus::LoggedOut);
    }

    #[test]
    fn reset_restart_attempts() {
        let (mgr, _) = make_manager();

        {
            let mut state = mgr.state.write().unwrap();
            let mut accounts = HashMap::new();
            let mut acct = AccountState::new(CancellationToken::new());
            acct.restart_attempts = 5;
            accounts.insert("main".to_string(), acct);
            state.insert("telegram".to_string(), accounts);
        }

        assert_eq!(
            mgr.runtime_snapshot().channels["telegram"]["main"].restart_attempts,
            5
        );

        mgr.reset_restart_attempts("telegram", "main");

        assert_eq!(
            mgr.runtime_snapshot().channels["telegram"]["main"].restart_attempts,
            0
        );
    }

    #[test]
    fn increment_restart_attempts() {
        let (mgr, _) = make_manager();

        {
            let mut state = mgr.state.write().unwrap();
            let mut accounts = HashMap::new();
            accounts.insert(
                "main".to_string(),
                AccountState::new(CancellationToken::new()),
            );
            state.insert("telegram".to_string(), accounts);
        }

        assert_eq!(mgr.increment_restart_attempts("telegram", "main"), 1);
        assert_eq!(mgr.increment_restart_attempts("telegram", "main"), 2);
        assert_eq!(mgr.increment_restart_attempts("telegram", "main"), 3);
    }

    #[test]
    fn stop_all_cancels_parent() {
        let (mgr, cancel) = make_manager();
        assert!(!cancel.is_cancelled());

        mgr.stop_all();
        assert!(cancel.is_cancelled());
    }

    #[test]
    fn clone_shares_state() {
        let (mgr1, _) = make_manager();
        let mgr2 = mgr1.clone();

        {
            let mut state = mgr1.state.write().unwrap();
            let mut accounts = HashMap::new();
            accounts.insert(
                "bot".to_string(),
                AccountState::new(CancellationToken::new()),
            );
            state.insert("slack".to_string(), accounts);
        }

        let snap = mgr2.runtime_snapshot();
        assert!(snap.channels.contains_key("slack"));
    }

    // -- start_channels with disabled channel ---------------------------------

    #[test]
    fn start_channels_skips_disabled() {
        let (mgr, _) = make_manager();

        let mut configs = HashMap::new();
        configs.insert(
            "telegram".to_string(),
            claw_config::ChannelConfig {
                enabled: Some(false),
                ..Default::default()
            },
        );

        let runtime = claw_core::RuntimeEnv::new("test");
        mgr.start_channels(&configs, &runtime);

        let snap = mgr.runtime_snapshot();
        assert!(snap.channels.is_empty());
    }

    #[test]
    fn start_channels_skips_no_accounts() {
        let (mgr, _) = make_manager();

        let mut configs = HashMap::new();
        configs.insert(
            "telegram".to_string(),
            claw_config::ChannelConfig {
                enabled: Some(true),
                accounts: None,
                ..Default::default()
            },
        );

        let runtime = claw_core::RuntimeEnv::new("test");
        mgr.start_channels(&configs, &runtime);

        let snap = mgr.runtime_snapshot();
        assert!(snap.channels.is_empty());
    }

    // -- AccountSnapshot serde ------------------------------------------------

    #[test]
    fn account_snapshot_omits_none_error() {
        let snap = ChannelAccountSnapshot {
            status: AccountStatus::Running,
            error: None,
            last_activity: None,
            restart_attempts: 0,
        };
        let json = serde_json::to_value(&snap).unwrap();
        let obj = json.as_object().unwrap();
        assert!(!obj.contains_key("error"));
        assert!(!obj.contains_key("lastActivity"));
        assert!(obj.contains_key("status"));
        assert!(obj.contains_key("restartAttempts"));
    }
}
