//! Template matcher using OpenCV matchTemplate.

use crate::config::MatchConfig;
use crate::mat_cache::MatCache;
use crate::template::cache::TemplateCache;
use crate::template::{MatchResult, TemplateMatcher};
use async_trait::async_trait;
use betternte_core::{Point, TemplateMatchParams};
use image::{DynamicImage, GrayImage, ImageBuffer, Luma, RgbaImage};
use opencv::core::{self, Mat};
use opencv::imgproc;
use opencv::prelude::*;
use std::collections::HashMap;
use std::sync::Arc;

/// OpenCV-based template matcher.
pub struct OpenCvTemplateMatcher {
    config: MatchConfig,
    cache: Arc<TemplateCache>,
    mat_cache: MatCache,
}

impl OpenCvTemplateMatcher {
    pub fn new() -> Self {
        Self::with_config(MatchConfig::default())
    }

    pub fn with_config(config: MatchConfig) -> Self {
        Self {
            config,
            cache: Arc::new(TemplateCache::new(16)),
            mat_cache: MatCache::new(3),
        }
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
        image: &DynamicImage,
        template: &DynamicImage,
        params: &TemplateMatchParams,
    ) -> anyhow::Result<Vec<MatchResult>> {
        let threshold = if params.threshold.is_finite() {
            params.threshold
        } else {
            self.config.default_threshold
        };

        // Acquire image and template matrices from cache or create new ones.
        let (img_mat, tpl_mat, tpl_w, tpl_h) = if params.grayscale {
            let img_gray = Self::to_gray_u8(image);
            let tpl_gray = Self::to_gray_u8(template);
            if tpl_gray.width() > img_gray.width() || tpl_gray.height() > img_gray.height() {
                return Ok(vec![]);
            }
            let w = tpl_gray.width();
            let h = tpl_gray.height();
            // For grayscale, we create new Mats from the image data.
            // The MatCache is used for the corr result matrix.
            (Self::gray_to_mat(&img_gray)?, Self::gray_to_mat(&tpl_gray)?, w, h)
        } else {
            let img_bgr = Self::to_bgr_mat(image)?;
            let tpl_bgr = Self::to_bgr_mat(template)?;
            if tpl_bgr.cols() > img_bgr.cols() || tpl_bgr.rows() > img_bgr.rows() {
                return Ok(vec![]);
            }
            let w = tpl_bgr.cols() as u32;
            let h = tpl_bgr.rows() as u32;
            (img_bgr, tpl_bgr, w, h)
        };

        // Get the correlation result matrix from cache.
        // The size depends on the image and template sizes.
        let corr_rows = img_mat.rows() - tpl_mat.rows() + 1;
        let corr_cols = img_mat.cols() - tpl_mat.cols() + 1;
        let mut corr = self.mat_cache.acquire(corr_rows, corr_cols, core::CV_32FC1)?;
        
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
        
        // Return the correlation matrix to the cache for reuse.
        self.mat_cache.release(corr);
        
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
        image: &DynamicImage,
        template: &DynamicImage,
        params: &TemplateMatchParams,
    ) -> anyhow::Result<Vec<MatchResult>> {
        self.match_opencv(image, template, params)
    }

    async fn match_multi(
        &self,
        image: &DynamicImage,
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
        let scene = rgba_row(&[[255, 0, 0, 255], [0, 0, 255, 255], [255, 0, 0, 255]]);
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
