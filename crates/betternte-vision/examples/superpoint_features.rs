//! SuperPoint 特征点检测示例
//!
//! 使用 ONNX Runtime 加载 SuperPoint 模型，检测图片中的特征点和描述子。
//!
//! Usage:
//!   cargo run --example superpoint_features -- <image_path>

use betternte_vision::models::superpoint::SuperPointDetector;
use std::path::Path;
use std::time::Instant;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let image_path = args
        .get(1)
        .map(|s| s.as_str())
        .unwrap_or("assets/models/superpoint/test.png");

    println!("=== SuperPoint 特征点检测 ===\n");

    // 加载模型 (SuperPoint 默认输入 320x240)
    let model_path = Path::new("assets/models/superpoint/superpoint.onnx");
    println!("加载模型: {}", model_path.display());
    let t0 = Instant::now();
    let detector = match SuperPointDetector::load(model_path, (320, 240)) {
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
            eprintln!("提示: 请提供图片路径作为参数，或确保 test.png 存在");
            return;
        }
    };
    println!("图片: {} ({}x{})", image_path, img.width(), img.height());

    // 推理
    let t1 = Instant::now();
    let result = match detector.detect(&img) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("推理失败: {}", e);
            return;
        }
    };
    println!("推理耗时: {:.3}s\n", t1.elapsed().as_secs_f64());

    // 输出结果
    println!("检测到 {} 个特征点:", result.keypoints.len());
    println!("{:<4} {:>10} {:>10} {:>12}", "序号", "X", "Y", "置信度");
    println!("{}", "-".repeat(40));
    for (i, kp) in result.keypoints.iter().take(20).enumerate() {
        println!(
            "{:<4} {:>10.1} {:>10.1} {:>11.4}",
            i + 1,
            kp.x,
            kp.y,
            kp.confidence
        );
    }
    if result.keypoints.len() > 20 {
        println!("... 共 {} 个特征点", result.keypoints.len());
    }
    let desc_rows = result.descriptors.len();
    let desc_cols = result.descriptors.first().map_or(0, |r| r.len());
    println!("\n描述子维度: {}x{}", desc_rows, desc_cols);
}
