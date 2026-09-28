//! The session controller's drain (DT§4.1, §4.5): a tokio task reads the
//! core's bounded event channel continuously, folds each event into the
//! timeline and queues the patches in the coalescing buffer; the UI pulls
//! them when it is ready. Separate from the fold so `timeline.rs` stays sync
//! and pure, and so a slow UI can never make the core wait on `emit`.

use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use cox_protocol::types::Event;
use tokio::sync::{Notify, mpsc};
use tokio::task::JoinHandle;
use tokio::time::Instant;

use crate::coalesce;
use crate::patch::{Block, TimelinePatch};
use crate::timeline::Timeline;
use crate::usage::Meter;

/// A pull hands back at most one batch per frame (DT§4.1), so the UI hops
/// to its main thread at most once per frame…
pub const FRAME: Duration = Duration::from_millis(16);
/// …unless this many patches are already queued.
pub const MAX_BATCH: usize = 64;

/// One open session's timeline, fed by its own drain task.
pub struct Controller {
    shared: Arc<Shared>,
    drain: JoinHandle<()>,
}

struct Shared {
    state: Mutex<State>,
    /// Signalled whenever patches are queued or the stream ends.
    ready: Notify,
}

struct State {
    timeline: Timeline,
    meter: Meter,
    /// The meter's clock origin; tokio's, so a paused-time test scripts it.
    opened: Instant,
    queue: Vec<TimelinePatch>,
    closed: bool,
    last_pull: Option<Instant>,
}

impl Shared {
    /// A panic elsewhere cannot leave the fold half-applied (`apply` takes
    /// the lock once per event), so a poisoned lock is still usable.
    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

impl Controller {
    /// Starts draining `events` into `timeline`. Must be called within a
    /// tokio runtime.
    pub fn spawn(timeline: Timeline, mut events: mpsc::Receiver<Event>) -> Self {
        let shared = Arc::new(Shared {
            state: Mutex::new(State {
                timeline,
                meter: Meter::default(),
                opened: Instant::now(),
                queue: Vec::new(),
                closed: false,
                last_pull: None,
            }),
            ready: Notify::new(),
        });
        let feed = Arc::clone(&shared);
        let drain = tokio::spawn(async move {
            while let Some(event) = events.recv().await {
                let queued = {
                    let mut state = feed.lock();
                    let mut patches = state.timeline.apply(&event);
                    let now = state.opened.elapsed();
                    if state.meter.apply(&event, now) {
                        let usage = Box::new(state.meter.view().clone());
                        patches.push(TimelinePatch::Usage { usage });
                    }
                    let queued = !patches.is_empty();
                    for patch in patches {
                        coalesce::push(&mut state.queue, patch);
                    }
                    queued
                };
                if queued {
                    feed.ready.notify_one();
                }
            }
            feed.lock().closed = true;
            feed.ready.notify_one();
        });
        Self { shared, drain }
    }

    /// The whole block list. Patches queued so far are already in it, so
    /// they are dropped — all but the meter's, which is not: the next pull
    /// continues from this state.
    pub fn snapshot(&self) -> Vec<Block> {
        let mut state = self.shared.lock();
        state
            .queue
            .retain(|p| matches!(p, TimelinePatch::Usage { .. }));
        state.timeline.blocks().to_vec()
    }

    /// The next coalesced batch, waiting until there is one; `None` once the
    /// event stream has ended (or `close` was called) and everything was
    /// pulled.
    pub async fn next_patches(&self) -> Option<Vec<TimelinePatch>> {
        loop {
            // Created before the check: a `notify_one` in between leaves a
            // permit, so no wake-up is lost.
            let ready = self.shared.ready.notified();
            {
                let state = self.shared.lock();
                if !state.queue.is_empty() {
                    break;
                }
                if state.closed {
                    return None;
                }
            }
            ready.await;
        }
        let due = {
            let state = self.shared.lock();
            state
                .last_pull
                .filter(|_| state.queue.len() < MAX_BATCH)
                .map(|last| last + FRAME)
        };
        if let Some(due) = due {
            // The queue keeps coalescing meanwhile.
            tokio::time::sleep_until(due).await;
        }
        let mut state = self.shared.lock();
        state.last_pull = Some(Instant::now());
        Some(std::mem::take(&mut state.queue))
    }

    /// Stops draining; a pending pull returns what is queued, then `None`.
    /// The session itself keeps running (DT§4.5 Cancellation).
    pub fn close(&self) {
        self.drain.abort();
        self.shared.lock().closed = true;
        self.shared.ready.notify_one();
    }
}

impl Drop for Controller {
    fn drop(&mut self) {
        self.drain.abort();
    }
}

#[cfg(test)]
mod tests {
    use cox_protocol::ids::ItemId;
    use cox_protocol::types::{ItemKind, Level};

    use super::*;

    fn notice(text: &str) -> Event {
        Event::ItemStarted {
            item: ItemId::new(),
            kind: ItemKind::Notice {
                level: Level::Info,
                text: text.into(),
            },
        }
    }

    #[tokio::test(start_paused = true)]
    async fn pulls_batch_per_frame_and_end_after_the_stream_closes() {
        let (tx, rx) = mpsc::channel(4);
        let controller = Controller::spawn(Timeline::default(), rx);
        tx.send(notice("a")).await.expect("send");
        let first = controller.next_patches().await.expect("open");
        assert_eq!(first.len(), 1);
        let pulled = Instant::now();
        tx.send(notice("b")).await.expect("send");
        tx.send(notice("c")).await.expect("send");
        drop(tx);
        let second = controller
            .next_patches()
            .await
            .expect("queued before close");
        assert!(
            pulled.elapsed() >= FRAME,
            "a second pull waits out the frame"
        );
        assert_eq!(second.len(), 2);
        assert_eq!(controller.next_patches().await, None);
        assert_eq!(controller.snapshot().len(), 3);
    }
}
