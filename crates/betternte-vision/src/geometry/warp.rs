//! Perspective warp / image transformation
//!
//! Provides perspective transformation using homography matrices.

use crate::error::VisionError;
use crate::geometry::homography::Homography;
use betternte_core::{PointF, Region};
use image::{DynamicImage, GenericImageView, Rgba, RgbaImage};
use opencv::core::{self, Mat, Scalar, Size};
use opencv::imgproc;
use opencv::prelude::*;

/// Perspective warp operations
pub struct PerspectiveWarp;

impl PerspectiveWarp {
    fn rgba_to_mat(image: &DynamicImage) -> Result<Mat, VisionError> {
        let rgba = image.to_rgba8();
        let rows = rgba.height() as i32;
        let mat_1d = Mat::from_slice(rgba.as_raw())
            .map_err(|e| VisionError::ImageProcessingError(format!("Mat::from_slice: {e}")))?;
        let mat_ref = mat_1d
            .reshape(4, rows)
            .map_err(|e| VisionError::ImageProcessingError(format!("Mat::reshape: {e}")))?;
        mat_ref
            .try_clone()
            .map_err(|e| VisionError::ImageProcessingError(format!("Mat::clone: {e}")))
    }

    fn mat_to_rgba(mat: &Mat) -> Result<DynamicImage, VisionError> {
        let rows = mat.rows();
        let cols = mat.cols();
        if rows <= 0 || cols <= 0 {
            return Err(VisionError::ImageProcessingError(
                "Invalid Mat shape after warp".into(),
            ));
        }
        let data = mat
            .data_bytes()
            .map_err(|e| VisionError::ImageProcessingError(format!("Mat::data_bytes: {e}")))?;
        let img =
            RgbaImage::from_vec(cols as u32, rows as u32, data.to_vec()).ok_or_else(|| {
                VisionError::ImageProcessingError("Failed to convert Mat to RGBA image".into())
            })?;
        Ok(DynamicImage::ImageRgba8(img))
    }

    fn try_warp_opencv(
        &self,
        image: &DynamicImage,
        homography: &Homography,
        target_width: u32,
        target_height: u32,
    ) -> Result<DynamicImage, VisionError> {
        let src = Self::rgba_to_mat(image)?;
        let h = &homography.data;
        let h_mat = Mat::from_slice_2d(&[
            &[h[0][0], h[0][1], h[0][2]],
            &[h[1][0], h[1][1], h[1][2]],
            &[h[2][0], h[2][1], h[2][2]],
        ])
        .map_err(|e| VisionError::ImageProcessingError(format!("Mat::from_slice_2d: {e}")))?;
        let mut dst = Mat::default();
        imgproc::warp_perspective(
            &src,
            &mut dst,
            &h_mat,
            Size::new(target_width as i32, target_height as i32),
            imgproc::INTER_LINEAR,
            core::BORDER_CONSTANT,
            Scalar::all(0.0),
        )
        .map_err(|e| VisionError::ImageProcessingError(format!("warp_perspective: {e}")))?;
        Self::mat_to_rgba(&dst)
    }

    pub fn new() -> Self {
        Self
    }

    /// Warp image using homography matrix
    ///
    /// Transforms the source image using the given homography.
    /// Output size determines the region of the transformed image to keep.
    pub fn warp(
        &self,
        image: &DynamicImage,
        homography: &Homography,
        target_width: u32,
        target_height: u32,
    ) -> Result<DynamicImage, VisionError> {
        if let Ok(warped) = self.try_warp_opencv(image, homography, target_width, target_height) {
            return Ok(warped);
        }
        let _src_width = image.width();
        let _src_height = image.height();

        let mut output = RgbaImage::new(target_width, target_height);

        // Compute inverse homography for backward mapping
        let inv_h = homography
            .inverse()
            .ok_or_else(|| VisionError::ImageProcessingError("Cannot invert homography".into()))?;

        // For each output pixel, find corresponding input pixel
        for y in 0..target_height {
            for x in 0..target_width {
                let dst_point = PointF {
                    x: x as f64 + 0.5,
                    y: y as f64 + 0.5,
                };
                let src_point = inv_h.transform_point(&dst_point);

                // Bilinear interpolation
                if let Some(pixel) = self.sample_bilinear(image, src_point.x, src_point.y) {
                    output.put_pixel(x, y, pixel);
                }
            }
        }

        Ok(DynamicImage::ImageRgba8(output))
    }

    /// Warp a specific region of the source image
    pub fn warp_region(
        &self,
        image: &DynamicImage,
        homography: &Homography,
        src_region: &Region,
    ) -> Result<DynamicImage, VisionError> {
        // First crop the region
        let cropped = image.crop_imm(
            src_region.x as u32,
            src_region.y as u32,
            src_region.width,
            src_region.height,
        );

        // Then warp
        let (w, h) = (src_region.width, src_region.height);
        self.warp(&cropped, homography, w, h)
    }

    /// Compute perspective transform from 4 point correspondences
    ///
    /// Computes the homography that maps src_points to dst_points.
    pub fn compute_perspective_transform(
        &self,
        src: &[PointF; 4],
        dst: &[PointF; 4],
    ) -> Result<Homography, VisionError> {
        Homography::dlt(src, dst).map_err(|e| VisionError::ImageProcessingError(e.into()))
    }

    /// Sample image at sub-pixel position using bilinear interpolation.
    ///
    /// `x1`/`y1` are clamped to the last valid pixel index so the final row
    /// and column are still sampled (with zero-distance weights collapsing to
    /// nearest-neighbor on the boundary).
    fn sample_bilinear(&self, image: &DynamicImage, x: f64, y: f64) -> Option<Rgba<u8>> {
        let w = image.width();
        let h = image.height();
        if w == 0 || h == 0 {
            return None;
        }

        if x < 0.0 || y < 0.0 || x > (w - 1) as f64 || y > (h - 1) as f64 {
            return None;
        }

        let x0 = x.floor() as u32;
        let y0 = y.floor() as u32;
        let x1 = (x0 + 1).min(w - 1);
        let y1 = (y0 + 1).min(h - 1);

        let fx = x - x.floor();
        let fy = y - y.floor();

        let p00 = image.get_pixel(x0, y0);
        let p10 = image.get_pixel(x1, y0);
        let p01 = image.get_pixel(x0, y1);
        let p11 = image.get_pixel(x1, y1);

        let r = (1.0 - fx) * (1.0 - fy) * p00[0] as f64
            + fx * (1.0 - fy) * p10[0] as f64
            + (1.0 - fx) * fy * p01[0] as f64
            + fx * fy * p11[0] as f64;

        let g = (1.0 - fx) * (1.0 - fy) * p00[1] as f64
            + fx * (1.0 - fy) * p10[1] as f64
            + (1.0 - fx) * fy * p01[1] as f64
            + fx * fy * p11[1] as f64;

        let b = (1.0 - fx) * (1.0 - fy) * p00[2] as f64
            + fx * (1.0 - fy) * p10[2] as f64
            + (1.0 - fx) * fy * p01[2] as f64
            + fx * fy * p11[2] as f64;

        let a = (1.0 - fx) * (1.0 - fy) * p00[3] as f64
            + fx * (1.0 - fy) * p10[3] as f64
            + (1.0 - fx) * fy * p01[3] as f64
            + fx * fy * p11[3] as f64;

        Some(Rgba([r as u8, g as u8, b as u8, a as u8]))
    }

    /// Get bounding box of transformed corners
    pub fn transformed_bounds(
        &self,
        width: u32,
        height: u32,
        homography: &Homography,
    ) -> (f64, f64, f64, f64) {
        let corners = [
            PointF { x: 0.0, y: 0.0 },
            PointF {
                x: width as f64,
                y: 0.0,
            },
            PointF {
                x: width as f64,
                y: height as f64,
            },
            PointF {
                x: 0.0,
                y: height as f64,
            },
        ];

        let transformed: Vec<PointF> = corners
            .iter()
            .map(|p| homography.transform_point(p))
            .collect();

        let min_x = transformed
            .iter()
            .map(|p| p.x)
            .fold(f64::INFINITY, f64::min);
        let max_x = transformed
            .iter()
            .map(|p| p.x)
            .fold(f64::NEG_INFINITY, f64::max);
        let min_y = transformed
            .iter()
            .map(|p| p.y)
            .fold(f64::INFINITY, f64::min);
        let max_y = transformed
            .iter()
            .map(|p| p.y)
            .fold(f64::NEG_INFINITY, f64::max);

        (min_x, min_y, max_x, max_y)
    }
}

impl Default for PerspectiveWarp {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_identity_warp() {
        let img = DynamicImage::new_rgb8(100, 100);
        let warp = PerspectiveWarp::new();
        let h = Homography::identity();

        let result = warp.warp(&img, &h, 100, 100).unwrap();
        assert_eq!(result.width(), 100);
        assert_eq!(result.height(), 100);
    }

    #[test]
    fn test_transformed_bounds() {
        let warp = PerspectiveWarp::new();
        let h = Homography::identity();

        let (min_x, min_y, max_x, max_y) = warp.transformed_bounds(100, 100, &h);
        assert!((min_x - 0.0).abs() < 0.001);
        assert!((min_y - 0.0).abs() < 0.001);
        assert!((max_x - 100.0).abs() < 0.001);
        assert!((max_y - 100.0).abs() < 0.001);
    }

    #[test]
    fn test_perspective_transform_simple() {
        let warp = PerspectiveWarp::new();

        // Unit square to unit square
        let src = [
            PointF { x: 0.0, y: 0.0 },
            PointF { x: 1.0, y: 0.0 },
            PointF { x: 1.0, y: 1.0 },
            PointF { x: 0.0, y: 1.0 },
        ];
        let dst = [
            PointF { x: 0.0, y: 0.0 },
            PointF { x: 1.0, y: 0.0 },
            PointF { x: 1.0, y: 1.0 },
            PointF { x: 0.0, y: 1.0 },
        ];

        let h = warp.compute_perspective_transform(&src, &dst).unwrap();
        let transformed = h.transform_point(&src[0]);
        assert!((transformed.x - 0.0).abs() < 0.01);
        assert!((transformed.y - 0.0).abs() < 0.01);
    }

    #[test]
    fn test_sample_bilinear_center() {
        let mut img = RgbaImage::new(10, 10);
        // Fill with gradient
        for y in 0..10 {
            for x in 0..10 {
                img.put_pixel(x, y, Rgba([x as u8, y as u8, 0, 255]));
            }
        }
        let dyn_img = DynamicImage::ImageRgba8(img);

        let warp = PerspectiveWarp::new();

        // Sample at center (4.5, 4.5) - should be between pixels
        let sample = warp.sample_bilinear(&dyn_img, 4.5, 4.5);
        assert!(sample.is_some());
        let s = sample.unwrap();
        // Should be close to (4, 4) pixel value
        assert!(s[0] >= 3 && s[0] <= 5);
        assert!(s[1] >= 3 && s[1] <= 5);
    }

    #[test]
    fn test_sample_bilinear_out_of_bounds() {
        let img = DynamicImage::new_rgb8(10, 10);
        let warp = PerspectiveWarp::new();

        let sample = warp.sample_bilinear(&img, -1.0, 5.0);
        assert!(sample.is_none());

        let sample = warp.sample_bilinear(&img, 5.0, 11.0);
        assert!(sample.is_none());
    }
}
