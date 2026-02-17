//! Telegram channel plugin for claw-rust.
//!
//! Implements the ChannelPlugin trait for Telegram Bot API via teloxide.
//! Supports long polling and webhook modes, inline keyboards, streaming
//! (edit-in-place), and custom command registration.

pub mod adapters;
pub mod gateway;
pub mod normalize;
pub mod outbound;
pub mod plugin;

pub use plugin::TelegramPlugin;
