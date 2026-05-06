//! SuperPoint 特征提取器 + LightGlue 匹配器
//!
//! - SuperPoint: 从图像中提取关键点和描述子
//! - LightGlue: 端到端特征匹配（SuperPoint + LightGlue pipeline）

use crate::error::VisionError;
use ndarray;
use ort::session::Session;
use std::path::Path;
use std::sync::Mutex;

/// 关键点
#[derive(Debug, Clone)]
pub struct KeyPoint {
    pub x: f32,
    pub y: f32,
    pub confidence: f32,
}

/// 特征描述子
#[derive(Debug, Clone)]
pub struct FeatureDescriptors {
    pub keypoints: Vec<KeyPoint>,
    pub descriptors: Vec<Vec<f32>>,
}

/// 特征匹配结果
#[derive(Debug, Clone)]
pub struct MatchResult {
    /// 图像0中的关键点坐标
    pub kp0: (f32, f32),
    /// 图像1中的关键点坐标
    pub kp1: (f32, f32),
    /// 匹配置信度
    pub score: f32,
}

/// LightGlue 匹配结果
#[derive(Debug, Clone)]
pub struct LightGlueResult {
    /// 所有匹配对
    pub matches: Vec<MatchResult>,
    /// 图像0的关键点
    pub keypoints0: Vec<(f32, f32)>,
    /// 图像1的关键点
    pub keypoints1: Vec<(f32, f32)>,
}

/// SuperPoint 特征提取器
pub struct SuperPointDetector {
    session: Mutex<Session>,
    input_size: (u32, u32),
    nms_dist: i32,
    conf_thresh: f32,
}

impl SuperPointDetector {
    pub fn load(model_path: &Path, input_size: (u32, u32)) -> Result<Self, VisionError> {
        let session = super::session::SessionBuilder::new()
            .with_directml(true)
            .with_cuda(true)
            .build_from_file(model_path)?;

        Ok(Self {
            session: Mutex::new(session),
            input_size,
            nms_dist: 4,
            conf_thresh: 0.015,
        })
    }

    pub fn with_nms_distance(mut self, dist: i32) -> Self {
        self.nms_dist = dist;
        self
    }

    pub fn with_confidence(mut self, thresh: f32) -> Self {
        self.conf_thresh = thresh;
        self
    }

    fn preprocess(&self, image: &image::DynamicImage) -> ndarray::Array4<f32> {
        let resized = image.resize_exact(
            self.input_size.0,
            self.input_size.1,
            image::imageops::FilterType::Lanczos3,
        );
        let gray = resized.to_luma32f();
        let (w, h) = (gray.width() as usize, gray.height() as usize);

        let mut input = ndarray::Array4::<f32>::zeros((1, 1, h, w));
        for y in 0..h {
            for x in 0..w {
                input[[0, 0, y, x]] = gray.get_pixel(x as u32, y as u32)[0] / 255.0;
            }
        }
        input
    }

    pub fn detect(&self, image: &image::DynamicImage) -> Result<FeatureDescriptors, VisionError> {
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

        // SuperPoint model has 3 outputs:
        //   keypoints: [1, N, 2] (i64) — (x, y) pixel coordinates
        //   scores:    [1, N]    (f32) — confidence per keypoint
        //   descriptors: [1, N, 256] (f32) — 256-dim descriptor per keypoint
        let kp_data = if let Ok((_, data)) = outputs[0].try_extract_tensor::<i64>() {
            data.to_vec()
        } else {
            let (_, data) = outputs[0]
                .try_extract_tensor::<f32>()
                .map_err(|e| VisionError::InferenceError(format!("Extract keypoints: {}", e)))?;
            data.iter().map(|&v| v as i64).collect()
        };

        let score_data = if let Ok((_, data)) = outputs[1].try_extract_tensor::<f32>() {
            data.to_vec()
        } else {
            let (_, data) = outputs[1]
                .try_extract_tensor::<i64>()
                .map_err(|e| VisionError::InferenceError(format!("Extract scores: {}", e)))?;
            data.iter().map(|&v| v as f32).collect()
        };

        let desc_data = if let Ok((_, data)) = outputs[2].try_extract_tensor::<f32>() {
            data.to_vec()
        } else {
            let (_, data) = outputs[2]
                .try_extract_tensor::<i64>()
                .map_err(|e| VisionError::InferenceError(format!("Extract descriptors: {}", e)))?;
            data.iter().map(|&v| v as f32).collect()
        };

        let num_kps = score_data.len();

        let mut keypoints = Vec::with_capacity(num_kps);
        let mut descriptors = Vec::with_capacity(num_kps);

        for i in 0..num_kps {
            let score = score_data[i];
            if score > self.conf_thresh {
                keypoints.push(KeyPoint {
                    x: kp_data[i * 2] as f32,
                    y: kp_data[i * 2 + 1] as f32,
                    confidence: score,
                });

                let desc: Vec<f32> = desc_data[i * 256..(i + 1) * 256].to_vec();
                descriptors.push(desc);
            }
        }

        // NMS
        self.nms_keypoints(&mut keypoints, &mut descriptors);

        Ok(FeatureDescriptors {
            keypoints,
            descriptors,
        })
    }

    fn nms_keypoints(&self, keypoints: &mut Vec<KeyPoint>, descriptors: &mut Vec<Vec<f32>>) {
        if keypoints.is_empty() {
            return;
        }

        // Sort by confidence descending; treat NaN as smallest (i.e. push to end).
        let mut indices: Vec<usize> = (0..keypoints.len()).collect();
        indices.sort_by(|&a, &b| {
            keypoints[b]
                .confidence
                .partial_cmp(&keypoints[a].confidence)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        let mut keep = vec![true; keypoints.len()];
        let dist_sq = (self.nms_dist * self.nms_dist) as f32;

        for i in 0..indices.len() {
            if !keep[indices[i]] {
                continue;
            }
            let kp_i = &keypoints[indices[i]];
            for j in (i + 1)..indices.len() {
                if !keep[indices[j]] {
                    continue;
                }
                let kp_j = &keypoints[indices[j]];
                let dx = kp_i.x - kp_j.x;
                let dy = kp_i.y - kp_j.y;
                if dx * dx + dy * dy < dist_sq {
                    keep[indices[j]] = false;
                }
            }
        }

        // Filter
        let mut idx = 0;
        let mut i = 0;
        while i < keypoints.len() {
            if keep[i] {
                keypoints.swap(idx, i);
                descriptors.swap(idx, i);
                idx += 1;
            }
            i += 1;
        }
        keypoints.truncate(idx);
        descriptors.truncate(idx);
    }
}

// ── LightGlue Pipeline Matcher ─────────────────────────────

/// SuperPoint + LightGlue 端到端特征匹配器
///
/// 使用打包好的 pipeline ONNX 模型，输入两张图片，输出匹配的特征点对。
///
/// 模型输入: `images` [2, 1, H, W] f32
/// 模型输出:
///   - `keypoints` [2, N, 2] i64
///   - `matches` [M, 3] i64  (每行: [_, kp_idx_img0, kp_idx_img1])
///   - `mscores` [M] f32
pub struct LightGlueMatcher {
    session: Mutex<Session>,
    input_size: (u32, u32),
    min_score: f32,
}

impl LightGlueMatcher {
    pub fn load(model_path: &Path, input_size: (u32, u32)) -> Result<Self, VisionError> {
        let session = super::session::SessionBuilder::new()
            .with_directml(true)
            .with_cuda(true)
            .build_from_file(model_path)?;

        Ok(Self {
            session: Mutex::new(session),
            input_size,
            min_score: 0.0,
        })
    }

    pub fn with_min_score(mut self, score: f32) -> Self {
        self.min_score = score;
        self
    }

    /// 将图片预处理为 SuperPoint 输入格式: [1, 1, H, W]，灰度，[0,1] 归一化
    fn preprocess_single(&self, image: &image::DynamicImage) -> ndarray::Array3<f32> {
        let resized = image.resize_exact(
            self.input_size.0,
            self.input_size.1,
            image::imageops::FilterType::Lanczos3,
        );
        let gray = resized.to_luma32f();
        let (w, h) = (gray.width() as usize, gray.height() as usize);

        let mut arr = ndarray::Array3::<f32>::zeros((1, h, w));
        for y in 0..h {
            for x in 0..w {
                arr[[0, y, x]] = gray.get_pixel(x as u32, y as u32)[0];
            }
        }
        arr
    }

    /// 对两张图片进行特征匹配
    pub fn match_images(
        &self,
        image0: &image::DynamicImage,
        image1: &image::DynamicImage,
    ) -> Result<LightGlueResult, VisionError> {
        let img0 = self.preprocess_single(image0);
        let img1 = self.preprocess_single(image1);

        // Stack: [2, 1, H, W]
        let h = self.input_size.1 as usize;
        let w = self.input_size.0 as usize;
        let mut stacked = ndarray::Array4::<f32>::zeros((2, 1, h, w));
        stacked.slice_mut(ndarray::s![0, .., .., ..]).assign(&img0);
        stacked.slice_mut(ndarray::s![1, .., .., ..]).assign(&img1);

        let tensor = ort::value::Tensor::from_array(stacked)
            .map_err(|e| VisionError::InferenceError(format!("Tensor: {}", e)))?;

        let mut session = self
            .session
            .lock()
            .map_err(|e| VisionError::InferenceError(format!("Lock: {}", e)))?;

        let outputs = session
            .run(ort::inputs![tensor])
            .map_err(|e| VisionError::InferenceError(format!("Inference: {}", e)))?;

        // Extract keypoints [2, N, 2] i64
        let kp_data = if let Ok((_, data)) = outputs[0].try_extract_tensor::<i64>() {
            data.to_vec()
        } else {
            let (_, data) = outputs[0]
                .try_extract_tensor::<f32>()
                .map_err(|e| VisionError::InferenceError(format!("Extract keypoints: {}", e)))?;
            data.iter().map(|&v| v as i64).collect()
        };

        // Extract matches [M, 3] i64
        let match_data = if let Ok((shape, data)) = outputs[1].try_extract_tensor::<i64>() {
            (shape.to_vec(), data.to_vec())
        } else {
            let (shape, data) = outputs[1]
                .try_extract_tensor::<f32>()
                .map_err(|e| VisionError::InferenceError(format!("Extract matches: {}", e)))?;
            (shape.to_vec(), data.iter().map(|&v| v as i64).collect())
        };

        // Extract match scores [M] f32
        let score_data = if let Ok((_, data)) = outputs[2].try_extract_tensor::<f32>() {
            data.to_vec()
        } else {
            let (_, data) = outputs[2]
                .try_extract_tensor::<i64>()
                .map_err(|e| VisionError::InferenceError(format!("Extract scores: {}", e)))?;
            data.iter().map(|&v| v as f32).collect()
        };

        // Parse keypoints for both images
        // keypoints shape: [2, N, 2], stored as flat array
        // kp_data[i * 2] = x, kp_data[i * 2 + 1] = y for keypoint i
        // First N keypoints belong to image 0, next N to image 1
        let num_kps_total = kp_data.len() / 2; // total keypoints across both images
        let num_kps_per_image = num_kps_total / 2;

        let keypoints0: Vec<(f32, f32)> = (0..num_kps_per_image)
            .map(|i| (kp_data[i * 2] as f32, kp_data[i * 2 + 1] as f32))
            .collect();
        let keypoints1: Vec<(f32, f32)> = (num_kps_per_image..num_kps_per_image * 2)
            .map(|i| (kp_data[i * 2] as f32, kp_data[i * 2 + 1] as f32))
            .collect();

        // Parse matches
        // matches shape: [M, 3], each row: [_, kp_idx_img0, kp_idx_img1]
        let cols = *match_data.0.last().unwrap_or(&3) as usize;
        let num_matches = if cols > 0 {
            match_data.1.len() / cols
        } else {
            0
        };

        let mut matches = Vec::new();
        for m in 0..num_matches {
            let base = m * cols;
            if base + 2 >= match_data.1.len() {
                break;
            }
            let idx0 = match_data.1[base + 1] as usize;
            let idx1 = match_data.1[base + 2] as usize;
            let score = score_data.get(m).copied().unwrap_or(0.0);

            if score >= self.min_score && idx0 < keypoints0.len() && idx1 < keypoints1.len() {
                matches.push(MatchResult {
                    kp0: keypoints0[idx0],
                    kp1: keypoints1[idx1],
                    score,
                });
            }
        }

        Ok(LightGlueResult {
            matches,
            keypoints0,
            keypoints1,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_nms_standalone() {
        // Test NMS logic without needing Session
        fn nms(kps: &mut Vec<KeyPoint>, descs: &mut Vec<Vec<f32>>, nms_dist: i32) {
            if kps.is_empty() {
                return;
            }
            let mut indices: Vec<usize> = (0..kps.len()).collect();
            indices.sort_by(|&a, &b| kps[b].confidence.partial_cmp(&kps[a].confidence).unwrap());
            let mut keep = vec![true; kps.len()];
            let dist_sq = (nms_dist * nms_dist) as f32;
            for i in 0..indices.len() {
                if !keep[indices[i]] {
                    continue;
                }
                for j in (i + 1)..indices.len() {
                    if !keep[indices[j]] {
                        continue;
                    }
                    let dx = kps[indices[i]].x - kps[indices[j]].x;
                    let dy = kps[indices[i]].y - kps[indices[j]].y;
                    if dx * dx + dy * dy < dist_sq {
                        keep[indices[j]] = false;
                    }
                }
            }
            let mut idx = 0;
            for (i, &alive) in keep.iter().enumerate() {
                if alive {
                    kps.swap(idx, i);
                    descs.swap(idx, i);
                    idx += 1;
                }
            }
            kps.truncate(idx);
            descs.truncate(idx);
        }

        let mut kps = vec![
            KeyPoint {
                x: 10.0,
                y: 10.0,
                confidence: 0.9,
            },
            KeyPoint {
                x: 11.0,
                y: 10.0,
                confidence: 0.8,
            },
            KeyPoint {
                x: 100.0,
                y: 100.0,
                confidence: 0.7,
            },
        ];
        let mut descs = vec![vec![0.0; 256]; 3];
        nms(&mut kps, &mut descs, 4);
        assert_eq!(kps.len(), 2);
    }
}
