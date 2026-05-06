//! PaddleOCR 文字识别示例
//!
//! 使用 ONNX Runtime 加载 PaddleOCR 模型 (det + rec)，对图片进行文字检测和识别。
//!
//! Usage:
//!   cargo run --example paddleocr_recognize -- <image_path>

use betternte_vision::models::ocr::PaddleOcrEngine;
use std::path::Path;
use std::time::Instant;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let image_path = args
        .get(1)
        .map(|s| s.as_str())
        .unwrap_or("assets/models/paddleocr/test.png");

    println!("=== PaddleOCR 文字识别 ===\n");

    // 加载模型
    let det_path = Path::new("assets/models/paddleocr/det.onnx");
    let rec_path = Path::new("assets/models/paddleocr/rec.onnx");
    let dict_path = Path::new("assets/models/paddleocr/ppocrv5_dict.txt");

    println!("加载检测模型: {}", det_path.display());
    println!("加载识别模型: {}", rec_path.display());
    let t0 = Instant::now();
    let engine = match PaddleOcrEngine::load(det_path, rec_path, Some(dict_path)) {
        Ok(e) => e,
        Err(e) => {
            eprintln!("模型加载失败: {}", e);
            return;
        }
    };
    println!("模型加载耗时: {:.2}s\n", t0.elapsed().as_secs_f64());

    // 加载图片
    let img = match image::open(Path::new(image_path)) {
        Ok(img) => img,
        Err(e) => {
            eprintln!("图片加载失败: {}", e);
            eprintln!("提示: 请提供图片路径作为参数，或确保 test.png 存在");
            return;
        }
    };
    println!("图片: {} ({}x{})", image_path, img.width(), img.height());

    // 推理
    let t1 = Instant::now();
    let results = match engine.recognize(&img) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("推理失败: {}", e);
            return;
        }
    };
    println!("推理耗时: {:.3}s\n", t1.elapsed().as_secs_f64());

    // 输出结果
    println!("识别到 {} 个文本区域:", results.len());
    println!("{:<4} {:<10} {:>30}", "序号", "置信度", "文本");
    println!("{}", "-".repeat(50));
    for (i, r) in results.iter().enumerate() {
        println!("{:<4} {:>8.2}%  {}", i + 1, r.confidence * 100.0, r.text);
    }
}
