//! Capture comparison example — tests all capture methods on a specified window.
//!
//! Usage: cargo run -p betternte-capture --example capture_compare -- <window_title>
//!
//! This example:
//! 1. Finds a window matching the given title keyword
//! 2. Tries all capture methods (BitBlt, PrintWindow, ScreenDC, WGC, DXGI)
//! 3. Captures a frame from each method
//! 4. Saves frames as PNG images in target/capture_compare/
//! 5. Crops all images to the same center region and compares them

#![cfg(windows)]

use std::path::PathBuf;
use std::time::Instant;

use betternte_capture::{
    BitBltCapture, CaptureTarget, DxgiDupCapture, PrintWindowCapture, ScreenCapture,
    ScreenDCCapture, WgcCapture, WindowFinder, WindowFinderImpl,
};
use betternte_core::config::CaptureMethod;

/// Check if a window looks like a terminal/console window that we should skip.
fn is_terminal_window(window: &betternte_capture::WindowInfo) -> bool {
    let title_lower = window.title.to_lowercase();
    let class_lower = window.class_name.to_lowercase();
    let process_lower = window.process_name.to_lowercase();

    // Skip cmd, powershell, windows terminal, conhost
    process_lower.contains("cmd.exe")
        || process_lower.contains("powershell.exe")
        || process_lower.contains("windowsterminal.exe")
        || process_lower.contains("conhost.exe")
        || process_lower.contains("wt.exe")
        // Skip by window class
        || class_lower == "consolewindowclass"
        || class_lower.starts_with("cascadia")
        // Skip if title looks like a terminal running cargo
        || (title_lower.contains("cargo") && title_lower.contains("run"))
        || title_lower.contains("administrator:")
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("betternte_capture=info".parse()?),
        )
        .init();

    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: {} <keyword|--list> [--static]", args[0]);
        eprintln!();
        eprintln!("Options:");
        eprintln!("  --list, -l   List all windows and let you select one");
        eprintln!("  --static     Capture each method 3 times to check consistency");
        eprintln!();
        eprintln!("Examples:");
        eprintln!("  {} --list", args[0]);
        eprintln!("  {} 异环", args[0]);
        eprintln!("  {} --list --static", args[0]);
        std::process::exit(1);
    }

    let keyword = &args[1];
    let static_mode = args.iter().any(|a| a == "--static");

    let output_dir = PathBuf::from("target/capture_compare");
    std::fs::create_dir_all(&output_dir)?;

    let finder = WindowFinderImpl::new();

    let selected_window = if keyword == "--list" || keyword == "-l" {
        // List all visible windows and let user select
        let all_windows = finder.find_by_keyword("")?;
        let visible: Vec<_> = all_windows
            .into_iter()
            .filter(|w| !w.title.is_empty() && !is_terminal_window(w))
            .collect();

        if visible.is_empty() {
            eprintln!("No visible windows found.");
            std::process::exit(1);
        }

        println!("\nAvailable windows:");
        for (i, w) in visible.iter().enumerate() {
            println!("  [{}] '{}' (pid: {})", i + 1, w.title, w.pid);
        }

        println!("\nEnter window number (1-{}): ", visible.len());
        let mut input = String::new();
        std::io::stdin().read_line(&mut input).expect("Failed to read input");
        let index: usize = input.trim().parse().unwrap_or(0);

        if index == 0 || index > visible.len() {
            eprintln!("Invalid selection.");
            std::process::exit(1);
        }

        visible[index - 1].clone()
    } else {
        // Search by keyword
        println!("Searching for window with keyword: '{}'", keyword);
        let windows = finder.find_by_keyword(keyword)?;

        if windows.is_empty() {
            eprintln!("No window found matching '{}'", keyword);
            std::process::exit(1);
        }

        // Show all found windows for debugging
        println!("Found {} window(s):", windows.len());
        for w in &windows {
            let is_terminal = is_terminal_window(w);
            println!(
                "  {} '{}' (pid: {})",
                if is_terminal { "[SKIP]" } else { "[OK]   " },
                w.title,
                w.pid
            );
        }

        let window = windows
            .into_iter()
            .find(|w| !is_terminal_window(w));

        match window {
            Some(w) => w,
            None => {
                eprintln!("\nNo suitable window found (all matches are terminal windows).");
                eprintln!("Use '--list' to select from all windows.");
                std::process::exit(1);
            }
        }
    };

    let window = &selected_window;

    println!(
        "Found window: '{}' (hwnd: {:#x}, pid: {})",
        window.title, window.hwnd, window.pid
    );
    let target = CaptureTarget::Window { hwnd: window.hwnd };

    // Define capture methods to test
    let methods: Vec<(&str, CaptureMethod)> = vec![
        ("bitblt", CaptureMethod::BitBlt),
        ("print_window", CaptureMethod::PrintWindow),
        ("screen_dc", CaptureMethod::DwmSharedSurface),
        ("wgc", CaptureMethod::WindowsGraphicsCapture),
        ("dxgi", CaptureMethod::DxgiDesktopDuplication),
    ];

    if static_mode {
        // Static mode: capture each method 3 times to check consistency
        println!("\n========== Static Mode (Consistency Check) ==========");
        println!("Each method will be captured 3 times to verify consistency.");
        println!("Use with a static window (e.g., Notepad) for best results.\n");

        for (name, method) in &methods {
            println!("\n--- {} ---", name);
            let mut paths = Vec::new();

            for i in 0..3 {
                let suffix = format!("{}_{}", name, i + 1);
                let result = test_capture_method(&suffix, *method, &target, &output_dir).await;
                match result {
                    CaptureResult::Success { path, .. } => {
                        paths.push((format!("{} #{}", name, i + 1), path));
                    }
                    CaptureResult::Failed { error, .. } => {
                        println!("  [FAIL] Attempt {}: {}", i + 1, error);
                    }
                }
                // Small delay between captures
                tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            }

            if paths.len() >= 2 {
                println!("  Consistency check:");
                let path_refs: Vec<(&str, &str)> = paths.iter().map(|(n, p)| (n.as_str(), p.as_str())).collect();
                compare_frames_cropped(&path_refs, &output_dir);
            }
        }
    } else {
        // Normal mode: capture each method once and compare
        let mut results = Vec::new();

        for (name, method) in &methods {
            println!("\n--- Testing {} ---", name);
            let result = test_capture_method(*name, *method, &target, &output_dir).await;
            results.push(result);
        }

        // Print summary
        println!("\n\n========== Summary ==========");
        let mut successful_frames = Vec::new();

        for result in &results {
            match result {
                CaptureResult::Success {
                    name,
                    path,
                    width,
                    height,
                    latency_ms,
                    ..
                } => {
                    println!(
                        "[PASS] {}: {}x{}, {:.1}ms, saved to {}",
                        name, width, height, latency_ms, path
                    );
                    successful_frames.push((name.as_str(), path.as_str()));
                }
                CaptureResult::Failed { name, error } => {
                    println!("[FAIL] {}: {}", name, error);
                }
            }
        }

        // Compare successful frames with center crop
        if successful_frames.len() >= 2 {
            println!("\n========== Comparison (center-cropped) ==========");
            println!("NOTE: For animated content, captures are taken at different times.");
            println!("Use --static mode with a static window for meaningful comparison.\n");
            compare_frames_cropped(&successful_frames, &output_dir);
        } else {
            println!("\nNeed at least 2 successful captures to compare.");
        }
    }

    Ok(())
}

enum CaptureResult {
    Success {
        name: String,
        path: String,
        width: u32,
        height: u32,
        latency_ms: f64,
    },
    Failed {
        name: String,
        error: String,
    },
}

async fn test_capture_method(
    name: &str,
    method: CaptureMethod,
    target: &CaptureTarget,
    output_dir: &PathBuf,
) -> CaptureResult {
    let create_result = create_engine(method);
    let mut engine = match create_result {
        Ok(e) => e,
        Err(e) => {
            return CaptureResult::Failed {
                name: name.to_string(),
                error: format!("Failed to create engine: {}", e),
            };
        }
    };

    println!("  Engine: {}", engine.name());

    // Enable crop_to_client to get consistent client area only
    engine.configure(betternte_core::CaptureRuntimeOptions {
        crop_to_client: true,
        hdr_to_sdr: false,
        recover_on_resize: false,
        recover_on_monitor_switch: false,
    });

    // Start capture
    if let Err(e) = engine.start(target).await {
        return CaptureResult::Failed {
            name: name.to_string(),
            error: format!("Failed to start: {}", e),
        };
    }

    // Wait a bit for the engine to initialize
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    // Capture frame
    let capture_start = Instant::now();
    let frame = match engine.capture().await {
        Ok(f) => f,
        Err(e) => {
            let _ = engine.stop().await;
            return CaptureResult::Failed {
                name: name.to_string(),
                error: format!("Failed to capture: {}", e),
            };
        }
    };
    let latency = capture_start.elapsed().as_secs_f64() * 1000.0;

    // Stop capture
    let _ = engine.stop().await;

    println!(
        "  Captured {}x{} frame in {:.1}ms",
        frame.width, frame.height, latency
    );

    // Convert to image and save
    let image_path = output_dir.join(format!("{}.png", name));
    match save_frame_as_image(&frame, &image_path) {
        Ok(_) => {
            println!("  Saved to: {}", image_path.display());
            CaptureResult::Success {
                name: name.to_string(),
                path: image_path.to_string_lossy().to_string(),
                width: frame.width,
                height: frame.height,
                latency_ms: latency,
            }
        }
        Err(e) => CaptureResult::Failed {
            name: name.to_string(),
            error: format!("Failed to save image: {}", e),
        },
    }
}

fn create_engine(method: CaptureMethod) -> anyhow::Result<Box<dyn ScreenCapture>> {
    match method {
        CaptureMethod::BitBlt => Ok(Box::new(BitBltCapture::new())),
        CaptureMethod::PrintWindow => Ok(Box::new(PrintWindowCapture::new())),
        CaptureMethod::DwmSharedSurface => Ok(Box::new(ScreenDCCapture::new())),
        CaptureMethod::WindowsGraphicsCapture => {
            if WgcCapture::is_supported() {
                Ok(Box::new(WgcCapture::new()))
            } else {
                anyhow::bail!("WGC not supported on this system")
            }
        }
        CaptureMethod::DxgiDesktopDuplication => {
            Ok(DxgiDupCapture::new().map(|c| Box::new(c) as Box<dyn ScreenCapture>)?)
        }
        _ => anyhow::bail!("Unsupported capture method: {:?}", method),
    }
}

fn save_frame_as_image(
    frame: &betternte_core::CaptureFrame,
    path: &PathBuf,
) -> anyhow::Result<()> {
    use image::{ImageBuffer, RgbImage};

    let rgb_data = match frame.format {
        betternte_core::PixelFormat::Bgra => {
            let mut rgb = Vec::with_capacity((frame.width * frame.height * 3) as usize);
            for chunk in frame.data.chunks(4) {
                rgb.push(chunk[2]);
                rgb.push(chunk[1]);
                rgb.push(chunk[0]);
            }
            rgb
        }
        betternte_core::PixelFormat::Rgba => {
            let mut rgb = Vec::with_capacity((frame.width * frame.height * 3) as usize);
            for chunk in frame.data.chunks(4) {
                rgb.push(chunk[0]);
                rgb.push(chunk[1]);
                rgb.push(chunk[2]);
            }
            rgb
        }
        betternte_core::PixelFormat::Bgr => {
            let mut rgb = Vec::with_capacity((frame.width * frame.height * 3) as usize);
            for chunk in frame.data.chunks(3) {
                rgb.push(chunk[2]);
                rgb.push(chunk[1]);
                rgb.push(chunk[0]);
            }
            rgb
        }
        betternte_core::PixelFormat::Rgb => frame.data.clone(),
        betternte_core::PixelFormat::Gray => {
            let mut rgb = Vec::with_capacity((frame.width * frame.height * 3) as usize);
            for &gray in &frame.data {
                rgb.push(gray);
                rgb.push(gray);
                rgb.push(gray);
            }
            rgb
        }
    };

    let img: RgbImage = ImageBuffer::from_raw(frame.width, frame.height, rgb_data)
        .ok_or_else(|| anyhow::anyhow!("Failed to create image buffer"))?;

    img.save(path)?;
    Ok(())
}

/// Crop image to a center region of given dimensions.
fn center_crop(img: &image::DynamicImage, crop_w: u32, crop_h: u32) -> image::DynamicImage {
    use image::GenericImageView;

    let (w, h) = img.dimensions();
    let x = w.saturating_sub(crop_w) / 2;
    let y = h.saturating_sub(crop_h) / 2;
    let actual_w = crop_w.min(w);
    let actual_h = crop_h.min(h);
    img.crop_imm(x, y, actual_w, actual_h)
}

/// Crop image to center percentage of its dimensions.
fn center_crop_percent(img: &image::DynamicImage, percent: f32) -> image::DynamicImage {
    use image::GenericImageView;

    let (w, h) = img.dimensions();
    let crop_w = (w as f32 * percent / 100.0) as u32;
    let crop_h = (h as f32 * percent / 100.0) as u32;
    let x = (w - crop_w) / 2;
    let y = (h - crop_h) / 2;
    img.crop_imm(x, y, crop_w, crop_h)
}

fn compare_frames_cropped(frames: &[(&str, &str)], output_dir: &PathBuf) {
    use image::GenericImageView;

    let mut images: Vec<(&str, image::DynamicImage)> = Vec::new();

    for (name, path) in frames {
        match image::open(path) {
            Ok(img) => {
                images.push((*name, img));
            }
            Err(e) => {
                println!("  [WARN] Failed to load {} for comparison: {}", name, e);
            }
        }
    }

    if images.len() < 2 {
        println!("  Not enough images loaded for comparison.");
        return;
    }

    // Crop each image to its center 50% to remove window chrome differences
    let crop_percent = 50.0;
    println!("  Original dimensions:");
    for (name, img) in &images {
        let (w, h) = img.dimensions();
        println!("    {}: {}x{}", name, w, h);
    }
    println!("  Cropping each image to center {:.0}%", crop_percent);

    // Crop all images and save cropped versions
    let mut cropped: Vec<(&str, image::DynamicImage, u32, u32)> = Vec::new();
    for (name, img) in &images {
        let cropped_img = center_crop_percent(img, crop_percent);
        let (cw, ch) = cropped_img.dimensions();
        let cropped_path = output_dir.join(format!("{}_cropped.png", name));
        if let Err(e) = cropped_img.save(&cropped_path) {
            println!("  [WARN] Failed to save cropped {}: {}", name, e);
        }
        cropped.push((*name, cropped_img, cw, ch));
    }

    println!("\n  Cropped dimensions:");
    for (name, _, w, h) in &cropped {
        println!("    {}: {}x{}", name, w, h);
    }
    println!("  Cropped images saved to target/capture_compare/*_cropped.png");

    // Find minimum dimensions across cropped images for comparison
    let min_w = cropped.iter().map(|(_, _, w, _)| *w).min().unwrap_or(0);
    let min_h = cropped.iter().map(|(_, _, _, h)| *h).min().unwrap_or(0);
    println!("  Comparing center {}x{} region", min_w, min_h);

    // Further crop to common minimum dimensions for pixel-perfect comparison
    let mut aligned: Vec<(&str, image::DynamicImage)> = Vec::new();
    for (name, img, _, _) in &cropped {
        let aligned_img = center_crop(img, min_w, min_h);
        aligned.push((*name, aligned_img));
    }

    // Compare each pair
    println!("\n  Pixel comparison:");
    for i in 0..aligned.len() {
        for j in (i + 1)..aligned.len() {
            let (name_a, ref img_a) = aligned[i];
            let (name_b, ref img_b) = aligned[j];

            let pixels_a = img_a.to_rgba8();
            let pixels_b = img_b.to_rgba8();

            let mut total_diff: u64 = 0;
            let mut max_diff: u8 = 0;
            let mut diff_pixel_count: u64 = 0;
            let total_pixels = (min_w * min_h) as u64;

            for (pa, pb) in pixels_a.pixels().zip(pixels_b.pixels()) {
                let dr = pa[0].abs_diff(pb[0]);
                let dg = pa[1].abs_diff(pb[1]);
                let db = pa[2].abs_diff(pb[2]);

                let diff = dr.max(dg).max(db);
                if diff > 0 {
                    diff_pixel_count += 1;
                    total_diff += diff as u64;
                }
                max_diff = max_diff.max(diff);
            }

            let avg_diff = if diff_pixel_count > 0 {
                total_diff as f64 / diff_pixel_count as f64
            } else {
                0.0
            };

            let diff_percentage = (diff_pixel_count as f64 / total_pixels as f64) * 100.0;

            if diff_pixel_count == 0 {
                println!("    {} vs {}: IDENTICAL", name_a, name_b);
            } else {
                println!(
                    "    {} vs {}: {:.2}% pixels differ (avg: {:.1}, max: {})",
                    name_a, name_b, diff_percentage, avg_diff, max_diff
                );
            }
        }
    }
}
