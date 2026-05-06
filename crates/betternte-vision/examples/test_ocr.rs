//! OCR Engine Test Example
//!
//! Tests PaddleOcrEngine initialization and recognition.
//! Note: PaddleOcrEngine is currently a stub — returns empty results.
//!
//! Usage:
//!   cargo run --example test_ocr

use betternte_vision::{OcrConfig, OcrEngine, PaddleOcrEngine};
use std::time::Instant;

#[tokio::main]
async fn main() {
    let folder = "D:/code/BetterNTE/vendor/粉爪大劫案";

    let mut engine = PaddleOcrEngine::new();
    let config = OcrConfig {
        model_path: "assets/models/paddleocr".to_string(),
        language: "ch".to_string(),
        use_gpu: false,
        batch_size: 1,
        max_side_len: 960,
        det_threshold: 0.3,
        rec_threshold: 0.5,
        unclip_ratio: 2.0,
    };

    println!("Initializing OCR engine...");
    let init_start = Instant::now();
    engine.init(&config).await.expect("Failed to init OCR");
    println!("Init time: {:.2}s\n", init_start.elapsed().as_secs_f64());

    // Get all PNG files
    let entries: Vec<_> = std::fs::read_dir(folder)
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| {
            let p = e.path();
            p.extension().map_or(false, |ext| ext == "png")
                && p.file_name()
                    .map_or(false, |n| n.to_string_lossy().starts_with("ScreenShot"))
        })
        .collect();

    if entries.is_empty() {
        println!("No screenshots found in {}", folder);
        println!("Note: PaddleOcrEngine is a stub — results will be empty until ONNX models are integrated.");
        return;
    }

    println!("Found {} screenshots, processing...\n", entries.len());

    let total_start = Instant::now();
    let mut total_regions = 0;

    for (i, entry) in entries.iter().enumerate() {
        let img_path = entry.path();
        let filename = img_path.file_name().unwrap().to_string_lossy();

        print!("[{}/{}] {} ... ", i + 1, entries.len(), filename);
        std::io::Write::flush(&mut std::io::stdout()).unwrap();

        let img = match image::open(&img_path) {
            Ok(img) => img,
            Err(e) => {
                println!("FAILED to load: {}", e);
                continue;
            }
        };

        let ocr_start = Instant::now();
        let results = match engine.recognize(&img).await {
            Ok(r) => r,
            Err(e) => {
                println!("FAILED: {}", e);
                continue;
            }
        };

        let elapsed = ocr_start.elapsed();
        total_regions += results.len();

        if results.is_empty() {
            println!("0 regions in {:.2}s", elapsed.as_secs_f64());
        } else {
            println!("{} regions in {:.2}s", results.len(), elapsed.as_secs_f64());
            for region in &results {
                println!("  - '{}' (conf={:.2})", region.text, region.confidence);
            }
        }
    }

    println!(
        "\n=== Total: {} regions in {:.2}s ===",
        total_regions,
        total_start.elapsed().as_secs_f64()
    );
    println!(
        "Note: PaddleOcrEngine is a stub — results will be empty until ONNX models are integrated."
    );
}
