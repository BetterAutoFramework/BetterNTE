//! Load `replay_expect.json` and verify a recorded **`timeline.jsonl`** (R3).

use std::fs;
use std::path::Path;

use anyhow::{bail, Context};
use serde_json::Value;

use betternte_core::{ReplayArtifactManifest, ReplayExpect};

/// Load expectations from JSON (UTF‑8).
pub fn load_expect_from_file(path: &Path) -> anyhow::Result<ReplayExpect> {
    let s = fs::read_to_string(path).with_context(|| format!("replay expect {:?}", path))?;
    serde_json::from_str(&s).with_context(|| format!("replay expect parse {:?}", path))
}

/// Parse **`timeline.jsonl`** into serde values (skips blank lines).
pub fn parse_timeline_jsonl(path: &Path) -> anyhow::Result<Vec<Value>> {
    let s = fs::read_to_string(path).with_context(|| format!("timeline {:?}", path))?;
    Ok(parse_timelines_str(&s))
}

fn parse_timelines_str(raw: &str) -> Vec<Value> {
    let mut rows = Vec::new();
    for line in raw.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        match serde_json::from_str::<Value>(line) {
            Ok(v) => rows.push(v),
            Err(e) => {
                tracing::warn!(error = %e, "replay_verify: skipping bad jsonl line");
            }
        }
    }
    rows
}

/// Extract engine event variant string from one timeline row (`kind` must be `"engine_event"`).
pub fn row_engine_event_type(row: &Value) -> Option<String> {
    if row.get("kind").and_then(|k| k.as_str()) != Some("engine_event") {
        return None;
    }

    row.get("event_type")
        .and_then(|x| match x {
            Value::String(s) => Some(s.clone()),
            Value::Bool(b) => Some(b.to_string()),
            Value::Number(n) => Some(n.to_string()),
            _ => None,
        })
        .or_else(|| {
            row.get("payload")
                .and_then(|p| p.get("type"))
                .and_then(|x| match x {
                    Value::String(s) => Some(s.clone()),
                    _ => None,
                })
        })
}

fn count_kind(rows: &[Value], want: &str) -> usize {
    rows.iter()
        .filter(|v| v.get("kind").and_then(|k| k.as_str()) == Some(want))
        .count()
}

fn event_seq(rows: &[Value]) -> Vec<String> {
    let mut seq = Vec::new();
    for r in rows {
        if let Some(t) = row_engine_event_type(r) {
            seq.push(t);
        }
    }
    seq
}

/// True if **`needle`** appears as subsequence inside **`hay`** (order preserved).
pub fn event_types_subsequence_matches(needle: &[String], hay: &[String]) -> bool {
    if needle.is_empty() {
        return true;
    }
    let mut j = 0usize;
    for h in hay {
        if j < needle.len() && h == needle[j].as_str() {
            j += 1;
        }
    }
    j == needle.len()
}

pub fn verify_timeline(expect: &ReplayExpect, rows: &[Value]) -> anyhow::Result<()> {
    let n_eng = count_kind(rows, "engine_event");
    if expect.min_engine_event_lines > 0 && n_eng < expect.min_engine_event_lines {
        bail!(
            "replay verify: wanted at least {} engine_event rows, got {}",
            expect.min_engine_event_lines,
            n_eng,
        );
    }

    let n_frame = count_kind(rows, "frame");
    if expect.min_frame_lines > 0 && n_frame < expect.min_frame_lines {
        bail!(
            "replay verify: wanted at least {} frame rows, got {}",
            expect.min_frame_lines,
            n_frame,
        );
    }

    let seq = event_seq(rows);
    if !event_types_subsequence_matches(&expect.must_contain_event_types_in_order, &seq) {
        bail!(
            "replay verify: event subsequence {:?} not found in {:?}",
            expect.must_contain_event_types_in_order,
            seq,
        );
    }

    verify_last_task_stopped(expect.last_task_stopped.as_ref(), rows)?;
    verify_session_meta_input_pipeline(expect.session_meta_input_pipeline.as_ref(), rows)?;

    Ok(())
}

fn session_meta_input_pipeline_active(
    cfg: &betternte_core::ReplaySessionMetaInputPipelineExpect,
) -> bool {
    cfg.input_rate_limit_equals.is_some()
        || cfg.queue_enabled_equals.is_some()
        || cfg.queue_min_interval_ns_equals.is_some()
        || cfg.queue_min_interval_ms_equals.is_some()
}

fn first_session_meta_row(rows: &[Value]) -> Option<&Value> {
    rows.iter()
        .find(|row| row.get("kind").and_then(|k| k.as_str()) == Some("session_meta"))
}

pub fn verify_session_meta_input_pipeline(
    expect: Option<&betternte_core::ReplaySessionMetaInputPipelineExpect>,
    rows: &[Value],
) -> anyhow::Result<()> {
    let Some(cfg) = expect else {
        return Ok(());
    };
    if !session_meta_input_pipeline_active(cfg) {
        return Ok(());
    }

    let row = first_session_meta_row(rows)
        .ok_or_else(|| anyhow::anyhow!("replay verify: no session_meta row"))?;
    let pipeline = row
        .get("input_pipeline")
        .and_then(|v| v.as_object())
        .ok_or_else(|| anyhow::anyhow!("replay verify: session_meta missing input_pipeline"))?;

    if let Some(want) = cfg.input_rate_limit_equals {
        let got = pipeline.get("input_rate_limit").and_then(|v| v.as_u64());
        if got != Some(u64::from(want)) {
            anyhow::bail!(
                "replay verify: session_meta.input_pipeline.input_rate_limit {:?} expected {:?}",
                got,
                want,
            );
        }
    }
    if let Some(want) = cfg.queue_enabled_equals {
        let got = pipeline.get("queue_enabled").and_then(|v| v.as_bool());
        if got != Some(want) {
            anyhow::bail!(
                "replay verify: session_meta.input_pipeline.queue_enabled {:?} expected {:?}",
                got,
                want,
            );
        }
    }
    if let Some(want) = cfg.queue_min_interval_ns_equals {
        let got = pipeline
            .get("queue_min_interval_ns")
            .and_then(|v| v.as_u64());
        if got != Some(want) {
            anyhow::bail!(
                "replay verify: session_meta.input_pipeline.queue_min_interval_ns {:?} expected {:?}",
                got,
                want,
            );
        }
    }
    if let Some(want) = cfg.queue_min_interval_ms_equals {
        let got = pipeline
            .get("queue_min_interval_ms")
            .and_then(|v| v.as_u64());
        if got != Some(want) {
            anyhow::bail!(
                "replay verify: session_meta.input_pipeline.queue_min_interval_ms {:?} expected {:?}",
                got,
                want,
            );
        }
    }

    Ok(())
}

fn last_task_stopped_active(cfg: &betternte_core::ReplayLastTaskStoppedExpect) -> bool {
    cfg.task_name_equals.as_ref().is_some_and(|s| !s.is_empty())
        || cfg
            .reason_discriminant
            .as_ref()
            .is_some_and(|s| !s.is_empty())
        || cfg
            .reason_json_contains
            .as_ref()
            .is_some_and(|s| !s.is_empty())
}

fn last_task_stopped_row(rows: &[Value]) -> Option<&Value> {
    rows.iter()
        .rev()
        .find(|row| row_engine_event_type(row).as_deref() == Some("TaskStopped"))
}

/// Serde emits unit variants as a plain **`"completed"`** string or externally-tagged **`{"error":"…"}`** style object.
fn stop_reason_variant_key(reason: &Value) -> anyhow::Result<String> {
    match reason {
        Value::String(s) => Ok(s.clone()),
        Value::Object(map) => match map.len() {
            0 => anyhow::bail!("replay verify: TaskStopped reason empty object"),
            1 => map
                .keys()
                .next()
                .map(|k| k.to_string())
                .ok_or_else(|| anyhow::anyhow!("replay verify: TaskStopped reason no key")),
            _ => anyhow::bail!(
                "replay verify: TaskStopped.reason must have single serde variant key: {:?}",
                reason
            ),
        },
        _ => anyhow::bail!(
            "replay verify: TaskStopped.reason has unexpected JSON shape {:?}",
            reason
        ),
    }
}

pub fn verify_last_task_stopped(
    expect: Option<&betternte_core::ReplayLastTaskStoppedExpect>,
    rows: &[Value],
) -> anyhow::Result<()> {
    let Some(cfg) = expect else {
        return Ok(());
    };
    if !last_task_stopped_active(cfg) {
        return Ok(());
    }

    let row = last_task_stopped_row(rows)
        .ok_or_else(|| anyhow::anyhow!("replay verify: no TaskStopped row"))?;

    let payload = row
        .get("payload")
        .ok_or_else(|| anyhow::anyhow!("replay verify: TaskStopped row missing payload"))?;

    let data = payload
        .get("data")
        .ok_or_else(|| anyhow::anyhow!("replay verify: TaskStopped payload missing data"))?;

    if let Some(want) = cfg.task_name_equals.as_ref().filter(|s| !s.is_empty()) {
        let got = data.get("task_name").and_then(|v| v.as_str()).unwrap_or("");
        if got != want {
            anyhow::bail!(
                "replay verify: last TaskStopped.task_name {:?} expected {:?}",
                got,
                want,
            );
        }
    }

    let reason_v = data
        .get("reason")
        .ok_or_else(|| anyhow::anyhow!("replay verify: TaskStopped.data missing reason"))?;

    if let Some(want) = cfg.reason_discriminant.as_ref().filter(|s| !s.is_empty()) {
        let key = stop_reason_variant_key(reason_v)?;
        if key != *want {
            anyhow::bail!(
                "replay verify: TaskStopped.reason key {:?}, expected discrimin {:?}",
                key,
                want,
            );
        }
    }

    if let Some(needle) = cfg.reason_json_contains.as_ref().filter(|s| !s.is_empty()) {
        let hay = serde_json::to_string(reason_v).unwrap_or_default();
        if !hay.contains(needle.as_str()) {
            anyhow::bail!(
                "replay verify: TaskStopped.reason JSON does not contain {:?} (had {})",
                needle,
                hay,
            );
        }
    }

    Ok(())
}

fn manifest_expect_active(expect: &betternte_core::ReplayManifestExpect) -> bool {
    expect.min_event_count.is_some()
        || expect.min_frame_count.is_some()
        || expect
            .session_name_equals
            .as_ref()
            .is_some_and(|s| !s.is_empty())
}

/// Validate **`ReplayArtifactManifest`** against nested **`expect.manifest`**.
pub fn verify_manifest_expect(
    expect: &betternte_core::ReplayManifestExpect,
    path: &Path,
) -> anyhow::Result<()> {
    let s = fs::read_to_string(path).with_context(|| format!("manifest {:?}", path))?;
    let m: ReplayArtifactManifest =
        serde_json::from_str(&s).with_context(|| format!("manifest parse {:?}", path))?;

    if let Some(min) = expect.min_event_count {
        if m.event_count < min {
            bail!(
                "replay manifest: event_count {} < required {}",
                m.event_count,
                min,
            );
        }
    }
    if let Some(min) = expect.min_frame_count {
        if m.frame_count < min {
            bail!(
                "replay manifest: frame_count {} < required {}",
                m.frame_count,
                min,
            );
        }
    }
    if let Some(want) = expect
        .session_name_equals
        .as_ref()
        .filter(|s| !s.is_empty())
    {
        if &m.session_name != want {
            bail!(
                "replay manifest: session_name {:?} want {:?}",
                m.session_name,
                want,
            );
        }
    }

    Ok(())
}

/// Timeline + optional **`manifest.json`** (required when **`expect.manifest`** has checks).
pub fn verify_artifact_paths(
    expect_path: &Path,
    timeline_path: &Path,
    manifest_path: Option<&Path>,
) -> anyhow::Result<()> {
    let expect = load_expect_from_file(expect_path)?;
    let rows = parse_timeline_jsonl(timeline_path)?;
    verify_timeline(&expect, &rows)?;

    if manifest_expect_active(&expect.manifest) {
        let mp = manifest_path.ok_or_else(|| {
            anyhow::anyhow!(
                "replay_expect.json sets manifest constraints; supply manifest.json (--manifest)"
            )
        })?;
        verify_manifest_expect(&expect.manifest, mp)?;
    }

    Ok(())
}

/// Verify **`timeline_path`** satisfies expectations (**`timeline` only**, no manifest).
pub fn verify_timeline_paths(expect_path: &Path, timeline_path: &Path) -> anyhow::Result<()> {
    verify_artifact_paths(expect_path, timeline_path, None)
}

/// Expected filenames inside a replay **session** artifact directory.
pub const SESSION_REPLAY_EXPECT: &str = "replay_expect.json";
pub const SESSION_TIMELINE: &str = "timeline.jsonl";
pub const SESSION_MANIFEST: &str = "manifest.json";

/// Load **`replay_expect.json`**, **`timeline.jsonl`**, and **`manifest.json`** when present /
/// required by expectations under **`session_dir`**.
pub fn verify_session_directory(session_dir: &Path) -> anyhow::Result<()> {
    let expect_p = session_dir.join(SESSION_REPLAY_EXPECT);
    let timeline_p = session_dir.join(SESSION_TIMELINE);
    let manifest_p = session_dir.join(SESSION_MANIFEST);

    if !expect_p.is_file() {
        anyhow::bail!(
            "replay session verify: missing {} in {}",
            SESSION_REPLAY_EXPECT,
            session_dir.display(),
        );
    }
    if !timeline_p.is_file() {
        anyhow::bail!(
            "replay session verify: missing {}/{}",
            session_dir.display(),
            SESSION_TIMELINE
        );
    }

    let exp = load_expect_from_file(&expect_p)?;
    if manifest_expect_active(&exp.manifest) && !manifest_p.is_file() {
        anyhow::bail!(
            "replay session verify: expect.manifest requires {}",
            manifest_p.display(),
        );
    }

    verify_artifact_paths(
        &expect_p,
        &timeline_p,
        manifest_p.is_file().then(|| manifest_p.as_path()),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use betternte_core::{
        ReplayArtifactManifest, ReplayExpect, ReplayLastTaskStoppedExpect,
        ReplaySessionMetaInputPipelineExpect,
    };

    fn row_engine(name: &str) -> Value {
        serde_json::json!({
            "t_ms": 0,
            "kind": "engine_event",
            "event_type": name,
            "payload": {"type": name}
        })
    }

    #[test]
    fn subsequence_and_counts() {
        let rows = vec![
            row_engine("ScriptLoaded"),
            row_engine("TaskStarted"),
            row_engine("LogMessage"),
            row_engine("TaskStopped"),
            serde_json::json!({"t_ms":1,"kind":"frame","frame_id":1}),
        ];

        let ex = ReplayExpect {
            must_contain_event_types_in_order: vec!["TaskStarted".into(), "TaskStopped".into()],
            min_engine_event_lines: 3,
            min_frame_lines: 1,
            ..Default::default()
        };

        verify_timeline(&ex, &rows).expect("ok");

        let fail = ReplayExpect {
            must_contain_event_types_in_order: vec!["TaskStopped".into(), "TaskStarted".into()],
            ..Default::default()
        };
        assert!(verify_timeline(&fail, &rows).is_err());
    }

    #[test]
    fn file_roundtrip_via_temp_files() {
        let td = tempfile::tempdir().unwrap();

        fs::write(
            td.path().join("expect.json"),
            r#"{"must_contain_event_types_in_order":["A","B"],"min_engine_event_lines":2}"#,
        )
        .unwrap();

        fs::write(
            td.path().join("timeline.jsonl"),
            "{\"kind\":\"engine_event\",\"event_type\":\"A\",\"payload\":{\"type\":\"A\"}}\n{\"kind\":\"engine_event\",\"event_type\":\"noise\",\"payload\":{\"type\":\"noise\"}}\n{\"kind\":\"engine_event\",\"event_type\":\"B\",\"payload\":{\"type\":\"B\"}}\n",
        )
        .unwrap();

        verify_timeline_paths(
            &td.path().join("expect.json"),
            &td.path().join("timeline.jsonl"),
        )
        .unwrap();

        fs::write(
            td.path().join("bad_expect.json"),
            r#"{"must_contain_event_types_in_order":["B","A"],"min_engine_event_lines":0}"#,
        )
        .unwrap();

        assert!(verify_timeline_paths(
            &td.path().join("bad_expect.json"),
            &td.path().join("timeline.jsonl")
        )
        .is_err());
    }

    #[test]
    fn verify_session_directory_standard_filenames() {
        let td = tempfile::tempdir().unwrap();

        fs::write(
            td.path().join(SESSION_REPLAY_EXPECT),
            r#"{"must_contain_event_types_in_order":["X"]}"#,
        )
        .unwrap();
        fs::write(
            td.path().join(SESSION_TIMELINE),
            "{\"kind\":\"engine_event\",\"event_type\":\"X\",\"payload\":{\"type\":\"X\"}}\n",
        )
        .unwrap();

        verify_session_directory(td.path()).unwrap();

        fs::write(
            td.path().join(SESSION_REPLAY_EXPECT),
            r#"{"manifest":{"min_event_count":1}}"#,
        )
        .unwrap();
        assert!(verify_session_directory(td.path()).is_err());
    }

    #[test]
    fn last_task_stopped_payload_checks() {
        use betternte_core::{EngineEvent, TaskStopReason};
        use chrono::Utc;

        fn wrap_ev(ev: &EngineEvent) -> Value {
            let payload = serde_json::to_value(ev).unwrap();
            let et = payload
                .get("type")
                .and_then(|v| v.as_str())
                .unwrap()
                .to_owned();
            serde_json::json!({
                "kind": "engine_event",
                "event_type": et,
                "payload": payload,
            })
        }

        let stopped = EngineEvent::TaskStopped {
            task_name: "demo".into(),
            reason: TaskStopReason::Completed,
            duration_ms: 1,
            timestamp: Utc::now(),
        };
        let rows = vec![
            wrap_ev(&EngineEvent::TaskStarted {
                task_name: "demo".into(),
                task_type: "solo".into(),
                timestamp: Utc::now(),
            }),
            wrap_ev(&stopped),
        ];

        let ok = ReplayLastTaskStoppedExpect {
            task_name_equals: Some("demo".into()),
            reason_discriminant: Some("completed".into()),
            ..Default::default()
        };
        verify_last_task_stopped(Some(&ok), &rows).unwrap();

        let bad_task = ReplayLastTaskStoppedExpect {
            task_name_equals: Some("other".into()),
            ..Default::default()
        };
        assert!(verify_last_task_stopped(Some(&bad_task), &rows).is_err());

        let bad_disc = ReplayLastTaskStoppedExpect {
            reason_discriminant: Some("timeout".into()),
            ..Default::default()
        };
        assert!(verify_last_task_stopped(Some(&bad_disc), &rows).is_err());

        let stopped_err = EngineEvent::TaskStopped {
            task_name: "demo".into(),
            reason: TaskStopReason::Error("boom timeout".into()),
            duration_ms: 2,
            timestamp: Utc::now(),
        };
        let rows2 = vec![wrap_ev(&stopped_err)];
        let sub = ReplayLastTaskStoppedExpect {
            reason_discriminant: Some("error".into()),
            reason_json_contains: Some("boom".into()),
            ..Default::default()
        };
        verify_last_task_stopped(Some(&sub), &rows2).unwrap();
    }

    #[test]
    fn session_meta_input_pipeline_checks() {
        let rows = vec![serde_json::json!({
            "kind": "session_meta",
            "input_pipeline": {
                "input_rate_limit": 25,
                "queue_enabled": true,
                "queue_min_interval_ns": 40_000_000u64,
                "queue_min_interval_ms": 40u64
            }
        })];

        let ok = ReplaySessionMetaInputPipelineExpect {
            input_rate_limit_equals: Some(25),
            queue_enabled_equals: Some(true),
            queue_min_interval_ns_equals: Some(40_000_000),
            queue_min_interval_ms_equals: Some(40),
        };
        verify_session_meta_input_pipeline(Some(&ok), &rows).unwrap();

        let bad = ReplaySessionMetaInputPipelineExpect {
            input_rate_limit_equals: Some(30),
            ..Default::default()
        };
        assert!(verify_session_meta_input_pipeline(Some(&bad), &rows).is_err());
    }

    #[test]
    fn verify_timeline_enforces_session_meta_input_pipeline_when_configured() {
        let rows = vec![
            serde_json::json!({
                "kind": "session_meta",
                "input_pipeline": {
                    "input_rate_limit": 20,
                    "queue_enabled": true,
                    "queue_min_interval_ns": 50_000_000u64,
                    "queue_min_interval_ms": 50u64
                }
            }),
            row_engine("TaskStarted"),
            row_engine("TaskStopped"),
        ];

        let expect = ReplayExpect {
            session_meta_input_pipeline: Some(ReplaySessionMetaInputPipelineExpect {
                input_rate_limit_equals: Some(20),
                queue_enabled_equals: Some(true),
                queue_min_interval_ns_equals: Some(50_000_000),
                queue_min_interval_ms_equals: Some(50),
            }),
            ..Default::default()
        };
        verify_timeline(&expect, &rows).unwrap();

        let expect_fail = ReplayExpect {
            session_meta_input_pipeline: Some(ReplaySessionMetaInputPipelineExpect {
                input_rate_limit_equals: Some(10),
                ..Default::default()
            }),
            ..Default::default()
        };
        assert!(verify_timeline(&expect_fail, &rows).is_err());
    }

    #[test]
    fn manifest_checks_require_manifest_path() {
        let td = tempfile::tempdir().unwrap();
        fs::write(
            td.path().join("expect.json"),
            r#"{"manifest":{"min_event_count":1}}"#,
        )
        .unwrap();
        fs::write(td.path().join("timeline.jsonl"), "").unwrap();
        assert!(verify_artifact_paths(
            &td.path().join("expect.json"),
            &td.path().join("timeline.jsonl"),
            None,
        )
        .is_err());

        fs::write(
            td.path().join("manifest.json"),
            serde_json::to_string(&ReplayArtifactManifest {
                schema_version: 1,
                session_name: "x".into(),
                mode: "record".into(),
                created_at: "t".into(),
                engine_version: "v".into(),
                config_hash: None,
                capture_method: "auto".into(),
                fps_cap: 30,
                input_rate_limit: None,
                queue_enabled: None,
                queue_min_interval_ns: None,
                queue_min_interval_ms: None,
                frame_count: 2,
                event_count: 5,
            })
            .unwrap(),
        )
        .unwrap();

        verify_artifact_paths(
            &td.path().join("expect.json"),
            &td.path().join("timeline.jsonl"),
            Some(&td.path().join("manifest.json")),
        )
        .unwrap();
    }

    #[test]
    fn manifest_session_name_mismatch_fails() {
        let td = tempfile::tempdir().unwrap();
        fs::write(
            td.path().join("expect.json"),
            r#"{"manifest":{"session_name_equals":"want"}}"#,
        )
        .unwrap();
        fs::write(td.path().join("timeline.jsonl"), "").unwrap();
        fs::write(
            td.path().join("manifest.json"),
            serde_json::to_string(&ReplayArtifactManifest {
                schema_version: 1,
                session_name: "wrong".into(),
                mode: "record".into(),
                created_at: "t".into(),
                engine_version: "v".into(),
                config_hash: None,
                capture_method: "auto".into(),
                fps_cap: 30,
                input_rate_limit: None,
                queue_enabled: None,
                queue_min_interval_ns: None,
                queue_min_interval_ms: None,
                frame_count: 0,
                event_count: 0,
            })
            .unwrap(),
        )
        .unwrap();

        assert!(verify_artifact_paths(
            &td.path().join("expect.json"),
            &td.path().join("timeline.jsonl"),
            Some(&td.path().join("manifest.json")),
        )
        .is_err());
    }
}
