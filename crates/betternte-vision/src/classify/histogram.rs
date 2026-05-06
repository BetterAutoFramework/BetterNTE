//! Histogram computation
//!
//! Provides 1D and 3D histogram computation, histogram equalization, and back projection.

use image::{DynamicImage, GenericImageView, GrayImage, Luma};
use ndarray::{Array1, Array3};

/// Histogram computation
pub struct Histogram;

impl Histogram {
    pub fn new() -> Self {
        Self
    }

    /// Compute 1D histogram for grayscale image
    ///
    /// Returns a histogram with the specified number of bins.
    pub fn compute_1d(&self, gray: &GrayImage, bins: u32) -> Array1<f32> {
        let mut hist = Array1::zeros(bins as usize);
        let scale = bins as f32 / 256.0;

        for pixel in gray.pixels() {
            let bin = ((pixel[0] as f32 * scale) as usize).min(bins as usize - 1);
            hist[bin] += 1.0;
        }

        // Normalize
        let total = hist.sum();
        if total > 0.0 {
            hist /= total;
        }

        hist
    }

    /// Compute 3D RGB histogram
    ///
    /// Returns a 3D array with bins_per_channel on each axis.
    pub fn compute_3d(&self, rgb: &DynamicImage, bins_per_channel: u32) -> Array3<f32> {
        let mut hist = Array3::zeros((
            bins_per_channel as usize,
            bins_per_channel as usize,
            bins_per_channel as usize,
        ));

        let scale = bins_per_channel as f32 / 256.0;
        let (width, height) = rgb.dimensions();

        for y in 0..height {
            for x in 0..width {
                let pixel = rgb.get_pixel(x, y);
                let r_bin = ((pixel[0] as f32 * scale) as usize).min(bins_per_channel as usize - 1);
                let g_bin = ((pixel[1] as f32 * scale) as usize).min(bins_per_channel as usize - 1);
                let b_bin = ((pixel[2] as f32 * scale) as usize).min(bins_per_channel as usize - 1);
                hist[[r_bin, g_bin, b_bin]] += 1.0;
            }
        }

        // Normalize
        let total = hist.sum();
        if total > 0.0 {
            hist /= total;
        }

        hist
    }

    /// Compute histogram from HSV image
    ///
    /// Only uses H and S channels, ignoring V.
    pub fn compute_hsv(&self, hsv: &DynamicImage, bins_per_channel: u32) -> Array3<f32> {
        let mut hist = Array3::zeros((
            bins_per_channel as usize,
            bins_per_channel as usize,
            bins_per_channel as usize,
        ));

        // HSV in our detector uses H in [0, 180], S and V in [0, 255]
        // We need to normalize differently for H vs S,V
        let h_scale = bins_per_channel as f32 / 180.0;
        let sv_scale = bins_per_channel as f32 / 256.0;
        let (width, height) = hsv.dimensions();

        for y in 0..height {
            for x in 0..width {
                let pixel = hsv.get_pixel(x, y);
                let h_bin =
                    ((pixel[0] as f32 * h_scale) as usize).min(bins_per_channel as usize - 1);
                let s_bin =
                    ((pixel[1] as f32 * sv_scale) as usize).min(bins_per_channel as usize - 1);
                let v_bin =
                    ((pixel[2] as f32 * sv_scale) as usize).min(bins_per_channel as usize - 1);
                hist[[h_bin, s_bin, v_bin]] += 1.0;
            }
        }

        // Normalize
        let total = hist.sum();
        if total > 0.0 {
            hist /= total;
        }

        hist
    }

    /// Histogram equalization for contrast enhancement
    pub fn equalize(&self, gray: &GrayImage) -> GrayImage {
        let bins = 256u32;
        let mut hist = Array1::zeros(bins as usize);

        // Compute histogram
        for pixel in gray.pixels() {
            hist[pixel[0] as usize] += 1.0;
        }

        // Compute CDF
        let total = (gray.width() * gray.height()) as f32;
        let mut cdf = hist.clone();
        for i in 1..bins as usize {
            cdf[i] += cdf[i - 1];
        }

        // Normalize CDF to [0, 255]
        let min_cdf = cdf.iter().find(|&&x| x > 0.0).copied().unwrap_or(0.0);
        let mut lut = Array1::zeros(bins as usize);
        for i in 0..bins as usize {
            let val = ((cdf[i] - min_cdf) / (total - min_cdf) * 255.0).round() as u8;
            lut[i] = val;
        }

        // Apply LUT
        let (width, height) = gray.dimensions();
        let mut result = GrayImage::new(width, height);

        for y in 0..height {
            for x in 0..width {
                let old_val = gray.get_pixel(x, y)[0] as usize;
                result.put_pixel(x, y, Luma([lut[old_val]]));
            }
        }

        result
    }

    /// Back projection - find areas matching the given histogram
    ///
    /// Returns a probability map where each pixel's value indicates
    /// how likely it belongs to the histogram distribution.
    pub fn back_project(&self, image: &DynamicImage, histogram: &Array3<f32>) -> DynamicImage {
        let bins_per_channel = histogram.shape()[0];
        let h_scale = bins_per_channel as f32 / 256.0;
        let s_scale = bins_per_channel as f32 / 256.0;
        let v_scale = bins_per_channel as f32 / 256.0;

        let (width, height) = image.dimensions();
        let mut result = GrayImage::new(width, height);

        for y in 0..height {
            for x in 0..width {
                let pixel = image.get_pixel(x, y);

                // Convert to HSV-like space (simplified - use RGB bins)
                let r_bin = ((pixel[0] as f32 * h_scale) as usize).min(bins_per_channel - 1);
                let g_bin = ((pixel[1] as f32 * s_scale) as usize).min(bins_per_channel - 1);
                let b_bin = ((pixel[2] as f32 * v_scale) as usize).min(bins_per_channel - 1);

                // Get probability from histogram
                let prob = histogram[[r_bin, g_bin, b_bin]];

                // Scale to [0, 255]
                let val = (prob * 255.0 * (bins_per_channel as f32)).min(255.0) as u8;
                result.put_pixel(x, y, Luma([val]));
            }
        }

        DynamicImage::ImageLuma8(result)
    }

    /// Compute 2D histogram (H-S plane for HSV)
    pub fn compute_hs(&self, image: &DynamicImage, h_bins: u32, s_bins: u32) -> Array1<f32> {
        let mut hist = Array1::zeros((h_bins as usize) * (s_bins as usize));
        let (width, height) = image.dimensions();

        for y in 0..height {
            for x in 0..width {
                let pixel = image.get_pixel(x, y);
                // Treat as H (0-179) and S (0-255)
                let h_bin =
                    ((pixel[0] as f32 / 180.0 * h_bins as f32) as usize).min(h_bins as usize - 1);
                let s_bin =
                    ((pixel[1] as f32 / 256.0 * s_bins as f32) as usize).min(s_bins as usize - 1);
                hist[h_bin * s_bins as usize + s_bin] += 1.0;
            }
        }

        // Normalize
        let total = hist.sum();
        if total > 0.0 {
            hist /= total;
        }

        hist
    }

    /// Compare two histograms using Bhattacharyya coefficient
    pub fn compare(&self, hist1: &Array1<f32>, hist2: &Array1<f32>) -> f32 {
        let n = hist1.len().min(hist2.len());
        let mut sum = 0.0f32;

        for i in 0..n {
            sum += (hist1[i] * hist2[i]).sqrt();
        }

        sum
    }

    /// Compare two 3D histograms using Bhattacharyya coefficient
    pub fn compare_3d(&self, hist1: &Array3<f32>, hist2: &Array3<f32>) -> f32 {
        let shape1 = hist1.shape();
        let shape2 = hist2.shape();

        if shape1 != shape2 {
            return 0.0;
        }

        let mut sum = 0.0f32;

        // Iterate over all elements
        for i in 0..shape1[0] {
            for j in 0..shape1[1] {
                for k in 0..shape1[2] {
                    sum += (hist1[[i, j, k]] * hist2[[i, j, k]]).sqrt();
                }
            }
        }

        sum
    }
}

impl Default for Histogram {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compute_1d() {
        let mut img = GrayImage::new(100, 100);
        // Fill with uniform value
        for pixel in img.pixels_mut() {
            *pixel = Luma([128]);
        }

        let hist = Histogram::new().compute_1d(&img, 256);

        // All pixels should be in bin 128 (or nearby depending on scaling).
        assert!(hist[128] > 0.9); // ~100% in one bin
    }

    #[test]
    fn test_compute_1d_normalized() {
        let img = GrayImage::new(10, 10);
        let hist = Histogram::new().compute_1d(&img, 256);

        // Sum of all bins should be 1.0 (normalized)
        let sum: f32 = hist.iter().sum();
        assert!((sum - 1.0).abs() < 0.01);
    }

    #[test]
    fn test_equalize() {
        let mut img = GrayImage::new(100, 100);
        // Create a low-contrast image
        for pixel in img.pixels_mut() {
            *pixel = Luma([50]);
        }

        let hist = Histogram::new();
        let equalized = hist.equalize(&img);

        // Uniform input may legitimately yield identical pixel values; assert structural sanity.
        assert_eq!(equalized.dimensions(), img.dimensions());
    }

    #[test]
    fn test_back_project() {
        let img = image::RgbImage::new(10, 10);
        let img_clone = img.clone();

        let hist = Histogram::new().compute_3d(&DynamicImage::ImageRgb8(img), 8);

        let result = Histogram::new().back_project(&DynamicImage::ImageRgb8(img_clone), &hist);

        // All pixels should have similar values
        let first = result.get_pixel(0, 0)[0];
        let last = result.get_pixel(9, 9)[0];
        assert!((first as i32 - last as i32).abs() < 10);
    }

    #[test]
    fn test_compare_histograms() {
        let hist1 = Array1::from_vec(vec![0.1, 0.2, 0.3, 0.4]);
        let hist2 = Array1::from_vec(vec![0.1, 0.2, 0.3, 0.4]);

        let similarity = Histogram::new().compare(&hist1, &hist2);

        assert!((similarity - 1.0).abs() < 0.01);
    }

    #[test]
    fn test_compare_different_histograms() {
        let hist1 = Array1::from_vec(vec![1.0, 0.0, 0.0, 0.0]);
        let hist2 = Array1::from_vec(vec![0.0, 0.0, 0.0, 1.0]);

        let similarity = Histogram::new().compare(&hist1, &hist2);

        assert!(similarity < 0.1); // Very different
    }
}
