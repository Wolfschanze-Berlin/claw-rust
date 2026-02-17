//! WhatsApp ChannelPlugin implementation.

use claw_channels::plugin::{
    ChannelGatewayAdapter, ChannelHeartbeatAdapter, ChannelOutboundAdapter,
    ChannelPairingAdapter, ChannelPlugin, ChannelSetupAdapter,
};
use claw_channels::types::{
    ChannelCapabilities, ChannelMeta, ChatType,
};

use crate::adapters::{WhatsAppHeartbeatAdapter, WhatsAppPairingAdapter, WhatsAppSetupAdapter};
use crate::gateway::WhatsAppGateway;
use crate::outbound::WhatsAppOutbound;

/// WhatsApp channel plugin — session-based multi-device messaging.
pub struct WhatsAppPlugin {
    meta: ChannelMeta,
    capabilities: ChannelCapabilities,
    gateway: WhatsAppGateway,
    outbound: WhatsAppOutbound,
    setup: WhatsAppSetupAdapter,
    heartbeat: WhatsAppHeartbeatAdapter,
    pairing: WhatsAppPairingAdapter,
}

impl WhatsAppPlugin {
    pub fn new() -> Self {
        Self {
            meta: ChannelMeta {
                id: "whatsapp".into(),
                label: "WhatsApp".into(),
                selection_label: Some("WA".into()),
                docs_path: Some("docs/channels/whatsapp.md".into()),
                blurb: Some("WhatsApp multi-device channel".into()),
                order: Some(2),
                aliases: Some(vec!["wa".into()]),
            },
            capabilities: ChannelCapabilities {
                chat_types: Some(vec![ChatType::Direct, ChatType::Group]),
                polls: Some(false),
                reactions: Some(true),
                edit: Some(false),
                unsend: Some(true),
                reply: Some(true),
                effects: Some(false),
                group_management: Some(true),
                threads: Some(false),
                media: Some(true),
                native_commands: Some(false),
                block_streaming: Some(true), // WA doesn't support edit-in-place streaming
            },
            gateway: WhatsAppGateway::new(),
            outbound: WhatsAppOutbound::new(),
            setup: WhatsAppSetupAdapter::new(),
            heartbeat: WhatsAppHeartbeatAdapter::new(),
            pairing: WhatsAppPairingAdapter::new(),
        }
    }
}

impl Default for WhatsAppPlugin {
    fn default() -> Self {
        Self::new()
    }
}

impl ChannelPlugin for WhatsAppPlugin {
    fn meta(&self) -> &ChannelMeta {
        &self.meta
    }

    fn capabilities(&self) -> &ChannelCapabilities {
        &self.capabilities
    }

    fn gateway_adapter(&self) -> Option<&dyn ChannelGatewayAdapter> {
        Some(&self.gateway)
    }

    fn outbound_adapter(&self) -> Option<&dyn ChannelOutboundAdapter> {
        Some(&self.outbound)
    }

    fn setup_adapter(&self) -> Option<&dyn ChannelSetupAdapter> {
        Some(&self.setup)
    }

    fn heartbeat_adapter(&self) -> Option<&dyn ChannelHeartbeatAdapter> {
        Some(&self.heartbeat)
    }

    fn pairing_adapter(&self) -> Option<&dyn ChannelPairingAdapter> {
        Some(&self.pairing)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plugin_meta() {
        let plugin = WhatsAppPlugin::new();
        assert_eq!(plugin.meta().id, "whatsapp");
        assert_eq!(plugin.meta().label, "WhatsApp");
    }

    #[test]
    fn plugin_capabilities() {
        let plugin = WhatsAppPlugin::new();
        let caps = plugin.capabilities();
        assert_eq!(caps.block_streaming, Some(true));
        assert_eq!(caps.native_commands, Some(false));
        assert_eq!(caps.media, Some(true));
    }

    #[test]
    fn adapter_accessors() {
        let plugin = WhatsAppPlugin::new();
        assert!(plugin.gateway_adapter().is_some());
        assert!(plugin.outbound_adapter().is_some());
        assert!(plugin.setup_adapter().is_some());
        assert!(plugin.heartbeat_adapter().is_some());
        assert!(plugin.pairing_adapter().is_some());
        assert!(plugin.command_adapter().is_none());
    }
}
