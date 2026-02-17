//! Per-session message queue for buffering inbound messages while a run is active.
//!
//! When a session already has an agent run in progress, new messages are
//! enqueued here and drained once the current run completes.

use std::collections::VecDeque;
use std::sync::Arc;

use tokio::sync::Mutex;

use crate::runner::QueuedMessage;

/// A thread-safe FIFO message queue for a single session.
///
/// Wraps a `VecDeque` behind an `Arc<Mutex<…>>` so it can be shared across
/// async tasks safely.
#[derive(Debug, Clone)]
pub struct MessageQueue {
    inner: Arc<Mutex<VecDeque<QueuedMessage>>>,
}

impl MessageQueue {
    /// Create an empty message queue.
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(VecDeque::new())),
        }
    }

    /// Append a message to the back of the queue.
    pub async fn enqueue(&self, message: QueuedMessage) {
        self.inner.lock().await.push_back(message);
    }

    /// Remove and return the message at the front of the queue, if any.
    pub async fn dequeue(&self) -> Option<QueuedMessage> {
        self.inner.lock().await.pop_front()
    }

    /// Return the number of queued messages.
    pub async fn len(&self) -> usize {
        self.inner.lock().await.len()
    }

    /// Return `true` if the queue contains no messages.
    pub async fn is_empty(&self) -> bool {
        self.inner.lock().await.is_empty()
    }
}

impl Default for MessageQueue {
    fn default() -> Self {
        Self::new()
    }
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::Instant;

    fn msg(content: &str) -> QueuedMessage {
        QueuedMessage {
            content: content.to_owned(),
            channel_context: serde_json::json!({}),
            queued_at: Instant::now(),
        }
    }

    #[tokio::test]
    async fn fifo_ordering() {
        let q = MessageQueue::new();
        q.enqueue(msg("first")).await;
        q.enqueue(msg("second")).await;
        q.enqueue(msg("third")).await;

        assert_eq!(q.dequeue().await.unwrap().content, "first");
        assert_eq!(q.dequeue().await.unwrap().content, "second");
        assert_eq!(q.dequeue().await.unwrap().content, "third");
        assert!(q.dequeue().await.is_none());
    }

    #[tokio::test]
    async fn len_and_is_empty() {
        let q = MessageQueue::new();
        assert!(q.is_empty().await);
        assert_eq!(q.len().await, 0);

        q.enqueue(msg("a")).await;
        assert!(!q.is_empty().await);
        assert_eq!(q.len().await, 1);

        q.dequeue().await;
        assert!(q.is_empty().await);
    }

    #[tokio::test]
    async fn dequeue_on_empty_returns_none() {
        let q = MessageQueue::new();
        assert!(q.dequeue().await.is_none());
    }
}
