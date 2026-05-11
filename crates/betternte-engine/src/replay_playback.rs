//! Artifact replay: enumerate frame PNG paths and decode for the capture ticker.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::Context;
use betternte_core::ReplayMode;
use chrono::Utc;

use crate::Engine;

/// Resolve ordered PNG paths under a replay session (`replay.mode = replay`).
///
/// 1. If `timeline.jsonl` exists, use `kind == "frame"` rows in file order.
/// 2. Else (or zero frame rows) fall back to `frames/*.png` sorted by numeric stem.
pub(crate) fn discover_replay_frames(
    base_dir: &Path,
    replay: &betternte_core::ReplayConfig,
) -> anyhow::Result<Vec<PathBuf>> {
    if replay.mode != ReplayMode::Replay {
        return Ok(Vec::new());
    }

    let root = replay.artifact_root.trim();
    let session = replay.session_name.trim();
    if root.is_empty() || session.is_empty() {
        anyhow::bail!(
            "replay.mode=replay requires non-empty replay.artifact_root and replay.session_name",
        );
    }

    let session_dir = Engine::resolve_path(root, base_dir)
        .join(session);
    if !session_dir.is_dir() {
        anyhow::bail!(
            "replay session directory does not exist: {}",
            session_dir.display()
        );
    }

    let timeline = session_dir.join("timeline.jsonl");
    let mut paths = if timeline.is_file() {
        let text = fs::read_to_string(&timeline)
            .with_context(|| format!("replay read {:?}", timeline.display()))?;
        paths_from_timeline(&session_dir, &text)?
    } else {
        Vec::new()
    };

    if paths.is_empty() {
        paths = collect_frames_sorted(&session_dir.join("frames"))
            .with_context(|| format!("replay collect frames {:?}", session_dir.display()))?;
    }

    validate_frame_paths_exist(&paths)?;

    if paths.is_empty() {
        anyhow::bail!(
            "replay: no PNG frames found under {}",
            session_dir.display()
        );
    }

    Ok(paths)
}

fn paths_from_timeline(session_dir: &Path, timeline: &str) -> anyhow::Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    for line in timeline.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let v: serde_json::Value =
            serde_json::from_str(line).with_context(|| format!("replay bad jsonl line: {line}"))?;
        if v.get("kind").and_then(|k| k.as_str()) != Some("frame") {
            continue;
        }
        let rel = match v.get("path").and_then(|p| p.as_str()) {
            Some(p) if !p.is_empty() => p,
            _ => continue,
        };
        out.push(normalize_session_join(session_dir, rel));
    }
    Ok(out)
}

fn normalize_session_join(session_dir: &Path, rel: &str) -> PathBuf {
    let trimmed = rel.trim_start_matches('/');
    let p = PathBuf::from(trimmed.replace('/', std::path::MAIN_SEPARATOR_STR));
    if p.is_absolute() {
        p
    } else {
        session_dir.join(p)
    }
}

fn collect_frames_sorted(frames_dir: &Path) -> anyhow::Result<Vec<PathBuf>> {
    if !frames_dir.is_dir() {
        anyhow::bail!("replay frames directory missing: {}", frames_dir.display());
    }

    let mut keyed: Vec<(u32, PathBuf)> = Vec::new();
    let mut lexical: Vec<PathBuf> = Vec::new();

    for ent in fs::read_dir(frames_dir).with_context(|| frames_dir.display().to_string())? {
        let ent = ent.with_context(|| "read frames dir entry")?;
        let path = ent.path();
        let is_png = path
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|s| s.eq_ignore_ascii_case("png"));
        if !is_png {
            continue;
        }
        if let Some(n) = path
            .file_stem()
            .and_then(|s| s.to_str())
            .and_then(|s| s.parse::<u32>().ok())
        {
            keyed.push((n, path));
        } else {
            lexical.push(path);
        }
    }

    keyed.sort_by_key(|(n, _)| *n);
    lexical.sort();

    Ok(keyed
        .into_iter()
        .map(|(_, p)| p)
        .chain(lexical.into_iter())
        .collect())
}

fn validate_frame_paths_exist(paths: &[PathBuf]) -> anyhow::Result<()> {
    for p in paths {
        if !p.is_file() {
            anyhow::bail!("replay frame file missing or not a file: {}", p.display());
        }
    }
    Ok(())
}

pub(crate) fn decode_png_into_core_frame(
    path: &Path,
    sequence: u64,
) -> anyhow::Result<betternte_core::CaptureFrame> {
    let img =
        image::open(path).with_context(|| format!("replay PNG decode {:?}", path.display()))?;
    let rgba = img.into_rgba8();
    Ok(betternte_core::CaptureFrame {
        width: rgba.width(),
        height: rgba.height(),
        data: Arc::new(rgba.into_raw()),
        format: betternte_core::image::PixelFormat::Rgba,
        timestamp: Utc::now(),
        sequence,
        source: format!(
            "replay:{}",
            path.file_name().and_then(|s| s.to_str()).unwrap_or("png")
        ),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::RgbImage;

    fn write_mini_png(path: &Path) {
        let img = RgbImage::new(1, 1);
        img.save(path).expect("save png");
    }

    #[test]
    fn timeline_orders_frame_paths() {
        let td = tempfile::tempdir().unwrap();
        let s = td.path();
        fs::write(
            s.join("timeline.jsonl"),
            r#"{"kind":"noise","path":"skip"}
{"kind":"frame","path":"frames/000002.png"}
{"kind":"frame","path":"frames/000001.png"}
"#,
        )
        .unwrap();
        fs::create_dir(s.join("frames")).unwrap();
        write_mini_png(&s.join("frames").join("000001.png"));
        write_mini_png(&s.join("frames").join("000002.png"));

        let text = fs::read_to_string(s.join("timeline.jsonl")).unwrap();
        let got = paths_from_timeline(s, &text).unwrap();

        assert_eq!(got.len(), 2);
        assert_eq!(
            got[0].file_name().and_then(|s| s.to_str()),
            Some("000002.png")
        );
        assert_eq!(
            got[1].file_name().and_then(|s| s.to_str()),
            Some("000001.png")
        );

        validate_frame_paths_exist(&got).unwrap();
    }

    #[test]
    fn sorted_dir_without_timeline() {
        let td = tempfile::tempdir().unwrap();
        let frames = td.path().join("frames");
        fs::create_dir_all(&frames).unwrap();
        write_mini_png(&frames.join("000010.png"));
        write_mini_png(&frames.join("000002.png"));

        let got = collect_frames_sorted(&frames).unwrap();
        assert_eq!(got.len(), 2);
        assert_eq!(
            got[0].file_name().and_then(|s| s.to_str()),
            Some("000002.png")
        );
        assert_eq!(
            got[1].file_name().and_then(|s| s.to_str()),
            Some("000010.png")
        );

        validate_frame_paths_exist(&got).unwrap();
    }
}
