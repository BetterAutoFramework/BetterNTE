//! Frame pool — single-producer, multi-consumer latest-frame broadcast.
//!
//! The capture loop pushes frames at a fixed FPS. Each consumer (trigger, script)
//! independently reads the latest available frame via [`FramePool::wait_latest`].
//! Slow consumers automatically skip stale frames — they always see the most
//! recent state, which is what automation logic cares about.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use betternte_core::CaptureFrame;
use tokio::sync::Notify;

/// Shared frame pool for decoupled capture ↔ consumption.
///
/// # Design
/// - **Single producer**: capture loop calls [`FramePool::push`] each frame.
/// - **Multiple consumers**: each trigger/script calls [`FramePool::wait_latest`].
/// - **Latest-only**: only the most recent frame is retained. Slow consumers
///   automatically skip intermediate frames.
/// - **Zero-copy**: frame pixel data is `Arc<Vec<u8>>`, so `clone()` is O(1).
#[derive(Clone)]
pub struct FramePool {
    inner: Arc<FramePoolInner>,
}

struct FramePoolInner {
    /// The most recent frame (replaced on every push).
    frame: std::sync::Mutex<Option<CaptureFrame>>,
    /// Sequence number of the latest frame.
    latest_sequence: AtomicU64,
    /// Notified on every push so consumers can wake up.
    notify: Notify,
}

impl FramePool {
    /// Create a new empty frame pool.
    pub fn new() -> Self {
        Self {
            inner: Arc::new(FramePoolInner {
                frame: std::sync::Mutex::new(None),
                latest_sequence: AtomicU64::new(0),
                notify: Notify::new(),
            }),
        }
    }

    /// Push a new frame into the pool (producer side).
    ///
    /// Replaces the previous frame and wakes all waiting consumers.
    pub fn push(&self, frame: CaptureFrame) {
        let seq = frame.sequence;
        {
            let mut guard = self.inner.frame.lock().unwrap();
            *guard = Some(frame);
        }
        self.inner.latest_sequence.store(seq, Ordering::Release);
        self.inner.notify.notify_waiters();
    }

    /// Get the latest frame without waiting (non-blocking).
    ///
    /// Returns `None` if no frame has been pushed yet.
    pub fn latest(&self) -> Option<CaptureFrame> {
        self.inner.frame.lock().unwrap().clone()
    }

    /// Wait for and return the latest frame (blocking/async).
    ///
    /// If a frame is already available, returns it immediately.
    /// Otherwise waits until the next [`FramePool::push`].
    ///
    /// **Note**: If multiple frames arrive while no consumer is waiting,
    /// only the latest one is returned — intermediate frames are skipped.
    pub async fn wait_latest(&self) -> CaptureFrame {
        loop {
            let notified = self.inner.notify.notified();
            tokio::pin!(notified);

            // Check first (avoids missed notification race).
            if let Some(frame) = self.latest() {
                return frame;
            }
            // No frame yet — wait for the next push.
            notified.await;
        }
    }

    /// Wait for a frame newer than `after_sequence`.
    ///
    /// Useful for consumers that want to process every frame without skipping.
    /// Returns the latest frame whose sequence > `after_sequence`.
    pub async fn wait_newer_than(&self, after_sequence: u64) -> CaptureFrame {
        loop {
            let notified = self.inner.notify.notified();
            tokio::pin!(notified);

            if let Some(frame) = self.latest() {
                if frame.sequence > after_sequence {
                    return frame;
                }
            }
            notified.await;
        }
    }

    /// Get the sequence number of the latest frame.
    pub fn latest_sequence(&self) -> u64 {
        self.inner.latest_sequence.load(Ordering::Acquire)
    }
}

impl Default for FramePool {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use betternte_core::PixelFormat;

    fn make_frame(seq: u64) -> CaptureFrame {
        let mut f = CaptureFrame::new(
            100,
            100,
            vec![0u8; 100 * 100 * 4],
            PixelFormat::Bgra,
            "test".into(),
        );
        f.sequence = seq;
        f
    }

    #[test]
    fn test_push_and_latest() {
        let pool = FramePool::new();
        assert!(pool.latest().is_none());

        pool.push(make_frame(1));
        let f = pool.latest().unwrap();
        assert_eq!(f.sequence, 1);

        pool.push(make_frame(2));
        let f = pool.latest().unwrap();
        assert_eq!(f.sequence, 2);
    }

    #[tokio::test]
    async fn test_wait_latest_returns_immediately_if_available() {
        let pool = FramePool::new();
        pool.push(make_frame(5));

        let f = pool.wait_latest().await;
        assert_eq!(f.sequence, 5);
    }

    #[tokio::test]
    async fn test_wait_latest_blocks_until_push() {
        let pool = FramePool::new();
        let pool_clone = pool.clone();

        let handle = tokio::spawn(async move {
            tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
            pool_clone.push(make_frame(42));
        });

        let f = pool.wait_latest().await;
        assert_eq!(f.sequence, 42);
        handle.await.unwrap();
    }

    #[tokio::test]
    async fn test_slow_consumer_skips_frames() {
        let pool = FramePool::new();

        // Push 3 frames rapidly
        pool.push(make_frame(1));
        pool.push(make_frame(2));
        pool.push(make_frame(3));

        // Consumer only sees the latest
        let f = pool.wait_latest().await;
        assert_eq!(f.sequence, 3);
    }

    #[tokio::test]
    async fn test_wait_newer_than() {
        let pool = FramePool::new();
        pool.push(make_frame(10));

        // Already newer than 5
        let f = pool.wait_newer_than(5).await;
        assert_eq!(f.sequence, 10);

        // Not newer than 10 — need to wait
        let pool_clone = pool.clone();
        let handle = tokio::spawn(async move {
            tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
            pool_clone.push(make_frame(11));
        });

        let f = pool.wait_newer_than(10).await;
        assert_eq!(f.sequence, 11);
        handle.await.unwrap();
    }
}
