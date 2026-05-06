//! Color range detection (replacement for OpenCV inRange)
//!
//! Detects pixels within a specified color range in RGB or HSV space.

use betternte_core::{Color, Point};
use image::{DynamicImage, GenericImageView, Rgb, RgbImage};
use opencv::core::{self, Mat, Scalar};
use opencv::imgproc;
use opencv::prelude::*;

/// Color range detector
pub struct ColorRangeDetector {
    /// Lower bound (RGB)
    lower: [u8; 3],
    /// Upper bound (RGB)
    upper: [u8; 3],
    /// Use HSV color space
    use_hsv: bool,
}

impl ColorRangeDetector {
    /// RGB → HSV via `cvt_color_def` so we do not depend on `cvt_color`'s optional
    /// `AlgorithmHint` / `dst_cn` arity across OpenCV binding generations.
    fn cvt_color_rgb_to_hsv(src: &Mat, hsv: &mut Mat) -> Option<()> {
        imgproc::cvt_color_def(src, hsv, imgproc::COLOR_RGB2HSV).ok()?;
        Some(())
    }

    fn image_to_rgb_mat(image: &DynamicImage) -> Option<Mat> {
        let rgb = image.to_rgb8();
        let rows = rgb.height() as i32;
        let mat_1d = Mat::from_slice(rgb.as_raw()).ok()?;
        let mat_ref = mat_1d.reshape(3, rows).ok()?;
        mat_ref.try_clone().ok()
    }

    fn in_range_mask(&self, image: &DynamicImage) -> Option<Mat> {
        let src = Self::image_to_rgb_mat(image)?;
        if self.use_hsv {
            let mut hsv = Mat::default();
            Self::cvt_color_rgb_to_hsv(&src, &mut hsv)?;
            if self.lower[0] <= self.upper[0] {
                let mut mask = Mat::default();
                core::in_range(
                    &hsv,
                    &Scalar::new(
                        self.lower[0] as f64,
                        self.lower[1] as f64,
                        self.lower[2] as f64,
                        0.0,
                    ),
                    &Scalar::new(
                        self.upper[0] as f64,
                        self.upper[1] as f64,
                        self.upper[2] as f64,
                        0.0,
                    ),
                    &mut mask,
                )
                .ok()?;
                Some(mask)
            } else {
                let mut mask_low = Mat::default();
                let mut mask_high = Mat::default();
                core::in_range(
                    &hsv,
                    &Scalar::new(0.0, self.lower[1] as f64, self.lower[2] as f64, 0.0),
                    &Scalar::new(
                        self.upper[0] as f64,
                        self.upper[1] as f64,
                        self.upper[2] as f64,
                        0.0,
                    ),
                    &mut mask_low,
                )
                .ok()?;
                core::in_range(
                    &hsv,
                    &Scalar::new(
                        self.lower[0] as f64,
                        self.lower[1] as f64,
                        self.lower[2] as f64,
                        0.0,
                    ),
                    &Scalar::new(180.0, self.upper[1] as f64, self.upper[2] as f64, 0.0),
                    &mut mask_high,
                )
                .ok()?;
                let mut mask = Mat::default();
                core::bitwise_or(&mask_low, &mask_high, &mut mask, &core::no_array()).ok()?;
                Some(mask)
            }
        } else {
            let mut mask = Mat::default();
            core::in_range(
                &src,
                &Scalar::new(
                    self.lower[0] as f64,
                    self.lower[1] as f64,
                    self.lower[2] as f64,
                    0.0,
                ),
                &Scalar::new(
                    self.upper[0] as f64,
                    self.upper[1] as f64,
                    self.upper[2] as f64,
                    0.0,
                ),
                &mut mask,
            )
            .ok()?;
            Some(mask)
        }
    }

    fn mask_to_binary_rgb(mask: &Mat) -> Option<DynamicImage> {
        let rows = mask.rows();
        let cols = mask.cols();
        let data = mask.data_bytes().ok()?;
        let mut out = RgbImage::new(cols as u32, rows as u32);
        for y in 0..rows as u32 {
            for x in 0..cols as u32 {
                let idx = (y * cols as u32 + x) as usize;
                let v = data.get(idx).copied().unwrap_or(0);
                out.put_pixel(x, y, Rgb([v, v, v]));
            }
        }
        Some(DynamicImage::ImageRgb8(out))
    }

    /// Create a new RGB-based detector
    pub fn new_rgb(lower: [u8; 3], upper: [u8; 3]) -> Self {
        Self {
            lower,
            upper,
            use_hsv: false,
        }
    }

    /// Create a new HSV-based detector
    ///
    /// HSV is often better for color detection as it's more robust to lighting changes.
    /// Hue is in range [0, 180] (OpenCV convention), Saturation and Value in [0, 255].
    pub fn new_hsv(lower: [u8; 3], upper: [u8; 3]) -> Self {
        Self {
            lower,
            upper,
            use_hsv: true,
        }
    }

    /// Check if a single RGB color is within range
    pub fn is_in_range_rgb(&self, color: &Color) -> bool {
        let rgb = [color.r, color.g, color.b];
        self.is_in_range_rgb_array(&rgb)
    }

    /// Check if an RGB array is within range
    pub fn is_in_range_rgb_array(&self, rgb: &[u8; 3]) -> bool {
        rgb[0] >= self.lower[0]
            && rgb[0] <= self.upper[0]
            && rgb[1] >= self.lower[1]
            && rgb[1] <= self.upper[1]
            && rgb[2] >= self.lower[2]
            && rgb[2] <= self.upper[2]
    }

    /// Convert RGB to HSV (OpenCV convention: H in [0, 180], S,V in [0, 255])
    fn rgb_to_hsv(r: u8, g: u8, b: u8) -> [u8; 3] {
        let r = r as f32 / 255.0;
        let g = g as f32 / 255.0;
        let b = b as f32 / 255.0;

        let max = r.max(g).max(b);
        let min = r.min(g).min(b);
        let delta = max - min;

        // Value
        let v = max * 255.0;

        // Saturation
        let s = if max > 0.0 { delta / max * 255.0 } else { 0.0 };

        // Hue
        let h = if delta < 1e-6 {
            0.0
        } else if max == r {
            60.0 * (((g - b) / delta) % 6.0)
        } else if max == g {
            60.0 * ((b - r) / delta + 2.0)
        } else {
            60.0 * ((r - g) / delta + 4.0)
        };

        let h = if h < 0.0 { h + 360.0 } else { h };
        let h = h / 2.0; // Scale to [0, 180]

        [h as u8, s as u8, v as u8]
    }

    /// Check if HSV values are within range (handling hue wrap-around)
    fn is_in_range_hsv(&self, hsv: &[u8; 3]) -> bool {
        // Hue can wrap around (e.g., red near 0/180)
        let h_in_range = if self.lower[0] <= self.upper[0] {
            // Normal case
            hsv[0] >= self.lower[0] && hsv[0] <= self.upper[0]
        } else {
            // Hue wraps around (e.g., lower=170, upper=10, valid range=[170,180] U [0,10])
            hsv[0] >= self.lower[0] || hsv[0] <= self.upper[0]
        };

        h_in_range
            && hsv[1] >= self.lower[1]
            && hsv[1] <= self.upper[1]
            && hsv[2] >= self.lower[2]
            && hsv[2] <= self.upper[2]
    }

    /// Detect pixels in range - returns binary image
    pub fn detect(&self, image: &DynamicImage) -> DynamicImage {
        if let Some(mask) = self.in_range_mask(image) {
            if let Some(binary) = Self::mask_to_binary_rgb(&mask) {
                return binary;
            }
        }
        let (width, height) = image.dimensions();
        let mut result = RgbImage::new(width, height);

        if self.use_hsv {
            for y in 0..height {
                for x in 0..width {
                    let pixel = image.get_pixel(x, y);
                    let hsv = Self::rgb_to_hsv(pixel[0], pixel[1], pixel[2]);
                    let in_range = self.is_in_range_hsv(&hsv);
                    result.put_pixel(
                        x,
                        y,
                        if in_range {
                            Rgb([255, 255, 255])
                        } else {
                            Rgb([0, 0, 0])
                        },
                    );
                }
            }
        } else {
            for y in 0..height {
                for x in 0..width {
                    let pixel = image.get_pixel(x, y);
                    let rgb = [pixel[0], pixel[1], pixel[2]];
                    let in_range = self.is_in_range_rgb_array(&rgb);
                    result.put_pixel(
                        x,
                        y,
                        if in_range {
                            Rgb([255, 255, 255])
                        } else {
                            Rgb([0, 0, 0])
                        },
                    );
                }
            }
        }

        DynamicImage::ImageRgb8(result)
    }

    /// Count pixels in range
    pub fn count_in_range(&self, image: &DynamicImage) -> usize {
        if let Some(mask) = self.in_range_mask(image) {
            if let Ok(data) = mask.data_bytes() {
                return data.iter().filter(|&&v| v > 0).count();
            }
        }
        let (width, height) = image.dimensions();
        let mut count = 0;

        if self.use_hsv {
            for y in 0..height {
                for x in 0..width {
                    let pixel = image.get_pixel(x, y);
                    let hsv = Self::rgb_to_hsv(pixel[0], pixel[1], pixel[2]);
                    if self.is_in_range_hsv(&hsv) {
                        count += 1;
                    }
                }
            }
        } else {
            for y in 0..height {
                for x in 0..width {
                    let pixel = image.get_pixel(x, y);
                    let rgb = [pixel[0], pixel[1], pixel[2]];
                    if self.is_in_range_rgb_array(&rgb) {
                        count += 1;
                    }
                }
            }
        }

        count
    }

    /// Find all positions in range
    pub fn find_positions(&self, image: &DynamicImage) -> Vec<Point> {
        if let Some(mask) = self.in_range_mask(image) {
            if let Ok(data) = mask.data_bytes() {
                let rows = mask.rows() as usize;
                let cols = mask.cols() as usize;
                let mut positions = Vec::new();
                for y in 0..rows {
                    for x in 0..cols {
                        let idx = y * cols + x;
                        if data.get(idx).copied().unwrap_or(0) > 0 {
                            positions.push(Point::new(x as i32, y as i32));
                        }
                    }
                }
                return positions;
            }
        }
        let (width, height) = image.dimensions();
        let mut positions = Vec::new();

        if self.use_hsv {
            for y in 0..height {
                for x in 0..width {
                    let pixel = image.get_pixel(x, y);
                    let hsv = Self::rgb_to_hsv(pixel[0], pixel[1], pixel[2]);
                    if self.is_in_range_hsv(&hsv) {
                        positions.push(Point::new(x as i32, y as i32));
                    }
                }
            }
        } else {
            for y in 0..height {
                for x in 0..width {
                    let pixel = image.get_pixel(x, y);
                    let rgb = [pixel[0], pixel[1], pixel[2]];
                    if self.is_in_range_rgb_array(&rgb) {
                        positions.push(Point::new(x as i32, y as i32));
                    }
                }
            }
        }

        positions
    }

    /// Get ratio of pixels in range
    pub fn ratio_in_range(&self, image: &DynamicImage) -> f32 {
        let (width, height) = image.dimensions();
        let total = (width * height) as usize;
        if total == 0 {
            return 0.0;
        }
        self.count_in_range(image) as f32 / total as f32
    }
}

impl Default for ColorRangeDetector {
    fn default() -> Self {
        // White by default
        Self::new_rgb([200, 200, 200], [255, 255, 255])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rgb_in_range() {
        let detector = ColorRangeDetector::new_rgb([100, 100, 100], [200, 200, 200]);
        let color = Color::rgb(150, 150, 150);
        assert!(detector.is_in_range_rgb(&color));
    }

    #[test]
    fn test_rgb_out_of_range() {
        let detector = ColorRangeDetector::new_rgb([100, 100, 100], [200, 200, 200]);
        let color = Color::rgb(50, 150, 150);
        assert!(!detector.is_in_range_rgb(&color));
    }

    #[test]
    fn test_rgb_boundary() {
        let detector = ColorRangeDetector::new_rgb([100, 100, 100], [200, 200, 200]);
        let color_low = Color::rgb(100, 100, 100);
        let color_high = Color::rgb(200, 200, 200);
        assert!(detector.is_in_range_rgb(&color_low));
        assert!(detector.is_in_range_rgb(&color_high));
    }

    #[test]
    fn test_detect_binary_image() {
        let mut img = image::RgbImage::new(10, 10);
        // Fill with colors
        for y in 0..10 {
            for x in 0..10 {
                if x < 5 {
                    img.put_pixel(x, y, Rgb([150, 150, 150]));
                } else {
                    img.put_pixel(x, y, Rgb([50, 50, 50]));
                }
            }
        }

        let detector = ColorRangeDetector::new_rgb([100, 100, 100], [200, 200, 200]);
        let result = detector.detect(&DynamicImage::ImageRgb8(img));

        // Left half should be white, right half black
        for y in 0..10 {
            for x in 0..5 {
                assert_eq!(result.get_pixel(x, y)[0], 255);
            }
            for x in 5..10 {
                assert_eq!(result.get_pixel(x, y)[0], 0);
            }
        }
    }

    #[test]
    fn test_count_in_range() {
        let mut img = image::RgbImage::new(10, 10);
        for y in 0..10 {
            for x in 0..10 {
                if x < 5 {
                    img.put_pixel(x, y, Rgb([150, 150, 150]));
                } else {
                    img.put_pixel(x, y, Rgb([50, 50, 50]));
                }
            }
        }

        let detector = ColorRangeDetector::new_rgb([100, 100, 100], [200, 200, 200]);
        let count = detector.count_in_range(&DynamicImage::ImageRgb8(img));

        assert_eq!(count, 50); // 5x10 pixels
    }

    #[test]
    fn test_find_positions() {
        let mut img = image::RgbImage::new(3, 3);
        img.put_pixel(0, 0, Rgb([150, 150, 150]));
        img.put_pixel(2, 2, Rgb([150, 150, 150]));

        let detector = ColorRangeDetector::new_rgb([100, 100, 100], [200, 200, 200]);
        let positions = detector.find_positions(&DynamicImage::ImageRgb8(img));

        assert_eq!(positions.len(), 2);
    }

    #[test]
    fn test_hsv_detector() {
        let detector = ColorRangeDetector::new_hsv([0, 100, 100], [10, 255, 255]);
        let mut img = image::RgbImage::new(10, 10);
        // Red-ish pixels
        for y in 0..10 {
            for x in 0..10 {
                img.put_pixel(x, y, Rgb([255, 0, 0]));
            }
        }

        let count = detector.count_in_range(&DynamicImage::ImageRgb8(img));
        assert_eq!(count, 100);
    }

    #[test]
    fn test_ratio_in_range() {
        let mut img = image::RgbImage::new(10, 10);
        for y in 0..10 {
            for x in 0..10 {
                if x < 3 {
                    img.put_pixel(x, y, Rgb([150, 150, 150]));
                } else {
                    img.put_pixel(x, y, Rgb([50, 50, 50]));
                }
            }
        }

        let detector = ColorRangeDetector::new_rgb([100, 100, 100], [200, 200, 200]);
        let ratio = detector.ratio_in_range(&DynamicImage::ImageRgb8(img));

        assert!((ratio - 0.3).abs() < 0.01);
    }

    #[test]
    fn test_rgb_to_hsv() {
        // Red
        let hsv = ColorRangeDetector::rgb_to_hsv(255, 0, 0);
        assert!(hsv[0] < 10 || hsv[0] > 170); // Red hue near 0 or 180
        assert_eq!(hsv[2], 255); // Max value

        // Green
        let hsv = ColorRangeDetector::rgb_to_hsv(0, 255, 0);
        assert!(hsv[0] > 50 && hsv[0] < 70); // Green hue around 60

        // Blue - hue for blue is exactly 120 in OpenCV convention
        let hsv = ColorRangeDetector::rgb_to_hsv(0, 0, 255);
        assert!(hsv[0] >= 100 && hsv[0] <= 120); // Blue hue around 120

        // White
        let hsv = ColorRangeDetector::rgb_to_hsv(255, 255, 255);
        assert_eq!(hsv[2], 255); // Max value
        assert_eq!(hsv[1], 0); // No saturation
    }
}
