//! Telegram ChannelPlugin implementation.

use claw_channels::plugin::{
    ChannelAuthAdapter, ChannelCommandAdapter, ChannelGatewayAdapter, ChannelGroupAdapter,
    ChannelMentionAdapter, ChannelMessageActionAdapter, ChannelMessagingAdapter,
    ChannelOutboundAdapter, ChannelPlugin, ChannelStatusAdapter, ChannelStreamingAdapter,
    ChannelThreadingAdapter,
};
use claw_channels::types::{
    ChannelCapabilities, ChannelMeta, ChatType,
};

use crate::adapters::{
    TelegramAuthAdapter, TelegramCommandAdapter, TelegramGroupAdapter, TelegramMentionAdapter,
    TelegramMessageActionAdapter, TelegramMessagingAdapter, TelegramStatusAdapter,
    TelegramStreamingAdapter, TelegramThreadingAdapter,
};
use crate::gateway::TelegramGateway;
use crate::outbound::TelegramOutbound;
use crate::{BotStore, new_bot_store};

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
    group: TelegramGroupAdapter,
    status: TelegramStatusAdapter,
    messaging: TelegramMessagingAdapter,
    auth: TelegramAuthAdapter,
    threading: TelegramThreadingAdapter,
}

impl TelegramPlugin {
    pub fn new() -> Self {
        let bots: BotStore = new_bot_store();

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
                threads: Some(true),
                media: Some(true),
                native_commands: Some(true),
                block_streaming: Some(false),
            },
            gateway: TelegramGateway::new(bots.clone()),
            outbound: TelegramOutbound::new(bots.clone()),
            mention: TelegramMentionAdapter::new(),
            command: TelegramCommandAdapter::new(bots.clone()),
            message_action: TelegramMessageActionAdapter::new(bots.clone()),
            streaming: TelegramStreamingAdapter::new(bots.clone()),
            group: TelegramGroupAdapter::new(bots.clone()),
            status: TelegramStatusAdapter::new(bots.clone()),
            messaging: TelegramMessagingAdapter::new(bots.clone()),
            auth: TelegramAuthAdapter::new(bots.clone()),
            threading: TelegramThreadingAdapter::new(bots),
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

    fn group_adapter(&self) -> Option<&dyn ChannelGroupAdapter> {
        Some(&self.group)
    }

    fn status_adapter(&self) -> Option<&dyn ChannelStatusAdapter> {
        Some(&self.status)
    }

    fn messaging_adapter(&self) -> Option<&dyn ChannelMessagingAdapter> {
        Some(&self.messaging)
    }

    fn auth_adapter(&self) -> Option<&dyn ChannelAuthAdapter> {
        Some(&self.auth)
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
        assert_eq!(caps.threads, Some(true));
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
        assert!(plugin.group_adapter().is_some());
        assert!(plugin.status_adapter().is_some());
        assert!(plugin.messaging_adapter().is_some());
        assert!(plugin.auth_adapter().is_some());
        assert!(plugin.threading_adapter().is_some());
    }
}
