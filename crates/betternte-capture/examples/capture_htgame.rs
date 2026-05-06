//! Example: Capture screenshot for HTGame.exe using different capture methods.
//!
//! This example demonstrates:
//! - Finding a window by process name
//! - Capturing screenshots using WGC, DXGI, ScreenDC, BitBlt, and win-screenshot
//! - Saving screenshots to files
//! - Benchmarking capture time (first capture and average over multiple runs)

use std::time::Instant;

use betternte_capture::bitblt::BitBltCapture;
use betternte_capture::dxgi_dup::DxgiDupCapture;
use betternte_capture::print_window::PrintWindowCapture;
use betternte_capture::screen_dc::ScreenDCCapture;
use betternte_capture::wgc::WgcCapture;
use betternte_capture::window::WindowFinderImpl;
use betternte_capture::{CaptureTarget, ScreenCapture, WindowFinder};

/// Find a window by process name
fn find_window_by_process(process_name: &str) -> Option<u64> {
    let finder = WindowFinderImpl::new();
    let windows = finder.find_by_process(process_name).ok()?;

    for window in &windows {
        tracing::info!(
            "Found window: {} (hwnd: {}, pid: {})",
            window.title,
            window.hwnd,
            window.pid
        );
    }

    windows
        .into_iter()
        .find(|w| !w.is_minimized && !w.title.is_empty())
        .map(|w| w.hwnd)
}

struct CaptureStats {
    first_capture_ms: f64,
    avg_capture_ms: f64,
    min_capture_ms: f64,
    max_capture_ms: f64,
    fps: f64,
}

async fn benchmark_engine(
    engine: &mut dyn ScreenCapture,
    target: &CaptureTarget,
    runs: usize,
    name: &str,
) -> Result<CaptureStats, String> {
    engine.configure(betternte_core::CaptureRuntimeOptions {
        crop_to_client: true,
        hdr_to_sdr: false,
        recover_on_resize: false,
        recover_on_monitor_switch: false,
    });

    engine
        .start(target)
        .await
        .map_err(|e| format!("Failed to start {}: {}", name, e))?;

    tracing::info!("{} started, capturing {} frames...", name, runs);

    // Warmup: discard first capture (D3D11 device init, WGC session, etc.)
    let warmup_start = Instant::now();
    let _ = engine
        .capture()
        .await
        .map_err(|e| format!("Warmup capture failed for {}: {}", name, e))?;
    let warmup_ms = warmup_start.elapsed().as_secs_f64() * 1000.0;
    tracing::info!("{} warmup: {:.2}ms (discarded)", name, warmup_ms);

    let mut times_ms = Vec::with_capacity(runs);

    for i in 0..runs {
        let start = Instant::now();
        let frame = engine
            .capture()
            .await
            .map_err(|e| format!("Failed to capture frame {}: {}", i, e))?;
        let elapsed = start.elapsed().as_secs_f64() * 1000.0;
        times_ms.push(elapsed);

        tracing::debug!("Frame {} captured in {:.2}ms", i, elapsed);

        if i == 0 {
            let filename = format!(
                "screenshot_{}_{}.png",
                name.to_lowercase().replace(" ", "_"),
                frame.sequence
            );
            if let Err(e) = save_frame(&frame, &filename) {
                tracing::warn!("Failed to save screenshot: {}", e);
            } else {
                tracing::info!("Saved screenshot to {}", filename);
            }
        }

        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
    }

    let fps = engine.fps();

    engine
        .stop()
        .await
        .map_err(|e| format!("Failed to stop {}: {}", name, e))?;

    let first = times_ms[0];
    let avg = times_ms.iter().sum::<f64>() / times_ms.len() as f64;
    let min = times_ms.iter().cloned().fold(f64::INFINITY, f64::min);
    let max = times_ms.iter().cloned().fold(f64::NEG_INFINITY, f64::max);

    Ok(CaptureStats {
        first_capture_ms: first,
        avg_capture_ms: avg,
        min_capture_ms: min,
        max_capture_ms: max,
        fps,
    })
}

fn save_frame(frame: &betternte_core::CaptureFrame, filename: &str) -> Result<(), String> {
    use image::{ImageBuffer, Rgba};

    let width = frame.width as u32;
    let height = frame.height as u32;

    let rgba_data: Vec<u8> = frame
        .data
        .chunks(4)
        .flat_map(|px| [px[2], px[1], px[0], px[3]])
        .collect();

    let img: ImageBuffer<Rgba<u8>, Vec<u8>> =
        ImageBuffer::from_raw(width, height, rgba_data).ok_or("Failed to create image buffer")?;

    img.save(filename)
        .map_err(|e| format!("Failed to save image: {}", e))?;

    Ok(())
}

fn print_stats(_name: &str, stats: &CaptureStats) {
    println!("  First capture:  {:.2} ms", stats.first_capture_ms);
    println!("  Average capture: {:.2} ms", stats.avg_capture_ms);
    println!("  Min capture:    {:.2} ms", stats.min_capture_ms);
    println!("  Max capture:    {:.2} ms", stats.max_capture_ms);
    println!("  FPS:            {:.2}", stats.fps);
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    let hwnd = find_window_by_process("HTGame")
        .or_else(|| {
            let finder = WindowFinderImpl::new();
            finder
                .find_by_keyword("异环")
                .ok()?
                .into_iter()
                .find(|w| !w.is_minimized && !w.title.is_empty())
                .map(|w| w.hwnd)
        })
        .or_else(|| {
            let finder = WindowFinderImpl::new();
            finder
                .find_by_keyword("HT")
                .ok()?
                .into_iter()
                .find(|w| !w.is_minimized && !w.title.is_empty())
                .map(|w| w.hwnd)
        });

    match hwnd {
        Some(hwnd) => {
            tracing::info!("Found HTGame window with hwnd: {}", hwnd);

            let target = CaptureTarget::Window { hwnd };
            let runs = 10;

            println!("\n{}", "=".repeat(70));
            println!("Screenshot Benchmark for HTGame.exe");
            println!("Target HWND: {}", hwnd);
            println!("Runs per method: {} (1 warmup + {} timed)", runs, runs);
            println!("{}", "=".repeat(70));

            // === GPU-accelerated engines ===
            println!("\n{}", "-".repeat(70));
            println!("  GPU-Accelerated (D3D11 / Windows.Graphics.Capture)");
            println!("{}", "-".repeat(70));

            // WGC (GPU-accelerated, persistent session)
            println!("\n--- WGC (Windows Graphics Capture) ---");
            {
                let mut engine = WgcCapture::new();
                match benchmark_engine(&mut engine, &target, runs, "WGC").await {
                    Ok(stats) => print_stats("WGC", &stats),
                    Err(e) => println!("  WGC failed: {}", e),
                }
            }

            // DXGI Desktop Duplication
            println!("\n--- DXGI Desktop Duplication ---");
            match DxgiDupCapture::new() {
                Ok(mut engine) => {
                    match benchmark_engine(&mut engine, &target, runs, "DXGI").await {
                        Ok(stats) => print_stats("DXGI", &stats),
                        Err(e) => println!("  DXGI failed: {}", e),
                    }
                }
                Err(e) => println!("  DXGI init failed: {}", e),
            }

            // === CPU/GDI engines ===
            println!("\n{}", "-".repeat(70));
            println!("  CPU / GDI (BitBlt / PrintWindow)");
            println!("{}", "-".repeat(70));

            // ScreenDC
            println!("\n--- ScreenDC ---");
            {
                let mut engine = ScreenDCCapture::new();
                match benchmark_engine(&mut engine, &target, runs, "ScreenDC").await {
                    Ok(stats) => print_stats("ScreenDC", &stats),
                    Err(e) => println!("  ScreenDC failed: {}", e),
                }
            }

            // BitBlt (self-implemented)
            println!("\n--- BitBlt (self-implemented) ---");
            {
                let mut engine = BitBltCapture::new();
                match benchmark_engine(&mut engine, &target, runs, "BitBlt").await {
                    Ok(stats) => print_stats("BitBlt", &stats),
                    Err(e) => println!("  BitBlt failed: {}", e),
                }
            }

            // PrintWindow (self-implemented)
            println!("\n--- PrintWindow (self-implemented) ---");
            {
                let mut engine = PrintWindowCapture::new();
                match benchmark_engine(&mut engine, &target, runs, "PrintWindow").await {
                    Ok(stats) => print_stats("PrintWindow", &stats),
                    Err(e) => println!("  PrintWindow failed: {}", e),
                }
            }

            println!("\n{}", "=".repeat(70));
            println!("Screenshots saved as: screenshot_<method>_<frame#>.png");
            println!("{}", "=".repeat(70));
        }
        None => {
            tracing::error!("HTGame.exe window not found! Make sure the game is running.");
            println!("\nTip: Run 'HTGame.exe' first, then re-run this example.");

            println!("\nAvailable windows:");
            let finder = WindowFinderImpl::new();
            if let Ok(windows) = finder.find_by_keyword("") {
                for w in windows.iter().take(20) {
                    if !w.title.is_empty() {
                        println!("  - {} (hwnd: {}, pid: {})", w.title, w.hwnd, w.pid);
                    }
                }
            }
        }
    }
}
