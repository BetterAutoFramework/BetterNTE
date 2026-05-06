//! 统一模型管理模块
//!
//! 所有 ONNX 模型的加载、推理、生命周期管理。
//!
//! ## 模型目录结构
//!
//!
//!
//! ## 使用示例
//!
//!

pub mod classifier;
pub mod detector;
pub mod ocr;
pub mod session;
pub mod superpoint;

use crate::error::VisionError;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

// ── 公共类型 ──────────────────────────────────────────────

/// 分类结果
#[derive(Debug, Clone)]
pub struct ClassifyResult {
    pub class_id: u32,
    pub label: String,
    pub confidence: f32,
}

/// 检测结果
#[derive(Debug, Clone)]
pub struct DetectResult {
    pub class_id: u32,
    pub label: String,
    pub confidence: f32,
    pub bbox: [f32; 4], // [x1, y1, x2, y2]
}

// ── Trait 定义 ────────────────────────────────────────────

/// 图像分类器 trait
pub trait Classifier: Send + Sync {
    fn classify(
        &self,
        image: &image::DynamicImage,
        top_k: usize,
    ) -> Result<Vec<ClassifyResult>, VisionError>;
}

/// 目标检测器 trait
pub trait Detector: Send + Sync {
    fn detect(
        &self,
        image: &image::DynamicImage,
        confidence: f32,
    ) -> Result<Vec<DetectResult>, VisionError>;
}

// ── ModelHub: 统一管理 ───────────────────────────────────

/// 模型管理中心
pub struct ModelHub {
    base_dir: PathBuf,
    classifiers: HashMap<String, Arc<dyn Classifier>>,
    detectors: HashMap<String, Arc<dyn Detector>>,
    ocr_engines: HashMap<String, Arc<ocr::PaddleOcrEngine>>,
    superpoint_detectors: HashMap<String, Arc<superpoint::SuperPointDetector>>,
}

impl ModelHub {
    pub fn new(base_dir: PathBuf) -> Self {
        Self {
            base_dir,
            classifiers: HashMap::new(),
            detectors: HashMap::new(),
            ocr_engines: HashMap::new(),
            superpoint_detectors: HashMap::new(),
        }
    }

    /// 从 assets/models/ 目录创建
    pub fn from_assets() -> Self {
        Self::new(PathBuf::from("assets/models"))
    }

    // ── 加载方法 ─────────────────────────────────────────

    /// 加载分类器
    pub async fn load_classifier(
        &mut self,
        name: &str,
        model_file: &str,
    ) -> Result<(), VisionError> {
        let path = self.base_dir.join(model_file);
        let model = classifier::MobileNetClassifier::load(&path)?;
        self.classifiers.insert(name.to_string(), Arc::new(model));
        tracing::info!(name, path = %path.display(), "Classifier loaded");
        Ok(())
    }

    /// 加载检测器
    pub async fn load_detector(&mut self, name: &str, model_file: &str) -> Result<(), VisionError> {
        let path = self.base_dir.join(model_file);
        let model = detector::YoloDetector::load(&path)?;
        self.detectors.insert(name.to_string(), Arc::new(model));
        tracing::info!(name, path = %path.display(), "Detector loaded");
        Ok(())
    }

    /// 加载 OCR 引擎
    pub async fn load_ocr(
        &mut self,
        name: &str,
        det_file: &str,
        rec_file: &str,
    ) -> Result<(), VisionError> {
        let det_path = self.base_dir.join(det_file);
        let rec_path = self.base_dir.join(rec_file);
        let engine = ocr::PaddleOcrEngine::load(&det_path, &rec_path, None)?;
        self.ocr_engines.insert(name.to_string(), Arc::new(engine));
        tracing::info!(name, "OCR engine loaded");
        Ok(())
    }

    /// 加载 SuperPoint
    pub async fn load_superpoint(
        &mut self,
        name: &str,
        model_file: &str,
        input_size: (u32, u32),
    ) -> Result<(), VisionError> {
        let path = self.base_dir.join(model_file);
        let model = superpoint::SuperPointDetector::load(&path, input_size)?;
        self.superpoint_detectors
            .insert(name.to_string(), Arc::new(model));
        tracing::info!(name, path = %path.display(), "SuperPoint loaded");
        Ok(())
    }

    // ── 推理方法 ─────────────────────────────────────────

    /// 图像分类
    pub async fn classify(
        &self,
        name: &str,
        image: &image::DynamicImage,
        top_k: usize,
    ) -> Result<Vec<ClassifyResult>, VisionError> {
        let model = self
            .classifiers
            .get(name)
            .ok_or_else(|| VisionError::ModelNotFound(format!("classifier: {}", name)))?;
        model.classify(image, top_k)
    }

    /// 目标检测
    pub async fn detect(
        &self,
        name: &str,
        image: &image::DynamicImage,
        confidence: f32,
    ) -> Result<Vec<DetectResult>, VisionError> {
        let model = self
            .detectors
            .get(name)
            .ok_or_else(|| VisionError::ModelNotFound(format!("detector: {}", name)))?;
        model.detect(image, confidence)
    }

    /// 文字识别
    pub async fn ocr(
        &self,
        name: &str,
        image: &image::DynamicImage,
    ) -> Result<Vec<ocr::OcrResult>, VisionError> {
        let engine = self
            .ocr_engines
            .get(name)
            .ok_or_else(|| VisionError::ModelNotFound(format!("ocr: {}", name)))?;
        engine.recognize(image)
    }

    /// 特征提取
    pub async fn extract_features(
        &self,
        name: &str,
        image: &image::DynamicImage,
    ) -> Result<superpoint::FeatureDescriptors, VisionError> {
        let model = self
            .superpoint_detectors
            .get(name)
            .ok_or_else(|| VisionError::ModelNotFound(format!("superpoint: {}", name)))?;
        model.detect(image)
    }

    // ── 查询方法 ─────────────────────────────────────────

    pub fn list_models(&self) -> Vec<String> {
        let mut names: Vec<String> = self.classifiers.keys().cloned().collect();
        names.extend(self.detectors.keys().cloned());
        names.extend(self.ocr_engines.keys().cloned());
        names.extend(self.superpoint_detectors.keys().cloned());
        names.sort();
        names
    }

    pub fn has_model(&self, name: &str) -> bool {
        self.classifiers.contains_key(name)
            || self.detectors.contains_key(name)
            || self.ocr_engines.contains_key(name)
            || self.superpoint_detectors.contains_key(name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_model_hub_creation() {
        let hub = ModelHub::from_assets();
        assert!(hub.list_models().is_empty());
    }

    #[test]
    fn test_model_hub_custom_dir() {
        let hub = ModelHub::new("/tmp/models".into());
        assert!(hub.list_models().is_empty());
    }
}
