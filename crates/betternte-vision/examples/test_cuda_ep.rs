//! 测试 CUDA EP 是否真正在推理中使用

use std::path::Path;
use std::time::Instant;

fn main() {
    let model_path = Path::new("assets/models/yolo/yolo11n.onnx");

    println!("=== CUDA EP 深度测试 ===\n");

    // 创建 CUDA session
    let mut session = ort::session::Session::builder()
        .unwrap()
        .with_execution_providers([ort::execution_providers::CUDA::default().build()])
        .unwrap()
        .commit_from_file(model_path)
        .unwrap();

    println!("Session 创建成功");

    // 准备输入
    let input = ndarray::Array4::<f32>::zeros((1, 3, 640, 640));
    let tensor = ort::value::Tensor::from_array(input).unwrap();

    // Warmup (首次推理会加载 GPU kernel)
    println!("Warmup...");
    for i in 0..3 {
        let t = Instant::now();
        let _ = session.run(ort::inputs![tensor.upcast_ref()]);
        println!(
            "  warmup {}: {:.2}ms",
            i,
            t.elapsed().as_secs_f64() * 1000.0
        );
    }

    // 正式测试
    println!("\n正式测试 (10 runs):");
    let mut durations = Vec::new();
    for _ in 0..10 {
        let t = Instant::now();
        let _ = session.run(ort::inputs![tensor.upcast_ref()]);
        durations.push(t.elapsed());
    }

    let avg: std::time::Duration =
        durations.iter().sum::<std::time::Duration>() / durations.len() as u32;
    let min = durations.iter().min().unwrap();
    let max = durations.iter().max().unwrap();
    println!(
        "  avg={:.2}ms  min={:.2}ms  max={:.2}ms",
        avg.as_secs_f64() * 1000.0,
        min.as_secs_f64() * 1000.0,
        max.as_secs_f64() * 1000.0,
    );

    // 对比 CPU
    println!("\n--- 对比 CPU ---");
    let mut cpu_session = ort::session::Session::builder()
        .unwrap()
        .commit_from_file(model_path)
        .unwrap();

    // Warmup
    for _ in 0..3 {
        let _ = cpu_session.run(ort::inputs![tensor.upcast_ref()]);
    }

    let mut cpu_durations = Vec::new();
    for _ in 0..10 {
        let t = Instant::now();
        let _ = cpu_session.run(ort::inputs![tensor.upcast_ref()]);
        cpu_durations.push(t.elapsed());
    }

    let cpu_avg: std::time::Duration =
        cpu_durations.iter().sum::<std::time::Duration>() / cpu_durations.len() as u32;
    let cpu_min = cpu_durations.iter().min().unwrap();
    let cpu_max = cpu_durations.iter().max().unwrap();
    println!(
        "  avg={:.2}ms  min={:.2}ms  max={:.2}ms",
        cpu_avg.as_secs_f64() * 1000.0,
        cpu_min.as_secs_f64() * 1000.0,
        cpu_max.as_secs_f64() * 1000.0,
    );

    println!(
        "\n加速比: {:.1}x",
        cpu_avg.as_secs_f64() / avg.as_secs_f64()
    );
}
