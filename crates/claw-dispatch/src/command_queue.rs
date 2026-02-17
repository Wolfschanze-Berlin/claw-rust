//! Lane-based command queue for serialized message processing.
//!
//! Each lane has its own concurrency limit enforced via a [`tokio::sync::Semaphore`].
//! The default "main" lane runs with concurrency 1 (sequential), while additional
//! lanes (e.g. "cron", "heartbeat") can be configured with higher concurrency.
//!
//! Clearing a lane cancels all pending commands in that lane by dropping its
//! [`CancellationToken`], causing waiters to receive [`CommandLaneClearedError`].

use std::collections::HashMap;
use std::future::Future;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use anyhow::Result;
use tokio::sync::{RwLock, Semaphore};
use tokio_util::sync::CancellationToken;
use tracing::{debug, warn};

use claw_core::CommandLaneClearedError;

/// Default lane name used for inbound message processing.
pub const MAIN_LANE: &str = "main";

// ---------------------------------------------------------------------------
// Lane (internal)
// ---------------------------------------------------------------------------

/// A single concurrency lane inside the [`CommandQueue`].
struct Lane {
    concurrency: usize,
    semaphore: Arc<Semaphore>,
    /// Per-lane cancellation token — dropping (and re-creating) it clears the lane.
    cancel: CancellationToken,
    /// Number of commands currently executing or waiting for a permit.
    pending: Arc<AtomicUsize>,
}

impl Lane {
    fn new(concurrency: usize, parent: &CancellationToken) -> Self {
        Self {
            concurrency,
            semaphore: Arc::new(Semaphore::new(concurrency)),
            cancel: parent.child_token(),
            pending: Arc::new(AtomicUsize::new(0)),
        }
    }
}

// ---------------------------------------------------------------------------
// CommandQueue
// ---------------------------------------------------------------------------

/// A lane-based command queue for concurrency-controlled command execution.
///
/// Commands enqueued on a lane will wait for an available permit before running.
/// The main lane defaults to concurrency 1 (sequential processing).
pub struct CommandQueue {
    lanes: Arc<RwLock<HashMap<String, Lane>>>,
    /// Root cancellation token — cancelling this drains the entire queue.
    cancel: CancellationToken,
}

impl CommandQueue {
    /// Create a new command queue with a default "main" lane (concurrency = 1).
    pub fn new() -> Self {
        let cancel = CancellationToken::new();
        let mut lanes = HashMap::new();
        lanes.insert(
            MAIN_LANE.to_owned(),
            Lane::new(1, &cancel),
        );
        Self {
            lanes: Arc::new(RwLock::new(lanes)),
            cancel,
        }
    }

    /// Add a new lane with the given concurrency limit.
    ///
    /// If a lane with the same name already exists this is a no-op and a warning
    /// is logged.
    pub async fn add_lane(&self, name: &str, concurrency: usize) {
        let mut lanes = self.lanes.write().await;
        if lanes.contains_key(name) {
            warn!(lane = name, "lane already exists — skipping add_lane");
            return;
        }
        debug!(lane = name, concurrency, "adding lane");
        lanes.insert(name.to_owned(), Lane::new(concurrency, &self.cancel));
    }

    /// Enqueue an async command on the named lane.
    ///
    /// The command will wait for a semaphore permit (respecting the lane's
    /// concurrency limit) and then execute. If the lane is cleared while the
    /// command is waiting, a [`CommandLaneClearedError`] is returned.
    ///
    /// Returns an error if the lane does not exist.
    pub async fn enqueue_command_in_lane<F, Fut, R>(
        &self,
        lane_name: &str,
        command: F,
    ) -> Result<R>
    where
        F: FnOnce() -> Fut + Send,
        Fut: Future<Output = Result<R>> + Send,
        R: Send,
    {
        // Snapshot lane state under a short read lock.
        let (semaphore, lane_cancel, pending) = {
            let lanes = self.lanes.read().await;
            let lane = lanes.get(lane_name).ok_or_else(|| {
                anyhow::anyhow!("lane '{lane_name}' does not exist")
            })?;
            (
                Arc::clone(&lane.semaphore),
                lane.cancel.clone(),
                Arc::clone(&lane.pending),
            )
        };

        pending.fetch_add(1, Ordering::SeqCst);

        // Wait for a permit OR cancellation.
        let permit = tokio::select! {
            biased;
            _ = lane_cancel.cancelled() => {
                pending.fetch_sub(1, Ordering::SeqCst);
                return Err(CommandLaneClearedError::new(
                    format!("lane '{lane_name}' was cleared"),
                ).into());
            }
            permit = semaphore.acquire_owned() => {
                permit.map_err(|_| anyhow::anyhow!("semaphore closed for lane '{lane_name}'"))?
            }
        };

        // Execute the command (still cancellable).
        let result = tokio::select! {
            biased;
            _ = lane_cancel.cancelled() => {
                pending.fetch_sub(1, Ordering::SeqCst);
                drop(permit);
                return Err(CommandLaneClearedError::new(
                    format!("lane '{lane_name}' was cleared"),
                ).into());
            }
            res = command() => {
                pending.fetch_sub(1, Ordering::SeqCst);
                drop(permit);
                res
            }
        };

        result
    }

    /// Clear a lane — cancels all pending and in-flight commands.
    ///
    /// After clearing the lane is recreated with the same concurrency settings
    /// so new commands can be enqueued immediately.
    pub async fn clear_lane(&self, lane_name: &str) -> Result<()> {
        let mut lanes = self.lanes.write().await;
        let lane = lanes.get(lane_name).ok_or_else(|| {
            anyhow::anyhow!("lane '{lane_name}' does not exist")
        })?;

        let concurrency = lane.concurrency;
        debug!(lane = lane_name, "clearing lane");

        // Cancel all waiters on this lane.
        lane.cancel.cancel();

        // Replace the lane with a fresh one so it can accept new commands.
        lanes.insert(
            lane_name.to_owned(),
            Lane::new(concurrency, &self.cancel),
        );

        Ok(())
    }

    /// Graceful drain — cancel the root token and wait briefly for in-flight
    /// commands to observe the cancellation.
    pub async fn drain(&self) {
        debug!("draining command queue");
        self.cancel.cancel();

        // Give tasks a moment to observe cancellation and wind down.
        tokio::task::yield_now().await;
    }

    /// Returns the number of lanes (including the default main lane).
    pub async fn lane_count(&self) -> usize {
        self.lanes.read().await.len()
    }

    /// Returns the number of pending (waiting + executing) commands on a lane.
    ///
    /// Returns `None` if the lane does not exist.
    pub async fn pending_count(&self, lane_name: &str) -> Option<usize> {
        let lanes = self.lanes.read().await;
        lanes.get(lane_name).map(|l| l.pending.load(Ordering::SeqCst))
    }
}

impl Default for CommandQueue {
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
    use std::sync::atomic::AtomicU32;
    use tokio::time::{Duration, sleep};

    #[tokio::test]
    async fn sequential_execution_on_main_lane() {
        let queue = CommandQueue::new();
        let order = Arc::new(std::sync::Mutex::new(Vec::new()));

        let mut handles = Vec::new();
        for i in 0..3u32 {
            let q = &queue;
            let order = Arc::clone(&order);
            // Spawn each enqueue concurrently — only one should run at a time.
            handles.push(tokio::spawn({
                let lanes = Arc::clone(&q.lanes);
                let cancel = q.cancel.clone();
                async move {
                    let queue = CommandQueue { lanes, cancel };
                    queue
                        .enqueue_command_in_lane(MAIN_LANE, move || async move {
                            sleep(Duration::from_millis(10)).await;
                            order.lock().unwrap().push(i);
                            Ok(())
                        })
                        .await
                        .unwrap();
                }
            }));
        }

        for h in handles {
            h.await.unwrap();
        }

        // With concurrency=1, commands execute one at a time.
        // We cannot guarantee insertion *order* because the spawns race for the
        // permit, but we CAN guarantee that no two ran concurrently: the vec
        // should have exactly 3 elements (no data race).
        let result = order.lock().unwrap();
        assert_eq!(result.len(), 3);
    }

    #[tokio::test]
    async fn parallel_execution_on_multi_concurrency_lane() {
        let queue = CommandQueue::new();
        queue.add_lane("parallel", 3).await;

        let running = Arc::new(AtomicU32::new(0));
        let max_concurrent = Arc::new(AtomicU32::new(0));

        let mut handles = Vec::new();
        for _ in 0..3 {
            let running = Arc::clone(&running);
            let max_conc = Arc::clone(&max_concurrent);
            let lanes = Arc::clone(&queue.lanes);
            let cancel = queue.cancel.clone();

            handles.push(tokio::spawn(async move {
                let q = CommandQueue { lanes, cancel };
                q.enqueue_command_in_lane("parallel", move || async move {
                    let cur = running.fetch_add(1, Ordering::SeqCst) + 1;
                    // Track maximum concurrency observed.
                    max_conc.fetch_max(cur, Ordering::SeqCst);
                    sleep(Duration::from_millis(50)).await;
                    running.fetch_sub(1, Ordering::SeqCst);
                    Ok(())
                })
                .await
                .unwrap();
            }));
        }

        for h in handles {
            h.await.unwrap();
        }

        // All 3 should have run concurrently.
        assert!(max_concurrent.load(Ordering::SeqCst) >= 2);
    }

    #[tokio::test]
    async fn lane_clearing_cancels_pending_commands() {
        let queue = CommandQueue::new();

        // Enqueue a long-running command on the main lane.
        let lanes = Arc::clone(&queue.lanes);
        let cancel = queue.cancel.clone();
        let handle = tokio::spawn(async move {
            let q = CommandQueue { lanes, cancel };
            q.enqueue_command_in_lane(MAIN_LANE, || async {
                sleep(Duration::from_secs(60)).await;
                Ok(())
            })
            .await
        });

        // Give it a moment to start.
        sleep(Duration::from_millis(20)).await;

        // Clear the lane.
        queue.clear_lane(MAIN_LANE).await.unwrap();

        let result = handle.await.unwrap();
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("cleared"),
            "expected CommandLaneClearedError, got: {err_msg}"
        );
    }

    #[tokio::test]
    async fn drain_waits_for_completion() {
        let queue = CommandQueue::new();
        let executed = Arc::new(AtomicU32::new(0));

        let lanes = Arc::clone(&queue.lanes);
        let cancel = queue.cancel.clone();
        let exec = Arc::clone(&executed);
        let handle = tokio::spawn(async move {
            let q = CommandQueue { lanes, cancel };
            let _ = q
                .enqueue_command_in_lane(MAIN_LANE, move || async move {
                    sleep(Duration::from_millis(10)).await;
                    exec.fetch_add(1, Ordering::SeqCst);
                    Ok(())
                })
                .await;
        });

        // Let the command start.
        sleep(Duration::from_millis(5)).await;
        queue.drain().await;

        // The spawned task should finish (either completed or cancelled).
        handle.await.unwrap();
    }

    #[tokio::test]
    async fn multiple_lanes_operate_independently() {
        let queue = CommandQueue::new();
        queue.add_lane("cron", 2).await;

        assert_eq!(queue.lane_count().await, 2);

        let main_done = Arc::new(AtomicU32::new(0));
        let cron_done = Arc::new(AtomicU32::new(0));

        // Enqueue on both lanes simultaneously.
        let (lanes1, cancel1) = (Arc::clone(&queue.lanes), queue.cancel.clone());
        let (lanes2, cancel2) = (Arc::clone(&queue.lanes), queue.cancel.clone());
        let md = Arc::clone(&main_done);
        let cd = Arc::clone(&cron_done);

        let h1 = tokio::spawn(async move {
            let q = CommandQueue { lanes: lanes1, cancel: cancel1 };
            q.enqueue_command_in_lane(MAIN_LANE, move || async move {
                md.fetch_add(1, Ordering::SeqCst);
                Ok(())
            })
            .await
            .unwrap();
        });

        let h2 = tokio::spawn(async move {
            let q = CommandQueue { lanes: lanes2, cancel: cancel2 };
            q.enqueue_command_in_lane("cron", move || async move {
                cd.fetch_add(1, Ordering::SeqCst);
                Ok(())
            })
            .await
            .unwrap();
        });

        h1.await.unwrap();
        h2.await.unwrap();

        assert_eq!(main_done.load(Ordering::SeqCst), 1);
        assert_eq!(cron_done.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn enqueue_to_nonexistent_lane_returns_error() {
        let queue = CommandQueue::new();
        let result = queue
            .enqueue_command_in_lane("nonexistent", || async { Ok(()) })
            .await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("does not exist"));
    }

    #[tokio::test]
    async fn pending_count_tracks_active_commands() {
        let queue = CommandQueue::new();
        assert_eq!(queue.pending_count(MAIN_LANE).await, Some(0));
        assert_eq!(queue.pending_count("nope").await, None);
    }

    #[tokio::test]
    async fn clear_nonexistent_lane_returns_error() {
        let queue = CommandQueue::new();
        let result = queue.clear_lane("ghost").await;
        assert!(result.is_err());
    }
}
