//! Frame ring buffer for sharing frames between capture engine and consumers.

use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use betternte_core::CaptureFrame;
use tokio::sync::Notify;

/// Frame ring buffer.
///
/// Used to share frame data between the capture engine and multiple consumers
/// (triggers, overlays). Frames are stored in a fixed-size circular buffer.
pub struct FrameRingBuffer {
    /// Buffer storage (slots that may contain a frame)
    buffer: Vec<Mutex<Option<CaptureFrame>>>,
    /// Current write position
    write_pos: AtomicUsize,
    /// Buffer capacity
    capacity: usize,
    /// Latest frame sequence number
    latest_sequence: AtomicU64,
    /// Whether at least one frame has been written
    has_frames: AtomicUsize,
    /// Notification for new frames
    notify: Arc<Notify>,
}

impl FrameRingBuffer {
    /// Create a new frame ring buffer with the given capacity.
    pub fn new(capacity: usize) -> Self {
        let buffer: Vec<_> = (0..capacity).map(|_| Mutex::new(None)).collect();
        Self {
            buffer,
            write_pos: AtomicUsize::new(0),
            capacity,
            latest_sequence: AtomicU64::new(0),
            has_frames: AtomicUsize::new(0),
            notify: Arc::new(Notify::new()),
        }
    }

    /// Push a frame into the buffer.
    ///
    /// This overwrites the oldest frame if the buffer is full.
    pub fn push(&self, frame: CaptureFrame) {
        let seq = frame.sequence;
        let pos = self.write_pos.fetch_add(1, Ordering::Relaxed) % self.capacity;
        if let Ok(mut slot) = self.buffer[pos].lock() {
            *slot = Some(frame);
        }
        self.latest_sequence.store(seq, Ordering::Relaxed);
        self.has_frames.store(1, Ordering::Relaxed);
        self.notify.notify_waiters();
    }

    /// Get the latest frame without waiting.
    pub fn latest(&self) -> Option<CaptureFrame> {
        if self.has_frames.load(Ordering::Relaxed) == 0 {
            return None;
        }
        let pos = self.write_pos.load(Ordering::Relaxed);
        let idx = pos.wrapping_sub(1) % self.capacity;
        self.buffer[idx].lock().ok()?.clone()
    }

    /// Get a frame by sequence number.
    pub fn get(&self, sequence: u64) -> Option<CaptureFrame> {
        let latest = self.latest_sequence.load(Ordering::Relaxed);
        if sequence > latest {
            return None;
        }
        for slot in &self.buffer {
            let guard = slot.lock().ok()?;
            if let Some(frame) = guard.as_ref() {
                if frame.sequence == sequence {
                    return Some(frame.clone());
                }
            }
        }
        None
    }

    /// Wait for and return the latest frame asynchronously.
    pub async fn wait_latest(&self) -> CaptureFrame {
        loop {
            let notified = self.notify.notified();
            tokio::pin!(notified);
            if let Some(frame) = self.latest() {
                return frame;
            }
            notified.await;
        }
    }

    /// Get the sequence number of the latest frame.
    pub fn latest_sequence(&self) -> u64 {
        self.latest_sequence.load(Ordering::Relaxed)
    }

    /// Get the buffer capacity.
    #[allow(dead_code)]
    pub fn capacity(&self) -> usize {
        self.capacity
    }

    /// Clear all frames from the buffer.
    pub fn clear(&self) {
        for slot in &self.buffer {
            if let Ok(mut guard) = slot.lock() {
                *guard = None;
            }
        }
        self.latest_sequence.store(0, Ordering::Relaxed);
        self.write_pos.store(0, Ordering::Relaxed);
        self.has_frames.store(0, Ordering::Relaxed);
    }
}

impl Default for FrameRingBuffer {
    fn default() -> Self {
        Self::new(3) // Default capacity of 3 frames
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use betternte_core::PixelFormat;

    fn make_frame() -> CaptureFrame {
        CaptureFrame::new(
            100,
            100,
            vec![0u8; 100 * 100 * 4],
            PixelFormat::Bgra,
            "test".into(),
        )
    }

    #[tokio::test]
    async fn test_ring_buffer_push_pop() {
        let buffer = FrameRingBuffer::new(3);

        // Initially empty
        assert!(buffer.latest().is_none());

        // Push frames
        buffer.push(make_frame());
        assert!(buffer.latest().is_some());

        buffer.push(make_frame());
        assert!(buffer.latest().is_some());

        // Latest should be the last pushed frame
        let latest = buffer.latest().unwrap();
        assert_eq!(latest.width, 100);
        assert_eq!(latest.height, 100);
    }

    #[tokio::test]
    async fn test_ring_buffer_overwrite() {
        let buffer = FrameRingBuffer::new(2);

        buffer.push(make_frame());
        buffer.push(make_frame());

        // Buffer is full, next push should overwrite
        buffer.push(make_frame());

        // Should still have a frame
        assert!(buffer.latest().is_some());
    }

    #[tokio::test]
    async fn test_wait_latest() {
        let buffer = Arc::new(FrameRingBuffer::new(3));
        let buffer_for_push = buffer.clone();

        // Start waiting task
        let handle = tokio::spawn({
            let buf = buffer.clone();
            async move { buf.wait_latest().await }
        });

        // Give the task time to start waiting
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

        // Push a frame
        buffer_for_push.push(make_frame());

        // Wait for the result
        let result = tokio::time::timeout(tokio::time::Duration::from_secs(1), handle)
            .await
            .unwrap()
            .unwrap();

        assert_eq!(result.width, 100);
    }
}
