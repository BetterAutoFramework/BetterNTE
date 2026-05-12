//! PaddleOCR text recognition (DBNet detection + CRNN recognition).
//!
//! All preprocessing uses OpenCV `Mat` directly — no DynamicImage conversion.

use crate::error::VisionError;
use ndarray::{s, Array2, Array4, Axis};
use opencv::core::{Mat, Rect, Size};
use opencv::imgproc;
use opencv::prelude::{MatTraitConst, MatTraitConstManual};
use ort::session::Session;
use std::path::Path;
use std::sync::Mutex;

#[derive(Debug, Clone)]
pub struct OcrResult {
    pub text: String,
    pub confidence: f32,
    pub bbox: [(f32, f32); 4],
}

#[derive(Debug, Clone, Copy)]
struct RecCropMeta {
    bbox: [(f32, f32); 4],
}

pub struct PaddleOcrEngine {
    det_session: Mutex<Session>,
    rec_session: Mutex<Session>,
    #[allow(dead_code)]
    dict: Vec<String>,
    det_threshold: f32,
    unclip_ratio: f32,
    max_side_len: u32,
    rec_batch_size: usize,
}

impl PaddleOcrEngine {
    pub fn load(
        det_path: &Path,
        rec_path: &Path,
        dict_path: Option<&Path>,
    ) -> Result<Self, VisionError> {
        let det_session = super::session::SessionBuilder::new()
            .with_directml(true)
            .with_cuda(true)
            .build_from_file(det_path)?;
        let rec_session = super::session::SessionBuilder::new()
            .with_directml(true)
            .with_cuda(true)
            .build_from_file(rec_path)?;

        let dict = if let Some(dp) = dict_path {
            Self::load_dict(dp)?
        } else {
            tracing::warn!(
                "PaddleOCR loaded without a dict file; CTC decoder will emit empty strings"
            );
            default_dict()
        };

        Ok(Self {
            det_session: Mutex::new(det_session),
            rec_session: Mutex::new(rec_session),
            dict,
            det_threshold: 0.3,
            unclip_ratio: 2.0,
            max_side_len: 960,
            rec_batch_size: usize::MAX,
        })
    }

    fn load_dict(path: &Path) -> Result<Vec<String>, VisionError> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| VisionError::ConfigError(format!("Load dict: {}", e)))?;
        Ok(content.lines().map(String::from).collect())
    }

    pub fn configure(
        &mut self,
        det_threshold: f32,
        unclip_ratio: f32,
        max_side_len: u32,
        rec_batch_size: usize,
    ) {
        self.det_threshold = det_threshold.clamp(0.0, 1.0);
        self.unclip_ratio = unclip_ratio.max(0.1);
        self.max_side_len = max_side_len.max(32);
        self.rec_batch_size = rec_batch_size.max(1);
    }

    /// Main entry point: recognize text in a BGRA Mat.
    pub fn recognize(&self, image: &Mat) -> Result<Vec<OcrResult>, VisionError> {
        // Convert BGRA → BGR once, used for all subsequent operations
        let bgr = Self::ensure_bgr(image)?;

        let boxes = self.detect_text_regions(&bgr)?;
        if boxes.is_empty() {
            return Ok(Vec::new());
        }

        let mut results = Vec::new();
        for chunk in boxes.chunks(self.rec_batch_size) {
            match self.recognize_regions_batch(&bgr, chunk) {
                Ok(mut part) => results.append(&mut part),
                Err(err) => {
                    tracing::warn!(
                        error = %err,
                        regions = chunk.len(),
                        "OCR rec batch failed, falling back to per-region inference"
                    );
                    let mut part = self.recognize_regions_sequential(&bgr, chunk)?;
                    results.append(&mut part);
                }
            }
        }
        Ok(results.into_iter().filter(|r| !r.text.is_empty()).collect())
    }

    /// Convert input Mat to 3-channel BGR.
    /// Handles BGRA→BGR and BGR passthrough.
    fn ensure_bgr(image: &Mat) -> Result<Mat, VisionError> {
        let channels = image.channels();
        match channels {
            4 => {
                let mut bgr = Mat::default();
                imgproc::cvt_color(image, &mut bgr, imgproc::COLOR_BGRA2BGR, 0)
                    .map_err(|e| VisionError::InferenceError(format!("cvtColor BGRA→BGR: {}", e)))?;
                Ok(bgr)
            }
            3 => Ok(image.clone()),
            1 => {
                let mut bgr = Mat::default();
                imgproc::cvt_color(image, &mut bgr, imgproc::COLOR_GRAY2BGR, 0)
                    .map_err(|e| VisionError::InferenceError(format!("cvtColor Gray→BGR: {}", e)))?;
                Ok(bgr)
            }
            _ => Err(VisionError::InferenceError(format!(
                "Unsupported Mat channels: {}",
                channels
            ))),
        }
    }

    fn recognize_regions_sequential(
        &self,
        image: &Mat,
        boxes: &[[(f32, f32); 4]],
    ) -> Result<Vec<OcrResult>, VisionError> {
        let mut results = Vec::new();
        for bbox in boxes {
            let text = self.recognize_region(image, bbox)?;
            if !text.text.is_empty() {
                results.push(text);
            }
        }
        Ok(results)
    }

    fn detect_text_regions(
        &self,
        bgr: &Mat,
    ) -> Result<Vec<[(f32, f32); 4]>, VisionError> {
        let (orig_w, orig_h) = (bgr.cols() as u32, bgr.rows() as u32);
        let (resized, _new_w, _new_h) = self.preprocess_det_resize(bgr)?;

        let input = Self::mat_to_chw_f32_normalize(
            &resized,
            &[0.485f32, 0.456, 0.406],
            &[0.229f32, 0.224, 0.225],
        );
        let tensor = ort::value::Tensor::from_array(input)
            .map_err(|e| VisionError::InferenceError(format!("Det tensor: {}", e)))?;

        let mut session = self
            .det_session
            .lock()
            .map_err(|e| VisionError::InferenceError(format!("Lock: {}", e)))?;

        let outputs = session
            .run(ort::inputs![tensor])
            .map_err(|e| VisionError::InferenceError(format!("Det inference: {}", e)))?;

        let (shape, data) = outputs[0]
            .try_extract_tensor::<f32>()
            .map_err(|e| VisionError::InferenceError(format!("Extract: {}", e)))?;

        let shape_vec: Vec<usize> = shape
            .iter()
            .map(|&d| usize::try_from(d).unwrap_or(0))
            .collect();
        let (map_h, map_w) = match shape_vec.as_slice() {
            [h, w] => (*h, *w),
            [_, h, w] => (*h, *w),
            [_, _, h, w] => (*h, *w),
            _ => {
                return Err(VisionError::InferenceError(format!(
                    "Unexpected det output shape: {:?}",
                    shape_vec
                )))
            }
        };

        let prob_map = Array2::from_shape_vec((map_h, map_w), data.to_vec())
            .map_err(|e| VisionError::InferenceError(format!("Reshape: {}", e)))?;
        let mut boxes = self.postprocess_det(&prob_map, self.det_threshold, self.unclip_ratio);
        if map_w > 0 && map_h > 0 {
            let sx = orig_w as f32 / map_w as f32;
            let sy = orig_h as f32 / map_h as f32;
            for b in &mut boxes {
                for p in b.iter_mut() {
                    p.0 *= sx;
                    p.1 *= sy;
                }
            }
        }

        Ok(boxes)
    }

    /// Resize BGR Mat for detection (INTER_LINEAR, padded to 32px multiples).
    fn preprocess_det_resize(&self, bgr: &Mat) -> Result<(Mat, u32, u32), VisionError> {
        let orig_w = bgr.cols() as u32;
        let orig_h = bgr.rows() as u32;
        let ratio = if orig_w.max(orig_h) > self.max_side_len {
            self.max_side_len as f32 / orig_w.max(orig_h) as f32
        } else {
            1.0
        };
        let new_w = ((orig_w as f32 * ratio / 32.0).ceil() * 32.0) as i32;
        let new_h = ((orig_h as f32 * ratio / 32.0).ceil() * 32.0) as i32;

        let mut resized = Mat::default();
        imgproc::resize(
            bgr,
            &mut resized,
            Size::new(new_w, new_h),
            0.0,
            0.0,
            imgproc::INTER_LINEAR,
        )
        .map_err(|e| VisionError::InferenceError(format!("cv::resize: {}", e)))?;

        Ok((resized, new_w as u32, new_h as u32))
    }

    /// Convert a BGR u8 Mat to CHW f32 ndarray with (pixel/255 - mean) / std normalization.
    fn mat_to_chw_f32_normalize(bgr: &Mat, mean: &[f32; 3], std_val: &[f32; 3]) -> Array4<f32> {
        let h = bgr.rows() as usize;
        let w = bgr.cols() as usize;
        let mut input = Array4::<f32>::zeros((1, 3, h, w));

        // Get raw data — BGR interleaved u8
        let data = match bgr.data_bytes() {
            Ok(d) => d,
            Err(_) => return input,
        };
        let step = bgr.step1(0).unwrap_or(w * 3); // row stride in bytes

        for y in 0..h {
            let row_off = y * step;
            for x in 0..w {
                let px_off = row_off + x * 3;
                // BGR → RGB channel order for the CHW tensor
                let b = data[px_off] as f32 / 255.0;
                let g = data[px_off + 1] as f32 / 255.0;
                let r = data[px_off + 2] as f32 / 255.0;
                input[[0, 0, y, x]] = (r - mean[0]) / std_val[0]; // R
                input[[0, 1, y, x]] = (g - mean[1]) / std_val[1]; // G
                input[[0, 2, y, x]] = (b - mean[2]) / std_val[2]; // B
            }
        }
        input
    }

    fn recognize_regions_batch(
        &self,
        bgr: &Mat,
        boxes: &[[(f32, f32); 4]],
    ) -> Result<Vec<OcrResult>, VisionError> {
        let (input, metas) = self.preprocess_rec_batch(bgr, boxes)?;
        if metas.is_empty() {
            return Ok(Vec::new());
        }
        let req_batch = metas.len();
        let tensor = ort::value::Tensor::from_array(input)
            .map_err(|e| VisionError::InferenceError(format!("Rec tensor: {}", e)))?;

        let mut session = self
            .rec_session
            .lock()
            .map_err(|e| VisionError::InferenceError(format!("Lock: {}", e)))?;

        let outputs = session
            .run(ort::inputs![tensor])
            .map_err(|e| VisionError::InferenceError(format!("Rec inference: {}", e)))?;

        let (shape, data) = outputs[0]
            .try_extract_tensor::<f32>()
            .map_err(|e| VisionError::InferenceError(format!("Extract: {}", e)))?;

        let shape_vec: Vec<usize> = shape
            .iter()
            .map(|&d| usize::try_from(d).unwrap_or(0))
            .collect();
        let (batch, time_steps, classes) = Self::parse_rec_shape(&shape_vec, req_batch)?;
        if time_steps == 0 || classes == 0 {
            return Ok(Vec::new());
        }
        let expected = batch.saturating_mul(time_steps).saturating_mul(classes);
        if data.len() < expected {
            return Err(VisionError::InferenceError(format!(
                "Rec output too small: got {}, need >= {}",
                data.len(),
                expected
            )));
        }

        let out_batch = batch.min(metas.len());
        let mut results = Vec::with_capacity(out_batch);
        for (i, meta) in metas.iter().enumerate().take(out_batch) {
            let start = i * time_steps * classes;
            let end = start + time_steps * classes;
            let (text, confidence) = self.ctc_decode_single(time_steps, classes, &data[start..end]);
            results.push(OcrResult {
                text,
                confidence,
                bbox: meta.bbox,
            });
        }
        Ok(results)
    }

    fn parse_rec_shape(
        shape: &[usize],
        requested_batch: usize,
    ) -> Result<(usize, usize, usize), VisionError> {
        let (batch, t, c) = match shape {
            [t, c] => (1, *t, *c),
            [1, t, c] => (1, *t, *c),
            [b, t, c] => (*b, *t, *c),
            [1, 1, t, c] => (1, *t, *c),
            [b, 1, t, c] => (*b, *t, *c),
            _ => {
                return Err(VisionError::InferenceError(format!(
                    "Unexpected rec output shape: {:?}",
                    shape
                )))
            }
        };
        if batch == 0 || t == 0 || c == 0 {
            return Err(VisionError::InferenceError(format!(
                "Invalid rec output shape values: {:?}",
                shape
            )));
        }
        if batch < requested_batch {
            tracing::warn!(
                output_batch = batch,
                requested_batch,
                "Rec output batch smaller than requested; truncating results"
            );
        }
        Ok((batch, t, c))
    }

    fn recognize_region(
        &self,
        bgr: &Mat,
        bbox: &[(f32, f32); 4],
    ) -> Result<OcrResult, VisionError> {
        let Some(rect) = Self::clamped_mat_rect(bgr, bbox) else {
            return Ok(OcrResult {
                text: String::new(),
                confidence: 0.0,
                bbox: *bbox,
            });
        };

        // Zero-copy ROI crop (try_clone is O(1), only copies the Mat header)
        let roi = Mat::roi(bgr, rect)
            .map_err(|e| VisionError::InferenceError(format!("Mat roi: {}", e)))?
            .try_clone()
            .map_err(|e| VisionError::InferenceError(format!("Mat roi clone: {}", e)))?;

        let input = self.preprocess_rec(&roi)?;
        let tensor = ort::value::Tensor::from_array(input)
            .map_err(|e| VisionError::InferenceError(format!("Rec tensor: {}", e)))?;

        let mut session = self
            .rec_session
            .lock()
            .map_err(|e| VisionError::InferenceError(format!("Lock: {}", e)))?;

        let outputs = session
            .run(ort::inputs![tensor])
            .map_err(|e| VisionError::InferenceError(format!("Rec inference: {}", e)))?;

        let (shape, data) = outputs[0]
            .try_extract_tensor::<f32>()
            .map_err(|e| VisionError::InferenceError(format!("Extract: {}", e)))?;

        let shape_vec: Vec<usize> = shape
            .iter()
            .map(|&d| usize::try_from(d).unwrap_or(0))
            .collect();
        let (text, confidence) = self.ctc_decode(&shape_vec, data);

        Ok(OcrResult {
            text,
            confidence,
            bbox: *bbox,
        })
    }

    /// Preprocess a BGR Mat ROI for recognition: resize to 320×48, normalize.
    fn preprocess_rec(&self, roi: &Mat) -> Result<Array4<f32>, VisionError> {
        let mut resized = Mat::default();
        imgproc::resize(
            roi,
            &mut resized,
            Size::new(320, 48),
            0.0,
            0.0,
            imgproc::INTER_LINEAR,
        )
        .map_err(|e| VisionError::InferenceError(format!("cv::resize rec: {}", e)))?;

        Ok(Self::mat_to_chw_f32_normalize(
            &resized,
            &[0.5f32, 0.5, 0.5],
            &[0.5f32, 0.5, 0.5],
        ))
    }

    fn preprocess_rec_batch(
        &self,
        bgr: &Mat,
        boxes: &[[(f32, f32); 4]],
    ) -> Result<(Array4<f32>, Vec<RecCropMeta>), VisionError> {
        let mut samples = Vec::new();
        let mut metas = Vec::new();

        for bbox in boxes {
            let Some(rect) = Self::clamped_mat_rect(bgr, bbox) else {
                continue;
            };
            let roi = Mat::roi(bgr, rect)
                .map_err(|e| VisionError::InferenceError(format!("Mat roi: {}", e)))?;
            let roi_mat: Mat = roi.try_clone()
                .map_err(|e| VisionError::InferenceError(format!("Mat roi clone: {}", e)))?;
            let sample = self.preprocess_rec(&roi_mat)?;
            samples.push(sample);
            metas.push(RecCropMeta { bbox: *bbox });
        }

        if samples.is_empty() {
            return Ok((Array4::<f32>::zeros((0, 3, 48, 320)), metas));
        }

        let n = samples.len();
        let mut batch = Array4::<f32>::zeros((n, 3, 48, 320));
        for (i, sample) in samples.iter().enumerate() {
            let view = sample.index_axis(Axis(0), 0);
            batch.slice_mut(s![i, .., .., ..]).assign(&view);
        }
        Ok((batch, metas))
    }

    /// Compute clamped Rect for bbox within a Mat.
    fn clamped_mat_rect(mat: &Mat, bbox: &[(f32, f32); 4]) -> Option<Rect> {
        let min_x = bbox.iter().map(|p| p.0).fold(f32::INFINITY, f32::min);
        let min_y = bbox.iter().map(|p| p.1).fold(f32::INFINITY, f32::min);
        let max_x = bbox.iter().map(|p| p.0).fold(f32::NEG_INFINITY, f32::max);
        let max_y = bbox.iter().map(|p| p.1).fold(f32::NEG_INFINITY, f32::max);
        if !min_x.is_finite() || !min_y.is_finite() || !max_x.is_finite() || !max_y.is_finite() {
            return None;
        }

        let mw = mat.cols() as f32;
        let mh = mat.rows() as f32;
        let x0 = min_x.floor().clamp(0.0, mw) as i32;
        let y0 = min_y.floor().clamp(0.0, mh) as i32;
        let x1 = max_x.ceil().clamp(0.0, mw) as i32;
        let y1 = max_y.ceil().clamp(0.0, mh) as i32;
        if x1 <= x0 || y1 <= y0 {
            return None;
        }
        Some(Rect::new(x0, y0, x1 - x0, y1 - y0))
    }

    fn postprocess_det(
        &self,
        prob_map: &Array2<f32>,
        threshold: f32,
        unclip_ratio: f32,
    ) -> Vec<[(f32, f32); 4]> {
        let (h, w) = prob_map.dim();
        if h == 0 || w == 0 {
            return Vec::new();
        }
        let mut boxes = Vec::new();
        let mut visited = vec![vec![false; w]; h];

        for y in 0..h {
            for x in 0..w {
                if prob_map[[y, x]] <= threshold || visited[y][x] {
                    continue;
                }

                let mut min_x = x;
                let mut max_x = x;
                let mut min_y = y;
                let mut max_y = y;
                let mut pixels = 0usize;

                let mut queue = vec![(x, y)];
                visited[y][x] = true;

                while let Some((cx, cy)) = queue.pop() {
                    pixels += 1;
                    min_x = min_x.min(cx);
                    max_x = max_x.max(cx);
                    min_y = min_y.min(cy);
                    max_y = max_y.max(cy);

                    for (dx, dy) in &[(0, 1), (0, -1), (1, 0), (-1, 0)] {
                        let nx = cx as i32 + dx;
                        let ny = cy as i32 + dy;
                        if nx >= 0 && nx < w as i32 && ny >= 0 && ny < h as i32 {
                            let (nx, ny) = (nx as usize, ny as usize);
                            if !visited[ny][nx] && prob_map[[ny, nx]] > threshold {
                                visited[ny][nx] = true;
                                queue.push((nx, ny));
                            }
                        }
                    }
                }

                if pixels < 3 {
                    continue;
                }
                let rect = [
                    (min_x as f32, min_y as f32),
                    (max_x as f32, min_y as f32),
                    (max_x as f32, max_y as f32),
                    (min_x as f32, max_y as f32),
                ];
                boxes.push(Self::unclip_quad(
                    rect,
                    unclip_ratio,
                    (w - 1) as f32,
                    (h - 1) as f32,
                ));
            }
        }

        boxes
    }

    fn unclip_quad(quad: [(f32, f32); 4], ratio: f32, max_x: f32, max_y: f32) -> [(f32, f32); 4] {
        let mut x1 = quad.iter().map(|p| p.0).fold(f32::INFINITY, f32::min);
        let mut y1 = quad.iter().map(|p| p.1).fold(f32::INFINITY, f32::min);
        let mut x2 = quad.iter().map(|p| p.0).fold(f32::NEG_INFINITY, f32::max);
        let mut y2 = quad.iter().map(|p| p.1).fold(f32::NEG_INFINITY, f32::max);

        let width = (x2 - x1).max(1.0);
        let height = (y2 - y1).max(1.0);
        let area = width * height;
        let perimeter = 2.0 * (width + height);
        let distance = if perimeter > 0.0 {
            (area * ratio / perimeter).max(1.0)
        } else {
            1.0
        };

        x1 = (x1 - distance).clamp(0.0, max_x);
        y1 = (y1 - distance).clamp(0.0, max_y);
        x2 = (x2 + distance).clamp(0.0, max_x);
        y2 = (y2 + distance).clamp(0.0, max_y);

        [(x1, y1), (x2, y1), (x2, y2), (x1, y2)]
    }

    fn ctc_decode(&self, shape: &[usize], output: &[f32]) -> (String, f32) {
        let Ok((_, time_steps, classes)) = Self::parse_rec_shape(shape, 1) else {
            return (String::new(), 0.0);
        };
        if time_steps == 0 || classes == 0 || output.len() < time_steps * classes {
            return (String::new(), 0.0);
        }
        self.ctc_decode_single(time_steps, classes, &output[..time_steps * classes])
    }

    fn ctc_decode_single(
        &self,
        time_steps: usize,
        classes: usize,
        output: &[f32],
    ) -> (String, f32) {
        let mut text = String::new();
        let mut prev_idx = usize::MAX;
        let mut conf_sum = 0.0f32;
        let mut conf_count = 0usize;

        for t in 0..time_steps {
            let row = &output[t * classes..(t + 1) * classes];
            let mut max_idx = 0usize;
            let mut max_prob = f32::MIN;
            for (idx, &p) in row.iter().enumerate() {
                if p > max_prob {
                    max_prob = p;
                    max_idx = idx;
                }
            }

            if max_idx == 0 {
                prev_idx = max_idx;
                continue;
            }
            if max_idx == prev_idx {
                continue;
            }

            let dict_idx = max_idx - 1;
            if let Some(token) = self.dict.get(dict_idx) {
                text.push_str(token);
                conf_sum += max_prob;
                conf_count += 1;
            }
            prev_idx = max_idx;
        }

        let confidence = if conf_count > 0 {
            conf_sum / conf_count as f32
        } else {
            0.0
        };
        (text, confidence)
    }
}

fn default_dict() -> Vec<String> {
    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ocr_result_creation() {
        let result = OcrResult {
            text: "hello".to_string(),
            confidence: 0.95,
            bbox: [(0.0, 0.0), (100.0, 0.0), (100.0, 30.0), (0.0, 30.0)],
        };
        assert_eq!(result.text, "hello");
    }

    #[test]
    fn test_parse_rec_shape_variants() {
        let s = PaddleOcrEngine::parse_rec_shape(&[2, 25, 100], 2).unwrap();
        assert_eq!(s, (2, 25, 100));
        let s = PaddleOcrEngine::parse_rec_shape(&[2, 1, 25, 100], 2).unwrap();
        assert_eq!(s, (2, 25, 100));
        let s = PaddleOcrEngine::parse_rec_shape(&[1, 25, 100], 1).unwrap();
        assert_eq!(s, (1, 25, 100));
    }

    #[test]
    fn test_unclip_quad_expands_bbox() {
        let quad = [(10.0, 10.0), (20.0, 10.0), (20.0, 20.0), (10.0, 20.0)];
        let expanded = PaddleOcrEngine::unclip_quad(quad, 2.0, 100.0, 100.0);
        assert!(expanded[0].0 < 10.0);
        assert!(expanded[0].1 < 10.0);
        assert!(expanded[2].0 > 20.0);
        assert!(expanded[2].1 > 20.0);
    }

    #[test]
    fn test_ensure_bgr_passthrough() {
        // A 3-channel BGR Mat should pass through unchanged
        let bgr = Mat::new_rows_cols_with_default(2, 3, opencv::core::CV_8UC3, opencv::core::Scalar::all(128.0)).unwrap();
        let result = PaddleOcrEngine::ensure_bgr(&bgr).unwrap();
        assert_eq!(result.channels(), 3);
    }

    #[test]
    fn test_ensure_bgr_from_bgra() {
        let bgra = Mat::new_rows_cols_with_default(2, 3, opencv::core::CV_8UC4, opencv::core::Scalar::all(128.0)).unwrap();
        let result = PaddleOcrEngine::ensure_bgr(&bgra).unwrap();
        assert_eq!(result.channels(), 3);
        assert_eq!(result.rows(), 2);
        assert_eq!(result.cols(), 3);
    }

    #[test]
    fn test_clamped_mat_rect_basic() {
        let mat = Mat::new_rows_cols_with_default(100, 200, opencv::core::CV_8UC3, opencv::core::Scalar::default()).unwrap();
        let bbox = [(10.0, 20.0), (100.0, 20.0), (100.0, 80.0), (10.0, 80.0)];
        let rect = PaddleOcrEngine::clamped_mat_rect(&mat, &bbox).unwrap();
        assert_eq!(rect.x, 10);
        assert_eq!(rect.y, 20);
        assert_eq!(rect.width, 90);
        assert_eq!(rect.height, 60);
    }
}
