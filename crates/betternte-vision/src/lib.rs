//! betternte-vision: 计算机视觉（模板匹配、颜色检测、图像预处理、OCR）
//!
//! ## Core Traits (from betternte-core)
//!
//! - `TemplateMatcher` - Template matching using NCC
//! - `ColorDetector` - Color detection in images
//! - `OcrEngine` - OCR engine trait
//!
//! ## Modules
//!
//! - `template/` - OpenCV-based template matching
//! - `contour/` - Contour extraction, analysis, filtering
//! - `morphology/` - Morphological operations
//! - `geometry/` - Homography estimation, perspective warp
//! - `classify/` - Color range detection, histogram
//! - `feature/` - Feature detection (SuperPoint stub), matching
//! - `models/` - ONNX model management (classifier, detector, OCR)

pub mod classify;
pub mod color;
pub mod config;
pub mod contour;
pub mod error;
pub mod feature;
pub mod geometry;
pub mod image_utils;
pub mod models;
pub mod morphology;
pub mod pixel_math;
pub mod template;

// Re-exports
pub use color::ColorDetectorImpl;
pub use config::{MatchConfig, OcrConfig};
pub use error::VisionError;
pub use image_utils::ImagePreprocessor;

// Template exports
pub use template::{MatchResult, OpenCvTemplateMatcher, TemplateCache, TemplateMatcher};

// Contour exports
pub use contour::{
    Contour, ContourAnalyzer, ContourFilter, ContourFinder, ContourHierarchy, ContourProperties,
};

// Morphology exports
pub use morphology::{MorphKernel, Morphology};

// Geometry exports
pub use geometry::{Homography, PerspectiveWarp};

// Classify exports
pub use classify::{ColorRangeDetector, Histogram};

// Pixel math exports
pub use pixel_math::PixelMath;

// Feature exports
pub use feature::{ColorPoint, Feature, FeatureMatch, FeatureMatcher, TextRegion};

// SuperPoint / LightGlue (real ONNX-backed implementations)
pub use models::superpoint::{
    FeatureDescriptors, KeyPoint, LightGlueMatcher, LightGlueResult, SuperPointDetector,
};

// Re-export core traits
pub use betternte_core::{ColorDetector, OcrEngine};

// OCR engine implementations

use betternte_core::{BoundingBox, CaptureFrame, Color, Region};
use image::DynamicImage;
use std::path::PathBuf;

/// Parse a color string like "#RRGGBB", "RRGGBB", or "R,G,B" into (r, g, b).
pub fn parse_color_str(s: &str) -> Option<(u8, u8, u8)> {
    let s = s.trim();
    if let Some(hex) = s.strip_prefix('#') {
        if hex.len() == 6 {
            let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
            let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
            let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
            return Some((r, g, b));
        }
    } else if s.len() == 6 {
        let r = u8::from_str_radix(&s[0..2], 16).ok()?;
        let g = u8::from_str_radix(&s[2..4], 16).ok()?;
        let b = u8::from_str_radix(&s[4..6], 16).ok()?;
        return Some((r, g, b));
    }
    // Try "R,G,B" format
    let parts: Vec<&str> = s.split(',').collect();
    if parts.len() == 3 {
        let r: u8 = parts[0].trim().parse().ok()?;
        let g: u8 = parts[1].trim().parse().ok()?;
        let b: u8 = parts[2].trim().parse().ok()?;
        return Some((r, g, b));
    }
    None
}

/// Apply text color filter: keep pixels within tolerance of target color, make others black.
pub fn apply_text_color_filter(img: &DynamicImage, target: (u8, u8, u8), tolerance: u8) -> DynamicImage {
    let rgb = img.to_rgb8();
    let (w, h) = rgb.dimensions();
    let (tr, tg, tb) = target;
    let tol = tolerance as i16;

    let filtered = image::ImageBuffer::from_fn(w, h, |x, y| {
        let p = rgb.get_pixel(x, y);
        let dr = (p[0] as i16 - tr as i16).abs();
        let dg = (p[1] as i16 - tg as i16).abs();
        let db = (p[2] as i16 - tb as i16).abs();
        if dr <= tol && dg <= tol && db <= tol {
            // Keep original pixel (text)
            image::Rgb([p[0], p[1], p[2]])
        } else {
            // Background -> black
            image::Rgb([0u8, 0, 0])
        }
    });

    DynamicImage::ImageRgb8(filtered)
}

/// PaddleOCR 引擎（桩实现，待接入 ONNX 模型）
pub struct PaddleOcrEngine {
    config: OcrConfig,
    ready: bool,
    inner: Option<crate::models::ocr::PaddleOcrEngine>,
}

impl Default for PaddleOcrEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl PaddleOcrEngine {
    pub fn new() -> Self {
        Self {
            config: OcrConfig::default(),
            ready: false,
            inner: None,
        }
    }

    fn convert_results(
        raw: Vec<crate::models::ocr::OcrResult>,
        offset_x: f64,
        offset_y: f64,
    ) -> Vec<TextRegion> {
        raw.into_iter()
            .map(|r| {
                let (min_x, max_x) = r
                    .bbox
                    .iter()
                    .map(|(x, _)| *x as f64)
                    .fold((f64::INFINITY, f64::NEG_INFINITY), |(minv, maxv), v| {
                        (minv.min(v), maxv.max(v))
                    });
                let (min_y, max_y) = r
                    .bbox
                    .iter()
                    .map(|(_, y)| *y as f64)
                    .fold((f64::INFINITY, f64::NEG_INFINITY), |(minv, maxv), v| {
                        (minv.min(v), maxv.max(v))
                    });

                TextRegion {
                    text: r.text,
                    confidence: r.confidence,
                    bbox: BoundingBox {
                        x: min_x + offset_x,
                        y: min_y + offset_y,
                        width: (max_x - min_x).max(0.0),
                        height: (max_y - min_y).max(0.0),
                        confidence: r.confidence as f64,
                        label: None,
                    },
                    angle: 0.0,
                }
            })
            .collect()
    }
}

#[async_trait::async_trait]
impl OcrEngine for PaddleOcrEngine {
    fn name(&self) -> &str {
        "paddleocr"
    }

    async fn init(&mut self, config: &OcrConfig) -> Result<(), betternte_core::OcrError> {
        self.config = config.clone();
        let base = PathBuf::from(&config.model_path);
        let det_path = base.join("det.onnx");
        let rec_path = base.join("rec.onnx");

        // Try several common PaddleOCR dict filenames in order of recency.
        const DICT_CANDIDATES: &[&str] = &[
            "ppocrv5_dict.txt",
            "ppocr_keys_v5.txt",
            "ppocr_keys_v4.txt",
            "ppocr_keys_v3.txt",
            "ppocr_keys_v1.txt",
            "ppocr_dict.txt",
            "dict.txt",
        ];
        let dict_path = DICT_CANDIDATES
            .iter()
            .map(|name| base.join(name))
            .find(|p| p.exists());
        if dict_path.is_none() {
            tracing::warn!(
                base = %base.display(),
                candidates = ?DICT_CANDIDATES,
                "OCR dict not found, falling back to empty dict — recognition results will be empty"
            );
        }
        let dict_opt = dict_path.as_deref();

        let rec_batch_size = if config.batch_size <= 1 {
            usize::MAX
        } else {
            config.batch_size
        };
        match crate::models::ocr::PaddleOcrEngine::load(&det_path, &rec_path, dict_opt) {
            Ok(mut engine) => {
                engine.configure(
                    config.det_threshold as f32,
                    config.unclip_ratio as f32,
                    config.max_side_len,
                    rec_batch_size,
                );
                self.inner = Some(engine);
                tracing::info!(
                    det = %det_path.display(),
                    rec = %rec_path.display(),
                    det_threshold = config.det_threshold,
                    unclip_ratio = config.unclip_ratio,
                    rec_batch_size,
                    "PaddleOCR ONNX engine loaded"
                );
            }
            Err(e) => {
                self.inner = None;
                tracing::warn!(
                    error = %e,
                    det = %det_path.display(),
                    rec = %rec_path.display(),
                    "PaddleOCR ONNX load failed, OCR will return empty results"
                );
            }
        }
        self.ready = true;
        Ok(())
    }

    async fn recognize(
        &self,
        image: &DynamicImage,
    ) -> Result<Vec<TextRegion>, betternte_core::OcrError> {
        if !self.ready {
            return Err(betternte_core::OcrError::OcrError("引擎未初始化".into()));
        }
        let Some(inner) = self.inner.as_ref() else {
            return Ok(Vec::new());
        };

        // Apply text color filter if configured
        let filtered_img;
        let img = if let Some(ref color_str) = self.config.text_color {
            if let Some(target_color) = parse_color_str(color_str) {
                filtered_img = apply_text_color_filter(image, target_color, self.config.text_color_tolerance);
                &filtered_img
            } else {
                image
            }
        } else {
            image
        };

        let raw = inner.recognize(img).map_err(|e| {
            betternte_core::OcrError::OcrError(format!("OCR inference failed: {}", e))
        })?;
        Ok(Self::convert_results(raw, 0.0, 0.0))
    }

    async fn recognize_region(
        &self,
        frame: &CaptureFrame,
        region: &Region,
    ) -> Result<Vec<TextRegion>, betternte_core::OcrError> {
        if !self.ready {
            return Err(betternte_core::OcrError::OcrError("引擎未初始化".into()));
        }
        let Some(inner) = self.inner.as_ref() else {
            return Ok(Vec::new());
        };

        let img = frame
            .to_dynamic_image()
            .map_err(|e| betternte_core::OcrError::OcrError(format!("to_dynamic_image: {}", e)))?;

        let x = region.x.max(0) as u32;
        let y = region.y.max(0) as u32;
        let max_w = img.width().saturating_sub(x);
        let max_h = img.height().saturating_sub(y);
        let w = region.width.min(max_w);
        let h = region.height.min(max_h);
        if w == 0 || h == 0 {
            return Ok(Vec::new());
        }

        let cropped = img.crop_imm(x, y, w, h);

        // Apply text color filter if configured
        let filtered_img;
        let img_ref = if let Some(ref color_str) = self.config.text_color {
            if let Some(target_color) = parse_color_str(color_str) {
                filtered_img = apply_text_color_filter(&cropped, target_color, self.config.text_color_tolerance);
                &filtered_img
            } else {
                &cropped
            }
        } else {
            &cropped
        };

        let raw = inner.recognize(img_ref).map_err(|e| {
            betternte_core::OcrError::OcrError(format!("OCR region inference failed: {}", e))
        })?;
        Ok(Self::convert_results(raw, x as f64, y as f64))
    }

    fn is_ready(&self) -> bool {
        self.ready
    }
}

/// 视觉流水线步骤
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type")]
pub enum VisionStep {
    Ocr {
        region: Option<Region>,
        contains: Option<String>,
    },
    TemplateMatch {
        template_path: String,
        threshold: f32,
        max_results: Option<usize>,
    },
    ColorDetect {
        target_color: Color,
        tolerance: u8,
        region: Option<Region>,
    },
    Crop {
        region: Region,
    },
    Grayscale,
}

/// 视觉处理流水线
pub struct VisionPipeline {
    steps: Vec<VisionStep>,
}

impl VisionPipeline {
    pub fn new() -> Self {
        Self { steps: Vec::new() }
    }

    pub fn add_step(&mut self, step: VisionStep) -> &mut Self {
        self.steps.push(step);
        self
    }

    pub fn steps(&self) -> &[VisionStep] {
        &self.steps
    }
}

impl Default for VisionPipeline {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_paddle_ocr_engine_name() {
        let engine = PaddleOcrEngine::new();
        assert_eq!(engine.name(), "paddleocr");
        assert!(!engine.is_ready());
    }

    #[tokio::test]
    async fn test_paddle_ocr_engine_init() {
        let mut engine = PaddleOcrEngine::new();
        engine.init(&OcrConfig::default()).await.unwrap();
        assert!(engine.is_ready());
    }

    #[tokio::test]
    async fn test_paddle_ocr_engine_recognize_not_ready() {
        let engine = PaddleOcrEngine::new();
        let img = DynamicImage::new_rgb8(10, 10);
        let result = engine.recognize(&img).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_paddle_ocr_engine_recognize_empty() {
        let mut engine = PaddleOcrEngine::new();
        engine.init(&OcrConfig::default()).await.unwrap();
        let img = DynamicImage::new_rgb8(10, 10);
        let result = engine.recognize(&img).await.unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_vision_pipeline_steps() {
        let mut pipeline = VisionPipeline::new();
        pipeline.add_step(VisionStep::Grayscale);
        pipeline.add_step(VisionStep::ColorDetect {
            target_color: Color::RED,
            tolerance: 30,
            region: None,
        });
    }

    #[tokio::test]
    async fn test_ocr_not_ready_returns_error() {
        let engine = PaddleOcrEngine::new();
        assert!(!engine.is_ready());
        let image = image::DynamicImage::ImageRgb8(image::RgbImage::new(100, 100));
        let result = engine.recognize(&image).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_ocr_recognize_empty_image_returns_empty() {
        let mut engine = PaddleOcrEngine::new();
        engine.init(&OcrConfig::default()).await.unwrap();
        let image = image::DynamicImage::ImageRgb8(image::RgbImage::new(10, 10));
        let result = engine.recognize(&image).await;
        if let Ok(regions) = result {
            assert!(regions.is_empty());
        }
    }

    #[tokio::test]
    async fn test_ocr_recognize_returns_text_regions() {
        let mut engine = PaddleOcrEngine::new();
        engine.init(&OcrConfig::default()).await.unwrap();
        let image = image::DynamicImage::ImageRgb8(image::RgbImage::new(100, 50));
        let result = engine.recognize(&image).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_ocr_recognize_region_crops_correctly() {
        let mut engine = PaddleOcrEngine::new();
        engine.init(&OcrConfig::default()).await.unwrap();
        let data = vec![128u8; 400 * 300 * 4];
        let frame = CaptureFrame::new(
            400,
            300,
            data,
            betternte_core::PixelFormat::Rgba,
            "test".into(),
        );
        let region = Region {
            x: 10,
            y: 10,
            width: 200,
            height: 100,
        };
        let result = engine.recognize_region(&frame, &region).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_ocr_recognize_batch_matches_individual() {
        let mut engine = PaddleOcrEngine::new();
        engine.init(&OcrConfig::default()).await.unwrap();
        let regions = vec![
            Region {
                x: 0,
                y: 0,
                width: 100,
                height: 50,
            },
            Region {
                x: 100,
                y: 0,
                width: 50,
                height: 50,
            },
        ];
        let mut all_results = Vec::new();
        for region in &regions {
            let data = vec![0u8; (region.width * region.height * 4) as usize];
            let frame = CaptureFrame::new(
                region.width,
                region.height,
                data,
                betternte_core::PixelFormat::Rgba,
                "test".into(),
            );
            if let Ok(texts) = engine.recognize_region(&frame, region).await {
                all_results.extend(texts);
            }
        }
        let _ = all_results;
    }
}
