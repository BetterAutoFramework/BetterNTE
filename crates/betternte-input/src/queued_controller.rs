//! Queue-backed input controller wrapper.
//!
//! Wraps any initialized [`InputController`] and serializes all input actions
//! through [`InputQueue`], applying optional rate limiting.

use std::sync::Arc;
use std::time::Duration;

use anyhow::Result as AnyhowResult;
use async_trait::async_trait;

use crate::controller::InputController;
use crate::key::{Key, MouseButton};
use crate::queue::InputQueue;
use crate::target::InputTarget;
use crate::InputMode;

/// An [`InputController`] wrapper that routes all action methods through an
/// [`InputQueue`] to enforce FIFO ordering and rate limits.
///
/// The wrapped controller must already be initialized before constructing this
/// wrapper.
pub struct QueuedInputController {
    inner: Arc<dyn InputController>,
    queue: InputQueue,
}

impl std::fmt::Debug for QueuedInputController {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("QueuedInputController")
            .field("inner_name", &self.inner.name())
            .field("min_interval", &self.queue.min_interval())
            .finish()
    }
}

impl QueuedInputController {
    /// Wrap an already initialized input controller and apply queue-based rate
    /// limiting (`rate_limit = ops/sec`, `0 = unlimited`).
    pub fn from_initialized(inner: Box<dyn InputController>, rate_limit: u32) -> Self {
        let inner = Arc::<dyn InputController>::from(inner);
        let queue = InputQueue::new(rate_limit);
        Self { inner, queue }
    }

    /// Update queue rate limit (`ops/sec`, `0 = unlimited`) at runtime.
    pub fn set_rate_limit(&self, rate_limit: u32) {
        self.queue.set_rate_limit(rate_limit);
    }

    /// Returns the current minimum interval enforced by the underlying queue.
    pub fn min_interval(&self) -> Duration {
        self.queue.min_interval()
    }

    /// Whether queue-level throttling is currently active.
    pub fn is_rate_limited(&self) -> bool {
        !self.min_interval().is_zero()
    }

    fn inner_name(&self) -> &str {
        self.inner.name()
    }
}

#[async_trait]
impl InputController for QueuedInputController {
    fn name(&self) -> &str {
        "QueuedInput"
    }

    fn parse_key(&self, name: &str) -> Option<Key> {
        self.inner.parse_key(name)
    }

    async fn init(&mut self, _target: &InputTarget) -> AnyhowResult<()> {
        Err(anyhow::anyhow!(
            "QueuedInputController wraps an already initialized controller ({}) and cannot be re-initialized",
            self.inner_name()
        ))
    }

    async fn mouse_move(&self, x: i32, y: i32) -> AnyhowResult<()> {
        let inner = self.inner.clone();
        self.queue
            .submit(move || async move { inner.mouse_move(x, y).await })
            .await
    }

    async fn click(&self, x: i32, y: i32) -> AnyhowResult<()> {
        let inner = self.inner.clone();
        self.queue
            .submit(move || async move { inner.click(x, y).await })
            .await
    }

    async fn double_click(&self, x: i32, y: i32) -> AnyhowResult<()> {
        let inner = self.inner.clone();
        self.queue
            .submit(move || async move { inner.double_click(x, y).await })
            .await
    }

    async fn right_click(&self, x: i32, y: i32) -> AnyhowResult<()> {
        let inner = self.inner.clone();
        self.queue
            .submit(move || async move { inner.right_click(x, y).await })
            .await
    }

    async fn mouse_down(&self, button: MouseButton) -> AnyhowResult<()> {
        let inner = self.inner.clone();
        self.queue
            .submit(move || async move { inner.mouse_down(button).await })
            .await
    }

    async fn mouse_up(&self, button: MouseButton) -> AnyhowResult<()> {
        let inner = self.inner.clone();
        self.queue
            .submit(move || async move { inner.mouse_up(button).await })
            .await
    }

    async fn mouse_scroll(&self, delta: i32) -> AnyhowResult<()> {
        let inner = self.inner.clone();
        self.queue
            .submit(move || async move { inner.mouse_scroll(delta).await })
            .await
    }

    async fn swipe(
        &self,
        x1: i32,
        y1: i32,
        x2: i32,
        y2: i32,
        duration_ms: u32,
    ) -> AnyhowResult<()> {
        let inner = self.inner.clone();
        self.queue
            .submit(move || async move { inner.swipe(x1, y1, x2, y2, duration_ms).await })
            .await
    }

    async fn key_press(&self, key: Key) -> AnyhowResult<()> {
        let inner = self.inner.clone();
        self.queue
            .submit(move || async move { inner.key_press(key).await })
            .await
    }

    async fn key_release(&self, key: Key) -> AnyhowResult<()> {
        let inner = self.inner.clone();
        self.queue
            .submit(move || async move { inner.key_release(key).await })
            .await
    }

    async fn key_tap(&self, key: Key, duration_ms: Option<u32>) -> AnyhowResult<()> {
        let inner = self.inner.clone();
        self.queue
            .submit(move || async move { inner.key_tap(key, duration_ms).await })
            .await
    }

    async fn type_text(&self, text: &str) -> AnyhowResult<()> {
        let inner = self.inner.clone();
        let text_owned = text.to_string();
        self.queue
            .submit(move || async move { inner.type_text(&text_owned).await })
            .await
    }

    async fn key_combo(&self, keys: &[Key]) -> AnyhowResult<()> {
        let inner = self.inner.clone();
        let keys_owned = keys.to_vec();
        self.queue
            .submit(move || async move { inner.key_combo(&keys_owned).await })
            .await
    }

    fn supports_background(&self) -> bool {
        self.inner.supports_background()
    }

    fn last_latency_ms(&self) -> Option<f64> {
        self.inner.last_latency_ms()
    }

    fn mode(&self) -> InputMode {
        self.inner.mode()
    }
}

#[cfg(test)]
mod queued_controller_tests {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, Instant};

    use anyhow::Result as AnyhowResult;
    use async_trait::async_trait;
    use tokio::sync::oneshot;

    use super::QueuedInputController;
    use crate::controller::InputController;
    use crate::key::{Key, MouseButton};
    use crate::target::InputTarget;
    use crate::InputMode;

    #[derive(Debug, Clone)]
    struct QueueProbeController {
        calls: Arc<Mutex<Vec<i32>>>,
        call_at: Arc<Mutex<Vec<Instant>>>,
        in_flight: Arc<AtomicUsize>,
        max_in_flight: Arc<AtomicUsize>,
    }

    impl QueueProbeController {
        fn new() -> Self {
            Self {
                calls: Arc::new(Mutex::new(Vec::new())),
                call_at: Arc::new(Mutex::new(Vec::new())),
                in_flight: Arc::new(AtomicUsize::new(0)),
                max_in_flight: Arc::new(AtomicUsize::new(0)),
            }
        }

        fn calls(&self) -> Vec<i32> {
            self.calls.lock().map(|g| g.clone()).unwrap_or_default()
        }

        fn call_at(&self) -> Vec<Instant> {
            self.call_at.lock().map(|g| g.clone()).unwrap_or_default()
        }

        fn max_in_flight(&self) -> usize {
            self.max_in_flight.load(Ordering::SeqCst)
        }

        fn mark_enter(&self) {
            let cur = self.in_flight.fetch_add(1, Ordering::SeqCst) + 1;
            let _ = self
                .max_in_flight
                .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |prev| {
                    Some(prev.max(cur))
                });
        }

        fn mark_exit(&self) {
            let _ = self.in_flight.fetch_sub(1, Ordering::SeqCst);
        }
    }

    #[async_trait]
    impl InputController for QueueProbeController {
        fn name(&self) -> &str {
            "QueueProbe"
        }

        async fn init(&mut self, _target: &InputTarget) -> AnyhowResult<()> {
            Ok(())
        }

        async fn mouse_move(&self, _x: i32, _y: i32) -> AnyhowResult<()> {
            Ok(())
        }

        async fn click(&self, x: i32, _y: i32) -> AnyhowResult<()> {
            self.mark_enter();
            if let Ok(mut ts) = self.call_at.lock() {
                ts.push(Instant::now());
            }
            if let Ok(mut calls) = self.calls.lock() {
                calls.push(x);
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
            self.mark_exit();
            Ok(())
        }

        async fn double_click(&self, x: i32, y: i32) -> AnyhowResult<()> {
            self.click(x, y).await?;
            self.click(x, y).await
        }

        async fn right_click(&self, x: i32, y: i32) -> AnyhowResult<()> {
            self.click(x, y).await
        }

        async fn mouse_down(&self, _button: MouseButton) -> AnyhowResult<()> {
            Ok(())
        }

        async fn mouse_up(&self, _button: MouseButton) -> AnyhowResult<()> {
            Ok(())
        }

        async fn mouse_scroll(&self, _delta: i32) -> AnyhowResult<()> {
            Ok(())
        }

        async fn swipe(
            &self,
            x1: i32,
            y1: i32,
            _x2: i32,
            _y2: i32,
            _duration_ms: u32,
        ) -> AnyhowResult<()> {
            self.click(x1, y1).await
        }

        async fn key_press(&self, _key: Key) -> AnyhowResult<()> {
            Ok(())
        }

        async fn key_release(&self, _key: Key) -> AnyhowResult<()> {
            Ok(())
        }

        async fn key_tap(&self, _key: Key, _duration_ms: Option<u32>) -> AnyhowResult<()> {
            Ok(())
        }

        async fn type_text(&self, _text: &str) -> AnyhowResult<()> {
            Ok(())
        }

        async fn key_combo(&self, _keys: &[Key]) -> AnyhowResult<()> {
            Ok(())
        }

        fn supports_background(&self) -> bool {
            true
        }

        fn last_latency_ms(&self) -> Option<f64> {
            None
        }

        fn mode(&self) -> InputMode {
            InputMode::Foreground
        }
    }

    #[tokio::test]
    async fn queue_is_effective_and_serializes_concurrent_calls() {
        let probe = QueueProbeController::new();
        let queued = Arc::new(QueuedInputController::from_initialized(
            Box::new(probe.clone()),
            0,
        ));

        let mut handles = Vec::new();
        for i in 0..10 {
            let q = queued.clone();
            handles.push(tokio::spawn(async move { q.click(i, 0).await }));
        }
        for h in handles {
            h.await
                .expect("join should succeed")
                .expect("click should succeed");
        }

        assert_eq!(
            probe.max_in_flight(),
            1,
            "queued controller should serialize concurrent submissions",
        );
        assert_eq!(
            probe.calls().len(),
            10,
            "all calls should reach inner controller"
        );
    }

    #[tokio::test]
    async fn queue_preserves_submission_order_under_concurrent_submission() {
        let probe = QueueProbeController::new();
        let queued = Arc::new(QueuedInputController::from_initialized(
            Box::new(probe.clone()),
            0,
        ));

        let mut releases = Vec::new();
        let mut handles = Vec::new();
        for i in 0..5 {
            let (tx, rx) = oneshot::channel::<()>();
            releases.push(tx);
            let q = queued.clone();
            handles.push(tokio::spawn(async move {
                let _ = rx.await;
                q.click(i, 0).await
            }));
        }
        for tx in releases {
            let _ = tx.send(());
            tokio::time::sleep(Duration::from_millis(2)).await;
        }
        for h in handles {
            h.await
                .expect("join should succeed")
                .expect("click should succeed");
        }

        assert_eq!(
            probe.calls(),
            vec![0, 1, 2, 3, 4],
            "queued controller should execute in FIFO submission order",
        );
    }

    #[tokio::test]
    async fn queue_preserves_submission_order_under_sequential_submission() {
        let probe = QueueProbeController::new();
        let queued = QueuedInputController::from_initialized(Box::new(probe.clone()), 0);

        for i in 0..5 {
            queued.click(i, 0).await.expect("click should succeed");
        }

        assert_eq!(
            probe.calls(),
            vec![0, 1, 2, 3, 4],
            "sequential submissions should keep their original order",
        );
    }

    #[tokio::test]
    async fn queue_rate_limit_assertion() {
        let probe = QueueProbeController::new();
        let queued = QueuedInputController::from_initialized(Box::new(probe.clone()), 20);
        for i in 0..3 {
            queued.click(i, 0).await.expect("click should succeed");
        }

        let times = probe.call_at();
        assert!(times.len() >= 3, "should record all call timestamps");
        let d1 = times[1].duration_since(times[0]);
        let d2 = times[2].duration_since(times[1]);
        assert!(
            d1 >= Duration::from_millis(40),
            "first interval should be rate-limited (>=40ms), got {:?}",
            d1
        );
        assert!(
            d2 >= Duration::from_millis(40),
            "second interval should be rate-limited (>=40ms), got {:?}",
            d2
        );
    }

    #[tokio::test]
    async fn queue_rate_limit_update_takes_effect_at_runtime() {
        let probe = QueueProbeController::new();
        let queued = QueuedInputController::from_initialized(Box::new(probe.clone()), 0);
        assert!(!queued.is_rate_limited());
        assert_eq!(queued.min_interval(), Duration::ZERO);

        // Baseline without throttling.
        let start = Instant::now();
        for i in 0..5 {
            queued.click(i, 0).await.expect("click should succeed");
        }
        let elapsed_unlimited = start.elapsed();

        queued.set_rate_limit(25); // ~40ms target interval
        assert!(queued.is_rate_limited());
        assert!(queued.min_interval() >= Duration::from_millis(40));
        let start = Instant::now();
        for i in 10..15 {
            queued.click(i, 0).await.expect("click should succeed");
        }
        let elapsed_with_limit = start.elapsed();
        assert!(
            elapsed_with_limit >= Duration::from_millis(200),
            "with 25 ops/s, throttled run should take >=200ms, got {:?}",
            elapsed_with_limit
        );
        assert!(
            elapsed_with_limit > elapsed_unlimited + Duration::from_millis(60),
            "runtime rate limit update should slow queue: limited={:?}, unlimited={:?}",
            elapsed_with_limit,
            elapsed_unlimited
        );

        queued.set_rate_limit(0); // remove throttle
        assert!(!queued.is_rate_limited());
        assert_eq!(queued.min_interval(), Duration::ZERO);
        let start = Instant::now();
        for i in 20..25 {
            queued.click(i, 0).await.expect("click should succeed");
        }
        let elapsed_after_reset = start.elapsed();
        assert!(
            elapsed_after_reset + Duration::from_millis(20) < elapsed_with_limit,
            "after removing rate limit, throughput should recover: reset={:?}, limited={:?}",
            elapsed_after_reset,
            elapsed_with_limit
        );
    }
}
