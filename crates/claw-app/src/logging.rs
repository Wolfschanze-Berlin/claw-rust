//! Custom tracing layer for GUI log capture.
//!
//! Sends log events via mpsc channel to the UI for real-time display.

use crate::state::LogEntry;
use std::collections::VecDeque;
use tokio::sync::mpsc;
use tracing::Subscriber;
use tracing_subscriber::Layer;

/// Custom tracing Layer that sends events to the GUI
pub struct GuiLogLayer {
    sender: mpsc::UnboundedSender<LogEntry>,
}

impl GuiLogLayer {
    /// Create a new GuiLogLayer with an unbounded sender
    pub fn new(sender: mpsc::UnboundedSender<LogEntry>) -> Self {
        Self { sender }
    }
}

impl<S: Subscriber> Layer<S> for GuiLogLayer {
    fn on_event(
        &self,
        event: &tracing::Event<'_>,
        _ctx: tracing_subscriber::layer::Context<'_, S>,
    ) {
        // Extract message from the event
        let mut visitor = MessageVisitor::default();
        event.record(&mut visitor);

        let entry = LogEntry {
            timestamp: chrono::Utc::now(),
            level: *event.metadata().level(),
            target: event.metadata().target().to_string(),
            message: visitor.message,
        };

        // Send to GUI — ignore error if receiver dropped
        let _ = self.sender.send(entry);
    }
}

/// Visitor to extract the message field from tracing events
#[derive(Default)]
struct MessageVisitor {
    message: String,
}

impl tracing::field::Visit for MessageVisitor {
    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        if field.name() == "message" {
            self.message = format!("{:?}", value);
        }
    }

    fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
        if field.name() == "message" {
            self.message = value.to_string();
        }
    }
}

/// Bounded ring buffer for log entries
pub struct LogBuffer {
    entries: VecDeque<LogEntry>,
    capacity: usize,
}

impl LogBuffer {
    pub fn new(capacity: usize) -> Self {
        Self {
            entries: VecDeque::with_capacity(capacity),
            capacity,
        }
    }

    /// Add an entry, evicting oldest if at capacity
    pub fn push(&mut self, entry: LogEntry) {
        if self.entries.len() >= self.capacity {
            self.entries.pop_front();
        }
        self.entries.push_back(entry);
    }

    /// Drain all available entries from the receiver into the buffer
    pub fn drain_receiver(&mut self, receiver: &mut mpsc::UnboundedReceiver<LogEntry>) {
        while let Ok(entry) = receiver.try_recv() {
            self.push(entry);
        }
    }

    /// Get all entries as a slice-like iterator
    pub fn entries(&self) -> impl Iterator<Item = &LogEntry> {
        self.entries.iter()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}
