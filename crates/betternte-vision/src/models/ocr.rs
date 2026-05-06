//! PaddleOCR text recognition (DBNet detection + CRNN recognition).

use crate::error::VisionError;
use ndarray::{s, Array2, Array4, Axis};
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

    pub fn recognize(&self, image: &image::DynamicImage) -> Result<Vec<OcrResult>, VisionError> {
        let boxes = self.detect_text_regions(image)?;
        if boxes.is_empty() {
            return Ok(Vec::new());
        }

        let mut results = Vec::new();
        for chunk in boxes.chunks(self.rec_batch_size) {
            match self.recognize_regions_batch(image, chunk) {
                Ok(mut part) => results.append(&mut part),
                Err(err) => {
                    tracing::warn!(
                        error = %err,
                        regions = chunk.len(),
                        "OCR rec batch failed, falling back to per-region inference"
                    );
                    let mut part = self.recognize_regions_sequential(image, chunk)?;
                    results.append(&mut part);
                }
            }
        }
        Ok(results.into_iter().filter(|r| !r.text.is_empty()).collect())
    }

    fn recognize_regions_sequential(
        &self,
        image: &image::DynamicImage,
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
        image: &image::DynamicImage,
    ) -> Result<Vec<[(f32, f32); 4]>, VisionError> {
        let input = self.preprocess_det(image, self.max_side_len);
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
            let sx = image.width() as f32 / map_w as f32;
            let sy = image.height() as f32 / map_h as f32;
            for b in &mut boxes {
                for p in b.iter_mut() {
                    p.0 *= sx;
                    p.1 *= sy;
                }
            }
        }

        Ok(boxes)
    }

    fn recognize_regions_batch(
        &self,
        image: &image::DynamicImage,
        boxes: &[[(f32, f32); 4]],
    ) -> Result<Vec<OcrResult>, VisionError> {
        let (input, metas) = self.preprocess_rec_batch(image, boxes);
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
        image: &image::DynamicImage,
        bbox: &[(f32, f32); 4],
    ) -> Result<OcrResult, VisionError> {
        let Some((x_min, y_min, w, h)) = Self::clamped_rect(image, bbox) else {
            return Ok(OcrResult {
                text: String::new(),
                confidence: 0.0,
                bbox: *bbox,
            });
        };

        let cropped = image.crop_imm(x_min, y_min, w, h);
        let input = self.preprocess_rec(&cropped);
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

    fn preprocess_det(&self, image: &image::DynamicImage, limit_side_len: u32) -> Array4<f32> {
        let (orig_w, orig_h) = (image.width(), image.height());
        let ratio = if orig_w.max(orig_h) > limit_side_len {
            limit_side_len as f32 / orig_w.max(orig_h) as f32
        } else {
            1.0
        };
        let new_w = ((orig_w as f32 * ratio / 32.0).ceil() * 32.0) as u32;
        let new_h = ((orig_h as f32 * ratio / 32.0).ceil() * 32.0) as u32;

        let resized = image.resize_exact(new_w, new_h, image::imageops::FilterType::Lanczos3);
        let rgb = resized.to_rgb8();
        let (w, h) = rgb.dimensions();

        let mean = [0.485f32, 0.456, 0.406];
        let std_val = [0.229f32, 0.224, 0.225];
        let mut input = Array4::<f32>::zeros((1, 3, h as usize, w as usize));
        for y in 0..h {
            for x in 0..w {
                let pixel = rgb.get_pixel(x, y);
                for c in 0..3 {
                    input[[0, c, y as usize, x as usize]] =
                        (pixel[c] as f32 / 255.0 - mean[c]) / std_val[c];
                }
            }
        }
        input
    }

    fn preprocess_rec(&self, image: &image::DynamicImage) -> Array4<f32> {
        let resized = image.resize_exact(320, 48, image::imageops::FilterType::Lanczos3);
        let rgb = resized.to_rgb8();
        let (w, h) = rgb.dimensions();

        let mean = [0.5f32, 0.5, 0.5];
        let std_val = [0.5f32, 0.5, 0.5];
        let mut input = Array4::<f32>::zeros((1, 3, h as usize, w as usize));
        for y in 0..h {
            for x in 0..w {
                let pixel = rgb.get_pixel(x, y);
                for c in 0..3 {
                    input[[0, c, y as usize, x as usize]] =
                        (pixel[c] as f32 / 255.0 - mean[c]) / std_val[c];
                }
            }
        }
        input
    }

    fn preprocess_rec_batch(
        &self,
        image: &image::DynamicImage,
        boxes: &[[(f32, f32); 4]],
    ) -> (Array4<f32>, Vec<RecCropMeta>) {
        let mut samples = Vec::new();
        let mut metas = Vec::new();

        for bbox in boxes {
            let Some((x, y, w, h)) = Self::clamped_rect(image, bbox) else {
                continue;
            };
            let cropped = image.crop_imm(x, y, w, h);
            let sample = self.preprocess_rec(&cropped);
            samples.push(sample);
            metas.push(RecCropMeta { bbox: *bbox });
        }

        if samples.is_empty() {
            return (Array4::<f32>::zeros((0, 3, 48, 320)), metas);
        }

        let n = samples.len();
        let mut batch = Array4::<f32>::zeros((n, 3, 48, 320));
        for (i, sample) in samples.iter().enumerate() {
            let view = sample.index_axis(Axis(0), 0);
            batch.slice_mut(s![i, .., .., ..]).assign(&view);
        }
        (batch, metas)
    }

    fn clamped_rect(
        image: &image::DynamicImage,
        bbox: &[(f32, f32); 4],
    ) -> Option<(u32, u32, u32, u32)> {
        let min_x = bbox.iter().map(|p| p.0).fold(f32::INFINITY, f32::min);
        let min_y = bbox.iter().map(|p| p.1).fold(f32::INFINITY, f32::min);
        let max_x = bbox.iter().map(|p| p.0).fold(f32::NEG_INFINITY, f32::max);
        let max_y = bbox.iter().map(|p| p.1).fold(f32::NEG_INFINITY, f32::max);
        if !min_x.is_finite() || !min_y.is_finite() || !max_x.is_finite() || !max_y.is_finite() {
            return None;
        }

        let iw = image.width() as f32;
        let ih = image.height() as f32;
        let x0 = min_x.floor().clamp(0.0, iw) as u32;
        let y0 = min_y.floor().clamp(0.0, ih) as u32;
        let x1 = max_x.ceil().clamp(0.0, iw) as u32;
        let y1 = max_y.ceil().clamp(0.0, ih) as u32;
        if x1 <= x0 || y1 <= y0 {
            return None;
        }
        Some((x0, y0, x1 - x0, y1 - y0))
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
}
