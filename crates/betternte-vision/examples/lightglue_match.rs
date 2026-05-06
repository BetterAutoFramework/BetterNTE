//! LightGlue 特征匹配示例
//!
//! 使用 SuperPoint + LightGlue pipeline 模型，对两张图片进行特征点匹配。
//!
//! Usage:
//!   cargo run --example lightglue_match -- <image1_path> <image2_path>

use betternte_vision::models::superpoint::LightGlueMatcher;
use std::path::Path;
use std::time::Instant;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let img1_path = args
        .get(1)
        .map(|s| s.as_str())
        .unwrap_or("assets/models/superpoint/test.png");
    let img2_path = args
        .get(2)
        .map(|s| s.as_str())
        .unwrap_or("assets/models/superpoint/test.png");

    println!("=== LightGlue 特征匹配 ===\n");

    // 加载模型
    let model_path = Path::new("assets/models/superpoint/superpoint_lightglue_pipeline.ort.onnx");
    println!("加载模型: {}", model_path.display());
    let t0 = Instant::now();
    let matcher = match LightGlueMatcher::load(model_path, (640, 480)) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("模型加载失败: {}", e);
            return;
        }
    };
    println!("模型加载耗时: {:.2}s\n", t0.elapsed().as_secs_f64());

    // 加载图片
    let img1 = match image::open(Path::new(img1_path)) {
        Ok(img) => img,
        Err(e) => {
            eprintln!("图片1加载失败: {}", e);
            return;
        }
    };
    let img2 = match image::open(Path::new(img2_path)) {
        Ok(img) => img,
        Err(e) => {
            eprintln!("图片2加载失败: {}", e);
            return;
        }
    };
    println!("图片1: {} ({}x{})", img1_path, img1.width(), img1.height());
    println!("图片2: {} ({}x{})", img2_path, img2.width(), img2.height());

    // 匹配
    let t1 = Instant::now();
    let result = match matcher.match_images(&img1, &img2) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("匹配失败: {}", e);
            return;
        }
    };
    println!("\n推理耗时: {:.3}s\n", t1.elapsed().as_secs_f64());

    // 输出结果
    println!("图像0 检测到 {} 个关键点", result.keypoints0.len());
    println!("图像1 检测到 {} 个关键点", result.keypoints1.len());
    println!("匹配到 {} 对特征点:\n", result.matches.len());

    println!(
        "{:<4} {:>12} {:>12} {:>12} {:>12} {:>10}",
        "序号", "img0.X", "img0.Y", "img1.X", "img1.Y", "置信度"
    );
    println!("{}", "-".repeat(70));
    for (i, m) in result.matches.iter().take(20).enumerate() {
        println!(
            "{:<4} {:>12.1} {:>12.1} {:>12.1} {:>12.1} {:>10.4}",
            i + 1,
            m.kp0.0,
            m.kp0.1,
            m.kp1.0,
            m.kp1.1,
            m.score
        );
    }
    if result.matches.len() > 20 {
        println!("... 共 {} 对匹配", result.matches.len());
    }
}
