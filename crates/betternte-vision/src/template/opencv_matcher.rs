//! Template matcher using OpenCV matchTemplate.

use crate::config::MatchConfig;
use crate::template::cache::{MatVariants, TemplateCache};
use crate::template::{MatchResult, TemplateMatcher};
use async_trait::async_trait;
use betternte_core::{Point, TemplateMatchParams};
use image::{DynamicImage, GrayImage, ImageBuffer, Luma, RgbaImage};
use opencv::core::{self, Mat};
use opencv::imgproc;
use opencv::prelude::*;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::{Arc, Mutex};

/// OpenCV-based template matcher.
pub struct OpenCvTemplateMatcher {
    config: MatchConfig,
    cache: Arc<TemplateCache>,
    /// Local cache of pre-processed Mat variants, keyed by template hash.
    mat_cache: Mutex<HashMap<u64, MatVariants>>,
}

impl OpenCvTemplateMatcher {
    pub fn new() -> Self {
        Self::with_config(MatchConfig::default())
    }

    pub fn with_config(config: MatchConfig) -> Self {
        Self {
            config,
            cache: Arc::new(TemplateCache::new(16)),
            mat_cache: Mutex::new(HashMap::new()),
        }
    }

    /// Compute a quick hash of a template image for cache key.
    fn template_hash(template: &DynamicImage) -> u64 {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        let rgba = template.to_rgba8();
        (rgba.width(), rgba.height()).hash(&mut hasher);
        // Hash first 256 bytes of pixel data for uniqueness
        let raw = rgba.as_raw();
        let sample_len = raw.len().min(256);
        raw[..sample_len].hash(&mut hasher);
        hasher.finish()
    }

    fn to_gray_u8(image: &DynamicImage) -> GrayImage {
        image.to_luma8()
    }

    fn gray_to_mat(gray: &GrayImage) -> anyhow::Result<Mat> {
        let rows = gray.height() as i32;
        let raw = gray.as_raw();
        let mat_1d = Mat::from_slice(raw)?;
        let mat_2d_ref = mat_1d.reshape(1, rows)?;
        Ok(mat_2d_ref.try_clone()?)
    }

    fn to_bgr_mat(image: &DynamicImage) -> anyhow::Result<Mat> {
        let rgb = image.to_rgb8();
        let (w, h) = (rgb.width() as i32, rgb.height() as i32);
        let mut bgr_data = Vec::with_capacity((w * h * 3) as usize);
        for pixel in rgb.pixels() {
            bgr_data.push(pixel[2]); // B
            bgr_data.push(pixel[1]); // G
            bgr_data.push(pixel[0]); // R
        }
        let mat_1d = Mat::from_slice(&bgr_data)?;
        let mat_3d = mat_1d.reshape(3, h)?;
        Ok(mat_3d.try_clone()?)
    }

    /// Convert BGRA frame to BGR using cvtColor (SIMD-optimized).
    fn bgra_to_bgr(bgra: &Mat) -> anyhow::Result<Mat> {
        let mut bgr = Mat::default();
        imgproc::cvt_color(bgra, &mut bgr, imgproc::COLOR_BGRA2BGR, 0)?;
        Ok(bgr)
    }

    /// Convert BGRA frame to Gray using cvtColor (proper weighted luminance).
    fn bgra_to_gray(bgra: &Mat) -> anyhow::Result<Mat> {
        let mut gray = Mat::default();
        imgproc::cvt_color(bgra, &mut gray, imgproc::COLOR_BGRA2GRAY, 0)?;
        Ok(gray)
    }

    /// Ensure Mat variants are cached for a template. Returns the variants.
    fn ensure_mat_variants(
        &self,
        template: &DynamicImage,
    ) -> anyhow::Result<MatVariants> {
        let key = Self::template_hash(template);

        // Check local mat cache first
        {
            let cache = self.mat_cache.lock().unwrap();
            if let Some(variants) = cache.get(&key) {
                return Ok(variants.clone());
            }
        }

        // Compute Mat variants
        let bgr = Self::to_bgr_mat(template).ok();
        let gray = Self::to_gray_u8(template);
        let gray_mat = Self::gray_to_mat(&gray).ok();

        let variants = MatVariants {
            bgr,
            gray: gray_mat,
        };

        // Store in local cache
        {
            let mut cache = self.mat_cache.lock().unwrap();
            cache.insert(key, variants.clone());
        }

        Ok(variants)
    }

    fn expand_mask_to_3ch(mask: &GrayImage) -> anyhow::Result<Mat> {
        let rows = mask.height() as i32;
        let cols = mask.width() as i32;
        let mut data = Vec::with_capacity((rows * cols * 3) as usize);
        for y in 0..mask.height() {
            for x in 0..mask.width() {
                let v = mask.get_pixel(x, y).0[0];
                data.push(v);
                data.push(v);
                data.push(v);
            }
        }
        let mat_1d = Mat::from_slice(&data)?;
        let mat_3d = mat_1d.reshape(3, rows)?;
        Ok(mat_3d.try_clone()?)
    }

    fn resolve_match_method(params: &TemplateMatchParams) -> (i32, bool) {
        if params.green_mask || params.use_alpha_mask {
            (imgproc::TM_CCORR_NORMED, true)
        } else {
            (imgproc::TM_CCOEFF_NORMED, false)
        }
    }

    fn match_opencv(
        &self,
        image: &opencv::core::Mat,
        template: &DynamicImage,
        params: &TemplateMatchParams,
    ) -> anyhow::Result<Vec<MatchResult>> {
        let threshold = if params.threshold.is_finite() {
            params.threshold
        } else {
            self.config.default_threshold
        };

        // Get or compute cached Mat variants for the template
        let mat_variants = self.ensure_mat_variants(template)?;

        let (img_mat, tpl_mat, tpl_w, tpl_h) = if params.grayscale {
            // BGRA → Gray via cvtColor (SIMD, proper luminance)
            let gray = Self::bgra_to_gray(image)?;
            // Use cached gray Mat or fall back to computing it
            let tpl_gray = match mat_variants.gray {
                Some(ref m) => m.clone(),
                None => {
                    let g = Self::to_gray_u8(template);
                    Self::gray_to_mat(&g)?
                }
            };
            if tpl_gray.cols() > gray.cols() || tpl_gray.rows() > gray.rows() {
                return Ok(vec![]);
            }
            let w = tpl_gray.cols() as u32;
            let h = tpl_gray.rows() as u32;
            (gray, tpl_gray, w, h)
        } else {
            // BGRA → BGR via cvtColor (SIMD, single pass)
            let bgr = Self::bgra_to_bgr(image)?;
            // Use cached BGR Mat or fall back to computing it
            let tpl_bgr = match mat_variants.bgr {
                Some(ref m) => m.clone(),
                None => Self::to_bgr_mat(template)?,
            };
            if tpl_bgr.cols() > bgr.cols() || tpl_bgr.rows() > bgr.rows() {
                return Ok(vec![]);
            }
            let w = tpl_bgr.cols() as u32;
            let h = tpl_bgr.rows() as u32;
            (bgr, tpl_bgr, w, h)
        };

        let mut corr = Mat::default();
        let (method, use_mask) = Self::resolve_match_method(params);
        if use_mask {
            let mask_img = Self::build_template_mask(template, params);
            let mask_mat = if params.grayscale {
                Self::gray_to_mat(&mask_img)?
            } else {
                Self::expand_mask_to_3ch(&mask_img)?
            };
            imgproc::match_template(
                &img_mat,
                &tpl_mat,
                &mut corr,
                method,
                &mask_mat,
            )?;
        } else {
            imgproc::match_template(
                &img_mat,
                &tpl_mat,
                &mut corr,
                method,
                &core::no_array(),
            )?;
        }

        // Fast path: for TM_CCOEFF_NORMED without mask, use minMaxLoc
        // when we only need the best single match above threshold.
        // This avoids the O(rows*cols) full matrix traversal.
        if !use_mask && method == imgproc::TM_CCOEFF_NORMED {
            let mut min_val = 0.0f64;
            let mut max_val = 0.0f64;
            let mut min_loc = core::Point::default();
            let mut max_loc = core::Point::default();
            core::min_max_loc(
                &corr,
                Some(&mut min_val),
                Some(&mut max_val),
                Some(&mut min_loc),
                Some(&mut max_loc),
                &core::no_array(),
            )?;

            // If best match is below threshold, return empty
            if (max_val as f32) < threshold {
                return Ok(vec![]);
            }

            // Check for multiple matches near the threshold.
            // If the best match is significantly above threshold (> 0.95),
            // it's likely the only match — return it directly.
            if (max_val as f32) >= 0.95 || (max_val as f32) >= threshold + 0.1 {
                return Ok(vec![MatchResult {
                    position: Point::new(max_loc.x, max_loc.y),
                    score: max_val as f32,
                    width: tpl_w,
                    height: tpl_h,
                    template_name: String::new(),
                }]);
            }
        }

        // Full scan fallback (for masked matching, non-CCOEFF, or multiple matches)
        let mut matches = Vec::new();
        let rows = corr.rows();
        let cols = corr.cols();
        for y in 0..rows {
            for x in 0..cols {
                let score = *corr.at_2d::<f32>(y, x)?;
                if score >= threshold {
                    matches.push(MatchResult {
                        position: Point::new(x, y),
                        score,
                        width: tpl_w,
                        height: tpl_h,
                        template_name: String::new(),
                    });
                }
            }
        }

        matches.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        let cap = self.cache.max_size().max(1) * 256;
        if matches.len() > cap {
            matches.truncate(cap);
        }
        Ok(matches)
    }

    fn build_template_mask(template: &DynamicImage, params: &TemplateMatchParams) -> GrayImage {
        let rgba: RgbaImage = template.to_rgba8();
        let tol = params.green_mask_tolerance;
        ImageBuffer::from_fn(rgba.width(), rgba.height(), |x, y| {
            let px = rgba.get_pixel(x, y);
            let r = px[0];
            let g = px[1];
            let b = px[2];
            let a = px[3];

            let green_hit = if params.green_mask {
                r.abs_diff(0) <= tol && g.abs_diff(255) <= tol && b.abs_diff(0) <= tol
            } else {
                false
            };
            let alpha_hit = params.use_alpha_mask && a <= params.alpha_mask_threshold;

            if green_hit || alpha_hit {
                Luma([0u8])
            } else {
                Luma([255u8])
            }
        })
    }
}

impl Default for OpenCvTemplateMatcher {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl TemplateMatcher for OpenCvTemplateMatcher {
    fn name(&self) -> &str {
        "opencv_match_template"
    }

    async fn match_template(
        &self,
        image: &opencv::core::Mat,
        template: &DynamicImage,
        params: &TemplateMatchParams,
    ) -> anyhow::Result<Vec<MatchResult>> {
        self.match_opencv(image, template, params)
    }

    async fn match_multi(
        &self,
        image: &opencv::core::Mat,
        templates: &HashMap<String, DynamicImage>,
        params: &TemplateMatchParams,
    ) -> anyhow::Result<HashMap<String, Vec<MatchResult>>> {
        let mut results = HashMap::new();

        for (name, template) in templates {
            let mut matches = self.match_template(image, template, params).await?;
            for m in &mut matches {
                m.template_name = name.clone();
            }
            results.insert(name.clone(), matches);
        }

        Ok(results)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{DynamicImage, Rgba, RgbaImage};

    fn rgba_row(colors: &[[u8; 4]]) -> DynamicImage {
        let mut img = RgbaImage::new(colors.len() as u32, 1);
        for (x, c) in colors.iter().enumerate() {
            img.put_pixel(x as u32, 0, Rgba(*c));
        }
        DynamicImage::ImageRgba8(img)
    }

    #[test]
    fn build_template_mask_respects_green_and_alpha_rules() {
        let template = rgba_row(&[
            [0, 255, 0, 255], // chroma green
            [255, 0, 0, 5],   // low alpha
            [255, 0, 0, 255], // keep
        ]);
        let params = TemplateMatchParams {
            threshold: 0.8,
            green_mask: true,
            green_mask_tolerance: 0,
            use_alpha_mask: true,
            alpha_mask_threshold: 8,
            grayscale: false,
        };
        let mask = OpenCvTemplateMatcher::build_template_mask(&template, &params);
        assert_eq!(mask.get_pixel(0, 0).0[0], 0);
        assert_eq!(mask.get_pixel(1, 0).0[0], 0);
        assert_eq!(mask.get_pixel(2, 0).0[0], 255);
    }

    #[tokio::test]
    async fn mask_params_switch_match_path_and_execute() {
        let matcher = OpenCvTemplateMatcher::new();
        // Build a 1×3 BGRA Mat as the scene image
        let bgra_data: [u8; 12] = [
            255, 0, 0, 255, // pixel 0: BGRA blue
            0, 0, 255, 255, // pixel 1: BGRA red
            255, 0, 0, 255, // pixel 2: BGRA blue
        ];
        let flat = Mat::from_slice(&bgra_data).unwrap();
        let scene = flat.reshape(4, 1).unwrap().try_clone().unwrap(); // 1 row, 4ch → 1×3 BGRA
        let template = rgba_row(&[[255, 0, 0, 255], [0, 255, 0, 255]]);

        let plain = TemplateMatchParams::default();
        let masked = TemplateMatchParams {
            green_mask: true,
            ..Default::default()
        };

        let (plain_method, plain_use_mask) = OpenCvTemplateMatcher::resolve_match_method(&plain);
        let (masked_method, masked_use_mask) =
            OpenCvTemplateMatcher::resolve_match_method(&masked);
        assert_eq!(plain_method, imgproc::TM_CCOEFF_NORMED);
        assert!(!plain_use_mask);
        assert_eq!(masked_method, imgproc::TM_CCORR_NORMED);
        assert!(masked_use_mask);

        let plain_hits = matcher.match_template(&scene, &template, &plain).await;
        let masked_hits = matcher.match_template(&scene, &template, &masked).await;
        assert!(plain_hits.is_ok());
        assert!(masked_hits.is_ok());
    }
}
