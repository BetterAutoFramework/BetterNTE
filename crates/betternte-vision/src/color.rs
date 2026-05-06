//! 颜色检测

use crate::ColorDetector;
use betternte_core::{Color, ColorTolerance, Point, Region};
use image::{DynamicImage, GenericImageView};

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
        image: &DynamicImage,
        pos: Point,
        target: Color,
        tolerance: ColorTolerance,
    ) -> bool {
        let (w, h) = image.dimensions();
        if pos.x < 0 || pos.y < 0 || pos.x as u32 >= w || pos.y as u32 >= h {
            return false;
        }
        let pixel = image.get_pixel(pos.x as u32, pos.y as u32);
        let color = Color::rgb(pixel[0], pixel[1], pixel[2]);
        tolerance.matches(color, target)
    }

    fn find_color(
        &self,
        image: &DynamicImage,
        target: Color,
        tolerance: ColorTolerance,
    ) -> Vec<Point> {
        let (w, h) = image.dimensions();
        let mut result = Vec::new();
        for y in 0..h {
            for x in 0..w {
                let pixel = image.get_pixel(x, y);
                let color = Color::rgb(pixel[0], pixel[1], pixel[2]);
                if tolerance.matches(color, target) {
                    result.push(Point::new(x as i32, y as i32));
                }
            }
        }
        result
    }

    fn detect_color_region(
        &self,
        image: &DynamicImage,
        region: &Region,
        target: Color,
        tolerance: ColorTolerance,
    ) -> f32 {
        let (img_w, img_h) = image.dimensions();
        let x_start = region.x.max(0) as u32;
        let y_start = region.y.max(0) as u32;
        let x_end = (region.x + region.width as i32).min(img_w as i32) as u32;
        let y_end = (region.y + region.height as i32).min(img_h as i32) as u32;

        if x_start >= x_end || y_start >= y_end {
            return 0.0;
        }

        let mut total = 0u32;
        let mut matched = 0u32;
        for y in y_start..y_end {
            for x in x_start..x_end {
                total += 1;
                let pixel = image.get_pixel(x, y);
                let color = Color::rgb(pixel[0], pixel[1], pixel[2]);
                if tolerance.matches(color, target) {
                    matched += 1;
                }
            }
        }

        if total == 0 {
            0.0
        } else {
            matched as f32 / total as f32
        }
    }

    fn get_pixel_color(&self, image: &DynamicImage, pos: Point) -> Option<Color> {
        let (w, h) = image.dimensions();
        if pos.x < 0 || pos.y < 0 || pos.x as u32 >= w || pos.y as u32 >= h {
            return None;
        }
        let pixel = image.get_pixel(pos.x as u32, pos.y as u32);
        Some(Color::rgb(pixel[0], pixel[1], pixel[2]))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use betternte_core::{Color, ColorTolerance, Point};
    use image::{DynamicImage, Rgb, RgbImage};

    fn create_solid_image(w: u32, h: u32, color: Rgb<u8>) -> DynamicImage {
        let mut img = RgbImage::new(w, h);
        for pixel in img.pixels_mut() {
            *pixel = color;
        }
        DynamicImage::ImageRgb8(img)
    }

    #[test]
    fn test_color_detect_pixel_exact_match() {
        let image = create_solid_image(10, 10, Rgb([255, 0, 0]));
        let detector = ColorDetectorImpl::new();
        let target = Color {
            r: 255,
            g: 0,
            b: 0,
            a: 255,
        };

        assert!(
            detector.detect_pixel(&image, Point::new(5, 5), target, ColorTolerance::from(0)),
            "精确匹配 tolerance=0 应返回 true"
        );
    }

    #[test]
    fn test_color_detect_pixel_within_tolerance() {
        let image = create_solid_image(10, 10, Rgb([255, 0, 0]));
        let detector = ColorDetectorImpl::new();
        let target = Color {
            r: 250,
            g: 0,
            b: 0,
            a: 255,
        };

        assert!(
            detector.detect_pixel(&image, Point::new(5, 5), target, ColorTolerance::from(10)),
            "容差 10 内应匹配"
        );
        assert!(
            detector.detect_pixel(&image, Point::new(5, 5), target, ColorTolerance::from(5)),
            "容差 5 恰好应匹配"
        );
    }

    #[test]
    fn test_color_detect_pixel_outside_tolerance() {
        let image = create_solid_image(10, 10, Rgb([255, 0, 0]));
        let detector = ColorDetectorImpl::new();
        let target = Color {
            r: 200,
            g: 0,
            b: 0,
            a: 255,
        };

        assert!(
            !detector.detect_pixel(&image, Point::new(5, 5), target, ColorTolerance::from(10)),
            "容差 10 不应匹配差值 55"
        );
    }

    #[test]
    fn test_color_find_color_returns_all_matching() {
        let mut image = RgbImage::new(10, 10);
        for pixel in image.pixels_mut() {
            *pixel = Rgb([255, 255, 255]);
        }
        image.put_pixel(2, 3, Rgb([255, 0, 0]));
        image.put_pixel(5, 7, Rgb([255, 0, 0]));
        image.put_pixel(9, 1, Rgb([255, 0, 0]));
        let image = DynamicImage::ImageRgb8(image);

        let detector = ColorDetectorImpl::new();
        let target = Color {
            r: 255,
            g: 0,
            b: 0,
            a: 255,
        };
        let results = detector.find_color(&image, target, ColorTolerance::from(0));

        assert_eq!(results.len(), 3, "应找到 3 个红色像素");
    }

    #[test]
    fn test_color_get_pixel_color() {
        let mut image = RgbImage::new(10, 10);
        image.put_pixel(5, 5, Rgb([100, 150, 200]));
        let image = DynamicImage::ImageRgb8(image);

        let detector = ColorDetectorImpl::new();
        let color = detector.get_pixel_color(&image, Point::new(5, 5));

        assert!(color.is_some());
        let c = color.unwrap();
        assert_eq!(c.r, 100);
        assert_eq!(c.g, 150);
        assert_eq!(c.b, 200);
    }

    #[test]
    fn test_color_detect_pixel_rgba_channel_tolerance() {
        let image = create_solid_image(10, 10, Rgb([250, 10, 10]));
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
            detector.detect_pixel(&image, Point::new(5, 5), target, tol),
            "per-channel deltas within caps should match"
        );
        let tol_strict_g = ColorTolerance::RgbaMaxDelta {
            r: 10,
            g: 5,
            b: 15,
            a: 255,
        };
        assert!(
            !detector.detect_pixel(&image, Point::new(5, 5), target, tol_strict_g),
            "green delta 10 > 5 should fail"
        );
    }

    #[test]
    fn test_color_detect_color_region_returns_ratio() {
        let mut image = RgbImage::new(100, 100);
        for pixel in image.pixels_mut() {
            *pixel = Rgb([255, 255, 255]);
        }
        // 左上角 50x50 区域设为红色
        for y in 0..50 {
            for x in 0..50 {
                image.put_pixel(x, y, Rgb([255, 0, 0]));
            }
        }
        let image = DynamicImage::ImageRgb8(image);

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

        let ratio = detector.detect_color_region(&image, &region, target, ColorTolerance::from(0));

        assert!(
            (ratio - 0.25).abs() < 0.01,
            "50x50 红色 / 100x100 总量 = 0.25"
        );
    }
}
