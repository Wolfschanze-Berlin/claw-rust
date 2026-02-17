//! Discord ChannelPlugin implementation.

use claw_channels::plugin::{
    ChannelCommandAdapter, ChannelGatewayAdapter, ChannelMentionAdapter,
    ChannelMessageActionAdapter, ChannelOutboundAdapter, ChannelPlugin,
    ChannelThreadingAdapter,
};
use claw_channels::types::{
    ChannelCapabilities, ChannelMeta, ChatType,
};

use crate::adapters::{
    DiscordCommandAdapter, DiscordMentionAdapter, DiscordMessageActionAdapter,
    DiscordThreadingAdapter,
};
use crate::gateway::DiscordGateway;
use crate::outbound::DiscordOutbound;

/// Discord channel plugin — guild/channel model with WebSocket shards.
pub struct DiscordPlugin {
    meta: ChannelMeta,
    capabilities: ChannelCapabilities,
    gateway: DiscordGateway,
    outbound: DiscordOutbound,
    mention: DiscordMentionAdapter,
    command: DiscordCommandAdapter,
    message_action: DiscordMessageActionAdapter,
    threading: DiscordThreadingAdapter,
}

impl DiscordPlugin {
    pub fn new() -> Self {
        Self {
            meta: ChannelMeta {
                id: "discord".into(),
                label: "Discord".into(),
                selection_label: Some("DC".into()),
                docs_path: Some("docs/channels/discord.md".into()),
                blurb: Some("Discord bot channel".into()),
                order: Some(3),
                aliases: Some(vec!["dc".into()]),
            },
            capabilities: ChannelCapabilities {
                chat_types: Some(vec![
                    ChatType::Direct,
                    ChatType::Group,
                    ChatType::Channel,
                    ChatType::Thread,
                ]),
                polls: Some(false),
                reactions: Some(true),
                edit: Some(true),
                unsend: Some(true),
                reply: Some(true),
                effects: Some(false),
                group_management: Some(true),
                threads: Some(true),
                media: Some(true),
                native_commands: Some(true),
                block_streaming: Some(false),
            },
            gateway: DiscordGateway::new(),
            outbound: DiscordOutbound::new(),
            mention: DiscordMentionAdapter::new(),
            command: DiscordCommandAdapter::new(),
            message_action: DiscordMessageActionAdapter::new(),
            threading: DiscordThreadingAdapter::new(),
        }
    }
}

impl Default for DiscordPlugin {
    fn default() -> Self {
        Self::new()
    }
}

impl ChannelPlugin for DiscordPlugin {
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

    fn mention_adapter(&self) -> Option<&dyn ChannelMentionAdapter> {
        Some(&self.mention)
    }

    fn command_adapter(&self) -> Option<&dyn ChannelCommandAdapter> {
        Some(&self.command)
    }

    fn message_action_adapter(&self) -> Option<&dyn ChannelMessageActionAdapter> {
        Some(&self.message_action)
    }

    fn threading_adapter(&self) -> Option<&dyn ChannelThreadingAdapter> {
        Some(&self.threading)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plugin_meta() {
        let plugin = DiscordPlugin::new();
        assert_eq!(plugin.meta().id, "discord");
        assert_eq!(plugin.meta().label, "Discord");
    }

    #[test]
    fn plugin_capabilities() {
        let plugin = DiscordPlugin::new();
        let caps = plugin.capabilities();
        assert_eq!(caps.threads, Some(true));
        assert_eq!(caps.native_commands, Some(true));
        assert_eq!(caps.edit, Some(true));
    }

    #[test]
    fn adapter_accessors() {
        let plugin = DiscordPlugin::new();
        assert!(plugin.gateway_adapter().is_some());
        assert!(plugin.outbound_adapter().is_some());
        assert!(plugin.mention_adapter().is_some());
        assert!(plugin.command_adapter().is_some());
        assert!(plugin.message_action_adapter().is_some());
        assert!(plugin.threading_adapter().is_some());
    }
}
