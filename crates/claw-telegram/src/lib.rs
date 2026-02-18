//! Telegram channel plugin for claw-rust.
//!
//! Implements the ChannelPlugin trait for Telegram Bot API via teloxide.
//! Supports long polling and webhook modes, inline keyboards, streaming
//! (edit-in-place), and custom command registration.

pub mod adapters;
pub mod file_resolver;
pub mod gateway;
pub mod normalize;
pub mod outbound;
pub mod plugin;

pub use plugin::TelegramPlugin;

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

/// Shared store of teloxide Bot instances keyed by account ID.
///
/// The gateway registers bots here when starting accounts; outbound and
/// adapter methods look them up by account_id at send time.
pub type BotStore = Arc<RwLock<HashMap<String, teloxide::Bot>>>;

/// Create a new empty bot store.
pub fn new_bot_store() -> BotStore {
    Arc::new(RwLock::new(HashMap::new()))
}
