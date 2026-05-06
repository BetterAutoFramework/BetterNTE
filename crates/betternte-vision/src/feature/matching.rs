//! Feature matching between descriptor sets

use crate::geometry::homography::Homography;
use betternte_core::PointF;
use ndarray::Array2;

/// Feature match result
#[derive(Debug, Clone)]
pub struct FeatureMatch {
    /// Query descriptor index
    pub query_idx: usize,
    /// Reference descriptor index
    pub train_idx: usize,
    /// Distance between descriptors
    pub distance: f32,
}

/// Feature matcher
pub struct FeatureMatcher;

impl FeatureMatcher {
    pub fn new() -> Self {
        Self
    }

    /// Match descriptors using nearest neighbor with distance ratio test
    ///
    /// Returns matches where the best match is significantly better than the second best.
    ///
    /// # Arguments
    /// * `query` - Query descriptors (N x D)
    /// * `reference` - Reference (training) descriptors (M x D)
    /// * `ratio_threshold` - Maximum ratio between best and second-best match
    ///
    /// # Returns
    /// Vector of matches with (query_idx, train_idx, distance)
    pub fn match_features(
        &self,
        query: &Array2<f32>,
        reference: &Array2<f32>,
        ratio_threshold: f32,
    ) -> Vec<FeatureMatch> {
        let mut matches = Vec::new();

        let n_query = query.nrows();
        let n_train = reference.nrows();

        if n_query == 0 || n_train == 0 {
            return matches;
        }

        for q_idx in 0..n_query {
            let q_row = query.row(q_idx);

            // Find two nearest neighbors
            let mut best_dist = f32::INFINITY;
            let mut second_dist = f32::INFINITY;
            let mut best_idx = 0;

            for t_idx in 0..n_train {
                let t_row = reference.row(t_idx);

                let dist = self.l2_distance(&q_row, &t_row);

                if dist < best_dist {
                    second_dist = best_dist;
                    best_dist = dist;
                    best_idx = t_idx;
                } else if dist < second_dist {
                    second_dist = dist;
                }
            }

            // Distance ratio test
            if best_dist < second_dist * ratio_threshold {
                matches.push(FeatureMatch {
                    query_idx: q_idx,
                    train_idx: best_idx,
                    distance: best_dist,
                });
            }
        }

        matches
    }

    /// L2 (Euclidean) distance between two descriptor vectors
    fn l2_distance(&self, a: &ndarray::ArrayView1<f32>, b: &ndarray::ArrayView1<f32>) -> f32 {
        let mut sum = 0.0f32;
        for (av, bv) in a.iter().zip(b.iter()) {
            let diff = av - bv;
            sum += diff * diff;
        }
        sum.sqrt()
    }

    /// Find homography using matched point pairs with RANSAC
    pub fn find_homography_ransac(
        &self,
        query_points: &[PointF],
        reference_points: &[PointF],
        ransac_threshold: f64,
        max_iterations: usize,
    ) -> Option<(Homography, Vec<bool>)> {
        if query_points.len() != reference_points.len() {
            return None;
        }

        Homography::ransac(
            query_points,
            reference_points,
            ransac_threshold,
            max_iterations,
        )
    }

    /// Filter matches using a known homography (inliers only)
    pub fn filter_matches_with_homography(
        &self,
        matches: &[FeatureMatch],
        query_points: &[PointF],
        reference_points: &[PointF],
        homography: &Homography,
        threshold: f64,
    ) -> Vec<FeatureMatch> {
        matches
            .iter()
            .filter(|m| {
                let q_pt = &query_points[m.query_idx];
                let r_pt = &reference_points[m.train_idx];
                let projected = homography.transform_point(q_pt);
                let dx = projected.x - r_pt.x;
                let dy = projected.y - r_pt.y;
                let error = dx * dx + dy * dy;
                error < threshold * threshold
            })
            .cloned()
            .collect()
    }
}

impl Default for FeatureMatcher {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ndarray::arr2;

    #[test]
    fn test_match_features_identical() {
        let matcher = FeatureMatcher::new();

        // Two identical 1x4 descriptors
        let query = arr2(&[[1.0, 2.0, 3.0, 4.0]]);
        let reference = arr2(&[[1.0, 2.0, 3.0, 4.0]]);

        let matches = matcher.match_features(&query, &reference, 0.7);

        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].query_idx, 0);
        assert_eq!(matches[0].train_idx, 0);
        assert!(matches[0].distance < 0.01);
    }

    #[test]
    fn test_match_features_different() {
        let matcher = FeatureMatcher::new();

        // Query: [[1.0, 2.0, 3.0, 4.0]]
        // Reference has one similar and one very different
        let query = arr2(&[[1.0, 2.0, 3.0, 4.0]]);
        let reference = arr2(&[
            [1.0, 2.0, 3.0, 4.0],         // close match
            [100.0, 200.0, 300.0, 400.0], // very different
        ]);

        let matches = matcher.match_features(&query, &reference, 0.7);

        // Only the first reference is close enough (distance ~0)
        // Second reference has distance ~490, ratio test should reject
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].train_idx, 0);
    }

    #[test]
    fn test_match_features_multiple() {
        let matcher = FeatureMatcher::new();

        let query = arr2(&[[1.0, 2.0, 3.0, 4.0], [10.0, 20.0, 30.0, 40.0]]);
        let reference = arr2(&[
            [1.0, 2.0, 3.0, 4.0],
            [5.0, 6.0, 7.0, 8.0],
            [10.0, 20.0, 30.0, 40.0],
        ]);

        let matches = matcher.match_features(&query, &reference, 0.7);

        // Should find matches for both query descriptors
        assert_eq!(matches.len(), 2);
    }

    #[test]
    fn test_l2_distance() {
        let matcher = FeatureMatcher::new();
        let a = ndarray::arr1(&[0.0, 0.0, 0.0, 0.0]);
        let b = ndarray::arr1(&[3.0, 4.0, 0.0, 0.0]);

        let dist = matcher.l2_distance(&a.view(), &b.view());

        // sqrt(3^2 + 4^2) = 5
        assert!((dist - 5.0).abs() < 0.001);
    }

    #[test]
    fn test_find_homography_ransac_simple() {
        let matcher = FeatureMatcher::new();

        // Simple translation by (10, 5)
        let query = vec![
            PointF { x: 0.0, y: 0.0 },
            PointF { x: 10.0, y: 0.0 },
            PointF { x: 0.0, y: 10.0 },
            PointF { x: 10.0, y: 10.0 },
        ];
        let reference = vec![
            PointF { x: 10.0, y: 5.0 },
            PointF { x: 20.0, y: 5.0 },
            PointF { x: 10.0, y: 15.0 },
            PointF { x: 20.0, y: 15.0 },
        ];

        let result = matcher.find_homography_ransac(&query, &reference, 1.0, 100);

        assert!(result.is_some());
    }

    #[test]
    fn test_filter_matches_with_homography() {
        let matcher = FeatureMatcher::new();

        let matches = vec![
            FeatureMatch {
                query_idx: 0,
                train_idx: 0,
                distance: 1.0,
            },
            FeatureMatch {
                query_idx: 1,
                train_idx: 1,
                distance: 5.0,
            },
        ];

        let query_points = vec![
            PointF { x: 0.0, y: 0.0 },
            PointF { x: 100.0, y: 100.0 }, // Way off
        ];
        let ref_points = vec![PointF { x: 0.0, y: 0.0 }, PointF { x: 0.0, y: 0.0 }];

        let h = Homography::identity();
        let filtered =
            matcher.filter_matches_with_homography(&matches, &query_points, &ref_points, &h, 1.0);

        // Only the first match should be inlier (second is way off)
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].query_idx, 0);
    }
}
