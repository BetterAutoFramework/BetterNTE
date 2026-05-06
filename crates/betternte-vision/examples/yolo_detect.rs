//! YOLO 目标检测示例
//!
//! 使用 ONNX Runtime 加载 YOLO11n 模型，对图片进行 COCO-80 目标检测。
//!
//! Usage:
//!   cargo run --example yolo_detect -- <image_path>

use betternte_vision::models::detector::YoloDetector;
use betternte_vision::models::Detector;
use std::path::Path;
use std::time::Instant;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let image_path = args
        .get(1)
        .map(|s| s.as_str())
        .unwrap_or("assets/models/yolo/test.jpg");

    println!("=== YOLO 目标检测 ===\n");

    // 加载模型
    let model_path = Path::new("assets/models/yolo/yolo11n.onnx");
    println!("加载模型: {}", model_path.display());
    let t0 = Instant::now();
    let detector = match YoloDetector::load(model_path) {
        Ok(d) => d,
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
            eprintln!("提示: 请提供图片路径作为参数，或确保 test.jpg 存在");
            return;
        }
    };
    println!("图片: {} ({}x{})", image_path, img.width(), img.height());

    // 推理
    let t1 = Instant::now();
    let results = match detector.detect(&img, 0.25) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("推理失败: {}", e);
            return;
        }
    };
    println!("推理耗时: {:.3}s\n", t1.elapsed().as_secs_f64());

    // 输出结果
    println!("检测到 {} 个目标:", results.len());
    println!(
        "{:<4} {:<15} {:>10} {:>30}",
        "序号", "类别", "置信度", "边界框 (x1,y1,x2,y2)"
    );
    println!("{}", "-".repeat(65));
    for (i, r) in results.iter().enumerate() {
        println!(
            "{:<4} {:<15} {:>9.2}% [{:.0}, {:.0}, {:.0}, {:.0}]",
            i + 1,
            r.label,
            r.confidence * 100.0,
            r.bbox[0],
            r.bbox[1],
            r.bbox[2],
            r.bbox[3]
        );
    }
}
