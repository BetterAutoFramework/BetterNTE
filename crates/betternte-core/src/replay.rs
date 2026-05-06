//! Replay session and artifact schema types (shared, engine-agnostic).

use serde::{Deserialize, Serialize};

/// Engine replay mode (`engine.yaml`: `replay.mode`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReplayMode {
    /// Default: no recording or replay overhead.
    #[default]
    Normal,
    /// Write manifest + timeline under `artifact_root/session_name`.
    Record,
    /// Feed recorded timeline (planned; behavior unchanged in R1).
    Replay,
}

/// Replay section of [`crate::EngineConfig`].
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ReplayConfig {
    pub mode: ReplayMode,
    /// Parent directory holding session folders (`artifact_root/session_name`).
    /// Relative paths resolve against engine `base_dir`.
    pub artifact_root: String,
    pub session_name: String,
    /// When `mode == record`: write every N-th captured frame under `frames/` plus a
    /// `timeline.jsonl` line (`kind`: `frame`). `0` disables frame PNG recording (events only).
    pub frame_sample_interval: u32,
}

/// Artifact `manifest.json` (R1+).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplayArtifactManifest {
    pub schema_version: u32,
    pub session_name: String,
    pub mode: String,
    pub created_at: String,
    pub engine_version: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub config_hash: Option<String>,
    pub capture_method: String,
    pub fps_cap: u32,
    /// Queue throttle configured for script input execution (ops/sec).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_rate_limit: Option<u32>,
    /// Whether input queue wrapper was enabled (`input_rate_limit > 0`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub queue_enabled: Option<bool>,
    /// Effective queue min interval in nanoseconds.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub queue_min_interval_ns: Option<u64>,
    /// Effective queue min interval in milliseconds (floor).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub queue_min_interval_ms: Option<u64>,
    pub frame_count: u64,
    pub event_count: u64,
}

/// Optional checks against **`manifest.json`** counters / fields.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct ReplayManifestExpect {
    /// Manifest `event_count` must be **`>=`** this when set.
    pub min_event_count: Option<u64>,
    /// Manifest `frame_count` must be **`>=`** this when set.
    pub min_frame_count: Option<u64>,
    /// When set, `manifest.session_name` must match exactly (UTF‑8 sensitive).
    pub session_name_equals: Option<String>,
}

/// Optional checks against the **last** `TaskStopped` `engine_event` in the timeline (`payload.type`/`data`).
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct ReplayLastTaskStoppedExpect {
    /// Must match **`payload.data.task_name`** exactly.
    pub task_name_equals: Option<String>,
    /// Must match serde JSON object key under **`payload.data.reason`**, e.g. `completed`,
    /// `user_cancelled`, `emergency_stop`, `timeout`, `error`.
    pub reason_discriminant: Option<String>,
    /// If set, substring match on **`serde_json::to_string(reason)`** (covers `error` payloads).
    pub reason_json_contains: Option<String>,
}

/// Optional checks against timeline `kind == "session_meta"` row's
/// `input_pipeline` object.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct ReplaySessionMetaInputPipelineExpect {
    /// Require exact equality against `session_meta.input_pipeline.input_rate_limit`.
    pub input_rate_limit_equals: Option<u32>,
    /// Require exact equality against `session_meta.input_pipeline.queue_enabled`.
    pub queue_enabled_equals: Option<bool>,
    /// Require exact equality against `session_meta.input_pipeline.queue_min_interval_ns`.
    pub queue_min_interval_ns_equals: Option<u64>,
    /// Require exact equality against `session_meta.input_pipeline.queue_min_interval_ms`.
    pub queue_min_interval_ms_equals: Option<u64>,
}

/// Assertions for **`timeline.jsonl`** regression checks (`replay_expect.json`).
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct ReplayExpect {
    /// Subsequence match on `engine_event` rows: `event_type` first, else `payload.type`.
    #[serde(default)]
    pub must_contain_event_types_in_order: Vec<String>,
    #[serde(default)]
    pub min_engine_event_lines: usize,
    #[serde(default)]
    pub min_frame_lines: usize,
    #[serde(default)]
    pub manifest: ReplayManifestExpect,
    /// When set with at least one non-empty predicate, verifies the timeline’s last `TaskStopped`.
    #[serde(default)]
    pub last_task_stopped: Option<ReplayLastTaskStoppedExpect>,
    /// When set with at least one predicate, verifies `session_meta.input_pipeline`.
    #[serde(default)]
    pub session_meta_input_pipeline: Option<ReplaySessionMetaInputPipelineExpect>,
}
