//! Recording input controller wrapper.
//!
//! Decorates an initialized [`InputController`] and emits a record callback for
//! each input action call.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use anyhow::Result as AnyhowResult;
use async_trait::async_trait;
use serde_json::{json, Value};

use crate::controller::InputController;
use crate::key::{Key, MouseButton};
use crate::target::InputTarget;
use crate::InputMode;

#[derive(Debug, Clone)]
pub struct InputRecordEvent {
    pub method: String,
    pub args: Value,
    pub ok: bool,
    pub error: Option<String>,
}

pub type InputRecordSink =
    Arc<dyn Fn(InputRecordEvent) -> Pin<Box<dyn Future<Output = ()> + Send>> + Send + Sync>;

pub struct RecordingInputController {
    inner: Box<dyn InputController>,
    sink: InputRecordSink,
}

impl RecordingInputController {
    pub fn new(inner: Box<dyn InputController>, sink: InputRecordSink) -> Self {
        Self { inner, sink }
    }

    async fn emit(&self, method: &str, args: Value, result: &AnyhowResult<()>) {
        let event = InputRecordEvent {
            method: method.to_string(),
            args,
            ok: result.is_ok(),
            error: result.as_ref().err().map(|e| e.to_string()),
        };
        (self.sink)(event).await;
    }
}

#[async_trait]
impl InputController for RecordingInputController {
    fn name(&self) -> &str {
        "RecordingInput"
    }

    fn parse_key(&self, name: &str) -> Option<Key> {
        self.inner.parse_key(name)
    }

    async fn init(&mut self, target: &InputTarget) -> AnyhowResult<()> {
        self.inner.init(target).await
    }

    async fn mouse_move(&self, x: i32, y: i32) -> AnyhowResult<()> {
        let result = self.inner.mouse_move(x, y).await;
        self.emit("mouse_move", json!({ "x": x, "y": y }), &result)
            .await;
        result
    }

    async fn click(&self, x: i32, y: i32) -> AnyhowResult<()> {
        let result = self.inner.click(x, y).await;
        self.emit("click", json!({ "x": x, "y": y }), &result).await;
        result
    }

    async fn double_click(&self, x: i32, y: i32) -> AnyhowResult<()> {
        let result = self.inner.double_click(x, y).await;
        self.emit("double_click", json!({ "x": x, "y": y }), &result)
            .await;
        result
    }

    async fn right_click(&self, x: i32, y: i32) -> AnyhowResult<()> {
        let result = self.inner.right_click(x, y).await;
        self.emit("right_click", json!({ "x": x, "y": y }), &result)
            .await;
        result
    }

    async fn mouse_down(&self, button: MouseButton) -> AnyhowResult<()> {
        let result = self.inner.mouse_down(button).await;
        self.emit(
            "mouse_down",
            json!({ "button": format!("{:?}", button) }),
            &result,
        )
        .await;
        result
    }

    async fn mouse_up(&self, button: MouseButton) -> AnyhowResult<()> {
        let result = self.inner.mouse_up(button).await;
        self.emit(
            "mouse_up",
            json!({ "button": format!("{:?}", button) }),
            &result,
        )
        .await;
        result
    }

    async fn mouse_scroll(&self, delta: i32) -> AnyhowResult<()> {
        let result = self.inner.mouse_scroll(delta).await;
        self.emit("scroll", json!({ "delta": delta }), &result)
            .await;
        result
    }

    async fn swipe(
        &self,
        x1: i32,
        y1: i32,
        x2: i32,
        y2: i32,
        duration_ms: u32,
    ) -> AnyhowResult<()> {
        let result = self.inner.swipe(x1, y1, x2, y2, duration_ms).await;
        self.emit(
            "swipe",
            json!({
                "x1": x1,
                "y1": y1,
                "x2": x2,
                "y2": y2,
                "duration": duration_ms
            }),
            &result,
        )
        .await;
        result
    }

    async fn key_press(&self, key: Key) -> AnyhowResult<()> {
        let result = self.inner.key_press(key).await;
        self.emit("key_down", json!({ "key": format!("{:?}", key) }), &result)
            .await;
        result
    }

    async fn key_release(&self, key: Key) -> AnyhowResult<()> {
        let result = self.inner.key_release(key).await;
        self.emit("key_up", json!({ "key": format!("{:?}", key) }), &result)
            .await;
        result
    }

    async fn key_tap(&self, key: Key, duration_ms: Option<u32>) -> AnyhowResult<()> {
        let result = self.inner.key_tap(key, duration_ms).await;
        self.emit(
            "key_press",
            json!({
                "key": format!("{:?}", key),
                "duration_ms": duration_ms
            }),
            &result,
        )
        .await;
        result
    }

    async fn type_text(&self, text: &str) -> AnyhowResult<()> {
        let result = self.inner.type_text(text).await;
        self.emit("type_text", json!({ "text": text }), &result)
            .await;
        result
    }

    async fn key_combo(&self, keys: &[Key]) -> AnyhowResult<()> {
        let keys_dbg = keys.iter().map(|k| format!("{:?}", k)).collect::<Vec<_>>();
        let result = self.inner.key_combo(keys).await;
        self.emit("key_combo", json!({ "keys": keys_dbg }), &result)
            .await;
        result
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
