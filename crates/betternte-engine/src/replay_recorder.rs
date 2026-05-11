//! Record replay artifacts under `replay.mode == record`:
//! - timeline.jsonl (`engine_event`, optional sampled `frame`, `script_input` from script APIs),
//! - manifest.json finalized on shutdown.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use anyhow::Context;
use betternte_core::{EngineConfig, EngineEvent, ReplayArtifactManifest};
use serde_json::{json, Map, Value};
use tokio::io::AsyncWriteExt;
use tokio::sync::broadcast::error::RecvError;
use tokio::sync::Mutex;
use tracing::{info, warn};

use crate::{event::EventBus, Engine};

/// Shared `spawn_blocking` error handling for replay PNG encode/write (log strings unchanged).
enum ReplayBlockingOp {
    PngEncode,
    PngWrite { path: String },
}

impl ReplayBlockingOp {
    fn warn_op_failed(&self, err: impl std::fmt::Display) {
        match self {
            Self::PngEncode => warn!(
                error = %err,
                "replay: PNG encode failed, skipping frame sample"
            ),
            Self::PngWrite { path } => warn!(
                error = %err,
                path = path.as_str(),
                "replay: write frame PNG failed"
            ),
        }
    }

    fn warn_join_failed(&self, err: impl std::fmt::Display) {
        match self {
            Self::PngEncode => warn!(error = %err, "replay: PNG encode task failed"),
            Self::PngWrite { .. } => warn!(error = %err, "replay: PNG write task failed"),
        }
    }
}

async fn replay_spawn_blocking<F, T, E>(op: ReplayBlockingOp, f: F) -> Option<T>
where
    F: FnOnce() -> Result<T, E> + Send + 'static,
    T: Send + 'static,
    E: std::fmt::Display + Send + 'static,
{
    match tokio::task::spawn_blocking(f).await {
        Ok(Ok(v)) => Some(v),
        Ok(Err(e)) => {
            op.warn_op_failed(e);
            None
        }
        Err(e) => {
            op.warn_join_failed(e);
            None
        }
    }
}

pub(crate) struct ReplaySessionInner {
    pub timeline: Arc<Mutex<tokio::fs::File>>,
    pub session_dir: PathBuf,
    t0: std::time::Instant,
    pub frame_sample_interval: u32,
    next_frame_id: AtomicU64,
    frames_saved: AtomicU64,
}

impl ReplaySessionInner {
    fn timeline_t_ms(&self) -> u64 {
        self.t0.elapsed().as_millis() as u64
    }

    pub(crate) async fn append_timeline_object(
        &self,
        mut obj: Map<String, Value>,
    ) -> anyhow::Result<()> {
        obj.insert("t_ms".into(), json!(self.timeline_t_ms()));

        let mut buf = serde_json::to_vec(&Value::Object(obj)).context("replay timeline serde")?;
        buf.push(b'\n');
        let mut f = self.timeline.lock().await;
        f.write_all(&buf).await?;
        Ok(())
    }

    pub(crate) async fn record_frame_sample(
        self: &Arc<Self>,
        frame: &betternte_core::CaptureFrame,
    ) {
        let n = self.next_frame_id.fetch_add(1, Ordering::Relaxed) + 1;
        let fname = format!("{:06}.png", n);
        let rel_png = Path::new("frames").join(&fname);
        let abs_png = self.session_dir.join(&rel_png);
        let png_path_logged = abs_png.display().to_string();

        let width = frame.width;
        let height = frame.height;
        let capture_seq = frame.sequence;

        let frame = frame.clone();
        let Some(png_bytes) =
            replay_spawn_blocking(ReplayBlockingOp::PngEncode, move || frame.to_bytes("png")).await
        else {
            return;
        };

        if replay_spawn_blocking(
            ReplayBlockingOp::PngWrite {
                path: png_path_logged,
            },
            move || std::fs::write(&abs_png, png_bytes),
        )
        .await
        .is_none()
        {
            return;
        }

        self.frames_saved.fetch_add(1, Ordering::Relaxed);

        let rel_str = rel_png.to_string_lossy().replace('\\', "/");

        let mut m = Map::new();
        m.insert("kind".into(), json!("frame"));
        m.insert("frame_id".into(), json!(n));
        m.insert("path".into(), json!(rel_str));
        m.insert("width".into(), json!(width));
        m.insert("height".into(), json!(height));
        m.insert("sequence".into(), json!(capture_seq));

        if let Err(e) = self.append_timeline_object(m).await {
            warn!(error = %e, "replay: append frame timeline failed");
        }
    }

    /// Log a script-driven input line into **`timeline.jsonl`** (`kind`: `script_input`).
    ///
    /// `args` include **`frame_ref`**: incrementing id of the shared capture snapshot when the action ran
    /// (helps correlate `script_input` with nearby `frame` rows and `CaptureFrame.sequence`).
    pub(crate) async fn append_script_input(
        &self,
        method: &str,
        args: Value,
        ok: bool,
        error: Option<String>,
    ) -> anyhow::Result<()> {
        let mut m = Map::new();
        m.insert("kind".into(), json!("script_input"));
        m.insert("method".into(), json!(method));
        m.insert("args".into(), args);
        m.insert("ok".into(), json!(ok));
        if let Some(e) = error {
            if !ok {
                m.insert("error".into(), json!(e));
            }
        }
        self.append_timeline_object(m).await
    }
}

pub(crate) struct ReplayRecording {
    pub stop_tx: tokio::sync::watch::Sender<bool>,
    pub join: tokio::task::JoinHandle<Result<(), anyhow::Error>>,
    pub session: Option<Arc<ReplaySessionInner>>,
}

fn queue_min_interval_ns(rate_limit: u32) -> u64 {
    if rate_limit == 0 {
        0
    } else {
        1_000_000_000u64 / u64::from(rate_limit).max(1)
    }
}

fn build_session_meta_object(manifest: &ReplayArtifactManifest) -> Map<String, Value> {
    let mut session_meta = serde_json::Map::new();
    session_meta.insert("kind".into(), json!("session_meta"));
    session_meta.insert("schema_version".into(), json!(manifest.schema_version));
    session_meta.insert("session_name".into(), json!(manifest.session_name));
    session_meta.insert("engine_version".into(), json!(manifest.engine_version));
    session_meta.insert("capture_method".into(), json!(manifest.capture_method));
    session_meta.insert("fps_cap".into(), json!(manifest.fps_cap));
    session_meta.insert(
        "input_pipeline".into(),
        json!({
            "input_rate_limit": manifest.input_rate_limit,
            "queue_enabled": manifest.queue_enabled,
            "queue_min_interval_ns": manifest.queue_min_interval_ns,
            "queue_min_interval_ms": manifest.queue_min_interval_ms,
        }),
    );
    session_meta
}

pub(crate) fn try_start_replay_recording(
    bus: EventBus,
    base_dir: &std::path::Path,
    config: &EngineConfig,
    engine_version: &'static str,
) -> anyhow::Result<Option<ReplayRecording>> {
    use betternte_core::ReplayMode;
    if config.replay.mode != ReplayMode::Record {
        return Ok(None);
    }
    let root = config.replay.artifact_root.trim();
    let session = config.replay.session_name.trim();
    if root.is_empty() || session.is_empty() {
        warn!("replay.record skipped: artifact_root or session_name is empty");
        return Ok(None);
    }

    let session_dir = Engine::resolve_path(root, base_dir)
        .join(session);
    std::fs::create_dir_all(&session_dir)
        .with_context(|| format!("replay record mkdir {:?}", session_dir.display()))?;

    let frames_interval = config.replay.frame_sample_interval;
    if frames_interval > 0 {
        let frames_dir = session_dir.join("frames");
        std::fs::create_dir_all(&frames_dir)
            .with_context(|| format!("replay record mkdir {:?}", frames_dir.display()))?;
    }

    let timeline_path = session_dir.join("timeline.jsonl");
    let manifest_path = session_dir.join("manifest.json");
    let created_at = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);

    let input_rate_limit = config.advanced.input_rate_limit;
    let queue_enabled = input_rate_limit > 0;
    let queue_min_interval_ns = if queue_enabled {
        Some(queue_min_interval_ns(input_rate_limit))
    } else {
        None
    };
    let queue_min_interval_ms = queue_min_interval_ns.map(|ns| ns / 1_000_000);

    let manifest = ReplayArtifactManifest {
        schema_version: 1,
        session_name: session.to_string(),
        mode: "record".to_string(),
        created_at,
        engine_version: engine_version.to_string(),
        config_hash: None,
        capture_method: config.capture.method.to_string(),
        fps_cap: config.capture.fps_cap,
        input_rate_limit: Some(input_rate_limit),
        queue_enabled: Some(queue_enabled),
        queue_min_interval_ns,
        queue_min_interval_ms,
        frame_count: 0,
        event_count: 0,
    };
    std::fs::write(
        &manifest_path,
        serde_json::to_string_pretty(&manifest).unwrap_or_else(|_| "{}".into()),
    )
    .with_context(|| format!("replay write {:?}", manifest_path.display()))?;

    let std_file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&timeline_path)
        .with_context(|| format!("replay open {:?}", timeline_path.display()))?;

    let file = tokio::fs::File::from_std(std_file);

    let t0 = std::time::Instant::now();

    let session_inner = Arc::new(ReplaySessionInner {
        timeline: Arc::new(Mutex::new(file)),
        session_dir: session_dir.clone(),
        t0,
        frame_sample_interval: frames_interval,
        next_frame_id: AtomicU64::new(0),
        frames_saved: AtomicU64::new(0),
    });

    let (stop_tx, stop_rx) = tokio::sync::watch::channel(false);
    let rx = bus.subscribe();
    let session_for_task = Arc::clone(&session_inner);

    info!(
        path = %session_dir.display(),
        frame_sample_interval = frames_interval,
        "replay recorder started",
    );

    let join = tokio::spawn(replay_record_loop(
        manifest_path.clone(),
        manifest,
        session_for_task,
        stop_rx,
        rx,
    ));

    Ok(Some(ReplayRecording {
        stop_tx,
        join,
        session: Some(session_inner),
    }))
}

async fn replay_record_loop(
    manifest_path: PathBuf,
    mut manifest: ReplayArtifactManifest,
    session: Arc<ReplaySessionInner>,
    mut stop_rx: tokio::sync::watch::Receiver<bool>,
    mut rx: tokio::sync::broadcast::Receiver<EngineEvent>,
) -> Result<(), anyhow::Error> {
    let mut count: u64 = 0;

    // Emit one structured session metadata row at the beginning so offline
    // replay analysis can inspect input pipeline settings without reading
    // engine runtime logs.
    let session_meta = build_session_meta_object(&manifest);
    session.append_timeline_object(session_meta).await?;

    loop {
        tokio::select! {
            biased;
            res = stop_rx.changed() => {
                match res {
                    Ok(_) if *stop_rx.borrow() => break,
                    Err(_) => break,
                    Ok(_) => {}
                }
            }
            recv = rx.recv() => {
                let ev = match recv {
                    Ok(v) => v,
                    Err(RecvError::Lagged(_)) => continue,
                    Err(RecvError::Closed) => break,
                };
                count += 1;
                let payload = serde_json::to_value(&ev)
                    .unwrap_or_else(|_| json!({ "serialization": "failed" }));

                let mut obj = serde_json::Map::new();
                obj.insert("kind".into(), json!("engine_event"));
                obj.insert(
                    "event_type".into(),
                    payload.get("type").cloned().unwrap_or(json!("unknown")),
                );
                obj.insert("payload".into(), payload);
                session.append_timeline_object(obj).await?;
            }
        }
    }

    manifest.event_count = count;
    manifest.frame_count = session.frames_saved.load(Ordering::Relaxed);

    std::fs::write(&manifest_path, serde_json::to_string_pretty(&manifest)?)
        .with_context(|| format!("replay finalize {:?}", manifest_path.display()))?;

    info!(
        manifest = %manifest_path.display(),
        events = count,
        frames = manifest.frame_count,
        "replay recorder finalized",
    );
    Ok(())
}

#[cfg(test)]
mod replay_recording_unit_tests {
    use super::*;

    #[test]
    fn queue_min_interval_ns_matches_rate_formula() {
        assert_eq!(queue_min_interval_ns(0), 0);
        assert_eq!(queue_min_interval_ns(1), 1_000_000_000);
        assert_eq!(queue_min_interval_ns(20), 50_000_000);
        assert_eq!(queue_min_interval_ns(1000), 1_000_000);
    }

    #[test]
    fn session_meta_contains_input_pipeline_queue_fields() {
        let manifest = ReplayArtifactManifest {
            schema_version: 1,
            session_name: "s".into(),
            mode: "record".into(),
            created_at: "t".into(),
            engine_version: "v".into(),
            config_hash: None,
            capture_method: "auto".into(),
            fps_cap: 60,
            input_rate_limit: Some(25),
            queue_enabled: Some(true),
            queue_min_interval_ns: Some(40_000_000),
            queue_min_interval_ms: Some(40),
            frame_count: 0,
            event_count: 0,
        };
        let obj = build_session_meta_object(&manifest);
        assert_eq!(obj.get("kind"), Some(&json!("session_meta")));
        let pipeline = obj
            .get("input_pipeline")
            .and_then(|v| v.as_object())
            .expect("input_pipeline object");
        assert_eq!(pipeline.get("input_rate_limit"), Some(&json!(25)));
        assert_eq!(pipeline.get("queue_enabled"), Some(&json!(true)));
        assert_eq!(
            pipeline.get("queue_min_interval_ns"),
            Some(&json!(40_000_000))
        );
        assert_eq!(pipeline.get("queue_min_interval_ms"), Some(&json!(40)));
    }
}
