use crate::{CaptureDto, WindowDto};

use base64::Engine;
use betternte_capture::PrintWindowCapture;
use betternte_core::capture::{CaptureTarget, ScreenCapture};
use xcap::Window;

fn encode_png_base64_dynamic(img: image::DynamicImage) -> Result<String, String> {
    let mut png_bytes = Vec::new();
    img.write_to(
        &mut std::io::Cursor::new(&mut png_bytes),
        image::ImageFormat::Png,
    )
    .map_err(|e| e.to_string())?;
    Ok(base64::engine::general_purpose::STANDARD.encode(png_bytes))
}

/// Enumerate visible, non-minimized windows using xcap.
pub fn list_windows() -> Vec<WindowDto> {
    let Ok(windows) = Window::all() else {
        return Vec::new();
    };

    let mut items = windows
        .into_iter()
        .filter_map(|window| {
            let id = window.id().ok()? as i64;
            let title = window.title().ok()?.trim().to_string();
            if title.is_empty() {
                return None;
            }
            if window.is_minimized().unwrap_or(false) {
                return None;
            }

            let app_name = window.app_name().unwrap_or_default();
            let display_title = if app_name.trim().is_empty() {
                title
            } else {
                format!("[{}] {}", app_name, title)
            };

            Some(WindowDto {
                hwnd: id,
                title: display_title,
            })
        })
        .collect::<Vec<_>>();

    items.sort_by(|a, b| a.title.to_lowercase().cmp(&b.title.to_lowercase()));
    items
}

/// Capture window client area only (no title bar / borders) using PrintWindowCapture.
///
/// Uses `PW_CLIENTONLY | PW_RENDERFULLCONTENT` under the hood via the
/// betternte-capture crate's PrintWindow engine.
/// Falls back to legacy xcap capture if PrintWindow fails.
pub async fn capture_client_area_png(hwnd_raw: i64) -> Result<CaptureDto, String> {
    let mut engine = PrintWindowCapture::new();
    engine.configure(betternte_core::CaptureRuntimeOptions {
        crop_to_client: true,
        hdr_to_sdr: false,
        recover_on_resize: false,
        recover_on_monitor_switch: false,
    });
    let target = CaptureTarget::Window {
        hwnd: hwnd_raw as u64,
    };

    match engine.start(&target).await {
        Ok(_) => match engine.capture().await {
            Ok(frame) => {
                let dynamic_img = frame.to_dynamic_image()?;
                let (width, height) = (dynamic_img.width(), dynamic_img.height());
                let png_base64 = encode_png_base64_dynamic(dynamic_img)?;
                return Ok(CaptureDto {
                    width,
                    height,
                    png_base64,
                });
            }
            Err(e) => {
                eprintln!("PrintWindow capture failed: {e}, falling back to xcap");
            }
        },
        Err(e) => {
            eprintln!("PrintWindow start failed: {e}, falling back to xcap");
        }
    }

    // Fallback: legacy xcap full-window capture
    capture_window_png(hwnd_raw)
}

/// Legacy full-window capture via xcap, cropped to client area (no title bar).
pub fn capture_window_png(hwnd_raw: i64) -> Result<CaptureDto, String> {
    use windows::Win32::Foundation::RECT;
    use windows::Win32::UI::WindowsAndMessaging::GetClientRect;

    let windows = Window::all().map_err(|e| e.to_string())?;
    let target_id = hwnd_raw as u32;

    let window = windows
        .into_iter()
        .find(|w| w.id().is_ok_and(|id| id == target_id))
        .ok_or_else(|| "Target window not found, refresh window list".to_string())?;

    if window.is_minimized().map_err(|e| e.to_string())? {
        return Err("Window is minimized, cannot capture".into());
    }

    let image = window.capture_image().map_err(|e| e.to_string())?;

    // Crop to client area: get client rect to compute title bar + border offsets
    let cropped = unsafe {
        let hwnd = windows::Win32::Foundation::HWND(hwnd_raw as *mut _);
        let mut rect = RECT::default();
        let _ = GetClientRect(hwnd, &mut rect);
        let client_w = (rect.right - rect.left) as u32;
        let client_h = (rect.bottom - rect.top) as u32;
        let full_w = image.width();
        let full_h = image.height();

        if client_w > 0 && client_h > 0 && client_w <= full_w && client_h <= full_h {
            let border_h = full_h.saturating_sub(client_h);
            let border_w = full_w.saturating_sub(client_w);
            let crop_x = border_w / 2;
            let crop_y = border_h;
            xcap::image::DynamicImage::ImageRgba8(image)
                .crop(crop_x, crop_y, client_w, client_h)
                .to_rgba8()
        } else {
            image
        }
    };

    let (width, height) = cropped.dimensions();
    let mut png_bytes = Vec::new();
    xcap::image::DynamicImage::ImageRgba8(cropped)
        .write_to(
            &mut std::io::Cursor::new(&mut png_bytes),
            xcap::image::ImageFormat::Png,
        )
        .map_err(|e| e.to_string())?;
    let png_base64 = base64::engine::general_purpose::STANDARD.encode(png_bytes);

    Ok(CaptureDto {
        width,
        height,
        png_base64,
    })
}
