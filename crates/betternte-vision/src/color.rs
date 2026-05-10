//! 颜色检测

use crate::ColorDetector;
use betternte_core::{Color, ColorTolerance, Point, Region};
use opencv::core::Vec4b;
use opencv::prelude::*;

/// 颜色检测器实现
pub struct ColorDetectorImpl;

impl ColorDetectorImpl {
    pub fn new() -> Self {
        Self
    }

    /// RGB Euclidean distance in 0–255 units (legacy helper).
    pub fn color_distance(a: Color, b: Color) -> u8 {
        let dr = a.r as f64 - b.r as f64;
        let dg = a.g as f64 - b.g as f64;
        let db = a.b as f64 - b.b as f64;
        (dr * dr + dg * dg + db * db).sqrt().min(255.0) as u8
    }
}

impl Default for ColorDetectorImpl {
    fn default() -> Self {
        Self::new()
    }
}

impl ColorDetector for ColorDetectorImpl {
    fn detect_pixel(
        &self,
        mat: &opencv::core::Mat,
        pos: Point,
        target: Color,
        tolerance: ColorTolerance,
    ) -> bool {
        let w = mat.cols();
        let h = mat.rows();
        if pos.x < 0 || pos.y < 0 || pos.x >= w || pos.y >= h {
            return false;
        }
        let pixel = match mat.at_2d::<Vec4b>(pos.y, pos.x) {
            Ok(p) => p,
            Err(_) => return false,
        };
        // BGRA: [0]=B, [1]=G, [2]=R, [3]=A
        let color = Color {
            r: pixel[2],
            g: pixel[1],
            b: pixel[0],
            a: pixel[3],
        };
        tolerance.matches(color, target)
    }

    fn find_color(
        &self,
        mat: &opencv::core::Mat,
        target: Color,
        tolerance: ColorTolerance,
    ) -> Vec<Point> {
        let w = mat.cols();
        let h = mat.rows();
        let mut result = Vec::new();
        for y in 0..h {
            for x in 0..w {
                if let Ok(pixel) = mat.at_2d::<Vec4b>(y, x) {
                    let color = Color {
                        r: pixel[2],
                        g: pixel[1],
                        b: pixel[0],
                        a: pixel[3],
                    };
                    if tolerance.matches(color, target) {
                        result.push(Point::new(x, y));
                    }
                }
            }
        }
        result
    }

    fn detect_color_region(
        &self,
        mat: &opencv::core::Mat,
        region: &Region,
        target: Color,
        tolerance: ColorTolerance,
    ) -> f32 {
        let mat_w = mat.cols();
        let mat_h = mat.rows();
        let x_start = region.x.max(0);
        let y_start = region.y.max(0);
        let x_end = (region.x + region.width as i32).min(mat_w);
        let y_end = (region.y + region.height as i32).min(mat_h);

        if x_start >= x_end || y_start >= y_end {
            return 0.0;
        }

        let mut total = 0u32;
        let mut matched = 0u32;
        for y in y_start..y_end {
            for x in x_start..x_end {
                total += 1;
                if let Ok(pixel) = mat.at_2d::<Vec4b>(y, x) {
                    let color = Color {
                        r: pixel[2],
                        g: pixel[1],
                        b: pixel[0],
                        a: pixel[3],
                    };
                    if tolerance.matches(color, target) {
                        matched += 1;
                    }
                }
            }
        }

        if total == 0 {
            0.0
        } else {
            matched as f32 / total as f32
        }
    }

    fn get_pixel_color(&self, mat: &opencv::core::Mat, pos: Point) -> Option<Color> {
        let w = mat.cols();
        let h = mat.rows();
        if pos.x < 0 || pos.y < 0 || pos.x >= w || pos.y >= h {
            return None;
        }
        let pixel = mat.at_2d::<Vec4b>(pos.y, pos.x).ok()?;
        Some(Color {
            r: pixel[2],
            g: pixel[1],
            b: pixel[0],
            a: pixel[3],
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use betternte_core::{Color, ColorTolerance, Point};
    use opencv::core::{Mat, Vec4b, CV_8UC4};

    /// Create a solid BGRA Mat of size `w` x `h` filled with the given BGRA color.
    fn create_solid_mat(w: i32, h: i32, bgra: [u8; 4]) -> Mat {
        let mut mat = Mat::new_rows_cols_with_default(h, w, CV_8UC4, Vec4b::from(bgra).into())
            .expect("failed to create mat");
        // Fill every pixel
        for y in 0..h {
            for x in 0..w {
                *mat.at_2d_mut::<Vec4b>(y, x).unwrap() = Vec4b::from(bgra);
            }
        }
        mat
    }

    #[test]
    fn test_color_detect_pixel_exact_match() {
        // BGRA for pure red: [0, 0, 255, 255]
        let mat = create_solid_mat(10, 10, [0, 0, 255, 255]);
        let detector = ColorDetectorImpl::new();
        let target = Color {
            r: 255,
            g: 0,
            b: 0,
            a: 255,
        };

        assert!(
            detector.detect_pixel(&mat, Point::new(5, 5), target, ColorTolerance::from(0)),
            "精确匹配 tolerance=0 应返回 true"
        );
    }

    #[test]
    fn test_color_detect_pixel_within_tolerance() {
        let mat = create_solid_mat(10, 10, [0, 0, 255, 255]);
        let detector = ColorDetectorImpl::new();
        let target = Color {
            r: 250,
            g: 0,
            b: 0,
            a: 255,
        };

        assert!(
            detector.detect_pixel(&mat, Point::new(5, 5), target, ColorTolerance::from(10)),
            "容差 10 内应匹配"
        );
        assert!(
            detector.detect_pixel(&mat, Point::new(5, 5), target, ColorTolerance::from(5)),
            "容差 5 恰好应匹配"
        );
    }

    #[test]
    fn test_color_detect_pixel_outside_tolerance() {
        let mat = create_solid_mat(10, 10, [0, 0, 255, 255]);
        let detector = ColorDetectorImpl::new();
        let target = Color {
            r: 200,
            g: 0,
            b: 0,
            a: 255,
        };

        assert!(
            !detector.detect_pixel(&mat, Point::new(5, 5), target, ColorTolerance::from(10)),
            "容差 10 不应匹配差值 55"
        );
    }

    #[test]
    fn test_color_find_color_returns_all_matching() {
        // White BGRA: [255, 255, 255, 255]
        let mut mat = create_solid_mat(10, 10, [255, 255, 255, 255]);
        // Red pixels in BGRA: [0, 0, 255, 255]
        *mat.at_2d_mut::<Vec4b>(3, 2).unwrap() = Vec4b::from([0, 0, 255, 255]);
        *mat.at_2d_mut::<Vec4b>(7, 5).unwrap() = Vec4b::from([0, 0, 255, 255]);
        *mat.at_2d_mut::<Vec4b>(1, 9).unwrap() = Vec4b::from([0, 0, 255, 255]);

        let detector = ColorDetectorImpl::new();
        let target = Color {
            r: 255,
            g: 0,
            b: 0,
            a: 255,
        };
        let results = detector.find_color(&mat, target, ColorTolerance::from(0));

        assert_eq!(results.len(), 3, "应找到 3 个红色像素");
    }

    #[test]
    fn test_color_get_pixel_color() {
        let mut mat = create_solid_mat(10, 10, [0, 0, 0, 255]);
        // BGRA for R=100, G=150, B=200: [200, 150, 100, 255]
        *mat.at_2d_mut::<Vec4b>(5, 5).unwrap() = Vec4b::from([200, 150, 100, 255]);

        let detector = ColorDetectorImpl::new();
        let color = detector.get_pixel_color(&mat, Point::new(5, 5));

        assert!(color.is_some());
        let c = color.unwrap();
        assert_eq!(c.r, 100);
        assert_eq!(c.g, 150);
        assert_eq!(c.b, 200);
    }

    #[test]
    fn test_color_detect_pixel_rgba_channel_tolerance() {
        // R=250, G=10, B=10 → BGRA [10, 10, 250, 255]
        let mat = create_solid_mat(10, 10, [10, 10, 250, 255]);
        let detector = ColorDetectorImpl::new();
        let target = Color {
            r: 255,
            g: 0,
            b: 0,
            a: 255,
        };
        let tol = ColorTolerance::RgbaMaxDelta {
            r: 10,
            g: 15,
            b: 15,
            a: 255,
        };
        assert!(
            detector.detect_pixel(&mat, Point::new(5, 5), target, tol),
            "per-channel deltas within caps should match"
        );
        let tol_strict_g = ColorTolerance::RgbaMaxDelta {
            r: 10,
            g: 5,
            b: 15,
            a: 255,
        };
        assert!(
            !detector.detect_pixel(&mat, Point::new(5, 5), target, tol_strict_g),
            "green delta 10 > 5 should fail"
        );
    }

    #[test]
    fn test_color_detect_color_region_returns_ratio() {
        // White background: BGRA [255, 255, 255, 255]
        let mut mat = create_solid_mat(100, 100, [255, 255, 255, 255]);
        // Top-left 50x50 region = red: BGRA [0, 0, 255, 255]
        for y in 0..50 {
            for x in 0..50 {
                *mat.at_2d_mut::<Vec4b>(y, x).unwrap() = Vec4b::from([0, 0, 255, 255]);
            }
        }

        let detector = ColorDetectorImpl::new();
        let target = Color {
            r: 255,
            g: 0,
            b: 0,
            a: 255,
        };
        let region = betternte_core::Region {
            x: 0,
            y: 0,
            width: 100,
            height: 100,
        };

        let ratio = detector.detect_color_region(&mat, &region, target, ColorTolerance::from(0));

        assert!(
            (ratio - 0.25).abs() < 0.01,
            "50x50 红色 / 100x100 总量 = 0.25"
        );
    }
}
