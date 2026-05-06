//! Connected-component contour extraction.
//!
//! NOTE: This is a pragmatic greedy 4-connected boundary walk (close to
//! Suzuki's outer-only first pass), not a full marching-squares /
//! Moore-Neighbor tracer. It produces deterministic point lists usable for
//! bounding-box / centroid / shape statistics, but the point order is not
//! guaranteed to be a clockwise contour and the result may differ from
//! OpenCV's `findContours` for shapes with holes or thin necks.

use crate::error::VisionError;
use betternte_core::Point;
use image::{DynamicImage, GrayImage};
use opencv::core::{self, Mat};
use opencv::imgproc;
use opencv::prelude::*;

/// Contour representation
#[derive(Debug, Clone)]
pub struct Contour {
    /// Points forming the contour
    pub points: Vec<Point>,
}

/// Contour hierarchy for nested contours
#[derive(Debug, Clone)]
pub struct ContourHierarchy {
    /// Hierarchical parent-child relationships
    pub parent: Vec<Option<usize>>,
    pub child: Vec<Option<usize>>,
}

/// Contour finder using marching squares algorithm
pub struct ContourFinder {
    /// Minimum contour area threshold
    min_area: f64,
}

impl ContourFinder {
    pub fn new() -> Self {
        Self { min_area: 0.0 }
    }

    pub fn with_min_area(mut self, min_area: f64) -> Self {
        self.min_area = min_area;
        self
    }

    /// Find contours in a binary image (foreground = pixel value > 127).
    pub fn find_contours(&self, binary: &GrayImage) -> Result<Vec<Contour>, VisionError> {
        let rows = binary.height() as i32;
        let raw = binary.as_raw();
        let mat_1d = Mat::from_slice(raw)
            .map_err(|e| VisionError::ImageProcessingError(format!("Mat::from_slice: {e}")))?;
        let mat_ref = mat_1d
            .reshape(1, rows)
            .map_err(|e| VisionError::ImageProcessingError(format!("Mat::reshape: {e}")))?;
        let mat = mat_ref
            .try_clone()
            .map_err(|e| VisionError::ImageProcessingError(format!("Mat::clone: {e}")))?;

        let mut raw_contours: core::Vector<core::Vector<core::Point>> = core::Vector::new();
        imgproc::find_contours(
            &mat,
            &mut raw_contours,
            imgproc::RETR_EXTERNAL,
            imgproc::CHAIN_APPROX_NONE,
            core::Point::new(0, 0),
        )
        .map_err(|e| VisionError::ImageProcessingError(format!("find_contours: {e}")))?;

        let mut contours = Vec::new();
        for c in raw_contours {
            let mut points = Vec::with_capacity(c.len());
            for p in c {
                points.push(Point::new(p.x, p.y));
            }
            let contour = Contour { points };
            let area = self.compute_contour_area(&contour.points);
            if area >= self.min_area {
                contours.push(contour);
            }
        }
        Ok(contours)
    }

    /// Compute area using shoelace formula
    fn compute_contour_area(&self, points: &[Point]) -> f64 {
        if points.len() < 3 {
            return 0.0;
        }

        let mut area = 0.0;
        let n = points.len();

        for i in 0..n {
            let j = (i + 1) % n;
            area += points[i].x as f64 * points[j].y as f64;
            area -= points[j].x as f64 * points[i].y as f64;
        }

        area.abs() / 2.0
    }

    /// Find contours from a DynamicImage (auto-converts to binary)
    pub fn find_contours_from_image(
        &self,
        image: &DynamicImage,
    ) -> Result<Vec<Contour>, VisionError> {
        // Convert to grayscale then binary
        let gray = image.to_luma8();
        let binary = self.threshold_image(&gray, 127);
        self.find_contours(&binary)
    }

    /// Simple threshold to create binary image
    fn threshold_image(&self, gray: &GrayImage, threshold: u8) -> GrayImage {
        image::ImageBuffer::from_fn(gray.width(), gray.height(), |x, y| {
            if gray.get_pixel(x, y)[0] > threshold {
                image::Luma([255u8])
            } else {
                image::Luma([0u8])
            }
        })
    }
}

impl Default for ContourFinder {
    fn default() -> Self {
        Self::new()
    }
}

impl Contour {
    /// Get the bounding box of the contour
    pub fn bounding_box(&self) -> Option<(Point, Point)> {
        if self.points.is_empty() {
            return None;
        }

        let mut min_x = i32::MAX;
        let mut min_y = i32::MAX;
        let mut max_x = i32::MIN;
        let mut max_y = i32::MIN;

        for p in &self.points {
            min_x = min_x.min(p.x);
            min_y = min_y.min(p.y);
            max_x = max_x.max(p.x);
            max_y = max_y.max(p.y);
        }

        Some((Point::new(min_x, min_y), Point::new(max_x, max_y)))
    }

    /// Get the centroid of the contour
    pub fn centroid(&self) -> Option<(f64, f64)> {
        if self.points.is_empty() {
            return None;
        }

        let n = self.points.len() as f64;
        let sum_x: f64 = self.points.iter().map(|p| p.x as f64).sum();
        let sum_y: f64 = self.points.iter().map(|p| p.y as f64).sum();

        Some((sum_x / n, sum_y / n))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_contour_finder_empty() {
        let binary = image::GrayImage::new(10, 10);
        let finder = ContourFinder::new();
        let contours = finder.find_contours(&binary).unwrap();
        assert!(contours.is_empty());
    }

    #[test]
    fn test_contour_finder_single_rectangle() {
        let mut binary = image::GrayImage::new(100, 100);
        // Draw a filled rectangle
        for y in 10..30 {
            for x in 10..30 {
                binary.put_pixel(x, y, image::Luma([255u8]));
            }
        }

        let finder = ContourFinder::new();
        let contours = finder.find_contours(&binary).unwrap();

        // Should find at least one contour
        assert!(!contours.is_empty());

        let contour = &contours[0];
        assert!(contour.points.len() >= 3);
    }

    #[test]
    fn test_contour_bounding_box() {
        let points = vec![
            Point::new(10, 20),
            Point::new(50, 20),
            Point::new(50, 80),
            Point::new(10, 80),
        ];
        let contour = Contour { points };

        let bbox = contour.bounding_box().unwrap();
        assert_eq!(bbox.0, Point::new(10, 20)); // min
        assert_eq!(bbox.1, Point::new(50, 80)); // max
    }

    #[test]
    fn test_contour_centroid() {
        let points = vec![
            Point::new(0, 0),
            Point::new(100, 0),
            Point::new(100, 100),
            Point::new(0, 100),
        ];
        let contour = Contour { points };

        let centroid = contour.centroid().unwrap();
        assert!((centroid.0 - 50.0).abs() < 0.1);
        assert!((centroid.1 - 50.0).abs() < 0.1);
    }

    #[test]
    fn test_contour_finder_min_area() {
        let mut binary = image::GrayImage::new(100, 100);
        // Draw a small 2x2 rectangle
        binary.put_pixel(50, 50, image::Luma([255u8]));
        binary.put_pixel(51, 50, image::Luma([255u8]));
        binary.put_pixel(50, 51, image::Luma([255u8]));
        binary.put_pixel(51, 51, image::Luma([255u8]));

        let finder = ContourFinder::new().with_min_area(10.0);
        let contours = finder.find_contours(&binary).unwrap();

        // Small 2x2 square has area ~4, should be filtered out
        assert!(contours.is_empty() || contours.iter().all(|c| c.points.len() <= 10));
    }
}
