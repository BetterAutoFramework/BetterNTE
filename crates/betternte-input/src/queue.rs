//! betternte-input/src/queue.rs
//! Input operation queue.
//!
//! Serialises input commands so concurrent producers cannot race the
//! underlying [`InputController`], and applies an optional rate limit so
//! anti-cheat heuristics don't flag artificially fast input.

use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::Result as AnyhowResult;
use tokio::sync::{mpsc, oneshot};
use tracing::warn;

use crate::error::{InputError, Result};

/// A unit of work submitted to the queue.
type BoxedJob = Box<dyn FnOnce() -> Pin<Box<dyn Future<Output = AnyhowResult<()>> + Send>> + Send>;

struct InputCommand {
    job: BoxedJob,
    response: oneshot::Sender<AnyhowResult<()>>,
}

/// Default queue depth. Producers backpressure when this is full.
const DEFAULT_QUEUE_CAPACITY: usize = 1024;

/// Input operation queue.
///
/// Construct via [`InputQueue::new`] or [`InputQueue::with_capacity`]. Submit
/// async closures with [`InputQueue::submit`]; they execute serially in a
/// dedicated tokio task. Rate-limit can be adjusted at runtime via
/// [`InputQueue::set_rate_limit`].
///
/// # Example
///
/// ```ignore
/// let queue = InputQueue::new(30);
/// queue
///     .submit({
///         let ctrl = controller.clone();
///         move || Box::pin(async move { ctrl.click(100, 200).await })
///     })
///     .await?;
/// ```
pub struct InputQueue {
    sender: mpsc::Sender<InputCommand>,
    /// Minimum interval between commands, expressed in nanoseconds.
    /// `0` disables rate limiting. Updated atomically by
    /// [`InputQueue::set_rate_limit`].
    min_interval_ns: Arc<AtomicU64>,
}

impl InputQueue {
    /// Create a new input queue with the default channel capacity (1024).
    ///
    /// `max_per_second = 0` disables rate limiting.
    pub fn new(max_per_second: u32) -> Self {
        Self::with_capacity(max_per_second, DEFAULT_QUEUE_CAPACITY)
    }

    /// Create a new input queue with an explicit channel capacity.
    pub fn with_capacity(max_per_second: u32, capacity: usize) -> Self {
        let (sender, mut receiver) = mpsc::channel::<InputCommand>(capacity.max(1));
        let min_interval_ns = Arc::new(AtomicU64::new(rate_to_ns(max_per_second)));
        let interval_for_worker = min_interval_ns.clone();

        tokio::spawn(async move {
            // Initialise as "long ago" so the first command is never delayed.
            let mut last_executed = Instant::now()
                .checked_sub(Duration::from_secs(1))
                .unwrap_or_else(Instant::now);

            while let Some(cmd) = receiver.recv().await {
                let target = Duration::from_nanos(interval_for_worker.load(Ordering::Relaxed));
                if !target.is_zero() {
                    let elapsed = last_executed.elapsed();
                    if elapsed < target {
                        tokio::time::sleep(target - elapsed).await;
                    }
                }
                last_executed = Instant::now();

                let result = (cmd.job)().await;
                if cmd.response.send(result).is_err() {
                    // Caller no longer cares about the result; nothing to do.
                }
            }
        });

        Self {
            sender,
            min_interval_ns,
        }
    }

    /// Submit an async input job to the queue, awaiting its completion.
    ///
    /// The job runs serially against any other queued work and respects the
    /// rate limit. Returns whatever the job returns; on infrastructure
    /// failures, the appropriate [`InputError`] is surfaced as `anyhow::Error`.
    pub async fn submit<F, Fut>(&self, job: F) -> AnyhowResult<()>
    where
        F: FnOnce() -> Fut + Send + 'static,
        Fut: Future<Output = AnyhowResult<()>> + Send + 'static,
    {
        let (tx, rx) = oneshot::channel();
        let boxed: BoxedJob = Box::new(move || Box::pin(job()));
        if let Err(err) = self
            .sender
            .send(InputCommand {
                job: boxed,
                response: tx,
            })
            .await
        {
            warn!(error = %err, "input queue sender failed; worker is likely down");
            return Err(InputError::WorkerTerminated.into());
        }

        rx.await
            .map_err(|_| InputError::WorkerTerminated.into())
            .and_then(|inner| inner)
    }

    /// Try to submit without blocking. Returns [`InputError::QueueFull`] if
    /// the channel buffer is full or [`InputError::WorkerTerminated`] when the
    /// background worker has stopped.
    pub fn try_submit<F, Fut>(&self, job: F) -> Result<oneshot::Receiver<AnyhowResult<()>>>
    where
        F: FnOnce() -> Fut + Send + 'static,
        Fut: Future<Output = AnyhowResult<()>> + Send + 'static,
    {
        let (tx, rx) = oneshot::channel();
        let boxed: BoxedJob = Box::new(move || Box::pin(job()));
        match self.sender.try_send(InputCommand {
            job: boxed,
            response: tx,
        }) {
            Ok(()) => Ok(rx),
            Err(mpsc::error::TrySendError::Full(_)) => Err(InputError::QueueFull),
            Err(mpsc::error::TrySendError::Closed(_)) => Err(InputError::WorkerTerminated),
        }
    }

    /// Update rate limit at runtime. `max_per_second = 0` disables rate
    /// limiting. The new interval applies to the next command pulled from the
    /// queue (in-flight work is unaffected).
    pub fn set_rate_limit(&self, max_per_second: u32) {
        self.min_interval_ns
            .store(rate_to_ns(max_per_second), Ordering::Relaxed);
    }

    /// Currently configured minimum interval between commands.
    pub fn min_interval(&self) -> Duration {
        Duration::from_nanos(self.min_interval_ns.load(Ordering::Relaxed))
    }
}

fn rate_to_ns(max_per_second: u32) -> u64 {
    if max_per_second == 0 {
        0
    } else {
        // 1e9 ns / max_per_second, saturating at u64::MAX.
        let denom = u64::from(max_per_second).max(1);
        1_000_000_000u64 / denom
    }
}
