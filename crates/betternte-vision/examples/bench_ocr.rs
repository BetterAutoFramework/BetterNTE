//! OCR Performance Benchmark Example
//!
//! Tests OCR engine initialization and recognition timing.
//! Note: PaddleOcrEngine is currently a stub — returns empty results.
//!
//! Usage:
//!   cargo run --example bench_ocr

use betternte_vision::{OcrConfig, OcrEngine, PaddleOcrEngine};
use opencv::prelude::*;
use std::path::Path;
use std::time::Instant;

const IMAGE_PATH: &str = "D:/code/BetterNTE/vendor/粉爪大劫案/ScreenShot_2026-04-28_213750_185.png";

fn truncate_text(s: &str, max_chars: usize) -> String {
    let chars: Vec<char> = s.chars().collect();
    if chars.len() <= max_chars {
        s.to_string()
    } else {
        chars[..max_chars].iter().collect::<String>() + "..."
    }
}

/// Load an image from disk as a BGRA Mat (to match screen capture format).
fn load_image_as_bgra_mat(path: &Path) -> Option<opencv::core::Mat> {
    let img = opencv::imgcodecs::imread(
        &path.to_string_lossy(),
        opencv::imgcodecs::IMREAD_COLOR,
    )
    .ok()?;
    // cvtColor BGR → BGRA so the OCR engine gets the same format as screen capture
    let mut bgra = opencv::core::Mat::default();
    opencv::imgproc::cvt_color(&img, &mut bgra, opencv::imgproc::COLOR_BGR2BGRA, 0).ok()?;
    Some(bgra)
}

#[tokio::main]
async fn main() {
    println!("=== OCR Performance Benchmark ===");
    println!();

    let img_path = Path::new(IMAGE_PATH);
    if !img_path.exists() {
        eprintln!("Error: Image not found: {}", IMAGE_PATH);
        eprintln!("Note: PaddleOcrEngine is currently a stub — this benchmark measures init/recognition timing only.");
        return;
    }

    let img = match load_image_as_bgra_mat(img_path) {
        Some(m) => m,
        None => {
            eprintln!("Failed to load image as Mat: {}", IMAGE_PATH);
            return;
        }
    };
    let w = img.cols() as u32;
    let h = img.rows() as u32;
    let mpx = (w as f64 * h as f64) / 1_000_000.0;
    println!("Image: {}", IMAGE_PATH);
    println!("Resolution: {} x {} ({:.2} MPx)\n", w, h, mpx);

    let configs = vec![
        (
            "Paddle-640",
            OcrConfig {
                model_path: "assets/models/paddleocr".to_string(),
                language: "ch".to_string(),
                use_gpu: false,
                batch_size: 1,
                max_side_len: 640,
                det_threshold: 0.3,
                rec_threshold: 0.5,
                unclip_ratio: 2.0,
                ..Default::default()
            },
        ),
        (
            "Paddle-960",
            OcrConfig {
                model_path: "assets/models/paddleocr".to_string(),
                language: "ch".to_string(),
                use_gpu: false,
                batch_size: 1,
                max_side_len: 960,
                det_threshold: 0.3,
                rec_threshold: 0.5,
                unclip_ratio: 2.0,
                ..Default::default()
            },
        ),
        (
            "Paddle-1280",
            OcrConfig {
                model_path: "assets/models/paddleocr".to_string(),
                language: "ch".to_string(),
                use_gpu: false,
                batch_size: 1,
                max_side_len: 1280,
                det_threshold: 0.3,
                rec_threshold: 0.5,
                unclip_ratio: 2.0,
                ..Default::default()
            },
        ),
    ];

    println!(
        "{:<18} {:>12} {:>12} {:>12}",
        "Mode", "Init(s)", "OCR(s)", "Regions"
    );
    println!("{}", "-".repeat(58));

    let mut results: Vec<(&str, f64, f64, usize)> = Vec::new();

    for (name, config) in configs {
        let init_start = Instant::now();
        let mut engine = PaddleOcrEngine::new();
        engine.init(&config).await.expect("Init failed");
        let init_time = init_start.elapsed().as_secs_f64();

        let ocr_start = Instant::now();
        let ocr_results = engine.recognize(&img).await.expect("OCR failed");
        let ocr_time = ocr_start.elapsed().as_secs_f64();

        println!(
            "{:<18} {:>12.3} {:>12.3} {:>12}",
            name,
            init_time,
            ocr_time,
            ocr_results.len()
        );

        for (i, region) in ocr_results.iter().take(3).enumerate() {
            let text = truncate_text(&region.text, 25);
            println!("  [{:>2}] {} (conf={:.2})", i + 1, text, region.confidence);
        }

        results.push((name, init_time, ocr_time, ocr_results.len()));
        println!();
    }

    println!("Benchmark complete!");
    println!(
        "Note: PaddleOcrEngine is a stub — results will be empty until ONNX models are integrated."
    );
}
