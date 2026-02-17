//! WhatsApp adapter trait implementations.
//!
//! Thin stubs for SetupAdapter, HeartbeatAdapter, and PairingAdapter.
//! The actual WhatsApp SDK calls are TODO — these stubs establish the
//! wiring so the plugin advertises its capabilities.

use async_trait::async_trait;
use tracing::info;

use claw_channels::plugin::{
    ChannelHeartbeatAdapter, ChannelPairingAdapter, ChannelSetupAdapter,
};
use claw_channels::types::ChannelError;

// ---------------------------------------------------------------------------
// SetupAdapter
// ---------------------------------------------------------------------------

/// Manages WhatsApp session setup and teardown (store initialization,
/// webhook registration, etc.).
pub struct WhatsAppSetupAdapter;

impl WhatsAppSetupAdapter {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl ChannelSetupAdapter for WhatsAppSetupAdapter {
    async fn setup(&self, account_id: &str) -> Result<(), ChannelError> {
        info!(account_id, "running whatsapp session setup");
        // TODO: initialize session store (SQLite), register webhooks if needed,
        // and verify the account credentials are valid.
        Ok(())
    }

    async fn teardown(&self, account_id: &str) -> Result<(), ChannelError> {
        info!(account_id, "tearing down whatsapp session");
        // TODO: close session store, clean up temp files.
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// HeartbeatAdapter
// ---------------------------------------------------------------------------

/// Monitors WhatsApp connection health via periodic heartbeats.
pub struct WhatsAppHeartbeatAdapter;

impl WhatsAppHeartbeatAdapter {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl ChannelHeartbeatAdapter for WhatsAppHeartbeatAdapter {
    async fn send_heartbeat(&self, account_id: &str) -> Result<(), ChannelError> {
        info!(account_id, "sending whatsapp heartbeat");
        // TODO: ping the WhatsApp WebSocket connection to verify it's alive.
        Ok(())
    }

    async fn is_alive(&self, account_id: &str) -> Result<bool, ChannelError> {
        info!(account_id, "checking whatsapp connection health");
        // TODO: check WebSocket state and last-seen timestamp.
        Ok(true)
    }
}

// ---------------------------------------------------------------------------
// PairingAdapter
// ---------------------------------------------------------------------------

/// Handles WhatsApp QR code and pairing code authentication flow.
pub struct WhatsAppPairingAdapter;

impl WhatsAppPairingAdapter {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl ChannelPairingAdapter for WhatsAppPairingAdapter {
    async fn initiate_pairing(
        &self,
        account_id: &str,
        user_id: &str,
    ) -> Result<String, ChannelError> {
        info!(account_id, user_id, "initiating whatsapp pairing");
        // TODO: generate QR code data or pairing code via WhatsApp SDK.
        // Return the QR data string or pairing URL.
        Ok("whatsapp_pairing_placeholder".into())
    }

    async fn confirm_pairing(
        &self,
        account_id: &str,
        user_id: &str,
        code: &str,
    ) -> Result<bool, ChannelError> {
        info!(account_id, user_id, "confirming whatsapp pairing");
        // TODO: verify the pairing code matches and the session is established.
        let _ = code;
        Ok(false)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn setup_adapter_runs() {
        let adapter = WhatsAppSetupAdapter::new();
        assert!(adapter.setup("test-account").await.is_ok());
        assert!(adapter.teardown("test-account").await.is_ok());
    }

    #[tokio::test]
    async fn heartbeat_adapter_alive() {
        let adapter = WhatsAppHeartbeatAdapter::new();
        assert!(adapter.send_heartbeat("test-account").await.is_ok());
        assert!(adapter.is_alive("test-account").await.unwrap());
    }

    #[tokio::test]
    async fn pairing_adapter_initiate() {
        let adapter = WhatsAppPairingAdapter::new();
        let result = adapter.initiate_pairing("test-account", "user-1").await;
        assert!(result.is_ok());
        assert!(!result.unwrap().is_empty());
    }

    #[tokio::test]
    async fn pairing_adapter_confirm_default_false() {
        let adapter = WhatsAppPairingAdapter::new();
        let confirmed = adapter
            .confirm_pairing("test-account", "user-1", "123456")
            .await
            .unwrap();
        assert!(!confirmed);
    }
}
