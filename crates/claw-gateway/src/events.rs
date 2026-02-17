//! Event broadcasting system for the gateway.
//!
//! Provides [`EventBroadcaster`] for distributing server-to-client events
//! to all connected WebSocket clients, with monotonic sequence numbering
//! and [`StateVersion`] tracking.

use std::collections::HashSet;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use tokio::sync::{RwLock, broadcast};

use crate::protocol::frames::EventFrame;
use crate::protocol::handshake::StateVersion;

// ---------------------------------------------------------------------------
// Built-in event type constants
// ---------------------------------------------------------------------------

/// Well-known event type names matching the OpenClaw TypeScript gateway.
pub mod event_types {
    pub const PRESENCE_UPDATE: &str = "presence.update";
    pub const HEALTH_UPDATE: &str = "health.update";
    pub const CHANNEL_STATUS: &str = "channel.status";
    pub const CONFIG_CHANGED: &str = "config.changed";
}

// ---------------------------------------------------------------------------
// StateVersionUpdate
// ---------------------------------------------------------------------------

/// Describes how to mutate the [`StateVersion`] when broadcasting an event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StateVersionUpdate {
    /// Increment the presence counter by 1.
    IncrementPresence,
    /// Increment the health counter by 1.
    IncrementHealth,
    /// Increment both presence and health counters by 1.
    Both,
    /// Replace with an entirely custom state version.
    Custom(StateVersion),
}

// ---------------------------------------------------------------------------
// EventFilter
// ---------------------------------------------------------------------------

/// Optional per-subscriber filter to include/exclude event types.
///
/// Clients can subscribe to all events (default), only specific event types,
/// or all events except certain types.
#[derive(Debug, Clone)]
pub struct EventFilter {
    /// If `Some`, only events matching these names pass through.
    pub include: Option<HashSet<String>>,
    /// If `Some`, events matching these names are dropped.
    pub exclude: Option<HashSet<String>>,
}

impl EventFilter {
    /// Accept all events (no filtering).
    pub fn all() -> Self {
        Self {
            include: None,
            exclude: None,
        }
    }

    /// Only accept events whose name is in `events`.
    pub fn only(events: &[&str]) -> Self {
        Self {
            include: Some(events.iter().map(|s| (*s).to_owned()).collect()),
            exclude: None,
        }
    }

    /// Accept all events except those whose name is in `events`.
    pub fn except(events: &[&str]) -> Self {
        Self {
            include: None,
            exclude: Some(events.iter().map(|s| (*s).to_owned()).collect()),
        }
    }

    /// Returns `true` if the given event name passes this filter.
    pub fn matches(&self, event: &str) -> bool {
        if let Some(ref include) = self.include {
            return include.contains(event);
        }
        if let Some(ref exclude) = self.exclude {
            return !exclude.contains(event);
        }
        true
    }
}

// ---------------------------------------------------------------------------
// EventBroadcaster
// ---------------------------------------------------------------------------

/// Multi-consumer event broadcaster for the gateway.
///
/// Uses a [`tokio::sync::broadcast`] channel to fan out [`EventFrame`]s
/// wrapped in `Arc` to all active subscribers. Each broadcast atomically
/// increments a sequence counter and optionally updates the
/// [`StateVersion`].
#[derive(Debug)]
pub struct EventBroadcaster {
    sender: broadcast::Sender<Arc<EventFrame>>,
    seq: AtomicU64,
    state_version: Arc<RwLock<StateVersion>>,
}

impl EventBroadcaster {
    /// Create a new broadcaster with the given channel capacity.
    ///
    /// `capacity` determines how many un-consumed events can be buffered
    /// per subscriber before lagging receivers start losing messages.
    pub fn new(capacity: usize) -> Self {
        let (sender, _) = broadcast::channel(capacity);
        Self {
            sender,
            seq: AtomicU64::new(0),
            state_version: Arc::new(RwLock::new(StateVersion {
                presence: 0,
                health: 0,
            })),
        }
    }

    /// Create a new receiver that will get all future events.
    ///
    /// Each WebSocket connection should call this once after handshake.
    pub fn subscribe(&self) -> broadcast::Receiver<Arc<EventFrame>> {
        self.sender.subscribe()
    }

    /// Broadcast an event to all subscribers without updating state version.
    ///
    /// Returns the sequence number assigned to this event.
    pub fn broadcast(&self, event: &str, payload: Option<serde_json::Value>) -> u64 {
        let seq = self.seq.fetch_add(1, Ordering::Relaxed) + 1;

        let frame = Arc::new(EventFrame {
            event: event.to_owned(),
            payload,
            seq,
            state_version: None,
        });

        // send() fails only when there are zero receivers, which is fine
        let _ = self.sender.send(frame);
        seq
    }

    /// Broadcast an event and update the state version atomically.
    ///
    /// The updated [`StateVersion`] is included in the event frame so
    /// clients can track state freshness.
    pub async fn broadcast_with_state(
        &self,
        event: &str,
        payload: Option<serde_json::Value>,
        update: StateVersionUpdate,
    ) -> u64 {
        let seq = self.seq.fetch_add(1, Ordering::Relaxed) + 1;

        let new_sv = {
            let mut sv = self.state_version.write().await;
            match update {
                StateVersionUpdate::IncrementPresence => sv.presence += 1,
                StateVersionUpdate::IncrementHealth => sv.health += 1,
                StateVersionUpdate::Both => {
                    sv.presence += 1;
                    sv.health += 1;
                }
                StateVersionUpdate::Custom(custom) => *sv = custom,
            }
            *sv
        };

        let frame = Arc::new(EventFrame {
            event: event.to_owned(),
            payload,
            seq,
            state_version: Some(new_sv),
        });

        let _ = self.sender.send(frame);
        seq
    }

    /// Get a snapshot of the current state version.
    pub async fn state_version(&self) -> StateVersion {
        *self.state_version.read().await
    }

    /// Get the current sequence number (the last assigned value).
    pub fn current_seq(&self) -> u64 {
        self.seq.load(Ordering::Relaxed)
    }

    /// Number of active subscribers (receivers currently listening).
    pub fn subscriber_count(&self) -> usize {
        self.sender.receiver_count()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // -- EventBroadcaster ---------------------------------------------------

    #[test]
    fn broadcast_increments_seq() {
        let broadcaster = EventBroadcaster::new(16);
        assert_eq!(broadcaster.current_seq(), 0);

        let seq1 = broadcaster.broadcast("test.event", None);
        assert_eq!(seq1, 1);
        assert_eq!(broadcaster.current_seq(), 1);

        let seq2 = broadcaster.broadcast("test.event", None);
        assert_eq!(seq2, 2);
        assert_eq!(broadcaster.current_seq(), 2);
    }

    #[tokio::test]
    async fn subscriber_receives_broadcast() {
        let broadcaster = EventBroadcaster::new(16);
        let mut rx = broadcaster.subscribe();

        broadcaster.broadcast("presence.update", Some(json!({"user": "alice"})));

        let event = rx.recv().await.expect("should receive event");
        assert_eq!(event.event, "presence.update");
        assert_eq!(event.payload, Some(json!({"user": "alice"})));
        assert_eq!(event.seq, 1);
        assert!(event.state_version.is_none());
    }

    #[tokio::test]
    async fn multiple_subscribers_receive_same_event() {
        let broadcaster = EventBroadcaster::new(16);
        let mut rx1 = broadcaster.subscribe();
        let mut rx2 = broadcaster.subscribe();
        let mut rx3 = broadcaster.subscribe();

        broadcaster.broadcast("test.multi", Some(json!(42)));

        let e1 = rx1.recv().await.unwrap();
        let e2 = rx2.recv().await.unwrap();
        let e3 = rx3.recv().await.unwrap();

        // All three should receive the exact same Arc
        assert!(Arc::ptr_eq(&e1, &e2));
        assert!(Arc::ptr_eq(&e2, &e3));
        assert_eq!(e1.event, "test.multi");
    }

    #[tokio::test]
    async fn state_version_increments_presence() {
        let broadcaster = EventBroadcaster::new(16);
        let mut rx = broadcaster.subscribe();

        broadcaster
            .broadcast_with_state(
                "presence.update",
                None,
                StateVersionUpdate::IncrementPresence,
            )
            .await;

        let event = rx.recv().await.unwrap();
        let sv = event.state_version.unwrap();
        assert_eq!(sv.presence, 1);
        assert_eq!(sv.health, 0);

        let current = broadcaster.state_version().await;
        assert_eq!(current.presence, 1);
        assert_eq!(current.health, 0);
    }

    #[tokio::test]
    async fn state_version_increments_health() {
        let broadcaster = EventBroadcaster::new(16);
        let mut rx = broadcaster.subscribe();

        broadcaster
            .broadcast_with_state(
                "health.update",
                None,
                StateVersionUpdate::IncrementHealth,
            )
            .await;

        let event = rx.recv().await.unwrap();
        let sv = event.state_version.unwrap();
        assert_eq!(sv.presence, 0);
        assert_eq!(sv.health, 1);
    }

    #[tokio::test]
    async fn state_version_increments_both() {
        let broadcaster = EventBroadcaster::new(16);

        broadcaster
            .broadcast_with_state("full.update", None, StateVersionUpdate::Both)
            .await;

        let sv = broadcaster.state_version().await;
        assert_eq!(sv.presence, 1);
        assert_eq!(sv.health, 1);
    }

    #[tokio::test]
    async fn state_version_custom() {
        let broadcaster = EventBroadcaster::new(16);

        let custom = StateVersion {
            presence: 100,
            health: 200,
        };
        broadcaster
            .broadcast_with_state(
                "reset",
                None,
                StateVersionUpdate::Custom(custom),
            )
            .await;

        let sv = broadcaster.state_version().await;
        assert_eq!(sv.presence, 100);
        assert_eq!(sv.health, 200);
    }

    #[test]
    fn subscriber_count_tracks_receivers() {
        let broadcaster = EventBroadcaster::new(16);
        assert_eq!(broadcaster.subscriber_count(), 0);

        let _rx1 = broadcaster.subscribe();
        assert_eq!(broadcaster.subscriber_count(), 1);

        let _rx2 = broadcaster.subscribe();
        assert_eq!(broadcaster.subscriber_count(), 2);

        drop(_rx1);
        assert_eq!(broadcaster.subscriber_count(), 1);

        drop(_rx2);
        assert_eq!(broadcaster.subscriber_count(), 0);
    }

    #[test]
    fn broadcast_with_no_subscribers_succeeds() {
        let broadcaster = EventBroadcaster::new(16);
        // Should not panic even with zero subscribers
        let seq = broadcaster.broadcast("orphan.event", None);
        assert_eq!(seq, 1);
    }

    // -- EventFilter --------------------------------------------------------

    #[test]
    fn filter_all_matches_everything() {
        let filter = EventFilter::all();
        assert!(filter.matches("presence.update"));
        assert!(filter.matches("health.update"));
        assert!(filter.matches("anything.else"));
    }

    #[test]
    fn filter_only_includes_specified() {
        let filter = EventFilter::only(&["presence.update", "health.update"]);
        assert!(filter.matches("presence.update"));
        assert!(filter.matches("health.update"));
        assert!(!filter.matches("config.changed"));
        assert!(!filter.matches("channel.status"));
    }

    #[test]
    fn filter_except_excludes_specified() {
        let filter = EventFilter::except(&["config.changed"]);
        assert!(filter.matches("presence.update"));
        assert!(filter.matches("health.update"));
        assert!(!filter.matches("config.changed"));
    }

    // -- Built-in event constants -------------------------------------------

    #[test]
    fn builtin_event_constants() {
        assert_eq!(event_types::PRESENCE_UPDATE, "presence.update");
        assert_eq!(event_types::HEALTH_UPDATE, "health.update");
        assert_eq!(event_types::CHANNEL_STATUS, "channel.status");
        assert_eq!(event_types::CONFIG_CHANGED, "config.changed");
    }

    // -- Sequence + state version integration --------------------------------

    #[tokio::test]
    async fn broadcast_and_broadcast_with_state_share_seq() {
        let broadcaster = EventBroadcaster::new(16);

        let s1 = broadcaster.broadcast("plain", None);
        let s2 = broadcaster
            .broadcast_with_state("stateful", None, StateVersionUpdate::IncrementPresence)
            .await;
        let s3 = broadcaster.broadcast("plain2", None);

        assert_eq!(s1, 1);
        assert_eq!(s2, 2);
        assert_eq!(s3, 3);
    }
}
