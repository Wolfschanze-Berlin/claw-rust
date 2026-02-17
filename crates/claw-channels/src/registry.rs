//! Channel registry, ChannelDock, and ChannelId types.
//!
//! Ports OpenClaw's `src/channels/registry.ts` and `src/channels/dock.ts`.
//!
//! The registry is a thread-safe, read-optimized store of lightweight
//! `ChannelDock` facades. It enables fast channel lookups during message
//! routing without loading full plugin implementations.

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use serde::{Deserialize, Serialize};

use crate::plugin::ChannelPlugin;
use crate::types::{ChannelCapabilities, ChannelMeta, DeliveryMode};

// ---------------------------------------------------------------------------
// ChatChannelId — well-known built-in channels
// ---------------------------------------------------------------------------

/// Well-known built-in channel identifiers.
///
/// These match the channel IDs recognized by the OpenClaw core.
/// Plugin/extension channels use [`ChannelId::Custom`] instead.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ChatChannelId {
    Telegram,
    #[serde(rename = "whatsapp")]
    WhatsApp,
    Discord,
    Irc,
    #[serde(rename = "googlechat")]
    GoogleChat,
    Slack,
    Signal,
    #[serde(rename = "imessage")]
    IMessage,
}

impl ChatChannelId {
    /// Returns the canonical string ID.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Telegram => "telegram",
            Self::WhatsApp => "whatsapp",
            Self::Discord => "discord",
            Self::Irc => "irc",
            Self::GoogleChat => "googlechat",
            Self::Slack => "slack",
            Self::Signal => "signal",
            Self::IMessage => "imessage",
        }
    }

    /// All built-in channel IDs in display order.
    pub const ALL: &'static [ChatChannelId] = &[
        Self::Telegram,
        Self::WhatsApp,
        Self::Discord,
        Self::Slack,
        Self::Signal,
        Self::GoogleChat,
        Self::Irc,
        Self::IMessage,
    ];
}

impl std::fmt::Display for ChatChannelId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl std::str::FromStr for ChatChannelId {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "telegram" | "tg" => Ok(Self::Telegram),
            "whatsapp" | "wa" => Ok(Self::WhatsApp),
            "discord" => Ok(Self::Discord),
            "irc" => Ok(Self::Irc),
            "googlechat" | "google_chat" | "gchat" => Ok(Self::GoogleChat),
            "slack" => Ok(Self::Slack),
            "signal" => Ok(Self::Signal),
            "imessage" | "imsg" => Ok(Self::IMessage),
            _ => Err(()),
        }
    }
}

// ---------------------------------------------------------------------------
// ChannelId — built-in or custom
// ---------------------------------------------------------------------------

/// A channel identifier that supports both built-in and extension channels.
///
/// Built-in channels get enum-level type safety. Custom/plugin channels
/// use a string identifier for extensibility.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ChannelId {
    /// A well-known built-in channel.
    BuiltIn(ChatChannelId),
    /// A custom/plugin channel identified by string.
    Custom(String),
}

impl ChannelId {
    /// Returns the string representation.
    pub fn as_str(&self) -> &str {
        match self {
            Self::BuiltIn(id) => id.as_str(),
            Self::Custom(s) => s.as_str(),
        }
    }
}

impl std::fmt::Display for ChannelId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl From<ChatChannelId> for ChannelId {
    fn from(id: ChatChannelId) -> Self {
        Self::BuiltIn(id)
    }
}

impl From<String> for ChannelId {
    fn from(s: String) -> Self {
        match s.parse::<ChatChannelId>() {
            Ok(id) => Self::BuiltIn(id),
            Err(()) => Self::Custom(s),
        }
    }
}

impl From<&str> for ChannelId {
    fn from(s: &str) -> Self {
        Self::from(s.to_owned())
    }
}

// ---------------------------------------------------------------------------
// CHAT_CHANNEL_ORDER
// ---------------------------------------------------------------------------

/// Default display ordering for built-in chat channels.
///
/// Matches OpenClaw's `CHAT_CHANNEL_ORDER` array used for UI rendering.
pub const CHAT_CHANNEL_ORDER: &[ChatChannelId] = ChatChannelId::ALL;

// ---------------------------------------------------------------------------
// ChannelDock
// ---------------------------------------------------------------------------

/// A lightweight facade for a registered channel.
///
/// Holds metadata, capabilities, and runtime hints without requiring
/// the full plugin implementation to be loaded. Used by the dispatch
/// pipeline for fast routing decisions.
#[derive(Clone)]
pub struct ChannelDock {
    /// Channel metadata (id, label, docs, etc).
    pub meta: ChannelMeta,

    /// Declared capabilities.
    pub capabilities: ChannelCapabilities,

    /// Preferred outbound delivery mode.
    pub delivery_mode: DeliveryMode,

    /// Whether the channel supports native command registration.
    pub supports_commands: bool,

    /// Whether the channel supports streaming delivery.
    pub supports_streaming: bool,

    /// Reference to the full plugin (if loaded).
    plugin: Option<Arc<dyn ChannelPlugin>>,
}

impl std::fmt::Debug for ChannelDock {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ChannelDock")
            .field("meta", &self.meta)
            .field("delivery_mode", &self.delivery_mode)
            .field("supports_commands", &self.supports_commands)
            .field("supports_streaming", &self.supports_streaming)
            .field("plugin_loaded", &self.plugin.is_some())
            .finish()
    }
}

impl ChannelDock {
    /// Create a dock from a channel plugin.
    pub fn from_plugin(plugin: Arc<dyn ChannelPlugin>) -> Self {
        let meta = plugin.meta().clone();
        let capabilities = plugin.capabilities().clone();

        let delivery_mode = plugin
            .outbound_adapter()
            .map(|a| a.delivery_mode())
            .unwrap_or_default();

        let supports_commands = plugin.command_adapter().is_some();
        let supports_streaming = plugin.streaming_adapter().is_some();

        Self {
            meta,
            capabilities,
            delivery_mode,
            supports_commands,
            supports_streaming,
            plugin: Some(plugin),
        }
    }

    /// Create a metadata-only dock (no plugin loaded).
    pub fn metadata_only(meta: ChannelMeta, capabilities: ChannelCapabilities) -> Self {
        Self {
            meta,
            capabilities,
            delivery_mode: DeliveryMode::default(),
            supports_commands: false,
            supports_streaming: false,
            plugin: None,
        }
    }

    /// The channel ID.
    pub fn id(&self) -> &str {
        &self.meta.id
    }

    /// Get a reference to the underlying plugin, if loaded.
    pub fn plugin(&self) -> Option<&Arc<dyn ChannelPlugin>> {
        self.plugin.as_ref()
    }
}

// ---------------------------------------------------------------------------
// ChannelRegistry
// ---------------------------------------------------------------------------

/// Thread-safe registry of channel docks.
///
/// Read-optimized via `RwLock` — concurrent readers, exclusive writers.
/// Typical access pattern: many reads per inbound message, rare writes
/// (startup, hot-reload).
#[derive(Debug, Clone)]
pub struct ChannelRegistry {
    docks: Arc<RwLock<HashMap<String, Arc<ChannelDock>>>>,
}

impl Default for ChannelRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl ChannelRegistry {
    /// Create an empty registry.
    pub fn new() -> Self {
        Self {
            docks: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Register a channel dock.
    pub fn register(&self, dock: ChannelDock) {
        let id = dock.id().to_owned();
        let mut map = self.docks.write().expect("registry lock poisoned");
        map.insert(id, Arc::new(dock));
    }

    /// Register a channel plugin (creates a dock automatically).
    pub fn register_plugin(&self, plugin: Arc<dyn ChannelPlugin>) {
        let dock = ChannelDock::from_plugin(plugin);
        self.register(dock);
    }

    /// Look up a channel dock by ID.
    pub fn get(&self, channel_id: &str) -> Option<Arc<ChannelDock>> {
        let map = self.docks.read().expect("registry lock poisoned");
        map.get(channel_id).cloned()
    }

    /// List all registered channel docks.
    pub fn list(&self) -> Vec<Arc<ChannelDock>> {
        let map = self.docks.read().expect("registry lock poisoned");
        map.values().cloned().collect()
    }

    /// List all registered channel IDs.
    pub fn list_ids(&self) -> Vec<String> {
        let map = self.docks.read().expect("registry lock poisoned");
        map.keys().cloned().collect()
    }

    /// Remove a channel from the registry.
    pub fn unregister(&self, channel_id: &str) -> Option<Arc<ChannelDock>> {
        let mut map = self.docks.write().expect("registry lock poisoned");
        map.remove(channel_id)
    }

    /// Number of registered channels.
    pub fn len(&self) -> usize {
        let map = self.docks.read().expect("registry lock poisoned");
        map.len()
    }

    /// Whether the registry is empty.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

// ---------------------------------------------------------------------------
// Normalization helpers
// ---------------------------------------------------------------------------

/// Normalize a channel ID string to its canonical form.
///
/// Handles aliases (e.g., "tg" → "telegram", "wa" → "whatsapp").
/// Returns the input unchanged for custom/unknown channels.
pub fn normalize_channel_id(id: &str) -> String {
    match id.parse::<ChatChannelId>() {
        Ok(ch) => ch.as_str().to_owned(),
        Err(()) => id.to_lowercase(),
    }
}

/// Get metadata for a built-in chat channel.
pub fn get_chat_channel_meta(id: ChatChannelId) -> ChannelMeta {
    let (label, blurb, order) = match id {
        ChatChannelId::Telegram => ("Telegram", "Telegram Bot API", 1),
        ChatChannelId::WhatsApp => ("WhatsApp", "WhatsApp Business API", 2),
        ChatChannelId::Discord => ("Discord", "Discord Bot", 3),
        ChatChannelId::Slack => ("Slack", "Slack App", 4),
        ChatChannelId::Signal => ("Signal", "Signal Messenger", 5),
        ChatChannelId::GoogleChat => ("Google Chat", "Google Workspace Chat", 6),
        ChatChannelId::Irc => ("IRC", "Internet Relay Chat", 7),
        ChatChannelId::IMessage => ("iMessage", "Apple iMessage", 8),
    };

    ChannelMeta {
        id: id.as_str().to_owned(),
        label: label.to_owned(),
        selection_label: Some(label.to_owned()),
        docs_path: None,
        blurb: Some(blurb.to_owned()),
        order: Some(order),
        aliases: None,
    }
}

/// List all built-in chat channels in display order.
pub fn list_chat_channels() -> Vec<ChatChannelId> {
    CHAT_CHANNEL_ORDER.to_vec()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::ChatType;

    #[test]
    fn chat_channel_id_serde_roundtrip() {
        let id = ChatChannelId::Telegram;
        let json = serde_json::to_string(&id).unwrap();
        assert_eq!(json, r#""telegram""#);
        let parsed: ChatChannelId = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, id);
    }

    #[test]
    fn chat_channel_id_from_str_aliases() {
        assert_eq!("tg".parse::<ChatChannelId>(), Ok(ChatChannelId::Telegram));
        assert_eq!("wa".parse::<ChatChannelId>(), Ok(ChatChannelId::WhatsApp));
        assert_eq!(
            "gchat".parse::<ChatChannelId>(),
            Ok(ChatChannelId::GoogleChat)
        );
        assert_eq!(
            "imsg".parse::<ChatChannelId>(),
            Ok(ChatChannelId::IMessage)
        );
        assert!("unknown".parse::<ChatChannelId>().is_err());
    }

    #[test]
    fn channel_id_from_builtin() {
        let id = ChannelId::from("telegram");
        assert_eq!(id, ChannelId::BuiltIn(ChatChannelId::Telegram));
        assert_eq!(id.as_str(), "telegram");
    }

    #[test]
    fn channel_id_from_custom() {
        let id = ChannelId::from("my-custom-channel");
        assert_eq!(id, ChannelId::Custom("my-custom-channel".into()));
        assert_eq!(id.as_str(), "my-custom-channel");
    }

    #[test]
    fn channel_id_from_alias() {
        let id = ChannelId::from("tg");
        assert_eq!(id, ChannelId::BuiltIn(ChatChannelId::Telegram));
    }

    #[test]
    fn normalize_builtin() {
        assert_eq!(normalize_channel_id("tg"), "telegram");
        assert_eq!(normalize_channel_id("wa"), "whatsapp");
        assert_eq!(normalize_channel_id("Telegram"), "telegram");
    }

    #[test]
    fn normalize_custom() {
        assert_eq!(normalize_channel_id("MyPlugin"), "myplugin");
    }

    #[test]
    fn chat_channel_meta() {
        let meta = get_chat_channel_meta(ChatChannelId::Telegram);
        assert_eq!(meta.id, "telegram");
        assert_eq!(meta.label, "Telegram");
        assert_eq!(meta.order, Some(1));
    }

    #[test]
    fn list_channels_returns_all() {
        let channels = list_chat_channels();
        assert_eq!(channels.len(), 8);
        assert_eq!(channels[0], ChatChannelId::Telegram);
    }

    // -- ChannelDock --------------------------------------------------------

    #[test]
    fn dock_metadata_only() {
        let meta = ChannelMeta {
            id: "test".into(),
            label: "Test".into(),
            selection_label: None,
            docs_path: None,
            blurb: None,
            order: None,
            aliases: None,
        };
        let caps = ChannelCapabilities {
            chat_types: Some(vec![ChatType::Direct]),
            ..Default::default()
        };
        let dock = ChannelDock::metadata_only(meta, caps);

        assert_eq!(dock.id(), "test");
        assert!(dock.plugin().is_none());
        assert!(!dock.supports_commands);
        assert!(!dock.supports_streaming);
    }

    // -- ChannelRegistry ----------------------------------------------------

    #[test]
    fn registry_register_and_get() {
        let registry = ChannelRegistry::new();
        assert!(registry.is_empty());

        let meta = ChannelMeta {
            id: "telegram".into(),
            label: "Telegram".into(),
            selection_label: None,
            docs_path: None,
            blurb: None,
            order: None,
            aliases: None,
        };
        let dock = ChannelDock::metadata_only(meta, ChannelCapabilities::default());
        registry.register(dock);

        assert_eq!(registry.len(), 1);
        assert!(!registry.is_empty());

        let found = registry.get("telegram");
        assert!(found.is_some());
        assert_eq!(found.unwrap().id(), "telegram");

        assert!(registry.get("nonexistent").is_none());
    }

    #[test]
    fn registry_list_and_ids() {
        let registry = ChannelRegistry::new();

        for ch in &["telegram", "discord", "slack"] {
            let meta = ChannelMeta {
                id: (*ch).to_string(),
                label: ch.to_string(),
                selection_label: None,
                docs_path: None,
                blurb: None,
                order: None,
                aliases: None,
            };
            registry.register(ChannelDock::metadata_only(meta, ChannelCapabilities::default()));
        }

        assert_eq!(registry.len(), 3);
        let ids = registry.list_ids();
        assert!(ids.contains(&"telegram".to_string()));
        assert!(ids.contains(&"discord".to_string()));
        assert!(ids.contains(&"slack".to_string()));
    }

    #[test]
    fn registry_unregister() {
        let registry = ChannelRegistry::new();
        let meta = ChannelMeta {
            id: "test".into(),
            label: "Test".into(),
            selection_label: None,
            docs_path: None,
            blurb: None,
            order: None,
            aliases: None,
        };
        registry.register(ChannelDock::metadata_only(meta, ChannelCapabilities::default()));
        assert_eq!(registry.len(), 1);

        let removed = registry.unregister("test");
        assert!(removed.is_some());
        assert_eq!(registry.len(), 0);

        let removed_again = registry.unregister("test");
        assert!(removed_again.is_none());
    }

    #[test]
    fn registry_clone_shares_state() {
        let r1 = ChannelRegistry::new();
        let r2 = r1.clone();

        let meta = ChannelMeta {
            id: "shared".into(),
            label: "Shared".into(),
            selection_label: None,
            docs_path: None,
            blurb: None,
            order: None,
            aliases: None,
        };
        r1.register(ChannelDock::metadata_only(meta, ChannelCapabilities::default()));

        // r2 sees the registration because they share the same Arc<RwLock<..>>
        assert_eq!(r2.len(), 1);
        assert!(r2.get("shared").is_some());
    }
}
