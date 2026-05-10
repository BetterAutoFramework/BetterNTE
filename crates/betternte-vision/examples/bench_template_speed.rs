//! Wall-clock benchmark for template matching (decode once, then benchmark matcher path).
//!
//! # Usage
//!
//! ```text
//! cargo run --release -p betternte-vision --example bench_template_speed -- \\
//!     <scene.png> <template.png> [warmup_iters] [timed_iters] [--mode full|core]
//! ```
//!
//! Default `warmup_iters` = 2, `timed_iters` = 20.
//! Default `mode` = `full`.
//!
//! OpenCV core-only mode (skips threshold scan/sort from runtime matcher):
//!
//! ```text
//! cargo run --release -p betternte-vision --example bench_template_speed -- \\
//!     <scene.png> <template.png> 2 20 --mode core
//! ```

use anyhow::Context;
use betternte_core::TemplateMatchParams;
use betternte_vision::{OpenCvTemplateMatcher, TemplateMatcher};
use image::open;
use opencv::core::{self, Mat};
use opencv::imgproc;
use opencv::prelude::*;
use std::env;
use std::sync::Arc;
use std::time::Instant;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BenchMode {
    Full,
    Core,
}

impl BenchMode {
    fn as_str(self) -> &'static str {
        match self {
            Self::Full => "full",
            Self::Core => "core",
        }
    }
}

async fn bench_loop(
    label: &str,
    matcher: Arc<dyn TemplateMatcher>,
    scene: &opencv::core::Mat,
    template: &image::DynamicImage,
    params: &TemplateMatchParams,
    warmup: u32,
    iters: u32,
) -> anyhow::Result<()> {
    for _ in 0..warmup {
        let _ = matcher
            .match_template(scene, template, params)
            .await
            .with_context(|| format!("{label} warmup"))?;
    }
    let t0 = Instant::now();
    for _ in 0..iters {
        let v = matcher
            .match_template(scene, template, params)
            .await
            .with_context(|| format!("{label} timed iter"))?;
        std::hint::black_box(v);
    }
    let elapsed = t0.elapsed();
    let ms = elapsed.as_secs_f64() * 1000.0 / f64::from(iters);
    println!(
        "{label}: {ms:.3} ms/iter (warmup={warmup}, timed_iters={iters}, total={:.3} ms)",
        elapsed.as_secs_f64() * 1000.0
    );
    Ok(())
}

fn gray_to_mat(gray: &image::GrayImage) -> anyhow::Result<Mat> {
    let rows = gray.height() as i32;
    let raw = gray.as_raw();
    let mat_1d = Mat::from_slice(raw)?;
    let mat_2d_ref = mat_1d.reshape(1, rows)?;
    Ok(mat_2d_ref.try_clone()?)
}

fn build_opencv_inputs(
    scene: &image::DynamicImage,
    template: &image::DynamicImage,
) -> anyhow::Result<(Mat, Mat)> {
    let img_gray = scene.to_luma8();
    let tpl_gray = template.to_luma8();
    let img_mat = gray_to_mat(&img_gray)?;
    let tpl_mat = gray_to_mat(&tpl_gray)?;
    Ok((img_mat, tpl_mat))
}

fn run_opencv_core_once(img_mat: &Mat, tpl_mat: &Mat) -> anyhow::Result<(f64, core::Point)> {
    let mut corr = Mat::default();
    imgproc::match_template(
        img_mat,
        tpl_mat,
        &mut corr,
        imgproc::TM_CCOEFF_NORMED,
        &core::no_array(),
    )?;
    let mut min_val = 0.0f64;
    let mut max_val = 0.0f64;
    let mut min_loc = core::Point::default();
    let mut max_loc = core::Point::default();
    core::min_max_loc(
        &corr,
        Some(&mut min_val),
        Some(&mut max_val),
        Some(&mut min_loc),
        Some(&mut max_loc),
        &core::no_array(),
    )?;
    Ok((max_val, max_loc))
}

fn bench_opencv_core(
    scene: &image::DynamicImage,
    template: &image::DynamicImage,
    warmup: u32,
    iters: u32,
) -> anyhow::Result<()> {
    let (img_mat, tpl_mat) = build_opencv_inputs(scene, template)?;

    for _ in 0..warmup {
        let _ = run_opencv_core_once(&img_mat, &tpl_mat).context("OpenCV core warmup")?;
    }
    let t0 = Instant::now();
    for _ in 0..iters {
        let v = run_opencv_core_once(&img_mat, &tpl_mat).context("OpenCV core timed iter")?;
        std::hint::black_box(v);
    }
    let elapsed = t0.elapsed();
    let ms = elapsed.as_secs_f64() * 1000.0 / f64::from(iters);
    println!(
        "OpenCvTemplateMatcher(core): {ms:.3} ms/iter (warmup={warmup}, timed_iters={iters}, total={:.3} ms)",
        elapsed.as_secs_f64() * 1000.0
    );
    Ok(())
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let mut args = env::args().skip(1);
    let scene_path = args
        .next()
        .context("usage: bench_template_speed <scene.png> <template.png> [warmup] [iters]")?;
    let template_path = args
        .next()
        .context("usage: bench_template_speed <scene.png> <template.png> [warmup] [iters]")?;
    let warmup: u32 = args.next().map(|s| s.parse()).transpose()?.unwrap_or(2);
    let iters: u32 = args.next().map(|s| s.parse()).transpose()?.unwrap_or(20);
    let mut mode = BenchMode::Full;
    let mut pending = args.next();
    while let Some(token) = pending {
        if token == "--mode" {
            let value = args
                .next()
                .context("missing value after --mode, expected full|core")?;
            mode = match value.as_str() {
                "full" => BenchMode::Full,
                "core" => BenchMode::Core,
                _ => anyhow::bail!("invalid --mode value: {value}, expected full|core"),
            };
            pending = args.next();
            continue;
        }
        anyhow::bail!("unrecognized argument: {token}");
    }

    let scene = open(&scene_path)
        .with_context(|| format!("open scene {}", scene_path))?
        .into_rgba8();
    let template = open(&template_path)
        .with_context(|| format!("open template {}", template_path))?
        .into_rgba8();

    let scene = image::DynamicImage::ImageRgba8(scene);
    let template = image::DynamicImage::ImageRgba8(template);

    // Convert scene to BGRA Mat (what TemplateMatcher now expects)
    let rgba = scene.to_rgba8();
    let (w, h) = (rgba.width() as i32, rgba.height() as i32);
    let mut bgra_data = Vec::with_capacity((w * h * 4) as usize);
    for pixel in rgba.pixels() {
        bgra_data.push(pixel[2]); // B
        bgra_data.push(pixel[1]); // G
        bgra_data.push(pixel[0]); // R
        bgra_data.push(pixel[3]); // A
    }
    let flat = Mat::from_slice(&bgra_data)?;
    let scene_mat = flat.reshape(4, h)?.try_clone()?;

    if template.width() > scene.width() || template.height() > scene.height() {
        anyhow::bail!(
            "template {}x{} larger than scene {}x{}",
            template.width(),
            template.height(),
            scene.width(),
            scene.height()
        );
    }

    let params = TemplateMatchParams::default();
    println!(
        "scene={} ({}x{}), template={} ({}x{}), threshold={}, mode={}",
        scene_path,
        scene.width(),
        scene.height(),
        template_path,
        template.width(),
        template.height(),
        params.threshold,
        mode.as_str()
    );

    let cv: Arc<dyn TemplateMatcher> = Arc::new(OpenCvTemplateMatcher::new());
    bench_loop(
        "OpenCvTemplateMatcher(full)",
        cv,
        &scene_mat,
        &template,
        &params,
        warmup,
        iters,
    )
    .await?;

    if mode == BenchMode::Core {
        if params.green_mask || params.use_alpha_mask {
            eprintln!("skip OpenCV core mode: mask options are not supported on OpenCV path");
        } else {
            bench_opencv_core(&scene, &template, warmup, iters)?;
        }
    }

    Ok(())
}
