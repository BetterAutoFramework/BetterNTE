//! Point-wise pixel operations
//!
//! Provides per-pixel arithmetic and logical operations on images.

use image::{DynamicImage, GenericImageView, Rgba, RgbaImage};

/// Point-wise pixel math operations
pub struct PixelMath;

impl PixelMath {
    pub fn new() -> Self {
        Self
    }

    /// Add constant to each pixel
    pub fn add_constant(&self, image: &DynamicImage, value: i32) -> DynamicImage {
        let (width, height) = image.dimensions();
        let mut result = RgbaImage::new(width, height);

        for y in 0..height {
            for x in 0..width {
                let pixel = image.get_pixel(x, y);
                let r = (pixel[0] as i32 + value).clamp(0, 255) as u8;
                let g = (pixel[1] as i32 + value).clamp(0, 255) as u8;
                let b = (pixel[2] as i32 + value).clamp(0, 255) as u8;
                let a = pixel[3];
                result.put_pixel(x, y, Rgba([r, g, b, a]));
            }
        }

        DynamicImage::ImageRgba8(result)
    }

    /// Multiply each pixel by a constant
    pub fn multiply_constant(&self, image: &DynamicImage, factor: f32) -> DynamicImage {
        let (width, height) = image.dimensions();
        let mut result = RgbaImage::new(width, height);

        for y in 0..height {
            for x in 0..width {
                let pixel = image.get_pixel(x, y);
                let r = ((pixel[0] as f32 * factor).clamp(0.0, 255.0)) as u8;
                let g = ((pixel[1] as f32 * factor).clamp(0.0, 255.0)) as u8;
                let b = ((pixel[2] as f32 * factor).clamp(0.0, 255.0)) as u8;
                let a = pixel[3];
                result.put_pixel(x, y, Rgba([r, g, b, a]));
            }
        }

        DynamicImage::ImageRgba8(result)
    }

    /// Per-pixel addition of two images
    pub fn add_images(&self, a: &DynamicImage, b: &DynamicImage) -> DynamicImage {
        let (w1, h1) = a.dimensions();
        let (w2, h2) = b.dimensions();

        let width = w1.min(w2);
        let height = h1.min(h2);
        let mut result = RgbaImage::new(width, height);

        for y in 0..height {
            for x in 0..width {
                let p1 = a.get_pixel(x, y);
                let p2 = b.get_pixel(x, y);
                let r = (p1[0] as i32 + p2[0] as i32).clamp(0, 255) as u8;
                let g = (p1[1] as i32 + p2[1] as i32).clamp(0, 255) as u8;
                let b = (p1[2] as i32 + p2[2] as i32).clamp(0, 255) as u8;
                let a1 = p1[3] as i32;
                let a2 = p2[3] as i32;
                let a = ((a1 + a2) / 2).clamp(0, 255) as u8;
                result.put_pixel(x, y, Rgba([r, g, b, a]));
            }
        }

        DynamicImage::ImageRgba8(result)
    }

    /// Per-pixel subtraction: a - b
    pub fn subtract(&self, a: &DynamicImage, b: &DynamicImage) -> DynamicImage {
        let (w1, h1) = a.dimensions();
        let (w2, h2) = b.dimensions();

        let width = w1.min(w2);
        let height = h1.min(h2);
        let mut result = RgbaImage::new(width, height);

        for y in 0..height {
            for x in 0..width {
                let p1 = a.get_pixel(x, y);
                let p2 = b.get_pixel(x, y);
                let r = (p1[0] as i32 - p2[0] as i32).clamp(0, 255) as u8;
                let g = (p1[1] as i32 - p2[1] as i32).clamp(0, 255) as u8;
                let b = (p1[2] as i32 - p2[2] as i32).clamp(0, 255) as u8;
                let a = ((p1[3] as i32 + p2[3] as i32) / 2).clamp(0, 255) as u8;
                result.put_pixel(x, y, Rgba([r, g, b, a]));
            }
        }

        DynamicImage::ImageRgba8(result)
    }

    /// Per-pixel absolute difference
    pub fn absdiff(&self, a: &DynamicImage, b: &DynamicImage) -> DynamicImage {
        let (w1, h1) = a.dimensions();
        let (w2, h2) = b.dimensions();

        let width = w1.min(w2);
        let height = h1.min(h2);
        let mut result = RgbaImage::new(width, height);

        for y in 0..height {
            for x in 0..width {
                let p1 = a.get_pixel(x, y);
                let p2 = b.get_pixel(x, y);
                let r = (p1[0] as i32 - p2[0] as i32).unsigned_abs() as u8;
                let g = (p1[1] as i32 - p2[1] as i32).unsigned_abs() as u8;
                let b = (p1[2] as i32 - p2[2] as i32).unsigned_abs() as u8;
                let a = ((p1[3] as i32 + p2[3] as i32) / 2).clamp(0, 255) as u8;
                result.put_pixel(x, y, Rgba([r, g, b, a]));
            }
        }

        DynamicImage::ImageRgba8(result)
    }

    /// Bitwise AND of two images
    pub fn bitwise_and(&self, a: &DynamicImage, b: &DynamicImage) -> DynamicImage {
        let (w1, h1) = a.dimensions();
        let (w2, h2) = b.dimensions();

        let width = w1.min(w2);
        let height = h1.min(h2);
        let mut result = RgbaImage::new(width, height);

        for y in 0..height {
            for x in 0..width {
                let p1 = a.get_pixel(x, y);
                let p2 = b.get_pixel(x, y);
                let r = p1[0] & p2[0];
                let g = p1[1] & p2[1];
                let b = p1[2] & p2[2];
                let a = p1[3] & p2[3];
                result.put_pixel(x, y, Rgba([r, g, b, a]));
            }
        }

        DynamicImage::ImageRgba8(result)
    }

    /// Bitwise OR of two images
    pub fn bitwise_or(&self, a: &DynamicImage, b: &DynamicImage) -> DynamicImage {
        let (w1, h1) = a.dimensions();
        let (w2, h2) = b.dimensions();

        let width = w1.min(w2);
        let height = h1.min(h2);
        let mut result = RgbaImage::new(width, height);

        for y in 0..height {
            for x in 0..width {
                let p1 = a.get_pixel(x, y);
                let p2 = b.get_pixel(x, y);
                let r = p1[0] | p2[0];
                let g = p1[1] | p2[1];
                let b = p1[2] | p2[2];
                let a = p1[3] | p2[3];
                result.put_pixel(x, y, Rgba([r, g, b, a]));
            }
        }

        DynamicImage::ImageRgba8(result)
    }

    /// Bitwise XOR of two images
    pub fn bitwise_xor(&self, a: &DynamicImage, b: &DynamicImage) -> DynamicImage {
        let (w1, h1) = a.dimensions();
        let (w2, h2) = b.dimensions();

        let width = w1.min(w2);
        let height = h1.min(h2);
        let mut result = RgbaImage::new(width, height);

        for y in 0..height {
            for x in 0..width {
                let p1 = a.get_pixel(x, y);
                let p2 = b.get_pixel(x, y);
                let r = p1[0] ^ p2[0];
                let g = p1[1] ^ p2[1];
                let b = p1[2] ^ p2[2];
                let a = p1[3] ^ p2[3];
                result.put_pixel(x, y, Rgba([r, g, b, a]));
            }
        }

        DynamicImage::ImageRgba8(result)
    }

    /// Invert image
    pub fn invert(&self, image: &DynamicImage) -> DynamicImage {
        let (width, height) = image.dimensions();
        let mut result = RgbaImage::new(width, height);

        for y in 0..height {
            for x in 0..width {
                let pixel = image.get_pixel(x, y);
                let r = 255 - pixel[0];
                let g = 255 - pixel[1];
                let b = 255 - pixel[2];
                let a = pixel[3];
                result.put_pixel(x, y, Rgba([r, g, b, a]));
            }
        }

        DynamicImage::ImageRgba8(result)
    }

    /// Power operation: raise each pixel to a power
    pub fn pow(&self, image: &DynamicImage, exponent: f32) -> DynamicImage {
        let (width, height) = image.dimensions();
        let mut result = RgbaImage::new(width, height);

        for y in 0..height {
            for x in 0..width {
                let pixel = image.get_pixel(x, y);
                // Apply power directly to raw pixel values (not normalized)
                let r = (pixel[0] as f32).powf(exponent).clamp(0.0, 255.0);
                let g = (pixel[1] as f32).powf(exponent).clamp(0.0, 255.0);
                let b = (pixel[2] as f32).powf(exponent).clamp(0.0, 255.0);
                result.put_pixel(x, y, Rgba([r as u8, g as u8, b as u8, pixel[3]]));
            }
        }

        DynamicImage::ImageRgba8(result)
    }

    /// Log transform: log(1 + pixel)
    pub fn log_transform(&self, image: &DynamicImage) -> DynamicImage {
        let (width, height) = image.dimensions();
        let mut result = RgbaImage::new(width, height);

        for y in 0..height {
            for x in 0..width {
                let pixel = image.get_pixel(x, y);
                let r = ((1.0 + pixel[0] as f32).log(2.0) / 8.0 * 255.0) as u8;
                let g = ((1.0 + pixel[1] as f32).log(2.0) / 8.0 * 255.0) as u8;
                let b = ((1.0 + pixel[2] as f32).log(2.0) / 8.0 * 255.0) as u8;
                result.put_pixel(x, y, Rgba([r, g, b, pixel[3]]));
            }
        }

        DynamicImage::ImageRgba8(result)
    }
}

impl Default for PixelMath {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_test_image() -> DynamicImage {
        let mut img = RgbaImage::new(10, 10);
        for y in 0..10 {
            for x in 0..10 {
                img.put_pixel(x, y, Rgba([100, 100, 100, 255]));
            }
        }
        DynamicImage::ImageRgba8(img)
    }

    #[test]
    fn test_add_constant() {
        let img = make_test_image();
        let result = PixelMath::new().add_constant(&img, 50);

        let pixel = result.get_pixel(5, 5);
        assert_eq!(pixel[0], 150);
        assert_eq!(pixel[1], 150);
        assert_eq!(pixel[2], 150);
    }

    #[test]
    fn test_add_constant_clamp() {
        let img = make_test_image();
        let result = PixelMath::new().add_constant(&img, 200);

        // Should clamp to 255
        let pixel = result.get_pixel(5, 5);
        assert_eq!(pixel[0], 255);
    }

    #[test]
    fn test_multiply_constant() {
        let img = make_test_image();
        let result = PixelMath::new().multiply_constant(&img, 2.0);

        let pixel = result.get_pixel(5, 5);
        assert_eq!(pixel[0], 200);
    }

    #[test]
    fn test_add_images() {
        let img1 = make_test_image();
        let img2 = make_test_image();
        let result = PixelMath::new().add_images(&img1, &img2);

        let pixel = result.get_pixel(5, 5);
        assert_eq!(pixel[0], 200); // 100 + 100
    }

    #[test]
    fn test_subtract() {
        let mut img1 = RgbaImage::new(10, 10);
        let mut img2 = RgbaImage::new(10, 10);
        for y in 0..10 {
            for x in 0..10 {
                img1.put_pixel(x, y, Rgba([200, 200, 200, 255]));
                img2.put_pixel(x, y, Rgba([100, 100, 100, 255]));
            }
        }

        let result = PixelMath::new().subtract(
            &DynamicImage::ImageRgba8(img1),
            &DynamicImage::ImageRgba8(img2),
        );

        let pixel = result.get_pixel(5, 5);
        assert_eq!(pixel[0], 100); // 200 - 100
    }

    #[test]
    fn test_absdiff() {
        let mut img1 = RgbaImage::new(10, 10);
        let mut img2 = RgbaImage::new(10, 10);
        for y in 0..10 {
            for x in 0..10 {
                img1.put_pixel(x, y, Rgba([200, 100, 50, 255]));
                img2.put_pixel(x, y, Rgba([50, 150, 100, 255]));
            }
        }

        let result = PixelMath::new().absdiff(
            &DynamicImage::ImageRgba8(img1),
            &DynamicImage::ImageRgba8(img2),
        );

        let pixel = result.get_pixel(5, 5);
        assert_eq!(pixel[0], 150); // |200 - 50|
        assert_eq!(pixel[1], 50); // |100 - 150|
        assert_eq!(pixel[2], 50); // |50 - 100|
    }

    #[test]
    fn test_invert() {
        let mut img = RgbaImage::new(10, 10);
        for y in 0..10 {
            for x in 0..10 {
                img.put_pixel(x, y, Rgba([100, 150, 200, 255]));
            }
        }

        let result = PixelMath::new().invert(&DynamicImage::ImageRgba8(img));

        let pixel = result.get_pixel(5, 5);
        assert_eq!(pixel[0], 155); // 255 - 100
        assert_eq!(pixel[1], 105); // 255 - 150
        assert_eq!(pixel[2], 55); // 255 - 200
    }

    #[test]
    fn test_bitwise_and() {
        let mut img1 = RgbaImage::new(10, 10);
        let mut img2 = RgbaImage::new(10, 10);
        for y in 0..10 {
            for x in 0..10 {
                img1.put_pixel(x, y, Rgba([0b11110000, 0b11110000, 0b11110000, 255]));
                img2.put_pixel(x, y, Rgba([0b11001100, 0b11001100, 0b11001100, 255]));
            }
        }

        let result = PixelMath::new().bitwise_and(
            &DynamicImage::ImageRgba8(img1),
            &DynamicImage::ImageRgba8(img2),
        );

        let pixel = result.get_pixel(5, 5);
        assert_eq!(pixel[0], 0b11000000);
    }

    #[test]
    fn test_bitwise_or() {
        let mut img1 = RgbaImage::new(10, 10);
        let mut img2 = RgbaImage::new(10, 10);
        for y in 0..10 {
            for x in 0..10 {
                img1.put_pixel(x, y, Rgba([0b00110000, 0b00110000, 0b00110000, 255]));
                img2.put_pixel(x, y, Rgba([0b00001100, 0b00001100, 0b00001100, 255]));
            }
        }

        let result = PixelMath::new().bitwise_or(
            &DynamicImage::ImageRgba8(img1),
            &DynamicImage::ImageRgba8(img2),
        );

        let pixel = result.get_pixel(5, 5);
        assert_eq!(pixel[0], 0b00111100);
    }

    #[test]
    fn test_pow() {
        let mut img = RgbaImage::new(10, 10);
        for y in 0..10 {
            for x in 0..10 {
                img.put_pixel(x, y, Rgba([128, 128, 128, 255]));
            }
        }

        // 0.5 (sqrt) of 128 = ~11.3 -> 11
        let result = PixelMath::new().pow(&DynamicImage::ImageRgba8(img), 0.5);

        let pixel = result.get_pixel(5, 5);
        assert!(pixel[0] >= 10 && pixel[0] <= 15);
    }
}
