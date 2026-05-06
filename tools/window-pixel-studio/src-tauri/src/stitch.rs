//! Image stitching engine for scroll and panoramic capture.
//!
//! Uses template matching (SAD — Sum of Absolute Differences) to find
//! the overlap offset between adjacent frames, then composites them.

use crate::CaptureDto;
use base64::Engine;
use betternte_capture::PrintWindowCapture;
use betternte_core::capture::{CaptureTarget, ScreenCapture};
use image::{DynamicImage, RgbaImage};
use std::time::Duration;
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{
    GetClientRect, PostMessageW, WM_MBUTTONDOWN, WM_MBUTTONUP, WM_MOUSEWHEEL,
};

/// Middle mouse button flag for WM_MBUTTONDOWN/UP (not in windows crate).
const MK_MBUTTON: usize = 0x0010;

// ─── Stitch progress payload ─────────────────────────────────────────────────

#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Progress {
    current: u32,
    total: u32,
    phase: String,
}

// ─── Frame capture helper ────────────────────────────────────────────────────

async fn capture_frame(hwnd: i64) -> Result<RgbaImage, String> {
    let mut engine = PrintWindowCapture::new();
    let target = CaptureTarget::Window {
        hwnd: hwnd as u64,
    };
    engine.start(&target).await.map_err(|e| e.to_string())?;
    let frame = engine.capture().await.map_err(|e| e.to_string())?;
    let dyn_img = frame.to_dynamic_image()?;
    Ok(dyn_img.to_rgba8())
}

fn encode_png_base64(img: &DynamicImage) -> Result<String, String> {
    let mut buf = Vec::new();
    img.write_to(&mut std::io::Cursor::new(&mut buf), image::ImageFormat::Png)
        .map_err(|e| e.to_string())?;
    Ok(base64::engine::general_purpose::STANDARD.encode(buf))
}

// ─── Win32 input helpers ─────────────────────────────────────────────────────

fn post_msg(hwnd: i64, msg: u32, wparam: usize, lparam: isize) -> Result<(), String> {
    unsafe {
        PostMessageW(Some(HWND(hwnd as *mut _)), msg, WPARAM(wparam), LPARAM(lparam))
            .map_err(|e| format!("PostMessageW failed: {}", e))
    }
}

fn make_lparam(x: i32, y: i32) -> isize {
    ((y as u32 as isize) << 16) | (x as u32 as isize & 0xFFFF)
}

fn scroll_delta_param(amount: i32) -> usize {
    // WM_MOUSEWHEEL: high word of wParam is wheel delta (positive = up)
    ((amount as u32 as usize) << 16) & 0xFFFF_0000
}

fn get_client_center(hwnd: i64) -> Result<(i32, i32), String> {
    let mut rect = windows::Win32::Foundation::RECT::default();
    unsafe {
        GetClientRect(HWND(hwnd as *mut _), &mut rect).map_err(|e| e.to_string())?;
    }
    let cx = (rect.left + rect.right) / 2;
    let cy = (rect.top + rect.bottom) / 2;
    Ok((cx, cy))
}

// ─── Template matching (SAD) ─────────────────────────────────────────────────

/// Find the best overlap offset between two frames along the given axis.
///
/// For vertical scroll (axis = "y"): search for horizontal strip overlap,
/// i.e., how many rows from the bottom of `prev` match the top of `curr`.
///
/// For horizontal pan (axis = "x"): search for vertical strip overlap,
/// i.e., how many columns from the right of `prev` match the left of `curr`.
///
/// Returns (offset, confidence). Lower SAD per pixel = better match.
fn find_overlap_offset(
    prev: &RgbaImage,
    curr: &RgbaImage,
    axis: &str,
    expected_offset: u32,
) -> (i32, f64) {
    let pw = prev.width();
    let ph = prev.height();
    let cw = curr.width();
    let ch = curr.height();

    let search_range = (expected_offset * 3).max(50).min(match axis {
        "y" => ph.min(ch) / 2,
        _ => pw.min(cw) / 2,
    });

    let min_overlap = 10u32;
    let max_offset = search_range;

    let mut best_offset = 0i32;
    let mut best_sad_per_px = f64::MAX;

    // Sample step for performance (check every Nth pixel)
    let sample_step = 4u32;

    for offset in min_overlap..=max_offset {
        let (overlap_w, overlap_h) = match axis {
            "y" => {
                // Vertical scroll: bottom `offset` rows of prev vs top `offset` rows of curr
                let w = pw.min(cw);
                let h = offset.min(ph).min(ch);
                (w, h)
            }
            _ => {
                // Horizontal pan: right `offset` cols of prev vs left `offset` cols of curr
                let w = offset.min(pw).min(cw);
                let h = ph.min(ch);
                (w, h)
            }
        };

        if overlap_w == 0 || overlap_h == 0 {
            continue;
        }

        let mut total_sad: u64 = 0;
        let mut pixel_count: u64 = 0;

        for dy in (0..overlap_h).step_by(sample_step as usize) {
            for dx in (0..overlap_w).step_by(sample_step as usize) {
                let (px, py) = match axis {
                    "y" => (dx, ph - overlap_h + dy),   // bottom of prev
                    _ => (pw - overlap_w + dx, dy),     // right of prev
                };
                let (cx, cy) = match axis {
                    "y" => (dx, dy),                     // top of curr
                    _ => (dx, dy),                       // left of curr
                };

                let p_pixel = prev.get_pixel(px, py);
                let c_pixel = curr.get_pixel(cx, cy);

                total_sad += (p_pixel[0] as i64 - c_pixel[0] as i64).unsigned_abs();
                total_sad += (p_pixel[1] as i64 - c_pixel[1] as i64).unsigned_abs();
                total_sad += (p_pixel[2] as i64 - c_pixel[2] as i64).unsigned_abs();
                pixel_count += 1;
            }
        }

        if pixel_count > 0 {
            let sad_per_px = total_sad as f64 / pixel_count as f64;
            if sad_per_px < best_sad_per_px {
                best_sad_per_px = sad_per_px;
                best_offset = offset as i32;
            }
        }
    }

    (best_offset, best_sad_per_px)
}

// ─── Stitch composite ────────────────────────────────────────────────────────

/// Stitch multiple frames into a single image.
///
/// `axis`:
/// - "y" → stack vertically (for scroll capture)
/// - "x" → place side by side (for horizontal panoramic)
fn stitch_frames(frames: &[RgbaImage], offsets: &[i32], axis: &str) -> RgbaImage {
    assert!(frames.len() >= 2);
    assert_eq!(offsets.len(), frames.len() - 1);

    // Calculate total dimensions
    let (total_w, total_h) = match axis {
        "y" => {
            let w = frames.iter().map(|f| f.width()).max().unwrap_or(0);
            let h = frames[0].height() + offsets.iter().map(|o| *o as u32).sum::<u32>();
            (w, h)
        }
        _ => {
            let w = frames[0].width() + offsets.iter().map(|o| *o as u32).sum::<u32>();
            let h = frames.iter().map(|f| f.height()).max().unwrap_or(0);
            (w, h)
        }
    };

    let mut output = RgbaImage::from_pixel(total_w, total_h, image::Rgba([0, 0, 0, 0]));
    let mut cursor: i64 = 0;

    for (i, frame) in frames.iter().enumerate() {
        let (ox, oy) = match axis {
            "y" => (0, cursor as i64),
            _ => (cursor as i64, 0),
        };

        // Copy pixels from frame to output
        for y in 0..frame.height() {
            for x in 0..frame.width() {
                let dx = ox + x as i64;
                let dy = oy + y as i64;
                if dx >= 0 && dy >= 0 && (dx as u32) < total_w && (dy as u32) < total_h {
                    output.put_pixel(dx as u32, dy as u32, *frame.get_pixel(x, y));
                }
            }
        }

        if i < offsets.len() {
            // Advance cursor: frame dimension minus overlap
            let frame_dim = match axis {
                "y" => frame.height() as i64,
                _ => frame.width() as i64,
            };
            cursor += frame_dim - offsets[i] as i64;
        }
    }

    output
}

// ─── Scroll capture ──────────────────────────────────────────────────────────

pub async fn scroll_capture(
    hwnd: i64,
    direction: String,
    scroll_amount: i32,
    frame_count: u32,
    delay_ms: u64,
    emit_progress: impl Fn(Progress) + Send + 'static,
) -> Result<CaptureDto, String> {
    let total_steps = frame_count;
    let delay = Duration::from_millis(delay_ms);

    // Capture initial frame
    emit_progress(Progress {
        current: 0,
        total: total_steps,
        phase: "Capturing frame 1".into(),
    });
    let first = capture_frame(hwnd).await?;
    let mut frames = vec![first];

    // Scroll and capture
    let delta = if direction == "up" {
        scroll_amount
    } else {
        -scroll_amount
    };

    for i in 1..frame_count {
        emit_progress(Progress {
            current: i,
            total: total_steps,
            phase: format!("Scrolling and capturing frame {}", i + 1),
        });

        // Simulate mouse wheel
        let center = get_client_center(hwnd)?;
        let lparam = make_lparam(center.0, center.1);
        let wparam = scroll_delta_param(delta);
        post_msg(hwnd, WM_MOUSEWHEEL, wparam, lparam)?;

        // Wait for content to settle
        tokio::time::sleep(delay).await;

        // Capture
        let frame = capture_frame(hwnd).await?;
        frames.push(frame);
    }

    // Stitch
    emit_progress(Progress {
        current: total_steps,
        total: total_steps,
        phase: "Stitching frames...".into(),
    });

    // Find overlaps between adjacent frames
    let expected_offset = frames[0].height() / 2; // rough estimate
    let mut offsets = Vec::new();
    for i in 0..frames.len() - 1 {
        let (offset, _conf) = find_overlap_offset(&frames[i], &frames[i + 1], "y", expected_offset);
        offsets.push(offset.max(1));
    }

    let stitched = stitch_frames(&frames, &offsets, "y");
    let dyn_img = DynamicImage::ImageRgba8(stitched);
    let (width, height) = (dyn_img.width(), dyn_img.height());
    let png_base64 = encode_png_base64(&dyn_img)?;

    Ok(CaptureDto {
        width,
        height,
        png_base64,
    })
}

// ─── Panoramic capture ───────────────────────────────────────────────────────

pub async fn panoramic_capture(
    hwnd: i64,
    direction: String,
    drag_distance: i32,
    frame_count: u32,
    delay_ms: u64,
    emit_progress: impl Fn(Progress) + Send + 'static,
) -> Result<CaptureDto, String> {
    let total_steps = frame_count;
    let delay = Duration::from_millis(delay_ms);
    let center = get_client_center(hwnd)?;

    // Capture initial frame
    emit_progress(Progress {
        current: 0,
        total: total_steps,
        phase: "Capturing frame 1".into(),
    });
    let first = capture_frame(hwnd).await?;
    let mut frames = vec![first];

    // Calculate drag step
    let step = drag_distance / frame_count as i32;
    let (dx, dy) = match direction.as_str() {
        "left" => (-step, 0),
        "right" => (step, 0),
        "up" => (0, -step),
        "down" => (0, step),
        _ => (step, 0),
    };

    // Press middle mouse button
    let start_lparam = make_lparam(center.0, center.1);
    post_msg(hwnd, WM_MBUTTONDOWN, MK_MBUTTON, start_lparam)?;

    let mut cur_x = center.0;
    let mut cur_y = center.1;

    // Drag, capture at intervals
    for i in 1..frame_count {
        emit_progress(Progress {
            current: i,
            total: total_steps,
            phase: format!("Dragging and capturing frame {}", i + 1),
        });

        cur_x += dx;
        cur_y += dy;
        let lparam = make_lparam(cur_x, cur_y);

        // Move mouse (send WM_MOUSEMOVE via WM_MBUTTONDOWN with updated coords)
        // Actually, we need to send LBUTTONDOWN/UP to simulate drag movement.
        // For middle-button drag, we move by sending WM_MBUTTONDOWN at new position.
        post_msg(hwnd, WM_MBUTTONDOWN, MK_MBUTTON, lparam)?;

        tokio::time::sleep(delay).await;

        let frame = capture_frame(hwnd).await?;
        frames.push(frame);
    }

    // Release middle mouse button
    let end_lparam = make_lparam(cur_x, cur_y);
    post_msg(hwnd, WM_MBUTTONUP, 0, end_lparam)?;

    // Stitch
    emit_progress(Progress {
        current: total_steps,
        total: total_steps,
        phase: "Stitching frames...".into(),
    });

    let (axis, expected_offset) = match direction.as_str() {
        "left" | "right" => ("x", frames[0].width() / 2),
        _ => ("y", frames[0].height() / 2),
    };

    let mut offsets = Vec::new();
    for i in 0..frames.len() - 1 {
        let (offset, _conf) = find_overlap_offset(&frames[i], &frames[i + 1], axis, expected_offset);
        offsets.push(offset.max(1));
    }

    let stitched = stitch_frames(&frames, &offsets, axis);
    let dyn_img = DynamicImage::ImageRgba8(stitched);
    let (width, height) = (dyn_img.width(), dyn_img.height());
    let png_base64 = encode_png_base64(&dyn_img)?;

    Ok(CaptureDto {
        width,
        height,
        png_base64,
    })
}
