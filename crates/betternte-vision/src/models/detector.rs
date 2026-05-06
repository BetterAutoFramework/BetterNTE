//! YOLO object detector.
//!
//! Supports YOLOv8/v11-style outputs `[1, 4 + num_classes, num_boxes]`
//! (the layout where bbox + class scores share the channel axis).
//!
//! Input: `(input_size, input_size)` RGB letterboxed image (default 640).
//! Output: filtered detections after class-aware NMS.

use super::{DetectResult, Detector};
use crate::error::VisionError;
use ndarray::Array4;
use ort::session::Session;
use std::path::Path;
use std::sync::Mutex;

/// YOLO detector.
pub struct YoloDetector {
    session: Mutex<Session>,
    labels: Vec<String>,
    input_size: u32,
    conf_threshold: f32,
    iou_threshold: f32,
}

impl YoloDetector {
    /// Load with default COCO 80 labels.
    pub fn load(model_path: &Path) -> Result<Self, VisionError> {
        Self::load_with_labels(model_path, coco_labels())
    }

    /// Load with custom labels file (one class name per line).
    pub fn load_with_labels_file(
        model_path: &Path,
        labels_path: &Path,
    ) -> Result<Self, VisionError> {
        let content = std::fs::read_to_string(labels_path)
            .map_err(|e| VisionError::ConfigError(format!("Load labels: {}", e)))?;
        let labels: Vec<String> = content.lines().map(String::from).collect();
        Self::load_with_labels(model_path, labels)
    }

    /// Load with explicit label list.
    pub fn load_with_labels(model_path: &Path, labels: Vec<String>) -> Result<Self, VisionError> {
        let session = super::session::SessionBuilder::new()
            .with_directml(true)
            .with_cuda(true)
            .build_from_file(model_path)?;

        Ok(Self {
            session: Mutex::new(session),
            labels,
            input_size: 640,
            conf_threshold: 0.25,
            iou_threshold: 0.45,
        })
    }

    pub fn with_input_size(mut self, size: u32) -> Self {
        self.input_size = size;
        self
    }

    pub fn with_confidence(mut self, thresh: f32) -> Self {
        self.conf_threshold = thresh;
        self
    }

    pub fn with_iou(mut self, thresh: f32) -> Self {
        self.iou_threshold = thresh;
        self
    }

    /// Letterbox preprocess: resize keeping aspect ratio and pad to a square.
    ///
    /// Returns the input tensor and the geometric parameters needed to undo
    /// the letterbox when mapping detections back to the original image.
    fn preprocess(&self, image: &image::DynamicImage) -> (Array4<f32>, LetterboxParams) {
        let s = self.input_size;
        let (orig_w, orig_h) = (image.width(), image.height());
        let scale = (s as f32 / orig_w as f32).min(s as f32 / orig_h as f32);
        let new_w = ((orig_w as f32) * scale).round().max(1.0) as u32;
        let new_h = ((orig_h as f32) * scale).round().max(1.0) as u32;
        let pad_x = (s.saturating_sub(new_w)) / 2;
        let pad_y = (s.saturating_sub(new_h)) / 2;

        let resized = image.resize_exact(new_w, new_h, image::imageops::FilterType::Triangle);
        let rgb = resized.to_rgb8();

        let mut input = Array4::<f32>::zeros((1, 3, s as usize, s as usize));
        // Fill grey background (114/255) per Ultralytics convention.
        let bg = 114.0 / 255.0;
        for c in 0..3 {
            input.slice_mut(ndarray::s![0, c, .., ..]).fill(bg);
        }
        let pixels = rgb.as_raw();
        for y in 0..new_h {
            let row = (y as usize) * (new_w as usize) * 3;
            for x in 0..new_w {
                let off = row + (x as usize) * 3;
                let dst_y = (pad_y + y) as usize;
                let dst_x = (pad_x + x) as usize;
                input[[0, 0, dst_y, dst_x]] = pixels[off] as f32 / 255.0;
                input[[0, 1, dst_y, dst_x]] = pixels[off + 1] as f32 / 255.0;
                input[[0, 2, dst_y, dst_x]] = pixels[off + 2] as f32 / 255.0;
            }
        }
        (
            input,
            LetterboxParams {
                scale,
                pad_x: pad_x as f32,
                pad_y: pad_y as f32,
                orig_w,
                orig_h,
            },
        )
    }
}

/// Parameters needed to undo the letterbox transform on raw detection coords.
#[derive(Debug, Clone, Copy)]
struct LetterboxParams {
    scale: f32,
    pad_x: f32,
    pad_y: f32,
    orig_w: u32,
    orig_h: u32,
}

fn yolo_postprocess(
    output: &[f32],
    shape: &[i64],
    labels: &[String],
    conf_threshold: f32,
    iou_threshold: f32,
    lb: LetterboxParams,
) -> Vec<DetectResult> {
    // YOLO output shape: [1, 4 + num_classes, num_boxes]
    let (num_classes, num_boxes) = match shape {
        [_, c, n] => ((*c - 4).max(0) as usize, *n as usize),
        _ => return Vec::new(),
    };
    if num_classes == 0 || num_boxes == 0 {
        return Vec::new();
    }

    let inv_scale = if lb.scale > 0.0 { 1.0 / lb.scale } else { 1.0 };
    let max_x = lb.orig_w.saturating_sub(1) as f32;
    let max_y = lb.orig_h.saturating_sub(1) as f32;

    let mut results = Vec::new();

    for i in 0..num_boxes {
        let mut max_conf = 0.0f32;
        let mut max_class = 0usize;
        for c in 0..num_classes {
            let conf = output[(c + 4) * num_boxes + i];
            if conf > max_conf {
                max_conf = conf;
                max_class = c;
            }
        }

        if max_conf < conf_threshold {
            continue;
        }

        // Network space (post letterbox).
        let cx = output[i];
        let cy = output[num_boxes + i];
        let w = output[2 * num_boxes + i];
        let h = output[3 * num_boxes + i];

        // Undo letterbox: subtract padding then divide by scale.
        let mut x1 = ((cx - w * 0.5) - lb.pad_x) * inv_scale;
        let mut y1 = ((cy - h * 0.5) - lb.pad_y) * inv_scale;
        let mut x2 = ((cx + w * 0.5) - lb.pad_x) * inv_scale;
        let mut y2 = ((cy + h * 0.5) - lb.pad_y) * inv_scale;

        // Clip to original image.
        x1 = x1.clamp(0.0, max_x);
        y1 = y1.clamp(0.0, max_y);
        x2 = x2.clamp(0.0, max_x);
        y2 = y2.clamp(0.0, max_y);

        if x2 <= x1 || y2 <= y1 {
            continue;
        }

        results.push(DetectResult {
            class_id: max_class as u32,
            label: labels
                .get(max_class)
                .cloned()
                .unwrap_or_else(|| format!("class_{}", max_class)),
            confidence: max_conf,
            bbox: [x1, y1, x2, y2],
        });
    }

    nms(&mut results, iou_threshold);
    results
}

/// Class-aware NMS in O(n²) using a `keep` mask (no quadratic Vec::remove).
fn nms(results: &mut Vec<DetectResult>, iou_threshold: f32) {
    if results.len() <= 1 {
        return;
    }
    results.sort_by(|a, b| {
        b.confidence
            .partial_cmp(&a.confidence)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let mut suppressed = vec![false; results.len()];
    for i in 0..results.len() {
        if suppressed[i] {
            continue;
        }
        for j in (i + 1)..results.len() {
            if suppressed[j] {
                continue;
            }
            if results[i].class_id == results[j].class_id
                && iou(&results[i].bbox, &results[j].bbox) > iou_threshold
            {
                suppressed[j] = true;
            }
        }
    }

    let mut kept = Vec::with_capacity(results.len());
    for (idx, det) in results.drain(..).enumerate() {
        if !suppressed[idx] {
            kept.push(det);
        }
    }
    *results = kept;
}

fn iou(a: &[f32; 4], b: &[f32; 4]) -> f32 {
    let x1 = a[0].max(b[0]);
    let y1 = a[1].max(b[1]);
    let x2 = a[2].min(b[2]);
    let y2 = a[3].min(b[3]);

    let inter = (x2 - x1).max(0.0) * (y2 - y1).max(0.0);
    let area_a = (a[2] - a[0]) * (a[3] - a[1]);
    let area_b = (b[2] - b[0]) * (b[3] - b[1]);

    inter / (area_a + area_b - inter + 1e-6)
}

impl Detector for YoloDetector {
    fn detect(
        &self,
        image: &image::DynamicImage,
        confidence_threshold: f32,
    ) -> Result<Vec<DetectResult>, VisionError> {
        let conf = if confidence_threshold.is_finite() && confidence_threshold > 0.0 {
            confidence_threshold
        } else {
            self.conf_threshold
        };

        let (input, lb) = self.preprocess(image);
        let tensor = ort::value::Tensor::from_array(input)
            .map_err(|e| VisionError::InferenceError(format!("Tensor: {}", e)))?;

        let mut session = self
            .session
            .lock()
            .map_err(|e| VisionError::InferenceError(format!("Lock: {}", e)))?;

        let outputs = session
            .run(ort::inputs![tensor])
            .map_err(|e| VisionError::InferenceError(format!("Inference: {}", e)))?;

        let (shape_ref, data) = outputs[0]
            .try_extract_tensor::<f32>()
            .map_err(|e| VisionError::InferenceError(format!("Extract: {}", e)))?;
        let shape: Vec<i64> = shape_ref.to_vec();
        let logits: Vec<f32> = data.to_vec();

        Ok(yolo_postprocess(
            &logits,
            &shape,
            &self.labels,
            conf,
            self.iou_threshold,
            lb,
        ))
    }
}

fn coco_labels() -> Vec<String> {
    [
        "person",
        "bicycle",
        "car",
        "motorcycle",
        "airplane",
        "bus",
        "train",
        "truck",
        "boat",
        "traffic light",
        "fire hydrant",
        "stop sign",
        "parking meter",
        "bench",
        "bird",
        "cat",
        "dog",
        "horse",
        "sheep",
        "cow",
        "elephant",
        "bear",
        "zebra",
        "giraffe",
        "backpack",
        "umbrella",
        "handbag",
        "tie",
        "suitcase",
        "frisbee",
        "skis",
        "snowboard",
        "sports ball",
        "kite",
        "baseball bat",
        "baseball glove",
        "skateboard",
        "surfboard",
        "tennis racket",
        "bottle",
        "wine glass",
        "cup",
        "fork",
        "knife",
        "spoon",
        "bowl",
        "banana",
        "apple",
        "sandwich",
        "orange",
        "broccoli",
        "carrot",
        "hot dog",
        "pizza",
        "donut",
        "cake",
        "chair",
        "couch",
        "potted plant",
        "bed",
        "dining table",
        "toilet",
        "tv",
        "laptop",
        "mouse",
        "remote",
        "keyboard",
        "cell phone",
        "microwave",
        "oven",
        "toaster",
        "sink",
        "refrigerator",
        "book",
        "clock",
        "vase",
        "scissors",
        "teddy bear",
        "hair drier",
        "toothbrush",
    ]
    .into_iter()
    .map(String::from)
    .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lb_identity(w: u32, h: u32) -> LetterboxParams {
        LetterboxParams {
            scale: 1.0,
            pad_x: 0.0,
            pad_y: 0.0,
            orig_w: w,
            orig_h: h,
        }
    }

    #[test]
    fn test_coco_labels_count() {
        let labels = coco_labels();
        assert_eq!(labels.len(), 80);
        assert_eq!(labels[0], "person");
    }

    #[test]
    fn test_iou_standalone() {
        let a = [0.0, 0.0, 10.0, 10.0];
        let b = [5.0, 5.0, 15.0, 15.0];
        assert!((iou(&a, &b) - 0.142).abs() < 0.01);

        let c = [0.0, 0.0, 10.0, 10.0];
        let d = [0.0, 0.0, 10.0, 10.0];
        assert!((iou(&c, &d) - 1.0).abs() < 0.01);
    }

    #[test]
    fn test_postprocess_auto_detect() {
        let labels = vec!["cat".to_string(), "dog".to_string()];

        // shape: [1, 6, 3] => 6 = 4(bbox) + 2(classes), 3 boxes
        let shape = vec![1, 6, 3];
        // Box 0: cx=100, cy=100, w=50, h=50, class0=0.9, class1=0.1
        // Box 1: cx=200, cy=200, w=60, h=60, class0=0.2, class1=0.8
        // Box 2: cx=300, cy=300, w=70, h=70, class0=0.3, class1=0.3 (below threshold)
        let data = vec![
            100.0, 200.0, 300.0, // cx
            100.0, 200.0, 300.0, // cy
            50.0, 60.0, 70.0, // w
            50.0, 60.0, 70.0, // h
            0.9, 0.2, 0.3, // class 0
            0.1, 0.8, 0.3, // class 1
        ];

        let results = yolo_postprocess(&data, &shape, &labels, 0.5, 0.45, lb_identity(640, 640));
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].label, "cat");
        assert_eq!(results[0].class_id, 0);
        assert_eq!(results[1].label, "dog");
        assert_eq!(results[1].class_id, 1);
    }

    #[test]
    fn test_nms_retain_keeps_higher_confidence() {
        let mut results = vec![
            DetectResult {
                class_id: 0,
                label: "a".into(),
                confidence: 0.9,
                bbox: [0.0, 0.0, 10.0, 10.0],
            },
            DetectResult {
                class_id: 0,
                label: "a".into(),
                confidence: 0.8,
                bbox: [1.0, 1.0, 11.0, 11.0], // heavy overlap, same class
            },
            DetectResult {
                class_id: 1,
                label: "b".into(),
                confidence: 0.7,
                bbox: [1.0, 1.0, 11.0, 11.0], // same box, different class
            },
        ];
        nms(&mut results, 0.45);
        assert_eq!(results.len(), 2);
        assert!((results[0].confidence - 0.9).abs() < f32::EPSILON);
    }
}
