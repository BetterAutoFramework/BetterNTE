//! 图像预处理

use betternte_core::Region;
use image::{DynamicImage, GenericImageView, Rgba};

/// Image preprocessing utilities.
pub struct ImagePreprocessor;

impl ImagePreprocessor {
    /// Add a constant-color border around `image`.
    pub fn add_border(
        image: &DynamicImage,
        padding: u32,
        color: betternte_core::Color,
    ) -> DynamicImage {
        let (w, h) = image.dimensions();
        let new_w = w + 2 * padding;
        let new_h = h + 2 * padding;
        let fill = Rgba([color.r, color.g, color.b, color.a]);
        let mut new_img = image::RgbaImage::from_pixel(new_w, new_h, fill);
        for y in 0..h {
            for x in 0..w {
                let pixel = image.get_pixel(x, y);
                new_img.put_pixel(x + padding, y + padding, pixel);
            }
        }
        DynamicImage::ImageRgba8(new_img)
    }

    /// 裁剪图像
    pub fn crop(image: &DynamicImage, region: &Region) -> Option<DynamicImage> {
        let (w, h) = image.dimensions();
        if region.x < 0 || region.y < 0 {
            return None;
        }
        let x = region.x as u32;
        let y = region.y as u32;
        if x + region.width > w || y + region.height > h {
            return None;
        }
        Some(image.crop_imm(x, y, region.width, region.height))
    }

    /// 缩放图像
    pub fn resize(image: &DynamicImage, width: u32, height: u32) -> DynamicImage {
        image.resize_exact(width, height, image::imageops::FilterType::Lanczos3)
    }

    /// 转灰度
    pub fn to_grayscale(image: &DynamicImage) -> DynamicImage {
        DynamicImage::ImageLuma8(image.to_luma8())
    }

    /// 二值化（阈值 0~255）
    pub fn threshold(image: &DynamicImage, value: u8) -> DynamicImage {
        let gray = image.to_luma8();
        let binary = image::ImageBuffer::from_fn(gray.width(), gray.height(), |x, y| {
            let pixel = gray.get_pixel(x, y);
            if pixel[0] >= value {
                image::Luma([255u8])
            } else {
                image::Luma([0u8])
            }
        });
        DynamicImage::ImageLuma8(binary)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{DynamicImage, Rgb, RgbImage};

    fn make_test_image(w: u32, h: u32) -> DynamicImage {
        let mut img = RgbImage::new(w, h);
        for y in 0..h {
            for x in 0..w {
                let val = if x < w / 2 { 100u8 } else { 200u8 };
                img.put_pixel(x, y, Rgb([val, val, val]));
            }
        }
        DynamicImage::ImageRgb8(img)
    }

    #[test]
    fn test_preprocessor_crop_dimensions() {
        let image = make_test_image(200, 150);
        let region = betternte_core::Region {
            x: 20,
            y: 10,
            width: 80,
            height: 60,
        };

        let cropped = ImagePreprocessor::crop(&image, &region).unwrap();

        assert_eq!(cropped.width(), 80);
        assert_eq!(cropped.height(), 60);
    }

    #[test]
    fn test_preprocessor_resize_dimensions() {
        let image = make_test_image(200, 100);

        let resized = ImagePreprocessor::resize(&image, 100, 50);

        assert_eq!(resized.width(), 100);
        assert_eq!(resized.height(), 50);
    }

    #[test]
    fn test_preprocessor_grayscale_single_channel() {
        let image = make_test_image(100, 100);

        let gray = ImagePreprocessor::to_grayscale(&image);

        if let DynamicImage::ImageLuma8(_) = &gray {
            // 正确
        } else {
            panic!("转灰度后应为 ImageLuma8");
        }
    }

    #[test]
    fn test_preprocessor_threshold_binary() {
        let image = make_test_image(100, 100);

        let binary = ImagePreprocessor::threshold(&image, 128);

        if let DynamicImage::ImageLuma8(img) = &binary {
            // 低于阈值的像素应为 0
            assert_eq!(img.get_pixel(25, 50)[0], 0, "低于阈值应为 0");
            assert_eq!(img.get_pixel(75, 50)[0], 255, "高于阈值应为 255");
        } else {
            panic!("二值化输出应为 ImageLuma8");
        }
    }

    #[test]
    fn test_preprocessor_add_border() {
        let image = DynamicImage::ImageRgb8(RgbImage::new(100, 80));
        let padding = 10;
        let color = betternte_core::Color {
            r: 0,
            g: 0,
            b: 0,
            a: 255,
        };

        let bordered = ImagePreprocessor::add_border(&image, padding, color);

        assert_eq!(bordered.width(), 120, "宽度应增加 2*padding = 20");
        assert_eq!(bordered.height(), 100, "高度应增加 2*padding = 20");
    }
}
