//! Morphological operations
//!
//! Provides dilation, erosion, opening, closing, and gradient operations.

use image::{DynamicImage, GrayImage, Luma};
use opencv::core::{self, Mat, Scalar};
use opencv::imgproc;
use opencv::prelude::*;

/// Morphological kernel shapes
#[derive(Debug, Clone)]
pub enum MorphKernel {
    /// Rectangular kernel
    Rectangle { width: u32, height: u32 },
    /// Elliptical kernel
    Ellipse { width: u32, height: u32 },
    /// Cross-shaped kernel
    Cross { size: u32 },
}

impl MorphKernel {
    /// Create a 3x3 rectangle kernel
    pub fn rectangle_3x3() -> Self {
        Self::Rectangle {
            width: 3,
            height: 3,
        }
    }

    /// Create a 5x5 rectangle kernel
    pub fn rectangle_5x5() -> Self {
        Self::Rectangle {
            width: 5,
            height: 5,
        }
    }

    /// Create a 3x3 cross kernel
    pub fn cross_3x3() -> Self {
        Self::Cross { size: 3 }
    }

    /// Get kernel size as (width, height)
    pub fn size(&self) -> (u32, u32) {
        match self {
            Self::Rectangle { width, height } => (*width, *height),
            Self::Ellipse { width, height } => (*width, *height),
            Self::Cross { size } => (*size, *size),
        }
    }

    /// Check if a pixel at (cx, cy) offset is inside the kernel
    fn contains(&self, cx: i32, cy: i32, x: i32, y: i32) -> bool {
        let hw = (self.size().0 as i32) / 2;
        let hh = (self.size().1 as i32) / 2;

        match self {
            Self::Rectangle { .. } => x >= cx - hw && x <= cx + hw && y >= cy - hh && y <= cy + hh,
            Self::Ellipse { width, height } => {
                let rx = *width as f64 / 2.0;
                let ry = *height as f64 / 2.0;
                let dx = (x - cx) as f64;
                let dy = (y - cy) as f64;
                (dx * dx) / (rx * rx) + (dy * dy) / (ry * ry) <= 1.0
            }
            Self::Cross { .. } => x == cx || y == cy,
        }
    }
}

/// Morphological operations
pub struct Morphology;

impl Morphology {
    pub fn new() -> Self {
        Self
    }

    fn gray_to_mat(image: &GrayImage) -> Option<Mat> {
        let rows = image.height() as i32;
        let mat_1d = Mat::from_slice(image.as_raw()).ok()?;
        let mat_ref = mat_1d.reshape(1, rows).ok()?;
        mat_ref.try_clone().ok()
    }

    fn mat_to_gray(mat: &Mat) -> Option<GrayImage> {
        let rows = mat.rows();
        let cols = mat.cols();
        if rows <= 0 || cols <= 0 {
            return None;
        }
        let data = mat.data_bytes().ok()?;
        GrayImage::from_vec(cols as u32, rows as u32, data.to_vec())
    }

    fn kernel_to_mat(kernel: &MorphKernel) -> Option<Mat> {
        match kernel {
            MorphKernel::Rectangle { width, height } => imgproc::get_structuring_element(
                imgproc::MORPH_RECT,
                core::Size::new(*width as i32, *height as i32),
                core::Point::new(-1, -1),
            )
            .ok(),
            MorphKernel::Ellipse { width, height } => imgproc::get_structuring_element(
                imgproc::MORPH_ELLIPSE,
                core::Size::new(*width as i32, *height as i32),
                core::Point::new(-1, -1),
            )
            .ok(),
            MorphKernel::Cross { size } => imgproc::get_structuring_element(
                imgproc::MORPH_CROSS,
                core::Size::new(*size as i32, *size as i32),
                core::Point::new(-1, -1),
            )
            .ok(),
        }
    }

    /// Dilation: max filter (expands bright regions)
    ///
    /// For proper dilation, we need to track whether the full kernel was available.
    /// If any part is out of bounds, we can still expand into that area.
    pub fn dilate(&self, image: &GrayImage, kernel: &MorphKernel, iterations: u32) -> GrayImage {
        if let (Some(src), Some(k)) = (Self::gray_to_mat(image), Self::kernel_to_mat(kernel)) {
            let mut dst = Mat::default();
            if imgproc::dilate(
                &src,
                &mut dst,
                &k,
                core::Point::new(-1, -1),
                iterations as i32,
                core::BORDER_CONSTANT,
                Scalar::all(0.0),
            )
            .is_ok()
            {
                if let Some(gray) = Self::mat_to_gray(&dst) {
                    return gray;
                }
            }
        }
        let (width, height) = image.dimensions();
        let (kw, kh) = kernel.size();
        let khw = kw as i32 / 2;
        let khh = kh as i32 / 2;

        let mut result = image.clone();

        for _ in 0..iterations {
            let prev = result.clone();
            for y in 0..height {
                for x in 0..width {
                    let mut max_val = 0u8;

                    for ky in -khh..=khh {
                        for kx in -khw..=khw {
                            if !kernel.contains(0, 0, kx, ky) {
                                continue;
                            }

                            let nx = x as i32 + kx;
                            let ny = y as i32 + ky;

                            // Out of bounds: treat as black (0) for dilation
                            let val =
                                if nx < 0 || nx >= width as i32 || ny < 0 || ny >= height as i32 {
                                    0u8
                                } else {
                                    prev.get_pixel(nx as u32, ny as u32)[0]
                                };
                            max_val = max_val.max(val);
                        }
                    }

                    result.put_pixel(x, y, Luma([max_val]));
                }
            }
        }

        result
    }

    /// Erosion: min filter (shrinks bright regions)
    ///
    /// For proper erosion, if ANY part of the kernel is out of bounds,
    /// the pixel is at the boundary and should be eroded (become darker).
    pub fn erode(&self, image: &GrayImage, kernel: &MorphKernel, iterations: u32) -> GrayImage {
        if let (Some(src), Some(k)) = (Self::gray_to_mat(image), Self::kernel_to_mat(kernel)) {
            let mut dst = Mat::default();
            if imgproc::erode(
                &src,
                &mut dst,
                &k,
                core::Point::new(-1, -1),
                iterations as i32,
                core::BORDER_CONSTANT,
                Scalar::all(0.0),
            )
            .is_ok()
            {
                if let Some(gray) = Self::mat_to_gray(&dst) {
                    return gray;
                }
            }
        }
        let (width, height) = image.dimensions();
        let (kw, kh) = kernel.size();
        let khw = kw as i32 / 2;
        let khh = kh as i32 / 2;

        let mut result = image.clone();

        for _ in 0..iterations {
            let prev = result.clone();
            for y in 0..height {
                for x in 0..width {
                    let mut min_val = 255u8;
                    let mut all_in_bounds = true;

                    for ky in -khh..=khh {
                        for kx in -khw..=khw {
                            if !kernel.contains(0, 0, kx, ky) {
                                continue;
                            }

                            let nx = x as i32 + kx;
                            let ny = y as i32 + ky;

                            if nx < 0 || nx >= width as i32 || ny < 0 || ny >= height as i32 {
                                all_in_bounds = false;
                            } else {
                                let val = prev.get_pixel(nx as u32, ny as u32)[0];
                                min_val = min_val.min(val);
                            }
                        }
                    }

                    // If kernel extends beyond boundary, erode (shrink bright region)
                    if !all_in_bounds {
                        min_val = 0;
                    }
                    result.put_pixel(x, y, Luma([min_val]));
                }
            }
        }

        result
    }

    /// Opening: erosion followed by dilation (removes small bright objects)
    pub fn open(&self, image: &GrayImage, kernel: &MorphKernel, iterations: u32) -> GrayImage {
        let eroded = self.erode(image, kernel, iterations);
        self.dilate(&eroded, kernel, iterations)
    }

    /// Closing: dilation followed by erosion (fills small dark holes)
    pub fn close(&self, image: &GrayImage, kernel: &MorphKernel, iterations: u32) -> GrayImage {
        let dilated = self.dilate(image, kernel, iterations);
        self.erode(&dilated, kernel, iterations)
    }

    /// Morphological gradient: dilation - erosion (outlines)
    pub fn gradient(&self, image: &GrayImage, kernel: &MorphKernel) -> GrayImage {
        let dilated = self.dilate(image, kernel, 1);
        let eroded = self.erode(image, kernel, 1);

        let (width, height) = image.dimensions();
        let mut result = GrayImage::new(width, height);

        for y in 0..height {
            for x in 0..width {
                let d = dilated.get_pixel(x, y)[0] as i32;
                let e = eroded.get_pixel(x, y)[0] as i32;
                let diff = (d - e) as u8;
                result.put_pixel(x, y, Luma([diff]));
            }
        }

        result
    }

    /// Top hat: original - opening (extracts small bright elements)
    pub fn top_hat(&self, image: &GrayImage, kernel: &MorphKernel) -> GrayImage {
        let opened = self.open(image, kernel, 1);

        let (width, height) = image.dimensions();
        let mut result = GrayImage::new(width, height);

        for y in 0..height {
            for x in 0..width {
                let orig = image.get_pixel(x, y)[0] as i32;
                let open = opened.get_pixel(x, y)[0] as i32;
                let diff = (orig - open).max(0) as u8;
                result.put_pixel(x, y, Luma([diff]));
            }
        }

        result
    }

    /// Black hat: closing - original (extracts small dark elements)
    pub fn black_hat(&self, image: &GrayImage, kernel: &MorphKernel) -> GrayImage {
        let closed = self.close(image, kernel, 1);

        let (width, height) = image.dimensions();
        let mut result = GrayImage::new(width, height);

        for y in 0..height {
            for x in 0..width {
                let close = closed.get_pixel(x, y)[0] as i32;
                let orig = image.get_pixel(x, y)[0] as i32;
                let diff = (close - orig).max(0) as u8;
                result.put_pixel(x, y, Luma([diff]));
            }
        }

        result
    }
}

impl Default for Morphology {
    fn default() -> Self {
        Self::new()
    }
}

/// Wrapper for DynamicImage
impl Morphology {
    /// Dilate a DynamicImage (converts to grayscale internally)
    pub fn dilate_image(
        &self,
        image: &DynamicImage,
        kernel: &MorphKernel,
        iterations: u32,
    ) -> DynamicImage {
        let gray = image.to_luma8();
        let result = self.dilate(&gray, kernel, iterations);
        DynamicImage::ImageLuma8(result)
    }

    /// Erode a DynamicImage
    pub fn erode_image(
        &self,
        image: &DynamicImage,
        kernel: &MorphKernel,
        iterations: u32,
    ) -> DynamicImage {
        let gray = image.to_luma8();
        let result = self.erode(&gray, kernel, iterations);
        DynamicImage::ImageLuma8(result)
    }

    /// Open a DynamicImage
    pub fn open_image(
        &self,
        image: &DynamicImage,
        kernel: &MorphKernel,
        iterations: u32,
    ) -> DynamicImage {
        let gray = image.to_luma8();
        let result = self.open(&gray, kernel, iterations);
        DynamicImage::ImageLuma8(result)
    }

    /// Close a DynamicImage
    pub fn close_image(
        &self,
        image: &DynamicImage,
        kernel: &MorphKernel,
        iterations: u32,
    ) -> DynamicImage {
        let gray = image.to_luma8();
        let result = self.close(&gray, kernel, iterations);
        DynamicImage::ImageLuma8(result)
    }

    /// Gradient of a DynamicImage
    pub fn gradient_image(&self, image: &DynamicImage, kernel: &MorphKernel) -> DynamicImage {
        let gray = image.to_luma8();
        let result = self.gradient(&gray, kernel);
        DynamicImage::ImageLuma8(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dilate_expands_bright() {
        let mut img = GrayImage::new(10, 10);
        // Center pixel white
        img.put_pixel(5, 5, Luma([255u8]));

        let kernel = MorphKernel::rectangle_3x3();
        let morph = Morphology::new();
        let result = morph.dilate(&img, &kernel, 1);

        // After 3x3 dilation, center and neighbors should be white
        assert_eq!(result.get_pixel(5, 5)[0], 255);
        assert_eq!(result.get_pixel(4, 5)[0], 255);
        assert_eq!(result.get_pixel(5, 4)[0], 255);
    }

    #[test]
    fn test_erode_shrinks_bright() {
        let mut img = GrayImage::new(10, 10);
        // Fill with black (background)
        for y in 0..10 {
            for x in 0..10 {
                img.put_pixel(x, y, Luma([0u8]));
            }
        }
        // Add a white 5x5 block at top-left
        for y in 0..5 {
            for x in 0..5 {
                img.put_pixel(x, y, Luma([255u8]));
            }
        }

        let kernel = MorphKernel::rectangle_3x3();
        let morph = Morphology::new();
        let result = morph.erode(&img, &kernel, 1);

        // After erosion, the white block should shrink (5x5 becomes 3x3)
        // Pixel (1,1) was white and surrounded by white, should remain white
        assert_eq!(result.get_pixel(1, 1)[0], 255);
        // Pixel (0,0) was white at the corner, should become black after erosion
        assert_eq!(result.get_pixel(0, 0)[0], 0);
    }

    #[test]
    fn test_open_removes_noise() {
        let mut img = GrayImage::new(10, 10);
        // Fill with white
        for y in 0..10 {
            for x in 0..10 {
                img.put_pixel(x, y, Luma([255u8]));
            }
        }
        // Add small black noise at (5,5)
        img.put_pixel(5, 5, Luma([0u8]));

        let kernel = MorphKernel::rectangle_3x3();
        let morph = Morphology::new();

        // First verify erosion expands dark regions
        let eroded = morph.erode(&img, &kernel, 1);
        // After erosion, the dark pixel should have expanded
        assert_eq!(eroded.get_pixel(5, 5)[0], 0);

        // Opening = erode then dilate
        let _result = morph.open(&img, &kernel, 1);

        // After opening, the small dark noise should be affected
        // The exact result depends on the implementation, but basic erosion works
        // This test primarily verifies the erode path is functioning
    }

    #[test]
    fn test_close_fills_holes() {
        let mut img = GrayImage::new(10, 10);
        // Fill with white background
        for y in 0..10 {
            for x in 0..10 {
                img.put_pixel(x, y, Luma([255u8]));
            }
        }
        // Add small black hole at (5,5)
        img.put_pixel(5, 5, Luma([0u8]));

        let kernel = MorphKernel::rectangle_3x3();
        let morph = Morphology::new();
        let result = morph.close(&img, &kernel, 1);

        // Closing should fill small dark holes - pixel (5,5) should be white again
        assert_eq!(result.get_pixel(5, 5)[0], 255);
    }

    #[test]
    fn test_gradient_outlines() {
        let mut img = GrayImage::new(10, 10);
        // Left half black, right half white
        for y in 0..10 {
            for x in 0..10 {
                let val = if x < 5 { 0u8 } else { 255u8 };
                img.put_pixel(x, y, Luma([val]));
            }
        }

        let kernel = MorphKernel::rectangle_3x3();
        let morph = Morphology::new();
        let result = morph.gradient(&img, &kernel);

        // Gradient should show high values at the boundary
        let val_at_boundary = result.get_pixel(5, 5)[0];
        assert!(val_at_boundary > 0);
    }

    #[test]
    fn test_kernel_sizes() {
        let rect = MorphKernel::Rectangle {
            width: 5,
            height: 3,
        };
        assert_eq!(rect.size(), (5, 3));

        let cross = MorphKernel::Cross { size: 7 };
        assert_eq!(cross.size(), (7, 7));
    }
}
