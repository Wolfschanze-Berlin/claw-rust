//! Telegram ChannelPlugin implementation.

use claw_channels::plugin::{
    ChannelCommandAdapter, ChannelGatewayAdapter, ChannelMentionAdapter,
    ChannelMessageActionAdapter, ChannelOutboundAdapter, ChannelPlugin,
    ChannelStreamingAdapter,
};
use claw_channels::types::{
    ChannelCapabilities, ChannelMeta, ChatType,
};

use crate::adapters::{
    TelegramCommandAdapter, TelegramMentionAdapter, TelegramMessageActionAdapter,
    TelegramStreamingAdapter,
};
use crate::gateway::TelegramGateway;
use crate::outbound::TelegramOutbound;

/// Telegram channel plugin — wraps the Telegram Bot API.
pub struct TelegramPlugin {
    meta: ChannelMeta,
    capabilities: ChannelCapabilities,
    gateway: TelegramGateway,
    outbound: TelegramOutbound,
    mention: TelegramMentionAdapter,
    command: TelegramCommandAdapter,
    message_action: TelegramMessageActionAdapter,
    streaming: TelegramStreamingAdapter,
}

impl TelegramPlugin {
    pub fn new() -> Self {
        Self {
            meta: ChannelMeta {
                id: "telegram".into(),
                label: "Telegram".into(),
                selection_label: Some("TG".into()),
                docs_path: Some("docs/channels/telegram.md".into()),
                blurb: Some("Telegram Bot API channel".into()),
                order: Some(1),
                aliases: Some(vec!["tg".into()]),
            },
            capabilities: ChannelCapabilities {
                chat_types: Some(vec![
                    ChatType::Direct,
                    ChatType::Group,
                    ChatType::Channel,
                ]),
                polls: Some(true),
                reactions: Some(true),
                edit: Some(true),
                unsend: Some(true),
                reply: Some(true),
                effects: Some(false),
                group_management: Some(true),
                threads: Some(false),
                media: Some(true),
                native_commands: Some(true),
                block_streaming: Some(false),
            },
            gateway: TelegramGateway::new(),
            outbound: TelegramOutbound::new(),
            mention: TelegramMentionAdapter::new(),
            command: TelegramCommandAdapter::new(),
            message_action: TelegramMessageActionAdapter::new(),
            streaming: TelegramStreamingAdapter::new(),
        }
    }
}

impl Default for TelegramPlugin {
    fn default() -> Self {
        Self::new()
    }
}

impl ChannelPlugin for TelegramPlugin {
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

    fn streaming_adapter(&self) -> Option<&dyn ChannelStreamingAdapter> {
        Some(&self.streaming)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plugin_meta() {
        let plugin = TelegramPlugin::new();
        assert_eq!(plugin.meta().id, "telegram");
        assert_eq!(plugin.meta().label, "Telegram");
    }

    #[test]
    fn plugin_capabilities() {
        let plugin = TelegramPlugin::new();
        let caps = plugin.capabilities();
        assert_eq!(caps.native_commands, Some(true));
        assert_eq!(caps.reactions, Some(true));
        assert_eq!(caps.threads, Some(false));
    }

    #[test]
    fn adapter_accessors() {
        let plugin = TelegramPlugin::new();
        assert!(plugin.gateway_adapter().is_some());
        assert!(plugin.outbound_adapter().is_some());
        assert!(plugin.mention_adapter().is_some());
        assert!(plugin.command_adapter().is_some());
        assert!(plugin.message_action_adapter().is_some());
        assert!(plugin.streaming_adapter().is_some());
        assert!(plugin.threading_adapter().is_none());
    }
}
