//! 视觉引擎 trait 和相关类型。
//!
//! 定义 `OcrEngine`、`TemplateMatcher`、`ColorDetector` trait，
//! 以及 `MatchResult`、`TextRegion`、`OcrConfig`、`MatchConfig` 等共享类型。

use async_trait::async_trait;
use image::DynamicImage;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::image::{BoundingBox, CaptureFrame, Color, Point, Region};

// ============================================================================
// 配置类型
// ============================================================================

/// OCR 配置。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct OcrConfig {
    pub model_path: String,
    pub language: String,
    pub use_gpu: bool,
    pub batch_size: usize,
    pub max_side_len: u32,
    pub det_threshold: f64,
    pub rec_threshold: f64,
    pub unclip_ratio: f64,
    /// Target text color for enhanced recognition (e.g. "#FFFFFF" or "255,255,255").
    /// If set, only pixels within tolerance of this color are kept.
    pub text_color: Option<String>,
    /// Tolerance for text color matching (per-channel, 0-255). Default: 32.
    pub text_color_tolerance: u8,
}

impl Default for OcrConfig {
    fn default() -> Self {
        Self {
            model_path: "assets/models/paddleocr".into(),
            language: "ch".into(),
            use_gpu: false,
            batch_size: 1,
            max_side_len: 960,
            det_threshold: 0.3,
            rec_threshold: 0.5,
            unclip_ratio: 2.0,
            text_color: None,
            text_color_tolerance: 32,
        }
    }
}

/// 模板匹配配置。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MatchConfig {
    pub default_threshold: f32,
    pub multi_scale: Vec<f32>,
    pub nms_threshold: f32,
}

impl Default for MatchConfig {
    fn default() -> Self {
        Self {
            default_threshold: 0.8,
            multi_scale: vec![0.8, 0.9, 1.0, 1.1, 1.2],
            nms_threshold: 0.3,
        }
    }
}

/// Parameters for [`TemplateMatcher::match_template`] (threshold + optional green-screen masks).
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(default)]
pub struct TemplateMatchParams {
    /// Normalized cross-correlation score threshold in \[0, 1\].
    pub threshold: f32,
    /// When `true`, template pixels matching chroma-key green `#00FF00` within [`Self::green_mask_tolerance`]
    /// receive zero mask weight (green-screen convention).
    pub green_mask: bool,
    /// Max absolute per-channel delta from R=0, G=255, B=0 for green detection.
    pub green_mask_tolerance: u8,
    /// When `true`, template pixels with alpha ≤ [`Self::alpha_mask_threshold`] receive zero mask weight.
    pub use_alpha_mask: bool,
    /// Mask out template pixels whose alpha is ≤ this value (inclusive, 0–255).
    pub alpha_mask_threshold: u8,
    /// When `true`, convert both scene and template to grayscale before matching.
    /// When `false` (default), use full-color BGR matching for better accuracy.
    pub grayscale: bool,
}

impl Default for TemplateMatchParams {
    fn default() -> Self {
        Self {
            threshold: 0.8,
            green_mask: false,
            green_mask_tolerance: 0,
            use_alpha_mask: false,
            alpha_mask_threshold: 8,
            grayscale: false,
        }
    }
}

impl TemplateMatchParams {
    pub fn with_threshold(threshold: f32) -> Self {
        Self {
            threshold,
            ..Default::default()
        }
    }
}

// ============================================================================
// 结果类型
// ============================================================================

/// 模板匹配结果。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MatchResult {
    /// 匹配位置（左上角）
    pub position: Point,
    /// 匹配分数（0.0 ~ 1.0）
    pub score: f32,
    /// 匹配区域宽度
    pub width: u32,
    /// 匹配区域高度
    pub height: u32,
    /// 模板名称（由 match_multi 填充）
    #[serde(default)]
    pub template_name: String,
}

impl MatchResult {
    /// 匹配区域的中心点。
    pub fn center(&self) -> Point {
        Point::new(
            self.position.x + self.width as i32 / 2,
            self.position.y + self.height as i32 / 2,
        )
    }

    /// 匹配区域。
    pub fn region(&self) -> Region {
        Region {
            x: self.position.x,
            y: self.position.y,
            width: self.width,
            height: self.height,
        }
    }
}

/// OCR 识别出的文字区域。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextRegion {
    pub text: String,
    pub confidence: f32,
    pub bbox: BoundingBox,
    pub angle: f32,
}

// ============================================================================
// 错误类型
// ============================================================================

/// OCR 错误类型（轻量级）。
#[derive(Debug, thiserror::Error)]
pub enum OcrError {
    #[error("Model not found: {0}")]
    ModelNotFound(String),

    #[error("OCR error: {0}")]
    OcrError(String),

    #[error("OCR timeout after {0}ms")]
    Timeout(u64),
}

// ============================================================================
// Traits
// ============================================================================

/// OCR 引擎 trait。
///
/// OCR engine trait. Implementations must be `Send + Sync` for use in async contexts.
#[async_trait]
pub trait OcrEngine: Send + Sync {
    fn name(&self) -> &str;
    async fn init(&mut self, config: &OcrConfig) -> Result<(), OcrError>;
    async fn recognize(&self, image: &DynamicImage) -> Result<Vec<TextRegion>, OcrError>;
    async fn recognize_region(
        &self,
        frame: &CaptureFrame,
        region: &Region,
    ) -> Result<Vec<TextRegion>, OcrError>;
    fn is_ready(&self) -> bool;
}

/// 模板匹配器 trait。
#[async_trait]
pub trait TemplateMatcher: Send + Sync {
    /// 获取匹配器名称
    fn name(&self) -> &str;

    /// 单模板匹配
    async fn match_template(
        &self,
        image: &DynamicImage,
        template: &DynamicImage,
        params: &TemplateMatchParams,
    ) -> anyhow::Result<Vec<MatchResult>>;

    /// 多模板匹配
    async fn match_multi(
        &self,
        image: &DynamicImage,
        templates: &HashMap<String, DynamicImage>,
        params: &TemplateMatchParams,
    ) -> anyhow::Result<HashMap<String, Vec<MatchResult>>>;
}

/// How [`ColorDetector`] compares a sampled pixel to a target [`Color`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorTolerance {
    /// Max RGB Euclidean distance in 0–255 units (same as legacy `tolerance: u8`). Target alpha is ignored.
    Euclidean(u8),
    /// Max absolute per-channel delta for R, G, B, and A. Captured RGB pixels are treated as alpha 255.
    RgbaMaxDelta { r: u8, g: u8, b: u8, a: u8 },
}

impl ColorTolerance {
    #[inline]
    pub fn matches(self, sample: Color, target: Color) -> bool {
        match self {
            Self::Euclidean(max) => {
                let dr = sample.r as f64 - target.r as f64;
                let dg = sample.g as f64 - target.g as f64;
                let db = sample.b as f64 - target.b as f64;
                let d = (dr * dr + dg * dg + db * db).sqrt().min(255.0) as u8;
                d <= max
            }
            Self::RgbaMaxDelta { r, g, b, a } => {
                u8_abs_diff(sample.r, target.r) <= r
                    && u8_abs_diff(sample.g, target.g) <= g
                    && u8_abs_diff(sample.b, target.b) <= b
                    && u8_abs_diff(sample.a, target.a) <= a
            }
        }
    }
}

#[inline]
fn u8_abs_diff(a: u8, b: u8) -> u8 {
    if a >= b {
        a - b
    } else {
        b - a
    }
}

impl From<u8> for ColorTolerance {
    fn from(max: u8) -> Self {
        Self::Euclidean(max)
    }
}

/// 颜色检测器 trait。
pub trait ColorDetector: Send + Sync {
    fn detect_pixel(
        &self,
        image: &DynamicImage,
        pos: Point,
        target: Color,
        tolerance: ColorTolerance,
    ) -> bool;

    fn find_color(
        &self,
        image: &DynamicImage,
        target: Color,
        tolerance: ColorTolerance,
    ) -> Vec<Point>;

    fn detect_color_region(
        &self,
        image: &DynamicImage,
        region: &Region,
        target: Color,
        tolerance: ColorTolerance,
    ) -> f32;

    fn get_pixel_color(&self, image: &DynamicImage, pos: Point) -> Option<Color>;
}
