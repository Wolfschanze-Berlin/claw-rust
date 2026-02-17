//! Plugin runtime and logging facilities.
//!
//! Each plugin gets a [`PluginRuntime`] that provides a scoped
//! [`PluginLogger`] so log output is automatically tagged with
//! the plugin's ID.

use tracing::{debug, error, info, warn};

// ---------------------------------------------------------------------------
// PluginRuntime
// ---------------------------------------------------------------------------

/// Runtime context given to a plugin during execution.
pub struct PluginRuntime {
    pub plugin_id: String,
    pub logger: PluginLogger,
}

impl PluginRuntime {
    pub fn new(plugin_id: String) -> Self {
        let logger = PluginLogger {
            plugin_id: plugin_id.clone(),
        };
        Self { plugin_id, logger }
    }
}

// ---------------------------------------------------------------------------
// PluginLogger
// ---------------------------------------------------------------------------

/// A logger scoped to a specific plugin, using the `tracing` crate.
pub struct PluginLogger {
    plugin_id: String,
}

impl PluginLogger {
    pub fn info(&self, msg: &str) {
        info!(plugin_id = %self.plugin_id, "{msg}");
    }

    pub fn warn(&self, msg: &str) {
        warn!(plugin_id = %self.plugin_id, "{msg}");
    }

    pub fn error(&self, msg: &str) {
        error!(plugin_id = %self.plugin_id, "{msg}");
    }

    pub fn debug(&self, msg: &str) {
        debug!(plugin_id = %self.plugin_id, "{msg}");
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runtime_creation() {
        let rt = PluginRuntime::new("my-plugin".into());
        assert_eq!(rt.plugin_id, "my-plugin");
        assert_eq!(rt.logger.plugin_id, "my-plugin");
    }
}
