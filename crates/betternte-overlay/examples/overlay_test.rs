//! Example: Test overlay drawing
//!
//! This example demonstrates the overlay module by:
//! 1. Creating an overlay window bound to a game window
//! 2. Looping and drawing various test shapes/text
//!
//! Run from Windows (not WSL):
//!   cd D:\code\BetterNTE
//!   cargo run --example overlay_test --features "betternte-overlay"

#![cfg(windows)]

use std::time::Instant;

use betternte_core::Color;
use betternte_overlay::OverlayRenderer;

/// Simple HSV to RGB conversion (for the color bar animation).
fn hsv_to_rgb(h: f32, s: f32, v: f32) -> Color {
    let c = v * s;
    let x = c * (1.0 - ((h / 60.0) % 2.0 - 1.0).abs());
    let m = v - c;
    let (r, g, b) = if h < 60.0 {
        (c, x, 0.0)
    } else if h < 120.0 {
        (x, c, 0.0)
    } else if h < 180.0 {
        (0.0, c, x)
    } else if h < 240.0 {
        (0.0, x, c)
    } else if h < 300.0 {
        (x, 0.0, c)
    } else {
        (c, 0.0, x)
    };
    Color {
        r: ((r + m) * 255.0) as u8,
        g: ((g + m) * 255.0) as u8,
        b: ((b + m) * 255.0) as u8,
        a: 255,
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== BetterNTE Overlay Test ===");

    // Use hardcoded resolution - in a real app you'd get this from the game window
    let width = 1920u32;
    let height = 1080u32;
    println!("Using resolution: {}x{}", width, height);

    // Create overlay - in a real app you'd bind to the game window
    // For now, just create a standalone overlay window
    println!("Creating overlay...");
    let config = betternte_overlay::OverlayConfig {
        enabled: true,
        width,
        height,
        opacity: 0.8,
        ..Default::default()
    };
    let mut window = betternte_overlay::OverlayWindow::new(&config)?;
    println!("Overlay created, showing...");
    window.show()?;

    let mut renderer = OverlayRenderer::new(window);

    println!("Starting draw loop at 30fps... (Ctrl+C to exit)");
    let start = Instant::now();
    let mut frame_count: u64 = 0;
    let frame_duration = std::time::Duration::from_millis(1000 / 30);

    loop {
        let loop_start = Instant::now();
        let elapsed = start.elapsed().as_secs_f64();

        // Render frame
        {
            renderer.begin_frame().unwrap();
            let w = renderer.window.width();
            let h = renderer.window.height();
            let mut api = renderer.draw();

            // Clear to transparent
            api.clear();

            // Info panel background
            let _ = api.fill_rect(
                10,
                10,
                280,
                100,
                Color {
                    r: 0,
                    g: 0,
                    b: 0,
                    a: 180,
                },
            );
            let _ = api.draw_rect(
                10,
                10,
                280,
                100,
                Color {
                    r: 0,
                    g: 120,
                    b: 255,
                    a: 255,
                },
                2,
            );

            // Text info
            let _ = api.draw_text(
                20,
                18,
                "BetterNTE Overlay Test",
                Color {
                    r: 0,
                    g: 180,
                    b: 255,
                    a: 255,
                },
                14,
            );
            let _ = api.draw_text(
                20,
                40,
                &format!("Frame: {}", frame_count),
                Color {
                    r: 200,
                    g: 200,
                    b: 200,
                    a: 255,
                },
                12,
            );
            let fps = if elapsed > 0.0 {
                frame_count as f64 / elapsed
            } else {
                0.0
            };
            let _ = api.draw_text(
                20,
                58,
                &format!("FPS: {:.1}  Elapsed: {:.1}s", fps, elapsed),
                Color {
                    r: 100,
                    g: 255,
                    b: 100,
                    a: 255,
                },
                12,
            );
            let _ = api.draw_text(
                20,
                76,
                &format!("Resolution: {}x{}", w, h),
                Color {
                    r: 200,
                    g: 200,
                    b: 200,
                    a: 255,
                },
                12,
            );

            // Colored rectangles
            let _ = api.draw_rect(
                320,
                20,
                120,
                40,
                Color {
                    r: 255,
                    g: 50,
                    b: 50,
                    a: 255,
                },
                2,
            );
            let _ = api.fill_rect(
                320,
                70,
                120,
                40,
                Color {
                    r: 50,
                    g: 255,
                    b: 50,
                    a: 180,
                },
            );
            let _ = api.draw_rect(
                320,
                70,
                120,
                40,
                Color {
                    r: 50,
                    g: 255,
                    b: 50,
                    a: 255,
                },
                2,
            );
            let _ = api.fill_rect(
                460,
                20,
                80,
                80,
                Color {
                    r: 255,
                    g: 200,
                    b: 0,
                    a: 150,
                },
            );
            let _ = api.draw_rect(
                460,
                20,
                80,
                80,
                Color {
                    r: 255,
                    g: 200,
                    b: 0,
                    a: 255,
                },
                2,
            );

            // Circles
            let _ = api.draw_circle(
                380,
                180,
                30,
                Color {
                    r: 255,
                    g: 100,
                    b: 50,
                    a: 200,
                },
                2,
            );
            let _ = api.fill_circle(
                380,
                180,
                15,
                Color {
                    r: 255,
                    g: 255,
                    b: 0,
                    a: 255,
                },
            );
            let _ = api.draw_circle(
                460,
                180,
                30,
                Color {
                    r: 100,
                    g: 100,
                    b: 255,
                    a: 255,
                },
                2,
            );

            // Cross pattern (lines)
            let _ = api.draw_line(
                320,
                250,
                540,
                250,
                Color {
                    r: 255,
                    g: 255,
                    b: 255,
                    a: 255,
                },
                1,
            );
            let _ = api.draw_line(
                430,
                220,
                430,
                280,
                Color {
                    r: 255,
                    g: 255,
                    b: 0,
                    a: 255,
                },
                1,
            );

            // Animated color bar
            let t = (frame_count % 120) as f32 / 120.0;
            for i in 0..120 {
                let x = 10 + i as i32 * 3;
                let hue = (t * 360.0 + i as f32 * 3.0) % 360.0;
                let c = hsv_to_rgb(hue, 0.9, 1.0);
                let _ = api.fill_rect(x, 350, 3, 20, c);
            }

            // Pulsing crosshair at center
            let cx = (w / 2) as i32;
            let cy = (h / 2) as i32;
            let pulse = (frame_count % 40) as f32 / 40.0;
            let size = 15 + (pulse * 10.0) as i32;
            let cross = Color {
                r: 0,
                g: 255,
                b: 0,
                a: 220,
            };
            let _ = api.draw_line(cx - size, cy, cx - 8, cy, cross, 1);
            let _ = api.draw_line(cx + 8, cy, cx + size, cy, cross, 1);
            let _ = api.draw_line(cx, cy - size, cx, cy - 8, cross, 1);
            let _ = api.draw_line(cx, cy + 8, cx, cy + size, cross, 1);
            let _ = api.draw_circle(
                cx,
                cy,
                3,
                Color {
                    r: 0,
                    g: 255,
                    b: 0,
                    a: 255,
                },
                1,
            );

            renderer.end_frame().unwrap();
        }

        // Commit the frame to the window
        renderer.window.commit().unwrap();

        frame_count += 1;

        if frame_count % 30 == 0 {
            let fps = frame_count as f64 / elapsed;
            println!(
                "Frame {}, elapsed={:.1}s, fps={:.1}",
                frame_count, elapsed, fps
            );
        }

        let elapsed_frame = loop_start.elapsed();
        if elapsed_frame < frame_duration {
            std::thread::sleep(frame_duration - elapsed_frame);
        }
    }
}
