//! 模型推理 Benchmark
//!
//! 测试各模型在不同 Execution Provider 下的纯推理耗时（排除模型加载）。
//!
//! Usage:
//!   cargo run --release --example bench_infer -- [cpu|cuda|directml]

use betternte_vision::models::session::SessionBuilder;
use ort::session::Session;
use std::path::Path;
use std::time::{Duration, Instant};

const WARMUP: usize = 3;
const ITERATIONS: usize = 10;

fn main() {
    let ep = std::env::args().nth(1).unwrap_or_else(|| "cpu".to_string());
    println!("=== 模型推理 Benchmark (EP: {}) ===\n", ep);

    let (use_cuda, use_directml) = match ep.as_str() {
        "cuda" => (true, false),
        "directml" => (false, true),
        _ => (false, false),
    };

    // 每个模型单独加载和测试，避免一个 crash 影响全部
    run_bench("YOLO", || bench_yolo(use_cuda, use_directml));
    run_bench("SuperPoint", || bench_superpoint(use_cuda, use_directml));
    run_bench("LightGlue", || bench_lightglue(use_cuda, use_directml));
    run_bench("MobileNet", || bench_mobilenet(use_cuda, use_directml));
    run_bench("PaddleOCR-Det", || {
        bench_paddleocr_det(use_cuda, use_directml)
    });
    run_bench("PaddleOCR-Rec", || {
        bench_paddleocr_rec(use_cuda, use_directml)
    });
}

fn run_bench<F: FnOnce() -> Option<()>>(name: &str, f: F) {
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(f));
    match result {
        Ok(Some(())) => {} // already printed stats
        Ok(None) => {}     // skipped
        Err(_) => println!("[{}] CRASH (可能是 EP 不兼容该模型)", name),
    }
}

fn load_session(model_path: &Path, use_cuda: bool, use_directml: bool) -> Option<Session> {
    if !model_path.exists() {
        println!("  模型文件不存在，跳过");
        return None;
    }
    match SessionBuilder::new()
        .with_cuda(use_cuda)
        .with_directml(use_directml)
        .build_from_file(model_path)
    {
        Ok(s) => Some(s),
        Err(e) => {
            println!("  加载失败: {}", e);
            None
        }
    }
}

fn bench_session(name: &str, session: &mut Session, input_shape: &[usize]) -> Option<()> {
    let data = vec![0.0f32; input_shape.iter().product()];
    let tensor = ort::value::Tensor::from_array(
        ndarray::Array::from_shape_vec(input_shape.to_vec(), data).unwrap(),
    )
    .unwrap();

    // Warmup
    for _ in 0..WARMUP {
        let _ = session.run(ort::inputs![tensor.upcast_ref()]);
    }

    // Benchmark
    let mut durations = Vec::new();
    for _ in 0..ITERATIONS {
        let t = Instant::now();
        let _ = session.run(ort::inputs![tensor.upcast_ref()]);
        durations.push(t.elapsed());
    }

    print_stats(name, &durations);
    Some(())
}

fn bench_yolo(use_cuda: bool, use_directml: bool) -> Option<()> {
    let mut session = load_session(
        Path::new("assets/models/yolo/yolo11n.onnx"),
        use_cuda,
        use_directml,
    )?;
    bench_session("YOLO", &mut session, &[1, 3, 640, 640])
}

fn bench_superpoint(use_cuda: bool, use_directml: bool) -> Option<()> {
    let mut session = load_session(
        Path::new("assets/models/superpoint/superpoint.onnx"),
        use_cuda,
        use_directml,
    )?;
    bench_session("SuperPoint", &mut session, &[1, 1, 240, 320])
}

fn bench_lightglue(use_cuda: bool, use_directml: bool) -> Option<()> {
    let mut session = load_session(
        Path::new("assets/models/superpoint/superpoint_lightglue_pipeline.ort.onnx"),
        use_cuda,
        use_directml,
    )?;
    bench_session("LightGlue", &mut session, &[2, 1, 480, 640])
}

fn bench_mobilenet(use_cuda: bool, use_directml: bool) -> Option<()> {
    let mut session = load_session(
        Path::new("assets/models/mobilenet_v3_small-onnx-float/mobilenet_v3_small.onnx"),
        use_cuda,
        use_directml,
    )?;
    bench_session("MobileNet", &mut session, &[1, 3, 224, 224])
}

fn bench_paddleocr_det(use_cuda: bool, use_directml: bool) -> Option<()> {
    let mut session = load_session(
        Path::new("assets/models/paddleocr/det.onnx"),
        use_cuda,
        use_directml,
    )?;
    bench_session("PaddleOCR-Det", &mut session, &[1, 3, 960, 960])
}

fn bench_paddleocr_rec(use_cuda: bool, use_directml: bool) -> Option<()> {
    let mut session = load_session(
        Path::new("assets/models/paddleocr/rec.onnx"),
        use_cuda,
        use_directml,
    )?;
    bench_session("PaddleOCR-Rec", &mut session, &[1, 3, 48, 320])
}

fn print_stats(name: &str, durations: &[Duration]) {
    let avg: Duration = durations.iter().sum::<Duration>() / durations.len() as u32;
    let min = durations.iter().min().unwrap();
    let max = durations.iter().max().unwrap();
    println!(
        "{:<18} avg={:.2}ms  min={:.2}ms  max={:.2}ms  ({} runs)",
        name,
        avg.as_secs_f64() * 1000.0,
        min.as_secs_f64() * 1000.0,
        max.as_secs_f64() * 1000.0,
        durations.len(),
    );
}
