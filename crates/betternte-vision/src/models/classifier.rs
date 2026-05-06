//! MobileNet-v3 图像分类器
//!
//! 支持自定义标签文件，适配微调后的任意类别数。
//!
//! 输入: 224x224 RGB 图像
//! 输出: 类别概率 (softmax)

use super::{Classifier, ClassifyResult};
use crate::error::VisionError;
use ndarray::Array4;
use ort::session::Session;
use std::path::Path;
use std::sync::Mutex;

pub struct MobileNetClassifier {
    session: Mutex<Session>,
    labels: Vec<String>,
}

impl MobileNetClassifier {
    /// 加载模型，使用 ImageNet-1000 标签
    pub fn load(model_path: &Path) -> Result<Self, VisionError> {
        Self::load_with_labels(model_path, imagenet_labels())
    }

    /// 加载模型，使用自定义标签文件（每行一个类别名）
    pub fn load_with_labels_file(
        model_path: &Path,
        labels_path: &Path,
    ) -> Result<Self, VisionError> {
        let content = std::fs::read_to_string(labels_path)
            .map_err(|e| VisionError::ConfigError(format!("Load labels: {}", e)))?;
        let labels: Vec<String> = content.lines().map(String::from).collect();
        Self::load_with_labels(model_path, labels)
    }

    /// 加载模型，使用自定义标签列表
    pub fn load_with_labels(model_path: &Path, labels: Vec<String>) -> Result<Self, VisionError> {
        let session = super::session::SessionBuilder::new()
            .with_directml(true)
            .with_cuda(true)
            .build_from_file(model_path)?;

        Ok(Self {
            session: Mutex::new(session),
            labels,
        })
    }

    pub fn from_session(session: Session) -> Self {
        Self {
            session: Mutex::new(session),
            labels: imagenet_labels(),
        }
    }

    fn preprocess(&self, image: &image::DynamicImage) -> Array4<f32> {
        let resized = image.resize_exact(224, 224, image::imageops::FilterType::Triangle);
        let rgb = resized.to_rgb8();

        let mean = [0.485f32, 0.456, 0.406];
        let std_val = [0.229f32, 0.224, 0.225];

        let mut input = Array4::<f32>::zeros((1, 3, 224, 224));
        for y in 0..224u32 {
            for x in 0..224u32 {
                let pixel = rgb.get_pixel(x, y);
                for c in 0..3 {
                    input[[0, c, y as usize, x as usize]] =
                        (pixel[c] as f32 / 255.0 - mean[c]) / std_val[c];
                }
            }
        }
        input
    }

    fn postprocess(&self, logits: &[f32], top_k: usize) -> Vec<ClassifyResult> {
        softmax_topk(logits, &self.labels, top_k)
    }
}

fn softmax_topk(logits: &[f32], labels: &[String], top_k: usize) -> Vec<ClassifyResult> {
    let max_val = logits.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
    let exp_sum: f32 = logits.iter().map(|&x| (x - max_val).exp()).sum();

    let mut probs: Vec<(usize, f32)> = logits
        .iter()
        .enumerate()
        .map(|(i, &logit)| (i, (logit - max_val).exp() / exp_sum))
        .collect();

    probs.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

    probs
        .into_iter()
        .take(top_k)
        .map(|(id, conf)| ClassifyResult {
            class_id: id as u32,
            label: labels
                .get(id)
                .cloned()
                .unwrap_or_else(|| format!("class_{}", id)),
            confidence: conf,
        })
        .collect()
}

impl Classifier for MobileNetClassifier {
    fn classify(
        &self,
        image: &image::DynamicImage,
        top_k: usize,
    ) -> Result<Vec<ClassifyResult>, VisionError> {
        let input = self.preprocess(image);
        let tensor = ort::value::Tensor::from_array(input)
            .map_err(|e| VisionError::InferenceError(format!("Tensor: {}", e)))?;

        let mut session = self
            .session
            .lock()
            .map_err(|e| VisionError::InferenceError(format!("Lock: {}", e)))?;

        let outputs = session
            .run(ort::inputs![tensor])
            .map_err(|e| VisionError::InferenceError(format!("Inference: {}", e)))?;

        let (_shape, data) = outputs[0]
            .try_extract_tensor::<f32>()
            .map_err(|e| VisionError::InferenceError(format!("Extract: {}", e)))?;

        let logits: Vec<f32> = data.to_vec();
        Ok(self.postprocess(&logits, top_k))
    }
}

fn imagenet_labels() -> Vec<String> {
    let mut labels: Vec<String> = vec![
        "tench",
        "goldfish",
        "great white shark",
        "tiger shark",
        "hammerhead",
        "electric ray",
        "stingray",
        "cock",
        "hen",
        "ostrich",
        "brambling",
        "goldfinch",
        "house finch",
        "junco",
        "indigo bunting",
        "robin",
        "bulbul",
        "jay",
        "magpie",
        "chickadee",
    ]
    .into_iter()
    .map(String::from)
    .collect();

    for i in labels.len()..1000 {
        labels.push(format!("class_{}", i));
    }
    labels
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_imagenet_labels_count() {
        let labels = imagenet_labels();
        assert_eq!(labels.len(), 1000);
        assert_eq!(labels[0], "tench");
    }

    #[test]
    fn test_postprocess_custom_labels() {
        let labels = vec!["cat".to_string(), "dog".to_string(), "bird".to_string()];
        let logits = vec![2.0, 1.0, 0.1];
        let results = softmax_topk(&logits, &labels, 3);

        assert_eq!(results.len(), 3);
        assert_eq!(results[0].label, "cat");
        assert_eq!(results[0].class_id, 0);
        assert!(results[0].confidence > results[1].confidence);
        assert_eq!(results[1].label, "dog");
    }
}
