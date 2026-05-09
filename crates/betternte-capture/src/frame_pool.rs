//! Frame buffer pool for reusing pixel buffers across captures.
//!
//! Instead of allocating a new `Vec<u8>` for every frame, the pool recycles
//! buffers of matching size.  This dramatically reduces allocator pressure in
//! the hot capture loop (especially on Windows where each frame is 1920×1080×4
//! ≈ 8 MiB).

use std::collections::VecDeque;
use std::sync::Mutex;

/// A thread-safe pool of reusable pixel buffers.
///
/// Buffers are keyed by their capacity in bytes.  When a buffer is returned
/// its length is reset to 0 but the capacity is preserved so the next
/// consumer can `set_len` without re-allocating.
pub struct FramePool {
    inner: Mutex<PoolInner>,
}

struct PoolInner {
    /// Free buffers organised by capacity bucket.
    /// Each bucket is a small ring-buffer so we don't hoard unlimited memory.
    buckets: Vec<(usize, VecDeque<Vec<u8>>)>,
    /// Maximum number of buffers kept per capacity bucket.
    max_per_bucket: usize,
}

impl FramePool {
    /// Create a new pool.
    ///
    /// `max_per_bucket` controls how many free buffers of each size are kept.
    /// A value of 2–4 is usually enough for double/triple buffering.
    pub fn new(max_per_bucket: usize) -> Self {
        Self {
            inner: Mutex::new(PoolInner {
                buckets: Vec::new(),
                max_per_bucket: max_per_bucket.max(1),
            }),
        }
    }

    /// Obtain a buffer with *at least* `min_bytes` capacity.
    ///
    /// If the pool has a matching buffer it is returned (with length reset to
    /// 0); otherwise a freshly allocated buffer is returned.
    pub fn acquire(&self, min_bytes: usize) -> Vec<u8> {
        let mut inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());

        // Find a bucket whose capacity >= min_bytes.
        if let Some((_, queue)) = inner
            .buckets
            .iter_mut()
            .find(|(cap, q)| *cap >= min_bytes && !q.is_empty())
        {
            if let Some(mut buf) = queue.pop_front() {
                // Safety: we only hand out buffers whose content was fully
                // written by the capture engine; resetting len to 0 lets the
                // next writer `set_len` to the exact frame size.
                // SAFETY: The buffer's content is irrelevant – it will be
                // overwritten by the capture engine before any reads.
                unsafe { buf.set_len(0) }
                // If the capacity is much larger than requested (>2×), shrink
                // to avoid hoarding oversized buffers after resolution changes.
                if buf.capacity() > min_bytes * 2 {
                    buf.shrink_to(min_bytes);
                }
                return buf;
            }
        }

        // Nothing suitable in pool – allocate fresh.
        Vec::with_capacity(min_bytes)
    }

    /// Return a buffer to the pool for reuse.
    ///
    /// Callers **must** ensure the buffer is not used after this call.
    pub fn release(&self, mut buf: Vec<u8>) {
        let cap = buf.capacity();
        if cap == 0 {
            return;
        }
        // Clear contents to avoid holding stale pixel data.
        // SAFETY: We are about to store the buffer; zeroing is cheap insurance.
        // The actual pixel data will be overwritten on next acquire().
        unsafe { buf.set_len(0) }

        let mut inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        let max_per_bucket = inner.max_per_bucket;

        // Find or create bucket for this capacity.
        match inner.buckets.iter_mut().find(|(c, _)| *c == cap) {
            Some((_, queue)) => {
                if queue.len() < max_per_bucket {
                    queue.push_back(buf);
                }
                // else: pool is full for this bucket, drop the buffer.
            }
            None => {
                if inner.buckets.len() < 16 {
                    // Limit total number of distinct capacity buckets to avoid
                    // unbounded growth after resolution switches.
                    let mut q = VecDeque::with_capacity(max_per_bucket);
                    q.push_back(buf);
                    inner.buckets.push((cap, q));
                }
                // else: too many distinct sizes, just drop.
            }
        }
    }

    /// Clear all cached buffers.
    pub fn clear(&self) {
        let mut inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        inner.buckets.clear();
    }

    /// Number of free buffers currently cached.
    pub fn len(&self) -> usize {
        let inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        inner.buckets.iter().map(|(_, q)| q.len()).sum()
    }

    /// Whether the pool is empty.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl Default for FramePool {
    fn default() -> Self {
        Self::new(3)
    }
}

/// RAII guard that returns the buffer to the pool on drop.
///
/// Use this when you want automatic lifecycle management:
///
/// ```ignore
/// let guard = pool.acquire_guard(1920 * 1080 * 4);
/// let buf: &Vec<u8> = &guard;
/// // ... fill buffer ...
/// // buffer is automatically returned to pool when `guard` is dropped.
/// ```
pub struct PooledBuffer<'a> {
    pool: &'a FramePool,
    buf: Option<Vec<u8>>,
}

impl<'a> PooledBuffer<'a> {
    /// Create a new guard.  Prefer [`FramePool::acquire_guard`].
    pub fn new(pool: &'a FramePool, min_bytes: usize) -> Self {
        Self {
            pool,
            buf: Some(pool.acquire(min_bytes)),
        }
    }

    /// Consume the guard and return the inner buffer without returning it to
    /// the pool.  Useful when you need to store the buffer elsewhere (e.g. in
    /// a `CaptureFrame` that outlives the guard).
    pub fn into_inner(mut self) -> Vec<u8> {
        self.buf.take().unwrap_or_default()
    }
}

impl<'a> std::ops::Deref for PooledBuffer<'a> {
    type Target = Vec<u8>;
    fn deref(&self) -> &Self::Target {
        self.buf.as_ref().unwrap()
    }
}

impl<'a> std::ops::DerefMut for PooledBuffer<'a> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.buf.as_mut().unwrap()
    }
}

impl<'a> Drop for PooledBuffer<'a> {
    fn drop(&mut self) {
        if let Some(buf) = self.buf.take() {
            self.pool.release(buf);
        }
    }
}

impl FramePool {
    /// Acquire a buffer wrapped in an RAII guard that returns it on drop.
    pub fn acquire_guard(&self, min_bytes: usize) -> PooledBuffer<'_> {
        PooledBuffer::new(self, min_bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_acquire_returns_buffer_with_sufficient_capacity() {
        let pool = FramePool::new(3);
        let buf = pool.acquire(1024);
        assert!(buf.capacity() >= 1024);
        assert_eq!(buf.len(), 0);
    }

    #[test]
    fn test_release_and_reuse() {
        let pool = FramePool::new(3);
        let buf1 = pool.acquire(1024);
        let cap1 = buf1.capacity();
        pool.release(buf1);

        let buf2 = pool.acquire(1024);
        assert_eq!(buf2.capacity(), cap1);
        assert_eq!(buf2.len(), 0);
    }

    #[test]
    fn test_pool_respects_max_per_bucket() {
        let pool = FramePool::new(2);
        let b1 = pool.acquire(1024);
        let b2 = pool.acquire(1024);
        let b3 = pool.acquire(1024);

        pool.release(b1);
        pool.release(b2);
        pool.release(b3); // should be dropped (pool full)

        assert_eq!(pool.len(), 2);
    }

    #[test]
    fn test_guard_returns_on_drop() {
        let pool = FramePool::new(3);
        {
            let _guard = pool.acquire_guard(1024);
            assert_eq!(pool.len(), 0);
        }
        assert_eq!(pool.len(), 1);
    }

    #[test]
    fn test_guard_into_inner_skips_return() {
        let pool = FramePool::new(3);
        let guard = pool.acquire_guard(1024);
        let _buf = guard.into_inner();
        assert_eq!(pool.len(), 0);
    }
}
