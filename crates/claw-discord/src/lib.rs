//! Discord channel plugin for claw-rust.
//!
//! Implements the ChannelPlugin trait for Discord using WebSocket gateway
//! connections. Supports guilds, text channels, threads, components
//! (buttons/select menus), and slash commands.

pub mod adapters;
pub mod gateway;
pub mod normalize;
pub mod outbound;
pub mod plugin;

pub use plugin::DiscordPlugin;
