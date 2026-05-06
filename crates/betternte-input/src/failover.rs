//! Input controller failover wrapper.
//!
//! Wraps two input backends (primary/fallback) and automatically switches
//! to fallback when primary fails repeatedly.

use std::future::Future;
use std::pin::Pin;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use anyhow::Result as AnyhowResult;
use async_trait::async_trait;
use tracing::{info, warn};

use crate::controller::InputController;
use crate::key::{Key, MouseButton};
use crate::target::InputTarget;
use crate::InputMode;

/// Extra slack added on top of an action's expected duration when computing
/// its failover timeout (e.g. swipe of 1000ms gets 1500ms timeout).
const LONG_ACTION_TIMEOUT_SLACK: Duration = Duration::from_millis(500);

#[derive(Debug, Clone)]
pub struct FailoverConfig {
    pub enabled: bool,
    pub fail_threshold: u32,
    pub probe_every: Duration,
    pub action_timeout: Duration,
    pub log_switch: bool,
}

impl Default for FailoverConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            fail_threshold: 3,
            probe_every: Duration::from_millis(3000),
            action_timeout: Duration::from_millis(300),
            log_switch: true,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ActiveBackend {
    Primary,
    Fallback,
}

#[derive(Debug)]
struct FailoverState {
    active: ActiveBackend,
    consecutive_failures: u32,
    next_probe_at: Option<Instant>,
}

pub struct FailoverInputController {
    primary: Box<dyn InputController>,
    fallback: Option<Box<dyn InputController>>,
    config: FailoverConfig,
    state: Mutex<FailoverState>,
}

impl FailoverInputController {
    pub fn new(
        primary: Box<dyn InputController>,
        fallback: Option<Box<dyn InputController>>,
        config: FailoverConfig,
    ) -> Self {
        Self {
            primary,
            fallback,
            config,
            state: Mutex::new(FailoverState {
                active: ActiveBackend::Primary,
                consecutive_failures: 0,
                next_probe_at: None,
            }),
        }
    }

    fn active_controller(&self, active: ActiveBackend) -> Option<&dyn InputController> {
        match active {
            ActiveBackend::Primary => Some(self.primary.as_ref()),
            ActiveBackend::Fallback => self.fallback.as_deref(),
        }
    }

    fn primary_name(&self) -> &str {
        self.primary.name()
    }

    fn fallback_name(&self) -> Option<&str> {
        self.fallback.as_ref().map(|c| c.name())
    }

    fn should_probe_primary(&self, now: Instant) -> bool {
        let guard = self.state.lock().ok();
        if let Some(state) = guard.as_ref() {
            state.active == ActiveBackend::Fallback
                && state.next_probe_at.is_some_and(|probe_at| now >= probe_at)
        } else {
            false
        }
    }

    fn mark_probe_failed(&self, now: Instant) {
        if let Ok(mut state) = self.state.lock() {
            state.next_probe_at = Some(now + self.config.probe_every);
        }
    }

    fn switch_to_fallback(&self, now: Instant) {
        if let Ok(mut state) = self.state.lock() {
            state.active = ActiveBackend::Fallback;
            state.consecutive_failures = 0;
            state.next_probe_at = Some(now + self.config.probe_every);
        }
        if self.config.log_switch {
            info!(
                primary = %self.primary_name(),
                fallback = %self.fallback_name().unwrap_or("none"),
                "input backend switched: primary -> fallback"
            );
        }
    }

    fn switch_to_primary(&self) {
        if let Ok(mut state) = self.state.lock() {
            state.active = ActiveBackend::Primary;
            state.consecutive_failures = 0;
            state.next_probe_at = None;
        }
        if self.config.log_switch {
            info!(
                primary = %self.primary_name(),
                "input backend switched: fallback -> primary"
            );
        }
    }

    fn mark_primary_success(&self) {
        if let Ok(mut state) = self.state.lock() {
            state.consecutive_failures = 0;
        }
    }

    fn mark_primary_failure(&self) -> u32 {
        if let Ok(mut state) = self.state.lock() {
            state.consecutive_failures += 1;
            state.consecutive_failures
        } else {
            0
        }
    }

    fn current_active(&self) -> ActiveBackend {
        self.state
            .lock()
            .map(|s| s.active)
            .unwrap_or(ActiveBackend::Primary)
    }

    /// Compute the effective timeout for an operation.
    ///
    /// For "long" operations (swipe, type_text, key_combo, key_tap-with-hold)
    /// the configured `action_timeout` (typically a few hundred ms) is far too
    /// strict and would always trip a false failover. For those we use
    /// `expected + slack`, capped to never go below `action_timeout`.
    fn effective_timeout(&self, expected: Option<Duration>) -> Duration {
        match expected {
            Some(d) => self
                .config
                .action_timeout
                .max(d.saturating_add(LONG_ACTION_TIMEOUT_SLACK)),
            None => self.config.action_timeout,
        }
    }

    async fn run_with_timeout<T>(
        &self,
        op_name: &str,
        timeout: Duration,
        fut: Pin<Box<dyn Future<Output = AnyhowResult<T>> + Send + '_>>,
    ) -> AnyhowResult<T> {
        match tokio::time::timeout(timeout, fut).await {
            Ok(result) => result,
            Err(_) => Err(anyhow::anyhow!(
                "input op '{}' timeout after {} ms",
                op_name,
                timeout.as_millis()
            )),
        }
    }

    async fn run_with_failover<T>(
        &self,
        op_name: &str,
        expected_duration: Option<Duration>,
        action: impl for<'a> Fn(
            &'a dyn InputController,
        ) -> Pin<Box<dyn Future<Output = AnyhowResult<T>> + Send + 'a>>,
    ) -> AnyhowResult<T> {
        let now = Instant::now();
        let timeout = self.effective_timeout(expected_duration);

        if self.should_probe_primary(now) {
            match self
                .run_with_timeout(op_name, timeout, action(self.primary.as_ref()))
                .await
            {
                Ok(value) => {
                    self.switch_to_primary();
                    return Ok(value);
                }
                Err(err) => {
                    self.mark_probe_failed(now);
                    warn!(
                        op = op_name,
                        error = %err,
                        "primary probe failed, keep fallback backend"
                    );
                }
            }
        }

        let active = self.current_active();
        let active_ctrl = self
            .active_controller(active)
            .ok_or_else(|| anyhow::anyhow!("active input backend is unavailable: {:?}", active))?;

        let active_result = self
            .run_with_timeout(op_name, timeout, action(active_ctrl))
            .await;
        match active_result {
            Ok(value) => {
                if active == ActiveBackend::Primary {
                    self.mark_primary_success();
                }
                Ok(value)
            }
            Err(primary_err) => {
                if active != ActiveBackend::Primary
                    || self.fallback.is_none()
                    || !self.config.enabled
                {
                    return Err(primary_err);
                }

                let failures = self.mark_primary_failure();
                if failures < self.config.fail_threshold {
                    return Err(primary_err);
                }

                let now = Instant::now();
                self.switch_to_fallback(now);
                if let Some(fallback) = self.fallback.as_ref() {
                    self.run_with_timeout(op_name, timeout, action(fallback.as_ref()))
                        .await
                } else {
                    Err(primary_err)
                }
            }
        }
    }
}

/// A reasonable upper bound for any per-character text injection: covers
/// typical keyboard layouts and IME composition pipelines.
fn estimate_type_text_duration(text: &str) -> Duration {
    let chars = text.chars().count() as u64;
    Duration::from_millis(200 + chars * 20)
}

fn estimate_key_combo_duration(keys: &[Key]) -> Duration {
    Duration::from_millis(50 + keys.len() as u64 * 30)
}

#[async_trait]
impl InputController for FailoverInputController {
    fn name(&self) -> &str {
        // We can't return a borrow that reflects which backend is active
        // (state is behind a Mutex), so use a stable static label here and
        // expose [`FailoverInputController::active_backend_label`] for
        // diagnostics that need the live value.
        "FailoverInput"
    }

    fn parse_key(&self, name: &str) -> Option<Key> {
        // Prefer the primary controller's mapping (it owns the user-defined
        // KeyMapper), but fall back to the secondary backend if the primary
        // can't resolve it.
        self.primary
            .parse_key(name)
            .or_else(|| self.fallback.as_ref().and_then(|ctrl| ctrl.parse_key(name)))
    }

    async fn init(&mut self, target: &InputTarget) -> AnyhowResult<()> {
        self.primary.init(target).await?;
        if let Some(fallback) = self.fallback.as_mut() {
            fallback.init(target).await?;
        }
        Ok(())
    }

    async fn mouse_move(&self, x: i32, y: i32) -> AnyhowResult<()> {
        self.run_with_failover("mouse_move", None, |ctrl| Box::pin(ctrl.mouse_move(x, y)))
            .await
    }

    async fn click(&self, x: i32, y: i32) -> AnyhowResult<()> {
        self.run_with_failover("click", None, |ctrl| Box::pin(ctrl.click(x, y)))
            .await
    }

    async fn double_click(&self, x: i32, y: i32) -> AnyhowResult<()> {
        // double_click sleeps ~50ms internally, so allow some headroom.
        self.run_with_failover("double_click", Some(Duration::from_millis(150)), |ctrl| {
            Box::pin(ctrl.double_click(x, y))
        })
        .await
    }

    async fn right_click(&self, x: i32, y: i32) -> AnyhowResult<()> {
        self.run_with_failover("right_click", None, |ctrl| Box::pin(ctrl.right_click(x, y)))
            .await
    }

    async fn mouse_down(&self, button: MouseButton) -> AnyhowResult<()> {
        self.run_with_failover("mouse_down", None, |ctrl| Box::pin(ctrl.mouse_down(button)))
            .await
    }

    async fn mouse_up(&self, button: MouseButton) -> AnyhowResult<()> {
        self.run_with_failover("mouse_up", None, |ctrl| Box::pin(ctrl.mouse_up(button)))
            .await
    }

    async fn mouse_scroll(&self, delta: i32) -> AnyhowResult<()> {
        self.run_with_failover("mouse_scroll", None, |ctrl| {
            Box::pin(ctrl.mouse_scroll(delta))
        })
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
        self.run_with_failover(
            "swipe",
            Some(Duration::from_millis(duration_ms as u64)),
            |ctrl| Box::pin(ctrl.swipe(x1, y1, x2, y2, duration_ms)),
        )
        .await
    }

    async fn key_press(&self, key: Key) -> AnyhowResult<()> {
        self.run_with_failover("key_press", None, |ctrl| Box::pin(ctrl.key_press(key)))
            .await
    }

    async fn key_release(&self, key: Key) -> AnyhowResult<()> {
        self.run_with_failover("key_release", None, |ctrl| Box::pin(ctrl.key_release(key)))
            .await
    }

    async fn key_tap(&self, key: Key, duration_ms: Option<u32>) -> AnyhowResult<()> {
        let expected = duration_ms.map(|ms| Duration::from_millis(ms as u64));
        self.run_with_failover("key_tap", expected, |ctrl| {
            Box::pin(ctrl.key_tap(key, duration_ms))
        })
        .await
    }

    async fn type_text(&self, text: &str) -> AnyhowResult<()> {
        let text_owned = text.to_string();
        let expected = estimate_type_text_duration(text);
        self.run_with_failover("type_text", Some(expected), move |ctrl| {
            let text_owned = text_owned.clone();
            Box::pin(async move { ctrl.type_text(&text_owned).await })
        })
        .await
    }

    async fn key_combo(&self, keys: &[Key]) -> AnyhowResult<()> {
        let keys_owned = keys.to_vec();
        let expected = estimate_key_combo_duration(keys);
        self.run_with_failover("key_combo", Some(expected), move |ctrl| {
            let keys_owned = keys_owned.clone();
            Box::pin(async move { ctrl.key_combo(&keys_owned).await })
        })
        .await
    }

    fn supports_background(&self) -> bool {
        self.primary.supports_background()
            || self
                .fallback
                .as_ref()
                .is_some_and(|ctrl| ctrl.supports_background())
    }

    fn last_latency_ms(&self) -> Option<f64> {
        let active = self.current_active();
        self.active_controller(active)
            .and_then(|ctrl| ctrl.last_latency_ms())
            .or_else(|| self.primary.last_latency_ms())
    }

    fn mode(&self) -> InputMode {
        let active = self.current_active();
        self.active_controller(active)
            .map(|ctrl| ctrl.mode())
            .unwrap_or_else(|| self.primary.mode())
    }
}

impl FailoverInputController {
    /// Display-friendly description of the currently active backend.
    pub fn active_backend_label(&self) -> String {
        let active = self.current_active();
        let active_name = self
            .active_controller(active)
            .map(|c| c.name())
            .unwrap_or("none");
        match active {
            ActiveBackend::Primary => format!("primary={}", active_name),
            ActiveBackend::Fallback => format!("fallback={}", active_name),
        }
    }
}

#[cfg(test)]
mod failover_unit_tests {
    use super::*;
    use crate::win32::Win32Input;
    use crate::KeyMapper;

    fn make_controller() -> FailoverInputController {
        // No need to ever invoke these — we only exercise pure helpers.
        let mapper = KeyMapper::new(Default::default());
        let primary = Box::new(Win32Input::new(mapper.clone())) as Box<dyn InputController>;
        FailoverInputController::new(primary, None, FailoverConfig::default())
    }

    #[test]
    fn effective_timeout_for_short_actions_uses_action_timeout() {
        let ctrl = make_controller();
        // No expected duration ⇒ exactly the configured action timeout.
        assert_eq!(
            ctrl.effective_timeout(None),
            FailoverConfig::default().action_timeout
        );
    }

    #[test]
    fn effective_timeout_for_long_actions_includes_slack() {
        let ctrl = make_controller();
        // Even for a tiny expected duration we always add slack on top so the
        // primary backend gets a fair window, never less than action_timeout.
        let small = Duration::from_millis(10);
        let expected =
            (small + LONG_ACTION_TIMEOUT_SLACK).max(FailoverConfig::default().action_timeout);
        assert_eq!(ctrl.effective_timeout(Some(small)), expected);
    }

    #[test]
    fn effective_timeout_grows_for_long_actions() {
        let ctrl = make_controller();
        // 1000ms swipe ⇒ 1500ms with default 500ms slack.
        let long = Duration::from_millis(1000);
        let expected = long + LONG_ACTION_TIMEOUT_SLACK;
        assert_eq!(ctrl.effective_timeout(Some(long)), expected);

        // Sanity: this is way larger than the default 300ms action timeout.
        assert!(expected > FailoverConfig::default().action_timeout);
    }

    #[test]
    fn estimate_type_text_duration_scales_with_length() {
        let zero = estimate_type_text_duration("");
        let many = estimate_type_text_duration(&"a".repeat(50));
        assert!(many > zero);
        assert!(zero >= Duration::from_millis(200));
    }

    #[test]
    fn estimate_key_combo_duration_scales_with_keys() {
        let one = estimate_key_combo_duration(&[Key::Control]);
        let three = estimate_key_combo_duration(&[Key::Control, Key::Shift, Key::A]);
        assert!(three > one);
    }

    #[test]
    fn active_backend_label_reports_primary_by_default() {
        let ctrl = make_controller();
        assert!(ctrl.active_backend_label().starts_with("primary="));
    }
}
