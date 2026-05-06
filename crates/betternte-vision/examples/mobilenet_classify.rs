//! MobileNet-v3 图像分类示例
//!
//! 使用 ONNX Runtime 加载 MobileNet-v3-Small 模型，对图片进行 ImageNet-1000 分类。
//!
//! Usage:
//!   cargo run --example mobilenet_classify -- <image_path>

use betternte_vision::models::classifier::MobileNetClassifier;
use betternte_vision::models::Classifier;
use std::path::Path;
use std::time::Instant;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let image_path = args
        .get(1)
        .map(|s| s.as_str())
        .unwrap_or("assets/models/mobilenet_v3_small-onnx-float/test.png");

    println!("=== MobileNet-v3 图像分类 ===\n");

    // 加载模型
    let model_path =
        Path::new("assets/models/mobilenet_v3_small-onnx-float/mobilenet_v3_small.onnx");
    println!("加载模型: {}", model_path.display());
    let t0 = Instant::now();
    let classifier = match MobileNetClassifier::load(model_path) {
        Ok(c) => c,
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
    let results = match classifier.classify(&img, 5) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("推理失败: {}", e);
            return;
        }
    };
    println!("推理耗时: {:.3}s\n", t1.elapsed().as_secs_f64());

    // 输出结果
    println!("Top-5 分类结果:");
    println!("{:<6} {:<25} {:>10}", "排名", "类别", "置信度");
    println!("{}", "-".repeat(45));
    for (i, r) in results.iter().enumerate() {
        println!(
            "{:<6} {:<25} {:>9.4}%",
            i + 1,
            r.label,
            r.confidence * 100.0
        );
    }
}
