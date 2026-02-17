//! Channel lifecycle manager.
//!
//! Manages the start/stop lifecycle of channel accounts, including
//! auto-restart with exponential backoff and per-account cancellation.
//! Ports OpenClaw's `ChannelManager` from TypeScript.

use std::collections::HashMap;
use std::sync::Arc;

use chrono::Utc;
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use tracing::{error, info, warn};

use claw_channels::registry::ChannelRegistry;
use claw_channels::types::{ChannelError, ChannelGatewayContext};
use claw_config::ChannelConfig;
use claw_core::backoff::{BackoffPolicy, compute_backoff, sleep_with_abort};
use claw_core::RuntimeEnv;

// ---------------------------------------------------------------------------
// AccountStatus
// ---------------------------------------------------------------------------

/// Lifecycle status of a managed channel account.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AccountStatus {
    Starting,
    Running,
    Stopping,
    Stopped,
    Error,
    #[serde(rename = "logged_out")]
    LoggedOut,
}

// ---------------------------------------------------------------------------
// ChannelAccountSnapshot
// ---------------------------------------------------------------------------

/// Point-in-time snapshot of a single channel account's state.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChannelAccountSnapshot {
    pub channel_id: String,
    pub account_id: String,
    pub status: AccountStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_activity: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub started_at: Option<String>,
    pub restart_count: u32,
}

// ---------------------------------------------------------------------------
// ChannelRuntimeSnapshot
// ---------------------------------------------------------------------------

/// Point-in-time snapshot of all managed channel accounts.
///
/// Keyed by channel_id → account_id → snapshot.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ChannelRuntimeSnapshot {
    pub channels: HashMap<String, HashMap<String, ChannelAccountSnapshot>>,
}

// ---------------------------------------------------------------------------
// ManagedAccount (internal)
// ---------------------------------------------------------------------------

/// Internal state for a managed channel account.
struct ManagedAccount {
    channel_id: String,
    account_id: String,
    status: AccountStatus,
    cancel: CancellationToken,
    task_handle: Option<JoinHandle<()>>,
    restart_count: u32,
    manually_stopped: bool,
    last_error: Option<String>,
    started_at: Option<chrono::DateTime<Utc>>,
    last_activity: Option<chrono::DateTime<Utc>>,
}

impl ManagedAccount {
    fn to_snapshot(&self) -> ChannelAccountSnapshot {
        ChannelAccountSnapshot {
            channel_id: self.channel_id.clone(),
            account_id: self.account_id.clone(),
            status: self.status,
            error: self.last_error.clone(),
            started_at: self.started_at.map(|t| t.to_rfc3339()),
            last_activity: self.last_activity.map(|t| t.to_rfc3339()),
            restart_count: self.restart_count,
        }
    }
}

// ---------------------------------------------------------------------------
// ChannelManager
// ---------------------------------------------------------------------------

/// Manages the lifecycle of channel accounts.
///
/// Each account runs in its own tokio task. If a task fails, the manager
/// automatically restarts it with exponential backoff (unless the account
/// was manually stopped or logged out).
pub struct ChannelManager {
    registry: ChannelRegistry,
    accounts: Arc<RwLock<HashMap<(String, String), ManagedAccount>>>,
    cancel: CancellationToken,
    backoff_policy: BackoffPolicy,
    runtime: RuntimeEnv,
}

impl ChannelManager {
    /// Create a new channel manager.
    pub fn new(
        registry: ChannelRegistry,
        backoff: BackoffPolicy,
        runtime: RuntimeEnv,
        cancel: CancellationToken,
    ) -> Self {
        Self {
            registry,
            accounts: Arc::new(RwLock::new(HashMap::new())),
            cancel,
            backoff_policy: backoff,
            runtime,
        }
    }

    /// Return a snapshot of all managed accounts.
    pub async fn get_runtime_snapshot(&self) -> ChannelRuntimeSnapshot {
        let accounts = self.accounts.read().await;
        let mut snapshot = ChannelRuntimeSnapshot::default();
        for account in accounts.values() {
            snapshot
                .channels
                .entry(account.channel_id.clone())
                .or_default()
                .insert(account.account_id.clone(), account.to_snapshot());
        }
        snapshot
    }

    /// Start all configured channel accounts.
    ///
    /// Iterates through the registry, discovers accounts via
    /// `ChannelConfigAdapter::list_account_ids`, and starts each one.
    pub async fn start_channels(&self) -> Result<(), ChannelError> {
        let docks = self.registry.list();
        for dock in &docks {
            let plugin = match dock.plugin() {
                Some(p) => p,
                None => continue,
            };
            let config_adapter = match plugin.config_adapter() {
                Some(a) => a,
                None => continue,
            };
            if plugin.gateway_adapter().is_none() {
                continue;
            }

            let account_ids = config_adapter.list_account_ids().await?;
            for account_id in account_ids {
                if let Err(e) = self.start_channel(dock.id(), &account_id).await {
                    error!(
                        channel = dock.id(),
                        account = %account_id,
                        "failed to start channel account: {e}"
                    );
                }
            }
        }
        Ok(())
    }

    /// Start a single channel account.
    pub async fn start_channel(
        &self,
        channel_id: &str,
        account_id: &str,
    ) -> Result<(), ChannelError> {
        let key = (channel_id.to_owned(), account_id.to_owned());

        // Check if already running
        {
            let accounts = self.accounts.read().await;
            if let Some(acct) = accounts.get(&key) {
                if acct.status == AccountStatus::Running
                    || acct.status == AccountStatus::Starting
                {
                    info!(channel = channel_id, account = account_id, "account already running");
                    return Ok(());
                }
            }
        }

        let dock = self.registry.get(channel_id).ok_or_else(|| {
            ChannelError::NotFound(channel_id.to_owned())
        })?;

        let plugin = dock.plugin().ok_or_else(|| {
            ChannelError::GatewayError(format!("no plugin loaded for channel {channel_id}"))
        })?;

        if plugin.gateway_adapter().is_none() {
            return Err(ChannelError::AdapterNotSupported {
                channel: channel_id.to_owned(),
                adapter: "gateway".into(),
            });
        }

        let account_cancel = self.cancel.child_token();
        let now = Utc::now();

        let managed = ManagedAccount {
            channel_id: channel_id.to_owned(),
            account_id: account_id.to_owned(),
            status: AccountStatus::Starting,
            cancel: account_cancel.clone(),
            task_handle: None,
            restart_count: 0,
            manually_stopped: false,
            last_error: None,
            started_at: Some(now),
            last_activity: Some(now),
        };

        // Resolve account config
        let account_config = if let Some(config_adapter) = plugin.config_adapter() {
            config_adapter
                .resolve_account(account_id)
                .await
                .unwrap_or_default()
        } else {
            serde_json::Value::Null
        };

        let ctx = ChannelGatewayContext {
            account_id: account_id.to_owned(),
            channel_id: channel_id.to_owned(),
            account_config,
            channel_config: ChannelConfig::default(),
            runtime: self.runtime.clone(),
            cancel: account_cancel.clone(),
            dispatch_tx: None,
        };

        // Spawn the account task with auto-restart logic
        let accounts = Arc::clone(&self.accounts);
        let registry = self.registry.clone();
        let backoff_policy = self.backoff_policy.clone();
        let manager_cancel = self.cancel.clone();
        let runtime = self.runtime.clone();
        let ch_id = channel_id.to_owned();
        let acct_id = account_id.to_owned();

        let handle = tokio::spawn(async move {
            run_account_loop(
                accounts,
                registry,
                backoff_policy,
                manager_cancel,
                runtime,
                ch_id,
                acct_id,
                ctx,
            )
            .await;
        });

        // Store the managed account with its task handle
        {
            let mut accounts = self.accounts.write().await;
            let entry = accounts.entry(key).or_insert(managed);
            entry.task_handle = Some(handle);
            entry.status = AccountStatus::Starting;
            entry.manually_stopped = false;
        }

        Ok(())
    }

    /// Stop a single channel account.
    pub async fn stop_channel(
        &self,
        channel_id: &str,
        account_id: &str,
    ) -> Result<(), ChannelError> {
        let key = (channel_id.to_owned(), account_id.to_owned());
        let mut accounts = self.accounts.write().await;

        if let Some(acct) = accounts.get_mut(&key) {
            info!(channel = channel_id, account = account_id, "stopping channel account");
            acct.status = AccountStatus::Stopping;
            acct.manually_stopped = true;
            acct.cancel.cancel();

            // Take the handle so we can await it outside the lock
            let handle = acct.task_handle.take();
            drop(accounts);

            if let Some(handle) = handle {
                let _ = handle.await;
            }

            // Update final status
            let mut accounts = self.accounts.write().await;
            if let Some(acct) = accounts.get_mut(&key) {
                acct.status = AccountStatus::Stopped;
                acct.last_activity = Some(Utc::now());
            }

            Ok(())
        } else {
            Err(ChannelError::AccountNotFound {
                channel: channel_id.to_owned(),
                account_id: account_id.to_owned(),
            })
        }
    }

    /// Mark an account as logged out (prevents auto-restart).
    pub async fn mark_channel_logged_out(&self, channel_id: &str, account_id: &str) {
        let key = (channel_id.to_owned(), account_id.to_owned());
        let mut accounts = self.accounts.write().await;
        if let Some(acct) = accounts.get_mut(&key) {
            acct.status = AccountStatus::LoggedOut;
            acct.manually_stopped = true;
            acct.cancel.cancel();
            acct.last_activity = Some(Utc::now());
            info!(channel = channel_id, account = account_id, "marked as logged out");
        }
    }

    /// Check if an account was manually stopped.
    pub async fn is_manually_stopped(&self, channel_id: &str, account_id: &str) -> bool {
        let key = (channel_id.to_owned(), account_id.to_owned());
        let accounts = self.accounts.read().await;
        accounts
            .get(&key)
            .map(|a| a.manually_stopped)
            .unwrap_or(false)
    }

    /// Reset the restart attempt counter for an account.
    pub async fn reset_restart_attempts(&self, channel_id: &str, account_id: &str) {
        let key = (channel_id.to_owned(), account_id.to_owned());
        let mut accounts = self.accounts.write().await;
        if let Some(acct) = accounts.get_mut(&key) {
            acct.restart_count = 0;
        }
    }

    /// Gracefully shut down all managed accounts.
    pub async fn shutdown(&self) {
        info!("shutting down all channel accounts");
        self.cancel.cancel();

        let handles: Vec<JoinHandle<()>> = {
            let mut accounts = self.accounts.write().await;
            accounts
                .values_mut()
                .filter_map(|acct| {
                    acct.status = AccountStatus::Stopping;
                    acct.task_handle.take()
                })
                .collect()
        };

        for handle in handles {
            let _ = handle.await;
        }

        // Mark all as stopped
        let mut accounts = self.accounts.write().await;
        for acct in accounts.values_mut() {
            acct.status = AccountStatus::Stopped;
        }

        info!("all channel accounts stopped");
    }
}

// ---------------------------------------------------------------------------
// Account run loop (auto-restart)
// ---------------------------------------------------------------------------

/// Runs a channel account in a loop with auto-restart on failure.
async fn run_account_loop(
    accounts: Arc<RwLock<HashMap<(String, String), ManagedAccount>>>,
    registry: ChannelRegistry,
    backoff_policy: BackoffPolicy,
    manager_cancel: CancellationToken,
    _runtime: RuntimeEnv,
    channel_id: String,
    account_id: String,
    initial_ctx: ChannelGatewayContext,
) {
    let key = (channel_id.clone(), account_id.clone());
    let mut ctx = initial_ctx;

    loop {
        // Update status to Running
        {
            let mut accts = accounts.write().await;
            if let Some(acct) = accts.get_mut(&key) {
                acct.status = AccountStatus::Running;
                acct.last_activity = Some(Utc::now());
            }
        }

        info!(channel = %channel_id, account = %account_id, "starting account");

        // Run the gateway adapter
        let result = {
            let dock = registry.get(&channel_id);
            if let Some(dock) = dock {
                if let Some(plugin) = dock.plugin() {
                    if let Some(gateway) = plugin.gateway_adapter() {
                        gateway.start_account(ctx.clone()).await
                    } else {
                        Err(ChannelError::AdapterNotSupported {
                            channel: channel_id.clone(),
                            adapter: "gateway".into(),
                        })
                    }
                } else {
                    Err(ChannelError::GatewayError(format!(
                        "plugin not loaded for {channel_id}"
                    )))
                }
            } else {
                Err(ChannelError::NotFound(channel_id.clone()))
            }
        };

        // Check if we should restart
        let should_restart = {
            let mut accts = accounts.write().await;
            let Some(acct) = accts.get_mut(&key) else {
                break;
            };

            match result {
                Ok(()) => {
                    // Clean exit — don't restart
                    info!(channel = %channel_id, account = %account_id, "account stopped cleanly");
                    acct.status = AccountStatus::Stopped;
                    acct.last_activity = Some(Utc::now());
                    false
                }
                Err(ref e) => {
                    error!(
                        channel = %channel_id,
                        account = %account_id,
                        error = %e,
                        "account failed"
                    );
                    acct.last_error = Some(e.to_string());
                    acct.status = AccountStatus::Error;
                    acct.last_activity = Some(Utc::now());

                    if acct.manually_stopped {
                        info!(
                            channel = %channel_id,
                            account = %account_id,
                            "manually stopped — not restarting"
                        );
                        false
                    } else if manager_cancel.is_cancelled() {
                        false
                    } else {
                        acct.restart_count += 1;
                        true
                    }
                }
            }
        };

        if !should_restart {
            break;
        }

        // Compute backoff delay
        let restart_count = {
            let accts = accounts.read().await;
            accts.get(&key).map(|a| a.restart_count).unwrap_or(0)
        };

        let delay = compute_backoff(&backoff_policy, restart_count.saturating_sub(1));
        warn!(
            channel = %channel_id,
            account = %account_id,
            attempt = restart_count,
            delay_ms = delay.as_millis() as u64,
            "restarting account after backoff"
        );

        // Sleep with abort (respects cancellation)
        if sleep_with_abort(delay, manager_cancel.clone()).await.is_err() {
            info!(
                channel = %channel_id,
                account = %account_id,
                "restart cancelled during backoff"
            );
            break;
        }

        // Refresh the cancellation token for the new attempt
        let new_cancel = manager_cancel.child_token();
        ctx.cancel = new_cancel.clone();

        {
            let mut accts = accounts.write().await;
            if let Some(acct) = accts.get_mut(&key) {
                acct.cancel = new_cancel;
                acct.status = AccountStatus::Starting;
                acct.started_at = Some(Utc::now());
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
    use claw_channels::types::{ChannelCapabilities, ChannelMeta};

    fn make_runtime() -> RuntimeEnv {
        RuntimeEnv::new("test")
    }

    fn make_registry() -> ChannelRegistry {
        ChannelRegistry::new()
    }

    fn make_manager() -> ChannelManager {
        ChannelManager::new(
            make_registry(),
            BackoffPolicy::default(),
            make_runtime(),
            CancellationToken::new(),
        )
    }

    #[tokio::test]
    async fn empty_registry_snapshot() {
        let mgr = make_manager();
        let snap = mgr.get_runtime_snapshot().await;
        assert!(snap.channels.is_empty());
    }

    #[tokio::test]
    async fn start_nonexistent_channel_returns_error() {
        let mgr = make_manager();
        let result = mgr.start_channel("nonexistent", "acct1").await;
        assert!(result.is_err());
        match result.unwrap_err() {
            ChannelError::NotFound(id) => assert_eq!(id, "nonexistent"),
            other => panic!("expected NotFound, got: {other}"),
        }
    }

    #[tokio::test]
    async fn stop_nonexistent_channel_returns_error() {
        let mgr = make_manager();
        let result = mgr.stop_channel("nonexistent", "acct1").await;
        assert!(result.is_err());
        match result.unwrap_err() {
            ChannelError::AccountNotFound { channel, account_id } => {
                assert_eq!(channel, "nonexistent");
                assert_eq!(account_id, "acct1");
            }
            other => panic!("expected AccountNotFound, got: {other}"),
        }
    }

    #[tokio::test]
    async fn mark_logged_out_on_unknown_is_noop() {
        let mgr = make_manager();
        // Should not panic
        mgr.mark_channel_logged_out("unknown", "acct").await;
    }

    #[tokio::test]
    async fn is_manually_stopped_default_false() {
        let mgr = make_manager();
        assert!(!mgr.is_manually_stopped("ch", "acct").await);
    }

    #[tokio::test]
    async fn reset_restart_attempts_on_unknown_is_noop() {
        let mgr = make_manager();
        // Should not panic
        mgr.reset_restart_attempts("ch", "acct").await;
    }

    #[tokio::test]
    async fn shutdown_empty_manager() {
        let mgr = make_manager();
        mgr.shutdown().await;
        // Should complete without error
    }

    #[tokio::test]
    async fn start_channel_no_plugin_loaded() {
        let registry = make_registry();
        let meta = ChannelMeta {
            id: "test".into(),
            label: "Test".into(),
            selection_label: None,
            docs_path: None,
            blurb: None,
            order: None,
            aliases: None,
        };
        let dock =
            claw_channels::registry::ChannelDock::metadata_only(meta, ChannelCapabilities::default());
        registry.register(dock);

        let mgr = ChannelManager::new(
            registry,
            BackoffPolicy::default(),
            make_runtime(),
            CancellationToken::new(),
        );

        let result = mgr.start_channel("test", "acct1").await;
        assert!(result.is_err());
    }

    #[test]
    fn account_status_serde() {
        let json = serde_json::to_string(&AccountStatus::Running).unwrap();
        assert_eq!(json, r#""running""#);

        let json = serde_json::to_string(&AccountStatus::LoggedOut).unwrap();
        assert_eq!(json, r#""logged_out""#);

        let parsed: AccountStatus = serde_json::from_str(r#""error""#).unwrap();
        assert_eq!(parsed, AccountStatus::Error);
    }

    #[test]
    fn snapshot_serde_camel_case() {
        let snap = ChannelAccountSnapshot {
            channel_id: "telegram".into(),
            account_id: "main".into(),
            status: AccountStatus::Running,
            error: None,
            last_activity: Some("2026-01-01T00:00:00Z".into()),
            started_at: Some("2026-01-01T00:00:00Z".into()),
            restart_count: 0,
        };
        let json = serde_json::to_value(&snap).unwrap();
        // Verify camelCase field names
        assert!(json.get("channelId").is_some());
        assert!(json.get("accountId").is_some());
        assert!(json.get("lastActivity").is_some());
        assert!(json.get("startedAt").is_some());
        assert!(json.get("restartCount").is_some());
        // Verify None fields are omitted
        assert!(json.get("error").is_none());
    }

    #[test]
    fn runtime_snapshot_default_empty() {
        let snap = ChannelRuntimeSnapshot::default();
        assert!(snap.channels.is_empty());
    }

    #[tokio::test]
    async fn start_channels_with_empty_registry() {
        let mgr = make_manager();
        let result = mgr.start_channels().await;
        assert!(result.is_ok());
    }
}
