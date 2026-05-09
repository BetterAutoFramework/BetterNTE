//! DebugScriptContext — wraps ScriptContext and focuses tracing on
//! recognition/input-related interfaces.

use anyhow::Result;
use async_trait::async_trait;
use base64::{engine::general_purpose, Engine as _};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use betternte_core::EngineEvent;
use betternte_script::{
    CaptureFrame, ColorMatchAllOpts, ColorMatchPoint, FindTemplateBatchEntry, FindTemplateOpts,
    LogLevel, MatchResult, OcrResult, Rect, Region, ScriptContext,
};

/// Wrapper around a ScriptContext that intercepts all calls and publishes
/// ScriptCallTrace events to the EventBus for frontend debug visualization.
pub struct DebugScriptContext {
    inner: Arc<dyn ScriptContext>,
    event_bus: crate::EventBus,
    call_counter: AtomicU64,
}

impl DebugScriptContext {
    pub fn new(inner: Arc<dyn ScriptContext>, event_bus: crate::EventBus) -> Self {
        Self {
            inner,
            event_bus,
            call_counter: AtomicU64::new(0),
        }
    }

    fn next_id(&self) -> String {
        let n = self.call_counter.fetch_add(1, Ordering::Relaxed);
        format!("trace-{}", n)
    }

    /// Capture a screenshot thumbnail (256px wide, base64 PNG).
    /// Returns None if capture fails.
    async fn capture_thumbnail(&self) -> Option<String> {
        let frame = self.inner.capture(false).await.ok()?;
        frame_to_thumbnail_base64(&frame)
    }

    fn trace_category_enabled(category: &str) -> bool {
        matches!(category, "recognition" | "operation")
    }

    fn publish_trace(&self, trace: EngineEvent) {
        if let EngineEvent::ScriptCallTrace { category, .. } = &trace {
            if !Self::trace_category_enabled(category) {
                return;
            }
        }
        let _ = self.event_bus.publish(trace);
    }

    /// Build a ScriptCallTrace event with before screenshot.
    fn make_trace(
        &self,
        id: String,
        category: &str,
        method: &str,
        args: serde_json::Value,
        screenshot_before: Option<String>,
    ) -> EngineEvent {
        EngineEvent::ScriptCallTrace {
            id,
            category: category.to_string(),
            method: method.to_string(),
            args,
            result: None,
            success: true,
            error: None,
            screenshot_before,
            screenshot_after: None,
            duration_ms: 0,
            timestamp: chrono::Utc::now(),
        }
    }

    /// Complete a trace: set result, duration, and publish.
    fn finish_trace_ok(
        &self,
        mut trace: EngineEvent,
        result: Option<String>,
        duration_ms: u64,
        screenshot_after: Option<String>,
    ) {
        if let EngineEvent::ScriptCallTrace {
            result: ref mut r,
            duration_ms: ref mut d,
            screenshot_after: ref mut sa,
            ..
        } = trace
        {
            *r = result;
            *d = duration_ms;
            *sa = screenshot_after;
        }
        self.publish_trace(trace);
    }

    fn finish_trace_err(&self, mut trace: EngineEvent, error: String, duration_ms: u64) {
        if let EngineEvent::ScriptCallTrace {
            ref mut success,
            error: ref mut e,
            duration_ms: ref mut d,
            ..
        } = trace
        {
            *success = false;
            *e = Some(error);
            *d = duration_ms;
        }
        self.publish_trace(trace);
    }
}

/// Convert a CaptureFrame to a 256px-wide base64 PNG thumbnail.
fn frame_to_thumbnail_base64(frame: &CaptureFrame) -> Option<String> {
    let img = image::RgbaImage::from_raw(frame.width, frame.height, frame.data.clone())?;
    let dyn_img = image::DynamicImage::ImageRgba8(img);

    // Scale to 256px wide, maintain aspect ratio
    let thumb = if frame.width > 256 {
        dyn_img.resize(
            256,
            256 * frame.height / frame.width,
            image::imageops::FilterType::Nearest,
        )
    } else {
        dyn_img
    };

    let mut buf = std::io::Cursor::new(Vec::new());
    thumb.write_to(&mut buf, image::ImageFormat::Png).ok()?;
    Some(general_purpose::STANDARD.encode(buf.get_ref()))
}

/// Truncate a string to max_len chars, adding "..." if truncated.
fn truncate(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}...", &s[..max_len])
    }
}

#[async_trait]
impl ScriptContext for DebugScriptContext {
    // === State ===

    fn is_cancelled(&self) -> bool {
        self.inner.is_cancelled()
    }

    fn request_cancel(&self) {
        self.inner.request_cancel();
    }

    fn reset_cancel(&self) {
        self.inner.reset_cancel();
    }

    fn get_config(&self) -> &serde_json::Value {
        self.inner.get_config()
    }

    fn progress(&self, current: u32, total: u32) {
        self.inner.progress(current, total);
    }

    fn get_fps(&self) -> f64 {
        self.inner.get_fps()
    }

    fn get_frame_number(&self) -> u64 {
        self.inner.get_frame_number()
    }

    fn set_template_dir(&self, dir: std::path::PathBuf) {
        self.inner.set_template_dir(dir);
    }

    fn get_template_dir(&self) -> Option<std::path::PathBuf> {
        self.inner.get_template_dir()
    }

    fn set_design_resolution(&self, resolution: Option<(u32, u32)>) {
        self.inner.set_design_resolution(resolution);
    }

    fn get_design_resolution(&self) -> Option<(u32, u32)> {
        self.inner.get_design_resolution()
    }

    fn get_scale_factors(&self) -> Option<(f64, f64)> {
        self.inner.get_scale_factors()
    }

    fn get_frame_size(&self) -> Option<(u32, u32)> {
        self.inner.get_frame_size()
    }

    fn manifest_security_strict(&self) -> bool {
        self.inner.manifest_security_strict()
    }

    fn push_manifest_permission_scope(&self, declared: &[String], strict: bool) {
        self.inner.push_manifest_permission_scope(declared, strict);
    }

    fn pop_manifest_permission_scope(&self) {
        self.inner.pop_manifest_permission_scope();
    }

    fn check_manifest_api_permission(&self, method: &str) -> Result<(), String> {
        self.inner.check_manifest_api_permission(method)
    }

    // === Capture ===

    async fn capture(&self, force: bool) -> Result<CaptureFrame> {
        let id = self.next_id();
        let args = serde_json::json!({"force": force});
        let trace = self.make_trace(id, "capture", "capture", args, None);
        let start = std::time::Instant::now();

        let result = self.inner.capture(force).await;
        let elapsed = start.elapsed().as_millis() as u64;

        match &result {
            Ok(_) => self.finish_trace_ok(trace, Some("ok".into()), elapsed, None),
            Err(e) => self.finish_trace_err(trace, e.to_string(), elapsed),
        }
        result
    }

    async fn capture_region(&self, region: &Region, force: bool) -> Result<CaptureFrame> {
        let id = self.next_id();
        let args = serde_json::json!({"region": region, "force": force});
        let trace = self.make_trace(id, "capture", "capture_region", args, None);
        let start = std::time::Instant::now();

        let result = self.inner.capture_region(region, force).await;
        let elapsed = start.elapsed().as_millis() as u64;

        match &result {
            Ok(_) => self.finish_trace_ok(trace, Some("ok".into()), elapsed, None),
            Err(e) => self.finish_trace_err(trace, e.to_string(), elapsed),
        }
        result
    }

    async fn save_screenshot(&self, force: bool) -> Result<String> {
        let id = self.next_id();
        let args = serde_json::json!({"force": force});
        let trace = self.make_trace(id, "capture", "save_screenshot", args, None);
        let start = std::time::Instant::now();

        let result = self.inner.save_screenshot(force).await;
        let elapsed = start.elapsed().as_millis() as u64;

        match &result {
            Ok(path) => self.finish_trace_ok(trace, Some(path.clone()), elapsed, None),
            Err(e) => self.finish_trace_err(trace, e.to_string(), elapsed),
        }
        result
    }

    // === Recognition ===

    async fn find_template(
        &self,
        name: &str,
        opts: Option<FindTemplateOpts>,
    ) -> Result<Option<MatchResult>> {
        let id = self.next_id();
        let args = serde_json::json!({"name": name, "opts": opts});
        let screenshot = self.capture_thumbnail().await;
        let trace = self.make_trace(id, "recognition", "find_template", args, screenshot);
        let start = std::time::Instant::now();

        let result = self.inner.find_template(name, opts).await;
        let elapsed = start.elapsed().as_millis() as u64;

        match &result {
            Ok(Some(m)) => self.finish_trace_ok(trace, Some(format!("{:?}", m)), elapsed, None),
            Ok(None) => self.finish_trace_ok(trace, Some("no match".into()), elapsed, None),
            Err(e) => self.finish_trace_err(trace, e.to_string(), elapsed),
        }
        result
    }

    async fn find_templates(
        &self,
        name: &str,
        opts: Option<FindTemplateOpts>,
    ) -> Result<Vec<MatchResult>> {
        let id = self.next_id();
        let args = serde_json::json!({"name": name, "opts": opts});
        let screenshot = self.capture_thumbnail().await;
        let trace = self.make_trace(id, "recognition", "find_templates", args, screenshot);
        let start = std::time::Instant::now();

        let result = self.inner.find_templates(name, opts).await;
        let elapsed = start.elapsed().as_millis() as u64;

        match &result {
            Ok(matches) => self.finish_trace_ok(
                trace,
                Some(format!("{} matches", matches.len())),
                elapsed,
                None,
            ),
            Err(e) => self.finish_trace_err(trace, e.to_string(), elapsed),
        }
        result
    }

    async fn find_template_batch(
        &self,
        entries: &[FindTemplateBatchEntry],
    ) -> Result<Vec<Option<MatchResult>>> {
        let id = self.next_id();
        let args = serde_json::json!({"entries": entries});
        let screenshot = self.capture_thumbnail().await;
        let trace = self.make_trace(id, "recognition", "find_template_batch", args, screenshot);
        let start = std::time::Instant::now();

        let result = self.inner.find_template_batch(entries).await;
        let elapsed = start.elapsed().as_millis() as u64;

        match &result {
            Ok(v) => {
                self.finish_trace_ok(trace, Some(format!("{} entries", v.len())), elapsed, None)
            }
            Err(e) => self.finish_trace_err(trace, e.to_string(), elapsed),
        }
        result
    }

    async fn ocr(&self, region: &Region, text_color: Option<&str>, text_color_tolerance: u8) -> Result<String> {
        let id = self.next_id();
        let args = serde_json::json!({"region": region, "text_color": text_color, "text_color_tolerance": text_color_tolerance});
        let screenshot = self.capture_thumbnail().await;
        let trace = self.make_trace(id, "recognition", "ocr", args, screenshot);
        let start = std::time::Instant::now();

        let result = self.inner.ocr(region, text_color, text_color_tolerance).await;
        let elapsed = start.elapsed().as_millis() as u64;

        match &result {
            Ok(text) => self.finish_trace_ok(trace, Some(truncate(text, 200)), elapsed, None),
            Err(e) => self.finish_trace_err(trace, e.to_string(), elapsed),
        }
        result
    }

    async fn ocr_all(&self) -> Result<Vec<OcrResult>> {
        let id = self.next_id();
        let args = serde_json::json!({});
        let screenshot = self.capture_thumbnail().await;
        let trace = self.make_trace(id, "recognition", "ocr_all", args, screenshot);
        let start = std::time::Instant::now();

        let result = self.inner.ocr_all().await;
        let elapsed = start.elapsed().as_millis() as u64;

        match &result {
            Ok(results) => {
                let summary = format!("{} regions found", results.len());
                self.finish_trace_ok(trace, Some(summary), elapsed, None);
            }
            Err(e) => self.finish_trace_err(trace, e.to_string(), elapsed),
        }
        result
    }

    async fn get_color(&self, x: i32, y: i32) -> Result<String> {
        let id = self.next_id();
        let args = serde_json::json!({"x": x, "y": y});
        let screenshot = self.capture_thumbnail().await;
        let trace = self.make_trace(id, "recognition", "get_color", args, screenshot);
        let start = std::time::Instant::now();

        let result = self.inner.get_color(x, y).await;
        let elapsed = start.elapsed().as_millis() as u64;

        match &result {
            Ok(color) => self.finish_trace_ok(trace, Some(color.clone()), elapsed, None),
            Err(e) => self.finish_trace_err(trace, e.to_string(), elapsed),
        }
        result
    }

    async fn color_match(&self, x: i32, y: i32, color: &str, tolerance: u8) -> Result<bool> {
        let id = self.next_id();
        let args = serde_json::json!({"x": x, "y": y, "color": color, "tolerance": tolerance});
        let screenshot = self.capture_thumbnail().await;
        let trace = self.make_trace(id, "recognition", "color_match", args, screenshot);
        let start = std::time::Instant::now();

        let result = self.inner.color_match(x, y, color, tolerance).await;
        let elapsed = start.elapsed().as_millis() as u64;

        match &result {
            Ok(matched) => self.finish_trace_ok(trace, Some(matched.to_string()), elapsed, None),
            Err(e) => self.finish_trace_err(trace, e.to_string(), elapsed),
        }
        result
    }

    async fn color_match_all(
        &self,
        points: &[ColorMatchPoint],
        opts: &ColorMatchAllOpts,
    ) -> Result<betternte_script::ColorMatchAllResult> {
        let id = self.next_id();
        let args = serde_json::json!({
            "points": points,
            "opts": opts,
        });
        let screenshot = self.capture_thumbnail().await;
        let trace = self.make_trace(id, "recognition", "color_match_all", args, screenshot);
        let start = std::time::Instant::now();

        let result = self.inner.color_match_all(points, opts).await;
        let elapsed = start.elapsed().as_millis() as u64;

        match &result {
            Ok(r) => self.finish_trace_ok(trace, serde_json::to_string(r).ok(), elapsed, None),
            Err(e) => self.finish_trace_err(trace, e.to_string(), elapsed),
        }
        result
    }

    async fn count_color(&self, color: &str, opts: Option<&serde_json::Value>) -> Result<u32> {
        let id = self.next_id();
        let args = serde_json::json!({
            "color": color,
            "opts": opts,
        });
        let screenshot = self.capture_thumbnail().await;
        let trace = self.make_trace(id, "recognition", "count_color", args, screenshot);
        let start = std::time::Instant::now();

        let result = self.inner.count_color(color, opts).await;
        let elapsed = start.elapsed().as_millis() as u64;

        match &result {
            Ok(count) => self.finish_trace_ok(trace, Some(count.to_string()), elapsed, None),
            Err(e) => self.finish_trace_err(trace, e.to_string(), elapsed),
        }
        result
    }

    async fn scan_slider_strip(&self, opts: &serde_json::Value) -> Result<serde_json::Value> {
        let id = self.next_id();
        let args = opts.clone();
        let screenshot = self.capture_thumbnail().await;
        let trace = self.make_trace(id, "recognition", "scan_slider_strip", args, screenshot);
        let start = std::time::Instant::now();

        let result = self.inner.scan_slider_strip(opts).await;
        let elapsed = start.elapsed().as_millis() as u64;

        match &result {
            Ok(v) => self.finish_trace_ok(trace, serde_json::to_string(v).ok(), elapsed, None),
            Err(e) => self.finish_trace_err(trace, e.to_string(), elapsed),
        }
        result
    }

    async fn scan_strip_edges(&self, opts: &serde_json::Value) -> Result<serde_json::Value> {
        let id = self.next_id();
        let args = opts.clone();
        let screenshot = self.capture_thumbnail().await;
        let trace = self.make_trace(id, "recognition", "scan_strip_edges", args, screenshot);
        let start = std::time::Instant::now();

        let result = self.inner.scan_strip_edges(opts).await;
        let elapsed = start.elapsed().as_millis() as u64;

        match &result {
            Ok(v) => self.finish_trace_ok(trace, serde_json::to_string(v).ok(), elapsed, None),
            Err(e) => self.finish_trace_err(trace, e.to_string(), elapsed),
        }
        result
    }

    // === Input (operation) ===

    async fn click(&self, x: i32, y: i32) -> Result<()> {
        let id = self.next_id();
        let args = serde_json::json!({"x": x, "y": y});
        let screenshot = self.capture_thumbnail().await;
        let trace = self.make_trace(id, "operation", "click", args, screenshot);
        let start = std::time::Instant::now();

        let result = self.inner.click(x, y).await;
        let elapsed = start.elapsed().as_millis() as u64;

        match &result {
            Ok(()) => self.finish_trace_ok(trace, None, elapsed, None),
            Err(e) => self.finish_trace_err(trace, e.to_string(), elapsed),
        }
        result
    }

    async fn double_click(&self, x: i32, y: i32) -> Result<()> {
        let id = self.next_id();
        let args = serde_json::json!({"x": x, "y": y});
        let screenshot = self.capture_thumbnail().await;
        let trace = self.make_trace(id, "operation", "double_click", args, screenshot);
        let start = std::time::Instant::now();

        let result = self.inner.double_click(x, y).await;
        let elapsed = start.elapsed().as_millis() as u64;

        match &result {
            Ok(()) => self.finish_trace_ok(trace, None, elapsed, None),
            Err(e) => self.finish_trace_err(trace, e.to_string(), elapsed),
        }
        result
    }

    async fn right_click(&self, x: i32, y: i32) -> Result<()> {
        let id = self.next_id();
        let args = serde_json::json!({"x": x, "y": y});
        let screenshot = self.capture_thumbnail().await;
        let trace = self.make_trace(id, "operation", "right_click", args, screenshot);
        let start = std::time::Instant::now();

        let result = self.inner.right_click(x, y).await;
        let elapsed = start.elapsed().as_millis() as u64;

        match &result {
            Ok(()) => self.finish_trace_ok(trace, None, elapsed, None),
            Err(e) => self.finish_trace_err(trace, e.to_string(), elapsed),
        }
        result
    }

    async fn mouse_move(&self, x: i32, y: i32) -> Result<()> {
        let id = self.next_id();
        let args = serde_json::json!({"x": x, "y": y});
        let trace = self.make_trace(id, "operation", "mouse_move", args, None);
        let start = std::time::Instant::now();

        let result = self.inner.mouse_move(x, y).await;
        let elapsed = start.elapsed().as_millis() as u64;

        match &result {
            Ok(()) => self.finish_trace_ok(trace, None, elapsed, None),
            Err(e) => self.finish_trace_err(trace, e.to_string(), elapsed),
        }
        result
    }

    async fn mouse_down(&self, button: &str) -> Result<()> {
        let id = self.next_id();
        let args = serde_json::json!({"button": button});
        let trace = self.make_trace(id, "operation", "mouse_down", args, None);
        let start = std::time::Instant::now();

        let result = self.inner.mouse_down(button).await;
        let elapsed = start.elapsed().as_millis() as u64;

        match &result {
            Ok(()) => self.finish_trace_ok(trace, None, elapsed, None),
            Err(e) => self.finish_trace_err(trace, e.to_string(), elapsed),
        }
        result
    }

    async fn mouse_up(&self, button: &str) -> Result<()> {
        let id = self.next_id();
        let args = serde_json::json!({"button": button});
        let trace = self.make_trace(id, "operation", "mouse_up", args, None);
        let start = std::time::Instant::now();

        let result = self.inner.mouse_up(button).await;
        let elapsed = start.elapsed().as_millis() as u64;

        match &result {
            Ok(()) => self.finish_trace_ok(trace, None, elapsed, None),
            Err(e) => self.finish_trace_err(trace, e.to_string(), elapsed),
        }
        result
    }

    async fn scroll(&self, delta: i32) -> Result<()> {
        let id = self.next_id();
        let args = serde_json::json!({"delta": delta});
        let trace = self.make_trace(id, "operation", "scroll", args, None);
        let start = std::time::Instant::now();

        let result = self.inner.scroll(delta).await;
        let elapsed = start.elapsed().as_millis() as u64;

        match &result {
            Ok(()) => self.finish_trace_ok(trace, None, elapsed, None),
            Err(e) => self.finish_trace_err(trace, e.to_string(), elapsed),
        }
        result
    }

    async fn swipe(&self, x1: i32, y1: i32, x2: i32, y2: i32, duration_ms: u32) -> Result<()> {
        let id = self.next_id();
        let args =
            serde_json::json!({"x1": x1, "y1": y1, "x2": x2, "y2": y2, "duration_ms": duration_ms});
        let screenshot = self.capture_thumbnail().await;
        let trace = self.make_trace(id, "operation", "swipe", args, screenshot);
        let start = std::time::Instant::now();

        let result = self.inner.swipe(x1, y1, x2, y2, duration_ms).await;
        let elapsed = start.elapsed().as_millis() as u64;

        match &result {
            Ok(()) => self.finish_trace_ok(trace, None, elapsed, None),
            Err(e) => self.finish_trace_err(trace, e.to_string(), elapsed),
        }
        result
    }

    async fn key_down(&self, key: &str) -> Result<()> {
        let id = self.next_id();
        let args = serde_json::json!({"key": key});
        let trace = self.make_trace(id, "operation", "key_down", args, None);
        let start = std::time::Instant::now();

        let result = self.inner.key_down(key).await;
        let elapsed = start.elapsed().as_millis() as u64;

        match &result {
            Ok(()) => self.finish_trace_ok(trace, None, elapsed, None),
            Err(e) => self.finish_trace_err(trace, e.to_string(), elapsed),
        }
        result
    }

    async fn key_up(&self, key: &str) -> Result<()> {
        let id = self.next_id();
        let args = serde_json::json!({"key": key});
        let trace = self.make_trace(id, "operation", "key_up", args, None);
        let start = std::time::Instant::now();

        let result = self.inner.key_up(key).await;
        let elapsed = start.elapsed().as_millis() as u64;

        match &result {
            Ok(()) => self.finish_trace_ok(trace, None, elapsed, None),
            Err(e) => self.finish_trace_err(trace, e.to_string(), elapsed),
        }
        result
    }

    async fn key_press(&self, key: &str, duration_ms: Option<u32>) -> Result<()> {
        let id = self.next_id();
        let args = serde_json::json!({"key": key, "duration_ms": duration_ms});
        let trace = self.make_trace(id, "operation", "key_press", args, None);
        let start = std::time::Instant::now();

        let result = self.inner.key_press(key, duration_ms).await;
        let elapsed = start.elapsed().as_millis() as u64;

        match &result {
            Ok(()) => self.finish_trace_ok(trace, None, elapsed, None),
            Err(e) => self.finish_trace_err(trace, e.to_string(), elapsed),
        }
        result
    }

    async fn key_combo(&self, keys: &[String]) -> Result<()> {
        let id = self.next_id();
        let args = serde_json::json!({"keys": keys});
        let trace = self.make_trace(id, "operation", "key_combo", args, None);
        let start = std::time::Instant::now();

        let result = self.inner.key_combo(keys).await;
        let elapsed = start.elapsed().as_millis() as u64;

        match &result {
            Ok(()) => self.finish_trace_ok(trace, None, elapsed, None),
            Err(e) => self.finish_trace_err(trace, e.to_string(), elapsed),
        }
        result
    }

    async fn type_text(&self, text: &str) -> Result<()> {
        let id = self.next_id();
        let args = serde_json::json!({"text": truncate(text, 100)});
        let trace = self.make_trace(id, "operation", "type_text", args, None);
        let start = std::time::Instant::now();

        let result = self.inner.type_text(text).await;
        let elapsed = start.elapsed().as_millis() as u64;

        match &result {
            Ok(()) => self.finish_trace_ok(trace, None, elapsed, None),
            Err(e) => self.finish_trace_err(trace, e.to_string(), elapsed),
        }
        result
    }

    // === Wait (time-based) ===

    async fn sleep(&self, ms: u64) -> Result<()> {
        let id = self.next_id();
        let args = serde_json::json!({"ms": ms});
        let trace = self.make_trace(id, "wait", "sleep", args, None);
        let start = std::time::Instant::now();

        let result = self.inner.sleep(ms).await;
        let elapsed = start.elapsed().as_millis() as u64;

        match &result {
            Ok(()) => self.finish_trace_ok(trace, None, elapsed, None),
            Err(e) => self.finish_trace_err(trace, e.to_string(), elapsed),
        }
        result
    }

    async fn wait_for_template(
        &self,
        name: &str,
        timeout_ms: u64,
        opts: Option<FindTemplateOpts>,
    ) -> Result<Option<MatchResult>> {
        let id = self.next_id();
        let args = serde_json::json!({
            "name": name,
            "timeout_ms": timeout_ms,
            "opts": opts,
        });
        let screenshot_before = self.capture_thumbnail().await;
        let trace = self.make_trace(id, "wait", "wait_for_template", args, screenshot_before);
        let start = std::time::Instant::now();

        let result = self.inner.wait_for_template(name, timeout_ms, opts).await;
        let elapsed = start.elapsed().as_millis() as u64;
        let screenshot_after = self.capture_thumbnail().await;

        match &result {
            Ok(Some(m)) => {
                self.finish_trace_ok(trace, Some(format!("{:?}", m)), elapsed, screenshot_after)
            }
            Ok(None) => {
                self.finish_trace_ok(trace, Some("timeout".into()), elapsed, screenshot_after)
            }
            Err(e) => self.finish_trace_err(trace, e.to_string(), elapsed),
        }
        result
    }

    async fn wait_gone(&self, name: &str, timeout_ms: u64) -> Result<bool> {
        let id = self.next_id();
        let args = serde_json::json!({"name": name, "timeout_ms": timeout_ms});
        let screenshot_before = self.capture_thumbnail().await;
        let trace = self.make_trace(id, "wait", "wait_gone", args, screenshot_before);
        let start = std::time::Instant::now();

        let result = self.inner.wait_gone(name, timeout_ms).await;
        let elapsed = start.elapsed().as_millis() as u64;
        let screenshot_after = self.capture_thumbnail().await;

        match &result {
            Ok(gone) => {
                self.finish_trace_ok(trace, Some(gone.to_string()), elapsed, screenshot_after)
            }
            Err(e) => self.finish_trace_err(trace, e.to_string(), elapsed),
        }
        result
    }

    async fn wait_for_color(&self, x: i32, y: i32, color: &str, timeout_ms: u64) -> Result<bool> {
        let id = self.next_id();
        let args = serde_json::json!({"x": x, "y": y, "color": color, "timeout_ms": timeout_ms});
        let screenshot_before = self.capture_thumbnail().await;
        let trace = self.make_trace(id, "wait", "wait_for_color", args, screenshot_before);
        let start = std::time::Instant::now();

        let result = self.inner.wait_for_color(x, y, color, timeout_ms).await;
        let elapsed = start.elapsed().as_millis() as u64;
        let screenshot_after = self.capture_thumbnail().await;

        match &result {
            Ok(matched) => {
                self.finish_trace_ok(trace, Some(matched.to_string()), elapsed, screenshot_after)
            }
            Err(e) => self.finish_trace_err(trace, e.to_string(), elapsed),
        }
        result
    }

    // === Wait (frame-based) ===

    async fn sleep_frames(&self, frames: u32) -> Result<()> {
        let id = self.next_id();
        let args = serde_json::json!({"frames": frames});
        let trace = self.make_trace(id, "wait", "sleep_frames", args, None);
        let start = std::time::Instant::now();

        let result = self.inner.sleep_frames(frames).await;
        let elapsed = start.elapsed().as_millis() as u64;

        match &result {
            Ok(()) => self.finish_trace_ok(trace, None, elapsed, None),
            Err(e) => self.finish_trace_err(trace, e.to_string(), elapsed),
        }
        result
    }

    async fn wait_for_template_frames(
        &self,
        name: &str,
        max_frames: u32,
        opts: Option<FindTemplateOpts>,
    ) -> Result<Option<MatchResult>> {
        let id = self.next_id();
        let args = serde_json::json!({
            "name": name,
            "max_frames": max_frames,
            "opts": opts,
        });
        let screenshot_before = self.capture_thumbnail().await;
        let trace = self.make_trace(
            id,
            "wait",
            "wait_for_template_frames",
            args,
            screenshot_before,
        );
        let start = std::time::Instant::now();

        let result = self
            .inner
            .wait_for_template_frames(name, max_frames, opts)
            .await;
        let elapsed = start.elapsed().as_millis() as u64;
        let screenshot_after = self.capture_thumbnail().await;

        match &result {
            Ok(Some(m)) => {
                self.finish_trace_ok(trace, Some(format!("{:?}", m)), elapsed, screenshot_after)
            }
            Ok(None) => self.finish_trace_ok(
                trace,
                Some("max_frames reached".into()),
                elapsed,
                screenshot_after,
            ),
            Err(e) => self.finish_trace_err(trace, e.to_string(), elapsed),
        }
        result
    }

    async fn wait_gone_frames(&self, name: &str, max_frames: u32) -> Result<bool> {
        let id = self.next_id();
        let args = serde_json::json!({"name": name, "max_frames": max_frames});
        let screenshot_before = self.capture_thumbnail().await;
        let trace = self.make_trace(id, "wait", "wait_gone_frames", args, screenshot_before);
        let start = std::time::Instant::now();

        let result = self.inner.wait_gone_frames(name, max_frames).await;
        let elapsed = start.elapsed().as_millis() as u64;
        let screenshot_after = self.capture_thumbnail().await;

        match &result {
            Ok(gone) => {
                self.finish_trace_ok(trace, Some(gone.to_string()), elapsed, screenshot_after)
            }
            Err(e) => self.finish_trace_err(trace, e.to_string(), elapsed),
        }
        result
    }

    async fn wait_for_color_frames(
        &self,
        x: i32,
        y: i32,
        color: &str,
        max_frames: u32,
    ) -> Result<bool> {
        let id = self.next_id();
        let args = serde_json::json!({"x": x, "y": y, "color": color, "max_frames": max_frames});
        let screenshot_before = self.capture_thumbnail().await;
        let trace = self.make_trace(id, "wait", "wait_for_color_frames", args, screenshot_before);
        let start = std::time::Instant::now();

        let result = self
            .inner
            .wait_for_color_frames(x, y, color, max_frames)
            .await;
        let elapsed = start.elapsed().as_millis() as u64;
        let screenshot_after = self.capture_thumbnail().await;

        match &result {
            Ok(matched) => {
                self.finish_trace_ok(trace, Some(matched.to_string()), elapsed, screenshot_after)
            }
            Err(e) => self.finish_trace_err(trace, e.to_string(), elapsed),
        }
        result
    }

    // === Window ===

    async fn find_window(&self, title: &str) -> Result<Option<u64>> {
        let id = self.next_id();
        let args = serde_json::json!({"title": title});
        let trace = self.make_trace(id, "window", "find_window", args, None);
        let start = std::time::Instant::now();

        let result = self.inner.find_window(title).await;
        let elapsed = start.elapsed().as_millis() as u64;

        match &result {
            Ok(Some(hwnd)) => self.finish_trace_ok(trace, Some(hwnd.to_string()), elapsed, None),
            Ok(None) => self.finish_trace_ok(trace, Some("not found".into()), elapsed, None),
            Err(e) => self.finish_trace_err(trace, e.to_string(), elapsed),
        }
        result
    }

    async fn activate_window(&self, hwnd: u64) -> Result<()> {
        let id = self.next_id();
        let args = serde_json::json!({"hwnd": hwnd});
        let trace = self.make_trace(id, "window", "activate_window", args, None);
        let start = std::time::Instant::now();

        let result = self.inner.activate_window(hwnd).await;
        let elapsed = start.elapsed().as_millis() as u64;

        match &result {
            Ok(()) => self.finish_trace_ok(trace, None, elapsed, None),
            Err(e) => self.finish_trace_err(trace, e.to_string(), elapsed),
        }
        result
    }

    async fn get_window_rect(&self, hwnd: u64) -> Result<Rect> {
        let id = self.next_id();
        let args = serde_json::json!({"hwnd": hwnd});
        let trace = self.make_trace(id, "window", "get_window_rect", args, None);
        let start = std::time::Instant::now();

        let result = self.inner.get_window_rect(hwnd).await;
        let elapsed = start.elapsed().as_millis() as u64;

        match &result {
            Ok(rect) => self.finish_trace_ok(trace, Some(format!("{:?}", rect)), elapsed, None),
            Err(e) => self.finish_trace_err(trace, e.to_string(), elapsed),
        }
        result
    }

    async fn get_screen_size(&self) -> Result<(u32, u32)> {
        let id = self.next_id();
        let args = serde_json::json!({});
        let trace = self.make_trace(id, "window", "get_screen_size", args, None);
        let start = std::time::Instant::now();

        let result = self.inner.get_screen_size().await;
        let elapsed = start.elapsed().as_millis() as u64;

        match &result {
            Ok((w, h)) => self.finish_trace_ok(trace, Some(format!("{}x{}", w, h)), elapsed, None),
            Err(e) => self.finish_trace_err(trace, e.to_string(), elapsed),
        }
        result
    }

    // === Inter-script ===

    async fn run_script(&self, name: &str, params: serde_json::Value) -> Result<serde_json::Value> {
        let id = self.next_id();
        let args = serde_json::json!({"name": name, "params": params});
        let trace = self.make_trace(id, "utility", "run_script", args, None);
        let start = std::time::Instant::now();

        let result = self.inner.run_script(name, params).await;
        let elapsed = start.elapsed().as_millis() as u64;

        match &result {
            Ok(val) => {
                self.finish_trace_ok(trace, Some(truncate(&val.to_string(), 200)), elapsed, None)
            }
            Err(e) => self.finish_trace_err(trace, e.to_string(), elapsed),
        }
        result
    }

    async fn call_library(
        &self,
        library: &str,
        function: &str,
        args_payload: serde_json::Value,
    ) -> Result<serde_json::Value> {
        let id = self.next_id();
        let args = serde_json::json!({
            "library": library,
            "function": function,
            "args": args_payload
        });
        let trace = self.make_trace(id, "utility", "call_library", args, None);
        let start = std::time::Instant::now();

        let result = self
            .inner
            .call_library(library, function, args_payload)
            .await;
        let elapsed = start.elapsed().as_millis() as u64;

        match &result {
            Ok(val) => {
                self.finish_trace_ok(trace, Some(truncate(&val.to_string(), 200)), elapsed, None)
            }
            Err(e) => self.finish_trace_err(trace, e.to_string(), elapsed),
        }
        result
    }

    // === Utilities ===

    fn log(&self, level: LogLevel, message: &str) {
        let id = self.next_id();
        let level_str = match level {
            LogLevel::Debug => "debug",
            LogLevel::Info => "info",
            LogLevel::Warn => "warn",
            LogLevel::Error => "error",
        };
        let args = serde_json::json!({"level": level_str, "message": message});
        let trace = EngineEvent::ScriptCallTrace {
            id,
            category: "log".to_string(),
            method: "log".to_string(),
            args,
            result: None,
            success: true,
            error: None,
            screenshot_before: None,
            screenshot_after: None,
            duration_ms: 0,
            timestamp: chrono::Utc::now(),
        };
        self.publish_trace(trace);
        self.inner.log(level, message);
    }

    async fn notify(&self, title: &str, body: &str) -> Result<()> {
        let id = self.next_id();
        let args = serde_json::json!({"title": title, "body": body});
        let trace = self.make_trace(id, "utility", "notify", args, None);
        let start = std::time::Instant::now();

        let result = self.inner.notify(title, body).await;
        let elapsed = start.elapsed().as_millis() as u64;

        match &result {
            Ok(()) => self.finish_trace_ok(trace, None, elapsed, None),
            Err(e) => self.finish_trace_err(trace, e.to_string(), elapsed),
        }
        result
    }

    // === File operations (manifest-scoped) ===

    async fn read_store_file(&self, path: &str) -> Result<String> {
        let id = self.next_id();
        let args = serde_json::json!({"path": path});
        let trace = self.make_trace(id, "utility", "read_store_file", args, None);
        let start = std::time::Instant::now();

        let result = self.inner.read_store_file(path).await;
        let elapsed = start.elapsed().as_millis() as u64;

        match &result {
            Ok(content) => {
                self.finish_trace_ok(trace, Some(truncate(&content, 200)), elapsed, None)
            }
            Err(e) => self.finish_trace_err(trace, e.to_string(), elapsed),
        }
        result
    }

    async fn write_store_file(&self, path: &str, content: &str) -> Result<()> {
        let id = self.next_id();
        let args = serde_json::json!({"path": path, "content_len": content.len()});
        let trace = self.make_trace(id, "utility", "write_store_file", args, None);
        let start = std::time::Instant::now();

        let result = self.inner.write_store_file(path, content).await;
        let elapsed = start.elapsed().as_millis() as u64;

        match &result {
            Ok(()) => self.finish_trace_ok(trace, None, elapsed, None),
            Err(e) => self.finish_trace_err(trace, e.to_string(), elapsed),
        }
        result
    }

    async fn list_store_files(&self, dir: &str) -> Result<Vec<String>> {
        let id = self.next_id();
        let args = serde_json::json!({"dir": dir});
        let trace = self.make_trace(id, "utility", "list_store_files", args, None);
        let start = std::time::Instant::now();

        let result = self.inner.list_store_files(dir).await;
        let elapsed = start.elapsed().as_millis() as u64;

        match &result {
            Ok(files) => {
                self.finish_trace_ok(trace, Some(format!("{} files", files.len())), elapsed, None)
            }
            Err(e) => self.finish_trace_err(trace, e.to_string(), elapsed),
        }
        result
    }

    // === File operations (system-level) ===

    async fn read_file(&self, path: &str) -> Result<String> {
        let id = self.next_id();
        let args = serde_json::json!({"path": path});
        let trace = self.make_trace(id, "utility", "read_file", args, None);
        let start = std::time::Instant::now();

        let result = self.inner.read_file(path).await;
        let elapsed = start.elapsed().as_millis() as u64;

        match &result {
            Ok(content) => {
                self.finish_trace_ok(trace, Some(truncate(&content, 200)), elapsed, None)
            }
            Err(e) => self.finish_trace_err(trace, e.to_string(), elapsed),
        }
        result
    }

    async fn write_file(&self, path: &str, content: &str) -> Result<()> {
        let id = self.next_id();
        let args = serde_json::json!({"path": path, "content_len": content.len()});
        let trace = self.make_trace(id, "utility", "write_file", args, None);
        let start = std::time::Instant::now();

        let result = self.inner.write_file(path, content).await;
        let elapsed = start.elapsed().as_millis() as u64;

        match &result {
            Ok(()) => self.finish_trace_ok(trace, None, elapsed, None),
            Err(e) => self.finish_trace_err(trace, e.to_string(), elapsed),
        }
        result
    }

    async fn list_files(&self, dir: &str) -> Result<Vec<String>> {
        let id = self.next_id();
        let args = serde_json::json!({"dir": dir});
        let trace = self.make_trace(id, "utility", "list_files", args, None);
        let start = std::time::Instant::now();

        let result = self.inner.list_files(dir).await;
        let elapsed = start.elapsed().as_millis() as u64;

        match &result {
            Ok(files) => {
                self.finish_trace_ok(trace, Some(format!("{} files", files.len())), elapsed, None)
            }
            Err(e) => self.finish_trace_err(trace, e.to_string(), elapsed),
        }
        result
    }

    async fn file_exists(&self, path: &str) -> Result<bool> {
        let id = self.next_id();
        let args = serde_json::json!({"path": path});
        let trace = self.make_trace(id, "utility", "file_exists", args, None);
        let start = std::time::Instant::now();

        let result = self.inner.file_exists(path).await;
        let elapsed = start.elapsed().as_millis() as u64;

        match &result {
            Ok(exists) => self.finish_trace_ok(trace, Some(exists.to_string()), elapsed, None),
            Err(e) => self.finish_trace_err(trace, e.to_string(), elapsed),
        }
        result
    }

    // === Network ===

    async fn http_get(&self, url: &str) -> Result<String> {
        let id = self.next_id();
        let args = serde_json::json!({"url": url});
        let trace = self.make_trace(id, "utility", "http_get", args, None);
        let start = std::time::Instant::now();

        let result = self.inner.http_get(url).await;
        let elapsed = start.elapsed().as_millis() as u64;

        match &result {
            Ok(body) => self.finish_trace_ok(trace, Some(truncate(&body, 200)), elapsed, None),
            Err(e) => self.finish_trace_err(trace, e.to_string(), elapsed),
        }
        result
    }

    async fn http_post(&self, url: &str, body: &str) -> Result<String> {
        let id = self.next_id();
        let args = serde_json::json!({"url": url, "body": truncate(body, 200)});
        let trace = self.make_trace(id, "utility", "http_post", args, None);
        let start = std::time::Instant::now();

        let result = self.inner.http_post(url, body).await;
        let elapsed = start.elapsed().as_millis() as u64;

        match &result {
            Ok(resp) => self.finish_trace_ok(trace, Some(truncate(&resp, 200)), elapsed, None),
            Err(e) => self.finish_trace_err(trace, e.to_string(), elapsed),
        }
        result
    }

    // === Storage ===

    async fn storage_get(&self, key: &str) -> Result<Option<serde_json::Value>> {
        let id = self.next_id();
        let args = serde_json::json!({"key": key});
        let trace = self.make_trace(id, "utility", "storage_get", args, None);
        let start = std::time::Instant::now();

        let result = self.inner.storage_get(key).await;
        let elapsed = start.elapsed().as_millis() as u64;

        match &result {
            Ok(Some(val)) => {
                self.finish_trace_ok(trace, Some(truncate(&val.to_string(), 200)), elapsed, None)
            }
            Ok(None) => self.finish_trace_ok(trace, Some("null".into()), elapsed, None),
            Err(e) => self.finish_trace_err(trace, e.to_string(), elapsed),
        }
        result
    }

    async fn storage_set(&self, key: &str, value: serde_json::Value) -> Result<()> {
        let id = self.next_id();
        let args = serde_json::json!({"key": key, "value": truncate(&value.to_string(), 200)});
        let trace = self.make_trace(id, "utility", "storage_set", args, None);
        let start = std::time::Instant::now();

        let result = self.inner.storage_set(key, value).await;
        let elapsed = start.elapsed().as_millis() as u64;

        match &result {
            Ok(()) => self.finish_trace_ok(trace, None, elapsed, None),
            Err(e) => self.finish_trace_err(trace, e.to_string(), elapsed),
        }
        result
    }

    async fn storage_delete(&self, key: &str) -> Result<()> {
        let id = self.next_id();
        let args = serde_json::json!({"key": key});
        let trace = self.make_trace(id, "utility", "storage_delete", args, None);
        let start = std::time::Instant::now();

        let result = self.inner.storage_delete(key).await;
        let elapsed = start.elapsed().as_millis() as u64;

        match &result {
            Ok(()) => self.finish_trace_ok(trace, None, elapsed, None),
            Err(e) => self.finish_trace_err(trace, e.to_string(), elapsed),
        }
        result
    }

    async fn storage_keys(&self) -> Result<Vec<String>> {
        let id = self.next_id();
        let args = serde_json::json!({});
        let trace = self.make_trace(id, "utility", "storage_keys", args, None);
        let start = std::time::Instant::now();

        let result = self.inner.storage_keys().await;
        let elapsed = start.elapsed().as_millis() as u64;

        match &result {
            Ok(keys) => {
                self.finish_trace_ok(trace, Some(format!("{} keys", keys.len())), elapsed, None)
            }
            Err(e) => self.finish_trace_err(trace, e.to_string(), elapsed),
        }
        result
    }

    async fn plugin_call(
        &self,
        plugin_id: &str,
        method: &str,
        args_json: &str,
    ) -> Result<String> {
        let id = self.next_id();
        let args = serde_json::json!({ "plugin_id": plugin_id, "method": method, "args_json": args_json });
        let trace = self.make_trace(id, "plugin", "plugin_call", args, None);
        let start = std::time::Instant::now();

        let result = self.inner.plugin_call(plugin_id, method, args_json).await;
        let elapsed = start.elapsed().as_millis() as u64;

        match &result {
            Ok(val) => self.finish_trace_ok(trace, Some(format!("{} bytes", val.len())), elapsed, None),
            Err(e) => self.finish_trace_err(trace, e.to_string(), elapsed),
        }
        result
    }

    async fn plugin_list(&self) -> Result<String> {
        let id = self.next_id();
        let args = serde_json::json!({});
        let trace = self.make_trace(id, "plugin", "plugin_list", args, None);
        let start = std::time::Instant::now();

        let result = self.inner.plugin_list().await;
        let elapsed = start.elapsed().as_millis() as u64;

        match &result {
            Ok(val) => self.finish_trace_ok(trace, Some(format!("{} bytes", val.len())), elapsed, None),
            Err(e) => self.finish_trace_err(trace, e.to_string(), elapsed),
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_truncate_short() {
        assert_eq!(truncate("hello", 10), "hello");
    }

    #[test]
    fn test_truncate_long() {
        assert_eq!(truncate("hello world", 5), "hello...");
    }

    #[test]
    fn test_truncate_exact() {
        assert_eq!(truncate("hello", 5), "hello");
    }
}
